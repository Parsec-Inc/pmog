//! Avatar parts: layers, shop rules on the server and the face drawing.

use std::time::Instant;

use pmog::avatar::{self, Avatar, Kind, Mood, Part};
use pmog::constants::avatar::{AVATAR_HEIGHT, AVATAR_WIDTH};
use pmog::game::Player;
use pmog::items::ItemId;
use pmog::protocol::{PlayerId, ServerMsg};
use pmog::world::World;

fn world(now: Instant) -> World {
    let mut w = World::new();
    for id in 1..=2 {
        w.join(id, format!("machine-{id}"), Player { name: format!("p{id}"), ..Default::default() }, now).unwrap();
    }
    w.outbox.clear();
    w
}

fn player(w: &World, id: PlayerId) -> &Player {
    &w.slot(id).unwrap().player
}

fn texts(w: &World, id: PlayerId) -> Vec<String> {
    w.outbox
        .iter()
        .filter(|(to, _)| *to == id)
        .filter_map(|(_, m)| if let ServerMsg::Event { text, .. } = m { Some(text.clone()) } else { None })
        .collect()
}

#[test]
fn every_part_and_mood_draws_a_full_face() {
    for kind in Kind::ALL {
        assert!(kind.parts().next().is_some_and(|d| d.free()), "{kind:?} starts with a free part");
        for d in kind.parts() {
            let mut a = Avatar::default();
            a.set(d.part);
            assert_eq!(a.get(kind), d.part);
            for mood in Mood::ALL {
                let px = avatar::compose(&a, mood, Some((999, (235, 60, 140))));
                assert_eq!((px.len(), px[0].len()), (AVATAR_HEIGHT, AVATAR_WIDTH), "{:?} {mood:?}", d.part);
            }
        }
    }
}

#[test]
fn the_forehead_number_only_shows_when_asked() {
    let a = Avatar::default();
    let pink = (235, 60, 140);
    let has_pink = |px: &avatar::Pixels| px.iter().flatten().any(|c| *c == Some(pink));
    assert!(!has_pink(&avatar::compose(&a, Mood::Neutral, None)));
    assert!(has_pink(&avatar::compose(&a, Mood::Neutral, Some((7, pink)))));
}

#[test]
fn buying_a_part_checks_money_rank_and_dlc_then_wears_it() {
    let now = Instant::now();
    let mut w = world(now);
    let cap = avatar::def(Part::Cap);
    w.buy_part(1, Part::Cap);
    assert!(!player(&w, 1).owns_part(Part::Cap), "no MP");
    w.slot_mut(1).unwrap().player.mp = cap.cost;
    w.slot_mut(1).unwrap().player.lifetime = 1e9;
    w.buy_part(1, Part::Cap);
    let p = player(&w, 1);
    assert!(p.owns_part(Part::Cap) && p.avatar.accessory == Part::Cap && p.mp == 0.0);
    assert!(texts(&w, 1).iter().any(|t| t.contains("já está usando")));

    w.slot_mut(1).unwrap().player.mp = 1e9;
    w.buy_part(1, Part::ZeroFace);
    assert!(!player(&w, 1).owns_part(Part::ZeroFace), "fruit faces need the DLC Cadeia");
    w.slot_mut(1).unwrap().player.items.insert(ItemId::PrisonDlc, 1);
    w.buy_part(1, Part::ZeroFace);
    assert_eq!(player(&w, 1).avatar.face, Part::ZeroFace);
}

#[test]
fn wearing_needs_the_part() {
    let now = Instant::now();
    let mut w = world(now);
    w.wear_part(1, Part::Crown);
    assert_eq!(player(&w, 1).avatar.accessory, Avatar::default().accessory);
    w.slot_mut(1).unwrap().player.avatar_parts.push(Part::Crown);
    w.wear_part(1, Part::Crown);
    assert_eq!(player(&w, 1).avatar.accessory, Part::Crown);
    let peer = w.view(2, now).unwrap().players.into_iter().find(|p| p.id == 1).unwrap();
    assert_eq!(peer.avatar.accessory, Part::Crown, "everyone sees the worn avatar");
}

#[test]
fn a_mog_makes_both_faces_react_for_everyone() {
    let now = Instant::now();
    let mut w = world(now);
    w.crit_override = Some(1.0);
    w.mog(1, 2, 1.0, now);
    let mood = |viewer: PlayerId, of: PlayerId| w.view(viewer, now).unwrap().players.into_iter().find(|p| p.id == of).unwrap().mood.map(|(m, _)| m);
    assert_eq!(mood(2, 1), Some(Mood::Smug));
    assert_eq!(mood(1, 2), Some(Mood::Mogged));
    assert_eq!(w.view(2, now).unwrap().mood.map(|(m, _)| m), Some(Mood::Mogged));
    let later = now + pmog::constants::avatar::MOOD_DURATION;
    assert_eq!(w.view(1, later).unwrap().mood, None, "the reaction wears off");
}
