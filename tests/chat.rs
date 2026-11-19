//! Chat: the hub against a real database, real TCP clients, the client state and the window.

mod common;

use std::time::{Duration, Instant};

use common::*;
use pmog::app::{App, Screen};
use pmog::chat::{ChatCmd, ChatCtx, ChatEvent, ChatHub, Conv, ConvView};
use pmog::constants::*;
use pmog::db::Db;
use pmog::game::{Overlay, Player};
use pmog::input::handle_key;
use pmog::machine_id;
use pmog::protocol::{ClientMsg, PlayerId, ServerMsg, StateView};
use pmog::server::Ports;
use pmog::ui;
use pmog::world::World;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

// ---------- hub level ----------

struct Rig {
    db: Db,
    world: World,
    hub: ChatHub,
    now: Instant,
    /// Time that passes before each command, so the rate limit stays out of the way.
    pace: Duration,
}

type Events = Vec<(PlayerId, ChatEvent)>;

impl Rig {
    fn new() -> Rig {
        let db = Db::in_memory().unwrap();
        let hub = ChatHub::load(&db);
        Rig { db, world: World::new(), hub, now: Instant::now(), pace: Duration::from_secs(2) }
    }

    /// Registers a player and brings them online.
    fn join(&mut self, name: &str) -> (PlayerId, Events) {
        let machine = machine_id::for_bot(name);
        let (id, player) = self.db.load_or_create(&machine, name).unwrap();
        self.world.join(id, machine, player, self.now).unwrap();
        let events = self.step(|hub, ctx| hub.joined(ctx, id));
        (id, events)
    }

    fn leave(&mut self, id: PlayerId) {
        self.step(|hub, ctx| hub.left(ctx, id));
        self.world.leave(id, self.now);
    }

    fn step(&mut self, f: impl FnOnce(&mut ChatHub, &mut ChatCtx)) -> Events {
        let mut out = Vec::new();
        let mut ctx = ChatCtx { db: &self.db, world: &self.world, out: &mut out, now: self.now };
        f(&mut self.hub, &mut ctx);
        out.into_iter()
            .map(|(to, msg)| match msg {
                ServerMsg::Chat(event) => (to, event),
                other => panic!("unexpected {other:?}"),
            })
            .collect()
    }

    fn cmd(&mut self, from: PlayerId, cmd: ChatCmd) -> Events {
        self.now += self.pace;
        self.step(|hub, ctx| hub.handle(ctx, from, cmd))
    }

    fn say(&mut self, from: PlayerId, conv: Conv, text: &str) -> Events {
        self.cmd(from, ChatCmd::Send { conv, text: text.into() })
    }
}

fn to(events: &Events, player: PlayerId) -> Vec<&ChatEvent> {
    events.iter().filter(|(p, _)| *p == player).map(|(_, e)| e).collect()
}

fn texts(events: &Events, player: PlayerId) -> Vec<String> {
    to(events, player)
        .into_iter()
        .filter_map(|e| match e {
            ChatEvent::Message { msg, .. } => Some(msg.text.clone()),
            _ => None,
        })
        .collect()
}

fn error(events: &Events, player: PlayerId) -> Option<String> {
    to(events, player).into_iter().find_map(|e| match e {
        ChatEvent::Error { text } => Some(text.clone()),
        _ => None,
    })
}

fn upsert(events: &Events, player: PlayerId, conv: Conv) -> Option<ConvView> {
    to(events, player).into_iter().find_map(|e| match e {
        ChatEvent::Upsert(v) if v.conv == conv => Some(v.clone()),
        _ => None,
    })
}

fn group_id(events: &Events, player: PlayerId) -> i64 {
    to(events, player)
        .into_iter()
        .find_map(|e| match e {
            ChatEvent::Upsert(ConvView { conv: Conv::Group(id), .. }) => Some(*id),
            _ => None,
        })
        .expect("group created")
}

#[test]
fn general_reaches_everyone_online_only() {
    let mut rig = Rig::new();
    let (ana, _) = rig.join("Ana");
    let (bia, _) = rig.join("Bia");
    let (cris, _) = rig.join("Cris");
    rig.leave(cris);
    let events = rig.say(ana, Conv::General, "Olá, pessoal!");
    assert_eq!(texts(&events, ana), ["Olá, pessoal!"], "echo to the sender");
    assert_eq!(texts(&events, bia), ["Olá, pessoal!"]);
    assert!(to(&events, cris).is_empty());
    let (_, joined) = rig.join("Dedé");
    let history = upsert(&joined, joined[0].0, Conv::General).expect("Geral on login");
    assert_eq!(history.messages.len(), 1, "short history for newcomers");
}

#[test]
fn login_starts_with_ready_and_geral() {
    let mut rig = Rig::new();
    let (ana, events) = rig.join("Ana");
    assert_eq!(to(&events, ana)[0], &ChatEvent::Ready { me: ana });
    assert!(upsert(&events, ana, Conv::General).is_some());
}

