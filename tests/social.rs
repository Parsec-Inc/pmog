//! Chat, clans and notifications working together over a real server.

mod common;

use common::*;
use pmog::chat::{ChatCmd, Conv};
use pmog::clan::ClanCmd;
use pmog::protocol::ClientMsg;

fn has_clan_channel(c: &Client) -> bool {
    c.game.chat.convs.iter().any(|cv| matches!(cv.conv, Conv::Clan(_)))
}

#[test]
fn joining_a_clan_opens_its_channel_and_messages_notify_and_tags_show() {
    let server = start_server(30, None);
    let mut a = Client::connect(&server, "Alfa");
    let mut b = Client::connect(&server, "Beta");
    let mut c = Client::connect(&server, "Gama");
    let b_id = b.conn.my_id;

    a.game.outbox.push(ClientMsg::Clan(ClanCmd::Create { name: "Gigachads".into(), tag: "giga".into() }));
    assert!(a.wait(|c| c.game.clan_tag.as_deref() == Some("GIGA")), "creator wears the tag");
    assert!(a.wait(has_clan_channel), "clan channel appears without touching the chat");

    a.game.outbox.push(ClientMsg::Clan(ClanCmd::Invite { target: b_id }));
    a.pump();
    let ok = b.wait(|c| !c.game.clan.invites.is_empty());
    a.pump();
    assert!(ok, "invite arrives; a status {:?} feed {:?}", a.game.clan.status, a.game.feed.iter().take(4).map(|l| &l.text).collect::<Vec<_>>());
    assert!(b.game.notices.iter().any(|n| n.title.to_lowercase().contains("clã") || n.body.contains("Gigachads")), "{:?}", b.game.notices);
    let clan = b.game.clan.invites[0].clan;
    b.game.outbox.push(ClientMsg::Clan(ClanCmd::Accept { clan }));
    assert!(b.wait(has_clan_channel), "new member gets the channel at once");
    assert!(c.wait(|c| c.game.peers.iter().any(|p| p.name == "Beta" && p.clan_tag.as_deref() == Some("GIGA"))), "others see the tag");

    let Some(conv) = a.game.chat.convs.iter().map(|cv| cv.conv).find(|cv| matches!(cv, Conv::Clan(_))) else { panic!() };
    b.game.notices.clear();
    a.game.outbox.push(ClientMsg::Chat(ChatCmd::Send { conv, text: "bora moggar".into() }));
    a.pump();
    assert!(b.wait(|c| c.game.notices.iter().any(|n| n.body.contains("bora moggar"))), "clan message notifies: {:?}", b.game.notices);
    c.pump();
    assert!(!has_clan_channel(&c), "outsiders never see the clan channel");

    // Clanmates can't mog each other; the outsider can.
    a.select("Beta");
    a.game.try_mog(std::time::Instant::now());
    assert!(a.wait(|c| c.feed_has("clã")), "friendly fire refused in the feed");

    b.game.outbox.push(ClientMsg::Clan(ClanCmd::Leave));
    assert!(b.wait(|c| !has_clan_channel(c) && c.game.clan_tag.is_none()), "leaving removes tag and channel");
    server.stop();
}
