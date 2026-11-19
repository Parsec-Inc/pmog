//! Clans: friendly-fire rules in the World, the server's clan system over TCP, and the clan window.

mod common;

use std::time::Instant;

use common::*;
use pmog::app::{App, Screen};
use pmog::clan::{ClanCmd, ClanEvent, ClanInviteView, ClanSnapshot, ClanView, MemberView, RankRow};
use pmog::db::Db;
use pmog::game::{Overlay, Player};
use pmog::input::handle_key;
use pmog::machine_id;
use pmog::protocol::{ClientMsg, PeerView, PlayerId, ServerMsg, StateView};
use pmog::server::Ports;
use pmog::ui;
use pmog::world::World;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

// ---------------------------------------------------------------- World rules

fn world(n: i64, now: Instant) -> World {
    let mut w = World::new();
    for id in 1..=n {
        w.join(id, format!("machine-{id}"), Player { name: format!("p{id}"), ..Default::default() }, now).unwrap();
    }
    w.outbox.clear();
    w
}

fn texts(w: &World, id: PlayerId) -> Vec<String> {
    w.outbox
        .iter()
        .filter(|(to, _)| *to == id)
        .filter_map(|(_, m)| match m {
            ServerMsg::Event { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

fn clan(w: &mut World, ids: &[PlayerId], clan: i64, tag: &str) {
    for id in ids {
        w.set_clan(*id, Some((clan, tag.into())));
    }
}

fn rich(w: &mut World, ids: &[PlayerId]) {
    for id in ids {
        w.slot_mut(*id).unwrap().player.mp = 1000.0;
    }
}

#[test]
fn clanmates_cannot_mog_each_other_and_keep_their_cooldown() {
    let now = Instant::now();
    let mut w = world(2, now);
    clan(&mut w, &[1, 2], 1, "MEW");
    w.mog(1, 2, 1.0, now);
    assert!(texts(&w, 1).iter().any(|t| t.contains("clã")), "{:?}", texts(&w, 1));
    assert!(w.slot(1).unwrap().cooldown_until <= now, "no cooldown spent");
    assert_eq!(w.slot(2).unwrap().player.mp, 0.0);
    assert_eq!(w.slot(1).unwrap().player.mogs_won, 0);
}

#[test]
fn clanmates_can_still_mog_the_training_bot() {
    let now = Instant::now();
    let mut w = world(1, now);
    clan(&mut w, &[1], 1, "MEW");
    w.mog(1, pmog::protocol::NPC_TARGET, 1.0, now);
    assert!(w.slot(1).unwrap().cooldown_until > now);
}

#[test]
fn different_clans_and_clanless_can_mog() {
    let now = Instant::now();
    let mut w = world(3, now);
    clan(&mut w, &[1], 1, "AAA");
    clan(&mut w, &[2], 2, "BBB");
    w.mog(1, 2, 1.0, now);
    assert!(w.slot(1).unwrap().cooldown_until > now);
    w.mog(2, 3, 1.0, now);
    assert!(w.slot(2).unwrap().cooldown_until > now);
}

#[test]
fn clanmates_cannot_bet_but_can_duel_for_fun() {
    let now = Instant::now();
    let mut w = world(2, now);
    rich(&mut w, &[1, 2]);
    clan(&mut w, &[1, 2], 1, "MEW");
    w.challenge(1, vec![2], 0.5, false, now);
    assert!(w.view(2, now).unwrap().invites.is_empty());
    assert!(texts(&w, 1).iter().any(|t| t.contains("amistoso")));
    w.challenge(1, vec![2], 0.0, false, now);
    assert_eq!(w.view(2, now).unwrap().invites.len(), 1);
    w.answer(2, 1, true, now);
    assert_eq!(w.duel_count(), 1);
    assert_eq!(w.view(1, now).unwrap().duel.unwrap().stake, 0.0);
}

#[test]
fn a_bet_invite_dies_if_the_players_become_clanmates() {
    let now = Instant::now();
    let mut w = world(2, now);
    rich(&mut w, &[1, 2]);
    w.challenge(1, vec![2], 0.5, false, now);
    clan(&mut w, &[1, 2], 1, "MEW");
    w.answer(2, 1, true, now);
    assert_eq!(w.duel_count(), 0);
}

#[test]
fn different_clans_can_bet() {
    let now = Instant::now();
    let mut w = world(2, now);
    rich(&mut w, &[1, 2]);
    clan(&mut w, &[1], 1, "AAA");
    clan(&mut w, &[2], 2, "BBB");
    w.challenge(1, vec![2], 0.5, false, now);
    w.answer(2, 1, true, now);
    assert_eq!(w.duel_count(), 1);
}

#[test]
fn the_queue_never_pairs_clanmates() {
    let now = Instant::now();
    let mut w = world(2, now);
    clan(&mut w, &[1, 2], 1, "MEW");
    w.queue(1, Some(0.0), false, false, now);
    w.queue(2, Some(0.0), false, false, now);
    assert_eq!(w.duel_count(), 0);
    let later = now + std::time::Duration::from_secs(60);
    w.queue(2, Some(0.0), false, false, later);
    assert_eq!(w.duel_count(), 0, "not even after a long wait");
}

#[test]
fn the_queue_skips_clanmates_for_an_outsider() {
    let now = Instant::now();
    let mut w = world(3, now);
    clan(&mut w, &[1, 2], 1, "MEW");
    w.queue(1, Some(0.0), false, false, now);
    w.queue(2, Some(0.0), false, false, now);
    w.queue(3, Some(0.0), false, false, now);
    assert_eq!(w.duel_count(), 1);
    assert!(w.in_duel(3));
}

#[test]
fn same_clan_needs_the_same_clan_id() {
    let mut w = world(3, Instant::now());
    clan(&mut w, &[1, 2], 1, "MEW");
    clan(&mut w, &[3], 2, "MEW2");
    assert!(w.same_clan(1, 2));
    assert!(!w.same_clan(1, 3));
    assert!(!w.same_clan(1, 1));
}

// ---------------------------------------------------------------- Server over TCP

/// Queues a command and sends it right away.
fn cmd(c: &mut Client, cmd: ClanCmd) {
    c.game.outbox.push(ClientMsg::Clan(cmd));
    c.pump();
}

fn id_of(c: &Client, name: &str) -> PlayerId {
    c.game.peers.iter().find(|p| p.name == name).unwrap_or_else(|| panic!("{name} is not listed")).id
}

fn tag_of(c: &Client, name: &str) -> Option<String> {
    c.game.peers.iter().find(|p| p.name == name).and_then(|p| p.clan_tag.clone())
}

fn create(c: &mut Client, name: &str, tag: &str) {
    cmd(c, ClanCmd::Create { name: name.into(), tag: tag.into() });
}

/// `leader` invites `invitee` and the invitee accepts.
fn join(leader: &mut Client, invitee: &mut Client) {
    let target = id_of(leader, &invitee.game.player.name);
    cmd(leader, ClanCmd::Invite { target });
    assert!(invitee.wait(|c| c.game.clan.pending() == 1), "invite arrives");
    let clan = invitee.game.clan.invites[0].clan;
    cmd(invitee, ClanCmd::Accept { clan });
    assert!(invitee.wait(|c| c.game.clan.clan.is_some()), "joined");
}

fn members(c: &Client) -> usize {
    c.game.clan.clan.as_ref().map_or(0, |cl| cl.members.len())
}

#[test]
fn create_invite_accept_kick_and_tags() {
    let server = start_server(25, None);
    let mut ana = Client::connect(&server, "Ana");
    let mut beto = Client::connect(&server, "Beto");
    let mut caio = Client::connect(&server, "Caio");
    assert!(wait_all(std::slice::from_mut(&mut ana), |c| c[0].game.peers.len() == 3));

    create(&mut ana, "Mewers", "mew");
    assert!(ana.wait(|c| c.game.clan_tag.as_deref() == Some("MEW")), "own tag is uppercase");
    let view = ana.game.clan.clan.clone().unwrap();
    assert!(view.is_leader() && view.name == "Mewers" && view.members.len() == 1);
    assert!(caio.wait(|c| tag_of(c, "Ana").as_deref() == Some("MEW")), "others see the tag");

    // Same tag in another case is refused.
    create(&mut beto, "Outros", "MeW");
    assert!(beto.wait(|c| c.feed_has("já está em uso")));
    assert!(beto.game.clan.clan.is_none());
    // Bad input is refused too.
    create(&mut beto, "X", "OK");
    assert!(beto.wait(|c| c.feed_has("nome do clã")));

    // Only the leader invites; Beto is not in a clan.
    let ana_id = id_of(&beto, "Ana");
    cmd(&mut beto, ClanCmd::Invite { target: ana_id });
    assert!(beto.wait(|c| c.feed_has("não está em um clã")));

    join(&mut ana, &mut beto);
    assert_eq!(beto.game.clan_tag.as_deref(), Some("MEW"));
    assert!(ana.wait(|c| members(c) == 2));
    assert!(ana.game.notices.iter().any(|n| n.body.contains("Beto")), "leader is told");
    assert!(beto.game.notices.iter().any(|n| n.title.contains("Convite")), "invitee is told");
    assert!(caio.wait(|c| tag_of(c, "Beto").as_deref() == Some("MEW")));

    // A member cannot kick; the leader can.
    cmd(&mut beto, ClanCmd::Kick { member: ana_id });
    assert!(beto.wait(|c| c.feed_has("líder")));
    let beto_id = id_of(&ana, "Beto");
    cmd(&mut ana, ClanCmd::Kick { member: beto_id });
    assert!(beto.wait(|c| c.game.clan.clan.is_none() && c.game.clan_tag.is_none()));
    assert!(beto.game.notices.iter().any(|n| n.body.contains("expulso")));
    assert!(caio.wait(|c| tag_of(c, "Beto").is_none()));
    assert!(ana.wait(|c| members(c) == 1));
}

#[test]
fn invites_are_declined_and_validated() {
    let server = start_server(26, None);
    let mut ana = Client::connect(&server, "Ana");
    let mut beto = Client::connect(&server, "Beto");
    assert!(wait_all(std::slice::from_mut(&mut ana), |c| c[0].game.peers.len() == 2));
    create(&mut ana, "Mewers", "MEW");
    assert!(ana.wait(|c| c.game.clan.clan.is_some()));

    // Nobody to invite: yourself, the bot, or an id that does not exist.
    for target in [0, 9999] {
        cmd(&mut ana, ClanCmd::Invite { target });
    }
    assert!(ana.wait(|c| c.feed_has("Escolha outro jogador")));

    let beto_id = id_of(&ana, "Beto");
    cmd(&mut ana, ClanCmd::Invite { target: beto_id });
    assert!(beto.wait(|c| c.game.clan.pending() == 1));
    cmd(&mut ana, ClanCmd::Invite { target: beto_id });
    assert!(ana.wait(|c| c.feed_has("já tem um convite")));

    let clan = beto.game.clan.invites[0].clan;
    cmd(&mut beto, ClanCmd::Decline { clan });
    assert!(beto.wait(|c| c.game.clan.pending() == 0));
    assert!(ana.wait(|c| c.feed_has("recusou")));
    // An answered invite cannot be used again.
    cmd(&mut beto, ClanCmd::Accept { clan });
    assert!(beto.wait(|c| c.feed_has("não existe mais")));
    assert!(beto.game.clan.clan.is_none());
}

#[test]
fn leadership_moves_on_leave_and_the_last_member_disbands() {
    let server = start_server(27, None);
    let mut ana = Client::connect(&server, "Ana");
    let mut beto = Client::connect(&server, "Beto");
    let mut caio = Client::connect(&server, "Caio");
    assert!(wait_all(std::slice::from_mut(&mut ana), |c| c[0].game.peers.len() == 3));
    create(&mut ana, "Mewers", "MEW");
    assert!(ana.wait(|c| c.game.clan.clan.is_some()));
    join(&mut ana, &mut beto);
    join(&mut ana, &mut caio);
    assert!(ana.wait(|c| members(c) == 3));

    // The leader leaves: the oldest member (Beto) takes over.
    cmd(&mut ana, ClanCmd::Leave);
    assert!(ana.wait(|c| c.game.clan.clan.is_none() && c.game.clan_tag.is_none()));
    assert!(beto.wait(|c| c.game.clan.clan.as_ref().is_some_and(|cl| cl.is_leader() && cl.members.len() == 2)));
    assert!(caio.wait(|c| tag_of(c, "Ana").is_none() && !c.game.clan.clan.as_ref().unwrap().is_leader()));

    // Promote hands it over explicitly.
    let caio_id = id_of(&beto, "Caio");
    cmd(&mut beto, ClanCmd::Promote { member: caio_id });
    assert!(caio.wait(|c| c.game.clan.clan.as_ref().is_some_and(|cl| cl.is_leader())));
    assert!(beto.wait(|c| c.game.clan.clan.as_ref().is_some_and(|cl| !cl.is_leader())));
    cmd(&mut beto, ClanCmd::Promote { member: caio_id });
    assert!(beto.wait(|c| c.feed_has("líder")));

    // Last one out turns off the lights, and the tag is free again.
    cmd(&mut beto, ClanCmd::Leave);
    assert!(beto.wait(|c| c.game.clan.clan.is_none()));
    cmd(&mut caio, ClanCmd::Leave);
    assert!(caio.wait(|c| c.game.clan.clan.is_none() && c.game.clan.ranking.is_empty()));
    create(&mut ana, "Novo", "mew");
    assert!(ana.wait(|c| c.game.clan_tag.as_deref() == Some("MEW")));
}

#[test]
fn disbanding_tells_the_members() {
    let server = start_server(28, None);
    let mut ana = Client::connect(&server, "Ana");
    let mut beto = Client::connect(&server, "Beto");
    assert!(wait_all(std::slice::from_mut(&mut ana), |c| c[0].game.peers.len() == 2));
    create(&mut ana, "Mewers", "MEW");
    assert!(ana.wait(|c| c.game.clan.clan.is_some()));
    join(&mut ana, &mut beto);
    cmd(&mut beto, ClanCmd::Disband);
    assert!(beto.wait(|c| c.feed_has("líder")));
    cmd(&mut ana, ClanCmd::Disband);
    assert!(beto.wait(|c| c.game.clan.clan.is_none() && c.game.clan_tag.is_none()));
    assert!(beto.game.notices.iter().any(|n| n.body.contains("desfeito")));
    assert!(ana.wait(|c| c.game.clan.clan.is_none() && c.game.clan.ranking.is_empty()));
}

#[test]
fn membership_survives_reconnect_and_restart() {
    let db = TempDb::new("clan");
    let server = start_server(29, Some(&db));
    let mut ana = Client::connect(&server, "Ana");
    let mut beto = Client::connect(&server, "Beto");
    assert!(wait_all(std::slice::from_mut(&mut ana), |c| c[0].game.peers.len() == 2));
    create(&mut ana, "Mewers", "MEW");
    assert!(ana.wait(|c| c.game.clan.clan.is_some()));
    join(&mut ana, &mut beto);

    // Reconnect: the tag is there from the very first state.
    drop(beto);
    assert!(ana.wait(|c| c.game.clan.clan.as_ref().is_some_and(|cl| cl.members.iter().filter(|m| m.online).count() == 1)));
    let beto = Client::connect(&server, "Beto");
    assert_eq!(beto.game.clan_tag.as_deref(), Some("MEW"));
    assert!(ana.wait(|c| c.game.clan.clan.as_ref().is_some_and(|cl| cl.members.iter().all(|m| m.online))));

    // Restart the server on the same database.
    drop((ana, beto));
    server.stop();
    let server = start_server(29, Some(&db));
    let mut ana = Client::connect(&server, "Ana");
    assert_eq!(ana.game.clan_tag.as_deref(), Some("MEW"));
    assert!(ana.wait(|c| members(c) == 2));
    let view = ana.game.clan.clan.clone().unwrap();
    assert!(view.is_leader());
    assert!(view.members.iter().any(|m| m.name == "Beto" && !m.online));
    assert_eq!(ana.game.clan.ranking.len(), 1);
}

#[test]
fn ranking_orders_clans_by_total_lifetime_using_saved_profiles_for_offline_members() {
    let db = TempDb::new("rank");
    let machine = |name: &str| machine_id::for_bot(name);
    {
        let seed = Db::open(&db.0).unwrap();
        for (name, lifetime) in [("Ana", 500.0), ("Beto", 100.0), ("Caio", 700.0), ("Duda", 50.0)] {
            let (id, mut p) = seed.load_or_create(&machine(name), name).unwrap();
            p.lifetime = lifetime;
            seed.save(id, &p).unwrap();
        }
    }
    let server = start_server(25, Some(&db));
    let mut ana = Client::connect(&server, "Ana");
    let mut beto = Client::connect(&server, "Beto");
    let mut caio = Client::connect(&server, "Caio");
    let mut duda = Client::connect(&server, "Duda");
    assert!(wait_all(std::slice::from_mut(&mut ana), |c| c[0].game.peers.len() == 4));
    create(&mut ana, "Alfa", "AAA");
    create(&mut caio, "Gama", "CCC");
    create(&mut duda, "Delta", "DDD");
    assert!(ana.wait(|c| c.game.clan.clan.is_some()));
    assert!(caio.wait(|c| c.game.clan.clan.is_some()));
    assert!(duda.wait(|c| c.game.clan.clan.is_some()));
    join(&mut ana, &mut beto);

    cmd(&mut ana, ClanCmd::Refresh);
    let tags = |c: &Client| c.game.clan.ranking.iter().map(|r| (r.tag.clone(), r.members, r.lifetime as i64)).collect::<Vec<_>>();
    let expected = vec![("CCC".to_string(), 1, 700), ("AAA".to_string(), 2, 600), ("DDD".to_string(), 1, 50)];
    assert!(ana.wait(|c| tags(c) == expected), "{:?}", tags(&ana));

    // Duda goes offline: her saved profile still counts.
    drop(duda);
    assert!(caio.wait(|c| c.game.peers.len() == 3));
    cmd(&mut ana, ClanCmd::Refresh);
    assert!(ana.wait(|c| tags(c) == expected), "{:?}", tags(&ana));
}

// ---------------------------------------------------------------- Window

fn offline_app() -> App {
    App::new("Igor".into(), machine_id::for_bot("clan-tui"), None, None, Ports::default())
}

fn playing(own_tag: Option<&str>, rival_tag: Option<&str>) -> App {
    let mut app = offline_app();
    app.screen = Screen::Playing;
    let peer = |id, name: &str, tag: Option<&str>| PeerView {
        id,
        name: name.into(),
        mp: 0.0,
        lifetime: 0.0,
        attack: 10.0,
        defense: 10.0,
        streak: 0,
        in_duel: false,
        immune: false,
        immune_ms: 0,
        clan_tag: tag.map(Into::into),
        statuses: Vec::new(),
        dlc: Default::default(),
        avatar: Default::default(),
        mood: None,
    };
    let view = StateView {
        me: Player { name: "Igor".into(), ..Default::default() },
        streak: 0,
        cooldown_ms: 0,
        cooldown_total_ms: 0,
        immune_ms: 0,
        preworkout_ms: 0,
        cologne_ms: 0,
        aura_farm_ms: 0,
        statuses: vec![],
        meter_ms: 0,
        players: vec![peer(2, "Rival", rival_tag), peer(3, "Livre", None)],
        duel: None,
        invites: Vec::new(),
        outgoing: None,
        queued: None,
        queued_group: false,
        rtt_ms: 0.0,
        clan_tag: own_tag.map(Into::into),
        mood: None,
    };
    app.game.apply(ServerMsg::State(Box::new(view)), Instant::now());
    app
}

fn render(app: &App, w: u16, h: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal.draw(|f| ui::draw(f, app)).unwrap();
    let buf = terminal.backend().buffer().clone();
    (0..h).map(|y| (0..w).map(|x| buf[(x, y)].symbol().to_string()).collect::<String>()).collect::<Vec<_>>().join("\n")
}

fn press(app: &mut App, code: KeyCode) {
    handle_key(app, KeyEvent::new(code, KeyModifiers::NONE), Instant::now());
}

fn typed(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, KeyCode::Char(c));
    }
}

fn sent(app: &App, wanted: &ClanCmd) -> bool {
    app.game.outbox.iter().any(|m| matches!(m, ClientMsg::Clan(c) if c == wanted))
}

fn member(id: PlayerId, name: &str, leader: bool, online: bool, lifetime: f64) -> MemberView {
    MemberView { id, name: name.into(), leader, online, lifetime }
}

fn snapshot(you: PlayerId, leader: PlayerId) -> ClanSnapshot {
    ClanSnapshot {
        clan: Some(ClanView {
            id: 1,
            name: "Clã Ágil".into(),
            tag: "AGIL".into(),
            you,
            leader,
            max_members: 20,
            members: vec![member(1, "Zé 🦀", leader == 1, true, 1200.0), member(2, "Rival", leader == 2, false, 30.0), member(9, "Igor", leader == 9, true, 5.0)],
        }),
        invites: Vec::new(),
        ranking: vec![
            RankRow { id: 1, name: "Clã Ágil".into(), tag: "AGIL".into(), members: 3, lifetime: 1235.0 },
            RankRow { id: 2, name: "Outro".into(), tag: "OUT".into(), members: 1, lifetime: 80.0 },
        ],
    }
}

fn feed(app: &mut App, snapshot: ClanSnapshot) {
    app.game.apply(ServerMsg::Clan(ClanEvent::Snapshot { snapshot: Box::new(snapshot) }), Instant::now());
}

fn invite() -> ClanInviteView {
    ClanInviteView { clan: 7, name: "Mewers".into(), tag: "MEW".into(), from: "Ana".into(), members: 4 }
}

const SIZES: [(u16, u16); 8] = [(1, 1), (3, 2), (10, 4), (30, 10), (60, 20), (84, 28), (140, 45), (300, 80)];

fn renders_everywhere(app: &App) {
    for (w, h) in SIZES {
        render(app, w, h);
    }
}

#[test]
fn tags_show_next_to_names_in_the_header_and_the_peer_list() {
    let app = playing(Some("GIGA"), Some("MEW"));
    let screen = render(&app, 170, 42);
    for needle in ["[GIGA] Igor", "[MEW] Rival"] {
        assert!(screen.contains(needle), "missing {needle:?}\n{screen}");
    }
    let mut app = playing(None, Some("MEW"));
    app.game.selected = 1;
    let screen = render(&app, 170, 42);
    assert!(screen.contains("Alvo: [MEW] Rival"), "{screen}");
    assert!(!render(&playing(None, None), 170, 42).contains("[MEW]"));
}

#[test]
fn clanless_window_shows_invites_and_the_create_form() {
    let mut app = playing(None, None);
    app.game.overlay = Overlay::Clan;
    renders_everywhere(&app);
    assert!(render(&app, 100, 36).contains("Você não está em nenhum clã"));

    feed(&mut app, ClanSnapshot { invites: vec![invite()], ..Default::default() });
    assert_eq!(app.game.clan.pending(), 1);
    let screen = render(&app, 100, 36);
    assert!(screen.contains("[MEW] Mewers") && screen.contains("convite de Ana"), "{screen}");
    renders_everywhere(&app);

    app.game.outbox.clear();
    press(&mut app, KeyCode::Char('a'));
    assert!(sent(&app, &ClanCmd::Accept { clan: 7 }));
    app.game.outbox.clear();
    press(&mut app, KeyCode::Char('R'));
    assert!(sent(&app, &ClanCmd::Decline { clan: 7 }));

    press(&mut app, KeyCode::Char('c'));
    typed(&mut app, "Clã Ágil");
    press(&mut app, KeyCode::Tab);
    typed(&mut app, "ágil1x2y3");
    renders_everywhere(&app);
    let screen = render(&app, 100, 36);
    assert!(screen.contains("Clã Ágil") && screen.contains("GIL1X"), "{screen}");
}

#[test]
fn create_form_sends_a_valid_clan_and_blocks_a_bad_one() {
    let mut app = playing(None, None);
    app.game.overlay = Overlay::Clan;
    app.game.outbox.clear();
    press(&mut app, KeyCode::Char('c'));
    typed(&mut app, "ab");
    press(&mut app, KeyCode::Enter);
    typed(&mut app, "x");
    press(&mut app, KeyCode::Enter);
    assert!(!app.game.outbox.iter().any(|m| matches!(m, ClientMsg::Clan(ClanCmd::Create { .. }))), "name too short");
    assert!(app.game.clan.status.as_deref().unwrap_or("").contains("nome"));
    press(&mut app, KeyCode::Tab);
    typed(&mut app, "c Mewers");
    press(&mut app, KeyCode::Tab);
    typed(&mut app, "e-w");
    press(&mut app, KeyCode::Enter);
    assert!(sent(&app, &ClanCmd::Create { name: "abc Mewers".into(), tag: "XEW".into() }), "{:?}", app.game.outbox);
    // Esc closes only the form first, then the window.
    press(&mut app, KeyCode::Char('c'));
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.game.overlay, Overlay::Clan);
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.game.overlay, Overlay::None);
}