#[test]
fn private_message_reaches_both_sides_with_the_right_view() {
    let mut rig = Rig::new();
    let (ana, _) = rig.join("Ana");
    let (bia, _) = rig.join("Bia");
    let events = rig.say(ana, Conv::Private(bia), "Oi Bia");
    let for_ana = to(&events, ana);
    let for_bia = to(&events, bia);
    assert!(matches!(for_ana[0], ChatEvent::Message { conv: Conv::Private(p), title, .. } if *p == bia && title == "Bia"));
    assert!(matches!(for_bia[0], ChatEvent::Message { conv: Conv::Private(p), title, .. } if *p == ana && title == "Ana"));
    let (cris, _) = rig.join("Cris");
    assert!(texts(&events, cris).is_empty());
}

#[test]
fn private_rules_self_and_unknown_are_refused_in_portuguese() {
    let mut rig = Rig::new();
    let (ana, _) = rig.join("Ana");
    let events = rig.say(ana, Conv::Private(ana), "sozinha");
    assert_eq!(error(&events, ana).as_deref(), Some("Você não pode enviar mensagem para si mesmo."));
    for ghost in [9_999, 0, -5] {
        let events = rig.say(ana, Conv::Private(ghost), "alô?");
        assert_eq!(error(&events, ana).as_deref(), Some("Jogador não encontrado."));
        assert_eq!(texts(&events, ana).len(), 0);
    }
    let events = rig.cmd(ana, ChatCmd::Open { with: 9_999 });
    assert!(error(&events, ana).is_some());
}

#[test]
fn offline_private_message_waits_with_unread_and_unread_survives_reconnects() {
    let mut rig = Rig::new();
    let (ana, _) = rig.join("Ana");
    let (bia, _) = rig.join("Bia");
    rig.leave(bia);
    rig.say(ana, Conv::Private(bia), "Está aí?");
    rig.say(ana, Conv::Private(bia), "Me responde");
    let (bia, events) = rig.join("Bia");
    let view = upsert(&events, bia, Conv::Private(ana)).expect("conversation on login");
    assert_eq!(view.unread, 2);
    assert_eq!(view.title, "Ana");
    assert_eq!(view.messages.len(), 2);
    // Reconnecting without reading keeps the count.
    rig.leave(bia);
    let (bia, events) = rig.join("Bia");
    assert_eq!(upsert(&events, bia, Conv::Private(ana)).unwrap().unread, 2);
    // Reading clears it for good; a new message counts again.
    rig.cmd(bia, ChatCmd::Read { conv: Conv::Private(ana) });
    rig.leave(bia);
    let (bia, events) = rig.join("Bia");
    assert_eq!(upsert(&events, bia, Conv::Private(ana)).unwrap().unread, 0);
    rig.leave(bia);
    rig.say(ana, Conv::Private(bia), "E agora?");
    let (bia, events) = rig.join("Bia");
    assert_eq!(upsert(&events, bia, Conv::Private(ana)).unwrap().unread, 1);
    // Own messages never count as unread for the sender.
    rig.leave(ana);
    let (ana, events) = rig.join("Ana");
    assert_eq!(upsert(&events, ana, Conv::Private(bia)).unwrap().unread, 0);
}

#[test]
fn group_lifecycle_and_membership_rules() {
    let mut rig = Rig::new();
    let (ana, _) = rig.join("Ana");
    let (bia, _) = rig.join("Bia");
    let (cris, _) = rig.join("Cris");
    let created = rig.cmd(ana, ChatCmd::CreateGroup { name: "  Academia\n".into() });
    let group = group_id(&created, ana);
    let view = upsert(&created, ana, Conv::Group(group)).unwrap();
    assert_eq!((view.title.as_str(), view.members.as_slice()), ("Academia", ["Ana".to_string()].as_slice()));

    // Outsiders can neither post nor read nor add nor leave.
    for cmd in [
        ChatCmd::Send { conv: Conv::Group(group), text: "intruso".into() },
        ChatCmd::Read { conv: Conv::Group(group) },
        ChatCmd::AddMember { group, player: cris },
        ChatCmd::Leave { group },
    ] {
        let events = rig.cmd(bia, cmd);
        assert_eq!(error(&events, bia).as_deref(), Some("Você não faz parte deste grupo."));
    }

    let events = rig.cmd(ana, ChatCmd::AddMember { group, player: bia });
    assert!(upsert(&events, bia, Conv::Group(group)).is_some(), "new member gets the conversation");
    assert_eq!(upsert(&events, ana, Conv::Group(group)).unwrap().members, ["Ana", "Bia"]);
    assert!(error(&rig.cmd(ana, ChatCmd::AddMember { group, player: bia }), ana).unwrap().contains("já está"));
    assert!(error(&rig.cmd(ana, ChatCmd::AddMember { group, player: 9_999 }), ana).is_some());

    let events = rig.say(bia, Conv::Group(group), "Bora treinar");
    assert_eq!(texts(&events, ana), ["Bora treinar"]);
    assert_eq!(texts(&events, bia), ["Bora treinar"]);
    assert!(texts(&events, cris).is_empty());
    assert!(matches!(to(&events, ana)[0], ChatEvent::Message { title, .. } if title == "Academia"));

    // Offline member gets it on login, with history and unread.
    rig.leave(ana);
    rig.say(bia, Conv::Group(group), "Chegou?");
    let (ana, events) = rig.join("Ana");
    let view = upsert(&events, ana, Conv::Group(group)).unwrap();
    assert_eq!((view.messages.len(), view.unread), (2, 2));

    // Leaving removes the conversation and the leaver can no longer post.
    let events = rig.cmd(ana, ChatCmd::Leave { group });
    assert!(to(&events, ana).contains(&&ChatEvent::Removed { conv: Conv::Group(group) }));
    assert_eq!(upsert(&events, bia, Conv::Group(group)).unwrap().members, ["Bia"]);
    assert!(error(&rig.say(ana, Conv::Group(group), "ainda aqui"), ana).is_some());

    // The last member leaving makes the group disappear for good.
    rig.cmd(bia, ChatCmd::Leave { group });
    assert!(error(&rig.say(bia, Conv::Group(group), "alô"), bia).is_some());
    assert_eq!(rig.db.chat_group_count().unwrap() + rig.db.chat_message_count(Some(2)).unwrap(), 0);
}

