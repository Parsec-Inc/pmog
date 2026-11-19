//! Cadeia DLC: the soap on the forehead, the prison titles and the potions, as decided by the server.

use std::time::{Duration, Instant};

use pmog::constants::prison::*;
use pmog::dlc::prison::{self, Armed, PrisonTitle};
use pmog::dlc::{DlcId, DlcStatus};
use pmog::game::{Player, Status, meter_speed};
use pmog::items::{CATALOG, Category, ItemId};
use pmog::protocol::{PlayerId, ServerMsg};
use pmog::world::World;

/// World with players 1..=n named p1, p2, ..., criticals off.
fn world(n: i64, now: Instant) -> World {
    let mut w = World::new();
    w.crit_override = Some(0.0);
    for id in 1..=n {
        w.join(id, format!("machine-{id}"), Player { name: format!("p{id}"), ..Default::default() }, now).unwrap();
    }
    w.outbox.clear();
    w
}

fn player(w: &World, id: PlayerId) -> &Player {
    &w.slot(id).unwrap().player
}

fn cell(w: &World, id: PlayerId) -> &prison::Profile {
    &player(w, id).dlc.prison
}

fn give(w: &mut World, id: PlayerId, item: ItemId, level: u32) {
    w.slot_mut(id).unwrap().player.items.insert(item, level);
}

/// Instant where player `id`'s meter shows `m`.
fn at_meter(w: &World, id: PlayerId, m: f64) -> Instant {
    let slot = w.slot(id).unwrap();
    let speed = meter_speed(&slot.player.stats(), slot.streak);
    slot.meter_started + Duration::from_secs_f64((1.0 - 2.0 * m).acos() / speed)
}

fn texts(w: &World, id: PlayerId) -> Vec<String> {
    w.outbox
        .iter()
        .filter(|(to, _)| *to == id)
        .filter_map(|(_, m)| match m {
            ServerMsg::Event { text, banner, .. } => Some(format!("{text} {}", banner.clone().unwrap_or_default())),
            _ => None,
        })
        .collect()
}

/// Player 1 strong enough to always beat player 2.
fn strong_pair(now: Instant) -> World {
    let mut w = world(3, now);
    give(&mut w, 1, ItemId::DiamondJaw, 50);
    w.slot_mut(2).unwrap().player.mp = 1_000.0;
    w
}

#[test]
fn cadeia_items_need_the_dlc() {
    let now = Instant::now();
    let mut w = world(1, now);
    w.slot_mut(1).unwrap().player.mp = 1e6;
    w.slot_mut(1).unwrap().player.lifetime = 1e6;
    w.buy(1, ItemId::PullUpBar);
    assert_eq!(player(&w, 1).level(ItemId::PullUpBar), 0);
    assert!(texts(&w, 1).iter().any(|t| t.contains("compre a DLC primeiro")));
    w.buy(1, ItemId::PrisonDlc);
    assert!(player(&w, 1).owns_dlc(DlcId::Prison));
    w.buy(1, ItemId::PrisonDlc);
    assert_eq!(player(&w, 1).level(ItemId::PrisonDlc), 1, "bought once");
    w.buy(1, ItemId::PullUpBar);
    assert_eq!(player(&w, 1).level(ItemId::PullUpBar), 1);
    let prison: Vec<_> = CATALOG.iter().filter(|d| d.category() == Category::Dlc(DlcId::Prison)).collect();
    assert_eq!(prison.len(), 13, "the DLC, 7 permanent items and 5 consumables");
    assert_eq!(prison.iter().filter(|d| d.consumable()).count(), 5);
}