#[test]
fn leader_window_lists_members_and_drives_actions() {
    let mut app = playing(Some("AGIL"), None);
    app.game.overlay = Overlay::Clan;
    feed(&mut app, snapshot(9, 9));
    renders_everywhere(&app);
    let screen = render(&app, 100, 36);
    for needle in ["Clã [AGIL] Clã Ágil", "Zé 🦀", "Rival", "3/20 membros", "(você)", "I convidar"] {
        assert!(screen.contains(needle), "missing {needle:?}\n{screen}");
    }
    // Online members come first, the offline rival last: Zé, Igor, Rival.
    app.game.outbox.clear();
    press(&mut app, KeyCode::Char('k'));
    press(&mut app, KeyCode::Char('n'));
    assert!(!app.game.outbox.iter().any(|m| matches!(m, ClientMsg::Clan(ClanCmd::Kick { .. }))));
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Char('k'));
    assert!(render(&app, 100, 36).contains("Expulsar"));
    press(&mut app, KeyCode::Char('s'));
    assert!(sent(&app, &ClanCmd::Kick { member: 2 }));
    app.game.outbox.clear();
    press(&mut app, KeyCode::Char('p'));
    press(&mut app, KeyCode::Char('y'));
    assert!(sent(&app, &ClanCmd::Promote { member: 2 }));
    app.game.outbox.clear();
    press(&mut app, KeyCode::Char('D'));
    press(&mut app, KeyCode::Enter);
    assert!(sent(&app, &ClanCmd::Disband));
    // Invite picker lists online players without a clan: Livre (Rival has none either in this state).
    press(&mut app, KeyCode::Char('i'));
    renders_everywhere(&app);
    assert!(render(&app, 100, 36).contains("Quem você quer convidar?"));
    app.game.outbox.clear();
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Enter);
    assert!(sent(&app, &ClanCmd::Invite { target: 3 }), "{:?}", app.game.outbox);
}