#[test]
fn group_names_and_limits() {
    let mut rig = Rig::new();
    let (ana, _) = rig.join("Ana");
    assert!(error(&rig.cmd(ana, ChatCmd::CreateGroup { name: " \n\u{1} ".into() }), ana).is_some());
    let events = rig.cmd(ana, ChatCmd::CreateGroup { name: "ç".repeat(CHAT_GROUP_NAME_LEN * 3) });
    let group = group_id(&events, ana);
    assert_eq!(upsert(&events, ana, Conv::Group(group)).unwrap().title.chars().count(), CHAT_GROUP_NAME_LEN);
    for _ in 1..CHAT_GROUPS_PER_PLAYER {
        rig.now += Duration::from_secs(5);
        assert!(error(&rig.cmd(ana, ChatCmd::CreateGroup { name: "g".into() }), ana).is_none());
    }
    rig.now += Duration::from_secs(5);
    assert!(error(&rig.cmd(ana, ChatCmd::CreateGroup { name: "demais".into() }), ana).is_some());
}

#[test]
fn clan_channel_is_for_members_of_the_same_clan() {
    let mut rig = Rig::new();
    let (ana, _) = rig.join("Ana");
    let (bia, _) = rig.join("Bia");
    let (cris, _) = rig.join("Cris");
    let (davi, _) = rig.join("Davi");
    rig.world.set_clan(ana, Some((1, "GIGA".into())));
    rig.world.set_clan(bia, Some((1, "GIGA".into())));
    rig.world.set_clan(cris, Some((2, "SUB5".into())));
    // The channel shows up on the next command.
    let events = rig.cmd(ana, ChatCmd::Sync {});
    assert_eq!(upsert(&events, ana, Conv::Clan(1)).unwrap().title, "GIGA");

    let events = rig.say(ana, Conv::Clan(1), "Salve, clã");
    assert_eq!(texts(&events, ana), ["Salve, clã"]);
    assert_eq!(texts(&events, bia), ["Salve, clã"]);
    assert!(texts(&events, cris).is_empty() && texts(&events, davi).is_empty());
    match to(&events, bia)[0] {
        ChatEvent::Message { conv: Conv::Clan(1), title, msg } => {
            assert_eq!(title, "GIGA");
            assert_eq!(msg.tag.as_deref(), Some("GIGA"), "tag at send time");
            assert_eq!(msg.name, "Ana");
        }
        other => panic!("{other:?}"),
    }
    for outsider in [cris, davi] {
        let events = rig.say(outsider, Conv::Clan(1), "entrei?");
        assert_eq!(error(&events, outsider).as_deref(), Some("Você não faz parte deste clã."));
        let events = rig.cmd(outsider, ChatCmd::Read { conv: Conv::Clan(1) });
        assert!(error(&events, outsider).is_some());
    }
    // Leaving the clan removes the channel; a later message no longer reaches the player.
    rig.cmd(bia, ChatCmd::Sync {});
    rig.world.set_clan(bia, None);
    let events = rig.cmd(bia, ChatCmd::Sync {});
    assert!(to(&events, bia).contains(&&ChatEvent::Removed { conv: Conv::Clan(1) }));
    assert!(texts(&rig.say(ana, Conv::Clan(1), "sumiu?"), bia).is_empty());
    // History per clan, shown to a member who logs in later.
    let (eva, _) = rig.join("Eva");
    rig.world.set_clan(eva, Some((1, "GIGA".into())));
    let events = rig.cmd(eva, ChatCmd::Sync {});
    let view = upsert(&events, eva, Conv::Clan(1)).unwrap();
    assert_eq!(view.messages.len(), 2);
    assert_eq!(view.unread, 0, "history from before joining is not unread");
}