#[test]
fn a_perfect_mog_with_the_dlc_makes_the_target_pick_up_the_soap() {
    let now = Instant::now();
    let mut w = strong_pair(now);
    w.mog(1, 2, 1.0, at_meter(&w, 1, 1.0));
    assert_eq!((cell(&w, 1).marks, cell(&w, 2).forehead), (0, 0), "without the DLC a PERFECT is just a PERFECT");

    give(&mut w, 1, ItemId::PrisonDlc, 1);
    let later = now + Duration::from_secs(120);
    w.slot_mut(1).unwrap().cooldown_until = later;
    w.slot_mut(2).unwrap().immune_until = later;
    w.slot_mut(1).unwrap().meter_started = later;
    w.outbox.clear();
    w.mog(1, 2, 1.0, at_meter(&w, 1, 1.0));
    assert_eq!((cell(&w, 1).marks, cell(&w, 2).forehead), (1, 1));
    assert!(texts(&w, 2).iter().any(|t| t.contains("p1 te fez pegar o sabonete. Agora você tem 1 na testa")));
    assert!(texts(&w, 3).iter().any(|t| t.contains("p2 pegou o sabonete pro p1")), "the whole cell hears about it");
    let peer = w.view(3, at_meter(&w, 1, 1.0)).unwrap().players.into_iter().find(|p| p.id == 2).unwrap();
    assert_eq!(peer.dlc.prison.forehead, 1);
    assert!(peer.statuses.iter().any(|(st, _)| *st == Status::Dlc(DlcStatus::Prison(prison::Status::Sabonete))), "the number is fresh");
}

#[test]
fn the_soap_marks_any_hit_and_the_pill_and_black_potion_land_with_it() {
    let now = Instant::now();
    let mut w = strong_pair(now);
    give(&mut w, 1, ItemId::PrisonDlc, 1);
    for item in [ItemId::Soap, ItemId::CarrotPill, ItemId::BlackPotion] {
        give(&mut w, 1, item, 1);
        w.use_item(1, item, now);
    }
    assert_eq!(cell(&w, 1).armed, [Armed::Soap, Armed::Pill, Armed::BlackPotion]);
    w.mog(1, 2, 0.0, now);
    assert_eq!(cell(&w, 2).forehead, 1, "a BAD hit still makes them pick it up");
    assert!(cell(&w, 1).armed.is_empty(), "all spent on the hit");
    assert!(w.slot(2).unwrap().cooldown_until >= now + PILL_SLEEP, "the pill puts them to sleep");

    // Obeying: p2 can't mog p1 back, but can mog someone else.
    let awake = now + PILL_SLEEP + Duration::from_secs(1);
    w.slot_mut(1).unwrap().immune_until = awake;
    w.outbox.clear();
    w.mog(2, 1, 0.0, awake);
    assert!(texts(&w, 2).iter().any(|t| t.contains("sob a Poção Negra de p1")));
    assert!(w.slot(2).unwrap().cooldown_until <= awake, "refusing costs no cooldown");
    w.mog(2, 3, 0.0, awake);
    assert!(w.slot(2).unwrap().cooldown_until > awake, "everyone else is fair game");
}

#[test]
fn potions_raise_power_for_a_while_and_dont_stack() {
    let now = Instant::now();
    let mut w = world(1, now);
    give(&mut w, 1, ItemId::GreenPotion, 2);
    let base = w.slot(1).unwrap().atk_mult(now);
    w.use_item(1, ItemId::GreenPotion, now);
    w.use_item(1, ItemId::GreenPotion, now);
    assert_eq!(player(&w, 1).level(ItemId::GreenPotion), 1, "a second one while it lasts is not spent");
    assert!((w.slot(1).unwrap().atk_mult(now) - base * GREEN_POTION_MULT).abs() < 1e-9);
    assert!((w.slot(1).unwrap().atk_mult(now + GREEN_POTION_DURATION) - base).abs() < 1e-9, "and it wears off");

    give(&mut w, 1, ItemId::RedPotion, 1);
    w.use_item(1, ItemId::RedPotion, now);
    assert!((w.slot(1).unwrap().atk_mult(now) - base * GREEN_POTION_MULT * RED_POTION_MULT).abs() < 1e-9, "different potions stack");
}

#[test]
fn prison_titles_climb_with_marks_and_zero_needs_a_clean_forehead() {
    assert_eq!(PrisonTitle::of(0, 0), None);
    assert_eq!(PrisonTitle::of(1, 5), Some(PrisonTitle::X9));
    assert_eq!(PrisonTitle::of(40, 3), Some(PrisonTitle::Cenoura));
    assert_eq!(PrisonTitle::of(1_000, 1), Some(PrisonTitle::Mil));
    assert_eq!(PrisonTitle::of(ZERO_TITLE_MARKS, 0), Some(PrisonTitle::Zero));
    assert_eq!(PrisonTitle::of(ZERO_TITLE_MARKS, 1), Some(PrisonTitle::Cenoura), "one soap and you're not Lucas anymore");
    assert!(PrisonTitle::ALL.windows(2).all(|w| w[0] < w[1]));
}