#[test]
fn member_window_only_offers_leaving_and_refuses_leader_actions() {
    let mut app = playing(Some("AGIL"), None);
    app.game.overlay = Overlay::Clan;
    feed(&mut app, snapshot(9, 1));
    let screen = render(&app, 100, 36);
    assert!(screen.contains("S sair do clã") && !screen.contains("I convidar"), "{screen}");
    app.game.outbox.clear();
    press(&mut app, KeyCode::Char('d'));
    assert!(render(&app, 100, 36).contains("Só o líder"));
    press(&mut app, KeyCode::Char('s'));
    press(&mut app, KeyCode::Char('s'));
    assert!(sent(&app, &ClanCmd::Leave));
}

#[test]
fn ranking_tab_shows_clans_and_marks_the_own_one() {
    let mut app = playing(Some("AGIL"), None);
    app.game.overlay = Overlay::Clan;
    feed(&mut app, snapshot(9, 1));
    app.game.outbox.clear();
    press(&mut app, KeyCode::Tab);
    assert!(sent(&app, &ClanCmd::Refresh), "opening the ranking asks for fresh data");
    let screen = render(&app, 100, 36);
    for needle in ["Ranking", "[AGIL] Clã Ágil", "[OUT] Outro", "3 membros", "1,2k MP"] {
        assert!(screen.contains(needle), "missing {needle:?}\n{screen}");
    }
    renders_everywhere(&app);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::PageDown);
    press(&mut app, KeyCode::PageUp);
    press(&mut app, KeyCode::Tab);
    assert!(render(&app, 100, 36).contains("Membros"));
}

#[test]
fn window_survives_data_shrinking_under_the_cursor() {
    let mut app = playing(Some("AGIL"), None);
    app.game.overlay = Overlay::Clan;
    feed(&mut app, snapshot(9, 9));
    for _ in 0..5 {
        press(&mut app, KeyCode::Down);
    }
    press(&mut app, KeyCode::Char('d'));
    feed(&mut app, ClanSnapshot::default());
    renders_everywhere(&app);
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Char('x'));
    assert_eq!(app.game.overlay, Overlay::Clan);
}

#[test]
fn footer_badge_counts_pending_invites() {
    let mut app = playing(None, None);
    assert!(!render(&app, 170, 42).contains("G (1)"));
    feed(&mut app, ClanSnapshot { invites: vec![invite()], ..Default::default() });
    assert!(render(&app, 170, 42).contains("G (1)"));
}