#[test]
fn clan_channel_unread_counts_messages_sent_while_offline() {
    let mut rig = Rig::new();
    let (ana, _) = rig.join("Ana");
    let (bia, _) = rig.join("Bia");
    for id in [ana, bia] {
        rig.world.set_clan(id, Some((7, "GIGA".into())));
        rig.cmd(id, ChatCmd::Sync {});
    }
    rig.leave(bia);
    rig.say(ana, Conv::Clan(7), "um");
    rig.say(ana, Conv::Clan(7), "dois");
    let machine = machine_id::for_bot("Bia");
    let (id, player) = rig.db.load_or_create(&machine, "Bia").unwrap();
    rig.world.join(id, machine, player, rig.now).unwrap();
    rig.world.set_clan(id, Some((7, "GIGA".into())));
    let events = rig.step(|hub, ctx| hub.joined(ctx, id));
    assert_eq!(upsert(&events, id, Conv::Clan(7)).unwrap().unread, 2);
}

#[test]
fn rate_limit_allows_a_burst_then_one_per_second() {
    let mut rig = Rig::new();
    rig.pace = Duration::ZERO;
    let (ana, _) = rig.join("Ana");
    let burst = CHAT_RATE_BURST as usize;
    for i in 0..burst {
        assert!(error(&rig.say(ana, Conv::General, &format!("m{i}")), ana).is_none(), "message {i}");
    }
    assert!(error(&rig.say(ana, Conv::General, "rápido"), ana).unwrap().contains("rápido demais"));
    rig.now += Duration::from_millis(500);
    assert!(error(&rig.say(ana, Conv::General, "cedo"), ana).is_some());
    rig.now += Duration::from_millis(600);
    assert!(error(&rig.say(ana, Conv::General, "ok"), ana).is_none());
    rig.now += Duration::from_secs(3600);
    for _ in 0..burst {
        assert!(error(&rig.say(ana, Conv::General, "x"), ana).is_none(), "the bucket never holds more than a burst");
    }
    assert!(error(&rig.say(ana, Conv::General, "x"), ana).is_some());
    // Reading is never limited.
    assert!(error(&rig.cmd(ana, ChatCmd::Read { conv: Conv::General }), ana).is_none());
}

#[test]
fn text_is_validated_and_cleaned() {
    let mut rig = Rig::new();
    let (ana, _) = rig.join("Ana");
    for blank in ["", "   ", "\n\t\u{7}", "\u{1b}"] {
        assert!(error(&rig.say(ana, Conv::General, blank), ana).is_some(), "{blank:?}");
    }
    let too_long = "ã".repeat(CHAT_MAX_CHARS + 1);
    assert!(error(&rig.say(ana, Conv::General, &too_long), ana).unwrap().contains("longa demais"));
    let exact = "ã".repeat(CHAT_MAX_CHARS);
    assert_eq!(texts(&rig.say(ana, Conv::General, &exact), ana), [exact]);
    let events = rig.say(ana, Conv::General, "a\u{1}b\u{1b}[31m\nc\u{0}");
    assert_eq!(texts(&events, ana), ["ab[31m c"]);
    let events = rig.say(ana, Conv::General, "  espaços  ");
    assert_eq!(texts(&events, ana), ["espaços"]);
}

#[test]
fn history_sent_on_login_is_capped_and_database_stays_bounded() {
    let mut rig = Rig::new();
    let (ana, _) = rig.join("Ana");
    let (bia, _) = rig.join("Bia");
    for i in 0..(CHAT_STORE_CAP + 30) {
        rig.now += Duration::from_secs(2);
        rig.say(ana, Conv::Private(bia), &format!("msg {i}"));
    }
    assert_eq!(rig.db.chat_message_count(None).unwrap() as usize, CHAT_STORE_CAP);
    rig.leave(bia);
    let (bia, events) = rig.join("Bia");
    let view = upsert(&events, bia, Conv::Private(ana)).unwrap();
    assert_eq!(view.messages.len(), CHAT_HISTORY);
    assert_eq!(view.messages.last().unwrap().text, format!("msg {}", CHAT_STORE_CAP + 29));
    assert_eq!(view.unread as usize, CHAT_STORE_CAP, "unread counts only what is still stored");
    for i in 0..(CHAT_GENERAL_STORE_CAP + 10) {
        rig.now += Duration::from_secs(2);
        rig.say(ana, Conv::General, &format!("g{i}"));
    }
    assert_eq!(rig.db.chat_message_count(Some(0)).unwrap() as usize, CHAT_GENERAL_STORE_CAP);
}

#[test]
fn messages_carry_sender_rank_and_name() {
    let mut rig = Rig::new();
    let (ana, _) = rig.join("Ana");
    rig.world.slot_mut(ana).unwrap().player.lifetime = 1e9;
    let events = rig.say(ana, Conv::General, "oi");
    match to(&events, ana)[0] {
        ChatEvent::Message { msg, .. } => {
            assert_eq!((msg.name.as_str(), msg.sender), ("Ana", ana));
            assert_eq!(msg.rank, pmog::game::Rank::Transcendent);
            assert!(msg.at > 1_600_000_000);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn hostile_commands_never_panic() {
    let mut rig = Rig::new();
    let (ana, _) = rig.join("Ana");
    for conv in [Conv::Group(i64::MAX), Conv::Group(-1), Conv::Clan(i64::MIN), Conv::Private(i64::MAX), Conv::Private(i64::MIN)] {
        rig.now += Duration::from_secs(2);
        assert!(error(&rig.say(ana, conv, "x"), ana).is_some());
        rig.cmd(ana, ChatCmd::Read { conv });
    }
    rig.cmd(ana, ChatCmd::Leave { group: 42 });
    rig.cmd(ana, ChatCmd::AddMember { group: 42, player: -1 });
}

// ---------- real TCP ----------

fn id_of(c: &Client) -> PlayerId {
    c.game.chat.me.expect("ready")
}

fn send(c: &mut Client, cmd: ChatCmd) {
    c.game.outbox.push(ClientMsg::Chat(cmd));
    c.pump();
}

fn has_text(c: &Client, conv: Conv, text: &str) -> bool {
    c.game.chat.convs.iter().any(|x| x.conv == conv && x.messages.iter().any(|m| m.text == text))
}

fn unread_in(c: &Client, conv: Conv) -> u32 {
    c.game.chat.convs.iter().find(|x| x.conv == conv).map_or(0, |x| x.unread.get())
}

fn connect(server: &pmog::server::ServerHandle, name: &str) -> Client {
    let mut c = Client::connect(server, name);
    assert!(c.wait(|c| c.game.chat.me.is_some()), "chat ready for {name}");
    c
}

#[test]
fn tcp_private_offline_and_unread_across_reconnects() {
    let db = TempDb::new("chat-private");
    let server = start_server(20, Some(&db));
    let mut ana = connect(&server, "Ana");
    let bia = connect(&server, "Bia");
    let bia_id = id_of(&bia);
    drop(bia);
    send(&mut ana, ChatCmd::Send { conv: Conv::Private(bia_id), text: "Oi Bia, você sumiu".into() });
    assert!(ana.wait(|c| has_text(c, Conv::Private(bia_id), "Oi Bia, você sumiu")), "echo to the sender");
    assert!(ana.game.notices.is_empty(), "own messages do not notify");

    let mut bia = connect(&server, "Bia");
    let ana_id = id_of(&ana);
    assert!(bia.wait(|c| has_text(c, Conv::Private(ana_id), "Oi Bia, você sumiu")), "offline message delivered on login");
    assert_eq!(unread_in(&bia, Conv::Private(ana_id)), 1);
    assert_eq!(bia.game.chat.unread(), 1);
    assert!(bia.game.notices.is_empty(), "history on login does not notify");

    // Live message: notice and unread.
    send(&mut ana, ChatCmd::Send { conv: Conv::Private(bia_id), text: "Voltou!".into() });
    assert!(bia.wait(|c| has_text(c, Conv::Private(ana_id), "Voltou!")));
    assert_eq!(bia.game.notices.len(), 1);
    assert_eq!(bia.game.notices[0].title, "Mensagem de Ana");
    assert_eq!(bia.game.chat.unread(), 2);

    // Reconnect without reading: still unread. Then read and reconnect: clean.
    drop(bia);
    let mut bia = connect(&server, "Bia");
    assert!(bia.wait(|c| unread_in(c, Conv::Private(ana_id)) == 2));
    send(&mut bia, ChatCmd::Read { conv: Conv::Private(ana_id) });
    bia.pump();
    drop(bia);
    let mut bia = connect(&server, "Bia");
    assert!(bia.wait(|c| has_text(c, Conv::Private(ana_id), "Voltou!")));
    assert_eq!(unread_in(&bia, Conv::Private(ana_id)), 0);
    server.stop();
}

#[test]
fn tcp_groups_work_end_to_end() {
    let db = TempDb::new("chat-groups");
    let server = start_server(21, Some(&db));
    let mut ana = connect(&server, "Ana");
    let mut bia = connect(&server, "Bia");
    let mut cris = connect(&server, "Cris");
    send(&mut ana, ChatCmd::CreateGroup { name: "Academia".into() });
    assert!(ana.wait(|c| c.game.chat.convs.iter().any(|x| matches!(x.conv, Conv::Group(_)))));
    let group = ana.game.chat.convs.iter().find_map(|c| if let Conv::Group(g) = c.conv { Some(g) } else { None }).unwrap();

    send(&mut cris, ChatCmd::Send { conv: Conv::Group(group), text: "intruso".into() });
    assert!(cris.wait(|c| c.game.chat.status.as_deref() == Some("Você não faz parte deste grupo.")));

    send(&mut ana, ChatCmd::AddMember { group, player: id_of(&bia) });
    assert!(bia.wait(|c| c.game.chat.convs.iter().any(|x| x.conv == Conv::Group(group))));
    send(&mut ana, ChatCmd::Send { conv: Conv::Group(group), text: "Treino às 6h".into() });
    assert!(bia.wait(|c| has_text(c, Conv::Group(group), "Treino às 6h")));
    assert_eq!(bia.game.notices.last().map(|n| n.title.as_str()), Some("Ana em Academia"));
    assert!(!has_text(&cris, Conv::Group(group), "Treino às 6h"));

    send(&mut bia, ChatCmd::Leave { group });
    assert!(bia.wait(|c| !c.game.chat.convs.iter().any(|x| x.conv == Conv::Group(group))));
    assert!(ana.wait(|c| c.game.chat.convs.iter().any(|x| x.conv == Conv::Group(group) && x.members == ["Ana"])));
    server.stop();
}

#[test]
fn tcp_general_reaches_everyone_online() {
    let server = start_server(22, None);
    let mut clients: Vec<Client> = ["Ana", "Bia", "Cris"].iter().map(|n| connect(&server, n)).collect();
    send(&mut clients[0], ChatCmd::Send { conv: Conv::General, text: "Bom dia, Geral".into() });
    assert!(wait_all(&mut clients, |cs| cs.iter().all(|c| has_text(c, Conv::General, "Bom dia, Geral"))));
    assert!(clients.iter().all(|c| c.game.notices.is_empty()), "Geral never notifies");
    assert!(clients.iter().all(|c| c.game.chat.unread() == 0));
    server.stop();
}

#[test]
fn tcp_rate_limit_and_validation_errors_reach_the_client() {
    let server = start_server(23, None);
    let mut ana = connect(&server, "Ana");
    send(&mut ana, ChatCmd::Send { conv: Conv::General, text: "   ".into() });
    assert!(ana.wait(|c| c.game.chat.status.as_deref() == Some("Escreva algo antes de enviar.")));
    send(&mut ana, ChatCmd::Send { conv: Conv::General, text: "a".repeat(CHAT_MAX_CHARS + 1) });
    assert!(ana.wait(|c| c.game.chat.status.as_deref().is_some_and(|s| s.contains("longa demais"))));
    send(&mut ana, ChatCmd::Send { conv: Conv::General, text: "ab\u{1}c".into() });
    assert!(ana.wait(|c| has_text(c, Conv::General, "abc")));
    for i in 0..10 {
        send(&mut ana, ChatCmd::Send { conv: Conv::General, text: format!("spam {i}") });
    }
    assert!(ana.wait(|c| c.game.chat.status.as_deref().is_some_and(|s| s.contains("rápido demais"))));
    let spam = ana.game.chat.convs[0].messages.iter().filter(|m| m.text.starts_with("spam")).count();
    assert!(spam < 10, "{spam} spam messages got through");
    server.stop();
}

#[test]
fn tcp_oversized_and_unknown_traffic_does_not_hurt_the_server() {
    let server = start_server(24, None);
    let mut ana = connect(&server, "Ana");
    send(&mut ana, ChatCmd::Send { conv: Conv::Group(77), text: "x".repeat(50_000) });
    send(&mut ana, ChatCmd::Open { with: i64::MAX });
    assert!(ana.wait(|c| c.game.chat.status.is_some()));
    send(&mut ana, ChatCmd::Send { conv: Conv::General, text: "ainda vivo".into() });
    assert!(ana.wait(|c| has_text(c, Conv::General, "ainda vivo")));
    assert!(!ana.closed);
    server.stop();
}

// ---------- window ----------

fn playing() -> App {
    let mut app = App::new("Igor".into(), machine_id::for_bot("chat-tui"), None, None, Ports::default());
    app.screen = Screen::Playing;
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
        players: vec![pmog::protocol::PeerView { id: 2, name: "Rival".into(), mp: 0.0, lifetime: 0.0, attack: 10.0, defense: 10.0, streak: 0, in_duel: false, immune: false, immune_ms: 0, clan_tag: None, statuses: Vec::new(), forehead: 0, marks: 0 }],
        duel: None,
        invites: Vec::new(),
        outgoing: None,
        queued: None,
        rtt_ms: 0.0,
        clan_tag: None,
    };
    app.game.apply(ServerMsg::State(Box::new(view)), Instant::now());
    app.game.apply(ServerMsg::Chat(ChatEvent::Ready { me: 1 }), Instant::now());
    app
}

fn msg(id: i64, sender: PlayerId, name: &str, text: &str) -> pmog::chat::ChatMsg {
    pmog::chat::ChatMsg { id, sender, name: name.into(), tag: Some("GIGA".into()), rank: pmog::game::Rank::Gigachad, text: text.into(), at: 0 }
}

fn chat_event(app: &mut App, event: ChatEvent) {
    app.game.apply(ServerMsg::Chat(event), Instant::now());
}

fn with_conversations() -> App {
    let mut app = playing();
    chat_event(&mut app, ChatEvent::Upsert(ConvView { conv: Conv::Group(5), title: "Academia".into(), members: vec!["Igor".into(), "Rival".into()], unread: 0, messages: vec![] }));
    chat_event(&mut app, ChatEvent::Upsert(ConvView { conv: Conv::Clan(1), title: "GIGA".into(), members: vec![], unread: 0, messages: vec![msg(1, 2, "Rival", "salve, clã")] }));
    chat_event(&mut app, ChatEvent::Message { conv: Conv::Private(2), title: "Rival".into(), msg: msg(2, 2, "Rival", "Oi, tudo bem? Ação é ótima!") });
    for i in 0..40 {
        chat_event(&mut app, ChatEvent::Message { conv: Conv::General, title: "Geral".into(), msg: msg(10 + i, 2, "Rival", &format!("mensagem número {i} com algum texto comprido para quebrar linha na janela")) });
    }
    app
}

fn render(app: &App, w: u16, h: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal.draw(|f| ui::draw(f, app)).unwrap();
    let buf = terminal.backend().buffer().clone();
    (0..h).map(|y| (0..w).map(|x| buf[(x, y)].symbol().to_string()).collect::<String>()).collect::<Vec<_>>().join("\n")
}

fn key(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    handle_key(app, KeyEvent::new(code, mods), Instant::now());
}

fn press(app: &mut App, code: KeyCode) {
    key(app, code, KeyModifiers::NONE);
}

fn ctrl(app: &mut App, c: char) {
    key(app, KeyCode::Char(c), KeyModifiers::CONTROL);
}

fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, KeyCode::Char(c));
    }
}

fn sent(app: &mut App) -> Vec<ChatCmd> {
    std::mem::take(&mut app.game.outbox)
        .into_iter()
        .filter_map(|m| match m {
            ClientMsg::Chat(ChatCmd::Sync {}) => None,
            ClientMsg::Chat(c) => Some(c),
            _ => None,
        })
        .collect()
}

#[test]
fn window_opens_with_t_and_shows_conversations_and_messages() {
    let mut app = with_conversations();
    press(&mut app, KeyCode::Char('t'));
    assert_eq!(app.game.overlay, Overlay::Chat);
    let screen = render(&app, 140, 40);
    for needle in ["Conversas", "Geral", "Clã [GIGA]", "# Academia", "@ Rival", "[GIGA] ", "Rival", "mensagem número 39", "> "] {
        assert!(screen.contains(needle), "missing {needle:?}\n{screen}");
    }
    assert!(screen.contains("(1)"), "unread badge for the private conversation:\n{screen}");
}

#[test]
fn window_renders_at_any_size() {
    let mut app = with_conversations();
    app.game.overlay = Overlay::Chat;
    app.game.chat.input = "ação ".repeat(40);
    for mode in [pmog::chat::ChatMode::Talk, pmog::chat::ChatMode::PickPlayer { cursor: 99 }, pmog::chat::ChatMode::NewGroup("ç".repeat(30)), pmog::chat::ChatMode::AddPlayer { group: 5, cursor: 0 }] {
        app.game.chat.mode = mode;
        for conv in [Conv::General, Conv::Group(5), Conv::Private(2), Conv::Clan(1)] {
            app.game.chat.select(conv);
            app.game.chat.scroll = 10_000;
            for (w, h) in [(1, 1), (2, 2), (5, 3), (10, 5), (20, 8), (40, 12), (59, 20), (60, 20), (80, 24), (120, 30), (300, 80)] {
                render(&app, w, h);
            }
        }
    }
}

#[test]
fn typing_enter_sends_and_clears_the_line() {
    let mut app = with_conversations();
    app.game.overlay = Overlay::Chat;
    type_text(&mut app, "olá, ação! Ótimo");
    assert!(render(&app, 140, 40).contains("olá, ação! Ótimo"));
    press(&mut app, KeyCode::Backspace);
    press(&mut app, KeyCode::Enter);
    assert_eq!(sent(&mut app).iter().find(|c| matches!(c, ChatCmd::Send { .. })), Some(&ChatCmd::Send { conv: Conv::General, text: "olá, ação! Ótim".into() }));
    assert!(app.game.chat.input.is_empty());
    press(&mut app, KeyCode::Enter);
    assert!(sent(&mut app).iter().all(|c| !matches!(c, ChatCmd::Send { .. })), "blank lines are not sent");
}

#[test]
fn input_is_bounded_and_ignores_control_keys() {
    let mut app = with_conversations();
    app.game.overlay = Overlay::Chat;
    type_text(&mut app, &"x".repeat(CHAT_MAX_CHARS + 50));
    assert_eq!(app.game.chat.input.chars().count(), CHAT_MAX_CHARS);
    key(&mut app, KeyCode::Char('q'), KeyModifiers::ALT);
    key(&mut app, KeyCode::Char('\u{1}'), KeyModifiers::NONE);
    assert_eq!(app.game.chat.input.chars().count(), CHAT_MAX_CHARS);
    assert_eq!(app.game.overlay, Overlay::Chat, "letters do not trigger game shortcuts");
}

#[test]
fn switching_conversations_marks_read_and_tells_the_server() {
    let mut app = with_conversations();
    app.game.overlay = Overlay::Chat;
    sent(&mut app);
    assert_eq!(app.game.chat.unread(), 1);
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.game.chat.selected, Conv::Private(2));
    assert_eq!(app.game.chat.unread(), 0);
    assert!(sent(&mut app).contains(&ChatCmd::Read { conv: Conv::Private(2) }));
    press(&mut app, KeyCode::Down);
    assert_eq!(app.game.chat.selected, Conv::General, "wraps around");
    press(&mut app, KeyCode::Up);
    assert_eq!(app.game.chat.selected, Conv::Private(2));
}

#[test]
fn drawing_the_open_conversation_reads_it() {
    let mut app = with_conversations();
    app.game.overlay = Overlay::Chat;
    app.game.chat.select(Conv::Private(2));
    sent(&mut app);
    chat_event(&mut app, ChatEvent::Message { conv: Conv::Private(2), title: "Rival".into(), msg: msg(99, 2, "Rival", "nova") });
    assert_eq!(app.game.chat.unread(), 1);
    render(&app, 100, 30);
    assert_eq!(app.game.chat.unread(), 0);
    press(&mut app, KeyCode::PageUp);
    assert!(sent(&mut app).contains(&ChatCmd::Read { conv: Conv::Private(2) }));
}

#[test]
fn scrolling_is_bounded_and_resets_on_send() {
    let mut app = with_conversations();
    app.game.overlay = Overlay::Chat;
    render(&app, 100, 30);
    press(&mut app, KeyCode::PageUp);
    assert_eq!(app.game.chat.scroll, CHAT_SCROLL_STEP);
    let before = render(&app, 100, 30);
    assert!(!before.contains("mensagem número 39"), "scrolled away from the newest");
    for _ in 0..200 {
        press(&mut app, KeyCode::PageUp);
    }
    assert_eq!(app.game.chat.scroll, app.game.chat.max_scroll.get());
    for _ in 0..200 {
        press(&mut app, KeyCode::PageDown);
    }
    assert_eq!(app.game.chat.scroll, 0);
}

#[test]
fn ctrl_n_picks_an_online_player_and_opens_a_private_conversation() {
    let mut app = playing();
    app.game.overlay = Overlay::Chat;
    ctrl(&mut app, 'n');
    assert!(render(&app, 100, 30).contains("Rival"));
    press(&mut app, KeyCode::Enter);
    assert_eq!(sent(&mut app), vec![ChatCmd::Open { with: 2 }]);
    chat_event(&mut app, ChatEvent::Upsert(ConvView { conv: Conv::Private(2), title: "Rival".into(), members: vec![], unread: 0, messages: vec![] }));
    assert_eq!(app.game.chat.selected, Conv::Private(2), "jumps to the new conversation");
    // Esc leaves the picker without closing the window.
    ctrl(&mut app, 'n');
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.game.overlay, Overlay::Chat);
    assert!(sent(&mut app).iter().all(|c| !matches!(c, ChatCmd::Open { .. })));
}

#[test]
fn ctrl_g_creates_a_group_and_group_shortcuts_work() {
    let mut app = playing();
    app.game.overlay = Overlay::Chat;
    ctrl(&mut app, 'g');
    assert!(render(&app, 100, 30).contains("Nome do grupo"));
    type_text(&mut app, "Treino Ç");
    press(&mut app, KeyCode::Enter);
    assert_eq!(sent(&mut app), vec![ChatCmd::CreateGroup { name: "Treino Ç".into() }]);
    chat_event(&mut app, ChatEvent::Upsert(ConvView { conv: Conv::Group(8), title: "Treino Ç".into(), members: vec!["Igor".into()], unread: 0, messages: vec![] }));
    assert_eq!(app.game.chat.selected, Conv::Group(8));
    assert!(render(&app, 100, 30).contains("# Treino Ç"));
    ctrl(&mut app, 'a');
    sent(&mut app);
    press(&mut app, KeyCode::Enter);
    assert_eq!(sent(&mut app), vec![ChatCmd::AddMember { group: 8, player: 2 }]);
    ctrl(&mut app, 'l');
    assert_eq!(sent(&mut app), vec![ChatCmd::Leave { group: 8 }]);
    // Outside a group those shortcuts only explain themselves.
    app.game.chat.select(Conv::General);
    sent(&mut app);
    ctrl(&mut app, 'a');
    ctrl(&mut app, 'l');
    assert!(sent(&mut app).is_empty());
    assert!(app.game.chat.status.is_some());
    // An empty name creates nothing.
    ctrl(&mut app, 'g');
    press(&mut app, KeyCode::Enter);
    assert!(sent(&mut app).is_empty());
}

#[test]
fn esc_closes_and_server_errors_show_in_the_window() {
    let mut app = with_conversations();
    app.game.overlay = Overlay::Chat;
    chat_event(&mut app, ChatEvent::Error { text: "Calma, você está enviando rápido demais.".into() });
    assert!(render(&app, 120, 30).contains("rápido demais"));
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.game.overlay, Overlay::None);
}

#[test]
fn footer_shows_unread_count() {
    let mut app = with_conversations();
    let screen = render(&app, 170, 42);
    assert!(screen.contains("T (1)"), "{screen}");
    app.game.overlay = Overlay::Chat;
    app.game.chat.select(Conv::Private(2));
    assert!(render(&app, 170, 42).contains(" T "));
}
