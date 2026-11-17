//! Game rules as decided by the server (World), without any network.

use std::f64::consts::PI;
use std::time::{Duration, Instant};

use pmog::constants::*;
use pmog::game::{Player, Rank, TimingTier, crit_chance, meter_speed};
use pmog::items::{CATALOG, ItemId};
use pmog::protocol::{Fx, NPC_TARGET, PlayerId, ServerMsg};
use pmog::world::World;

/// World with players 1..=n named p1, p2, ...
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

fn give(w: &mut World, id: PlayerId, item: ItemId, level: u32) {
    w.slot_mut(id).unwrap().player.items.insert(item, level);
}

/// Instant where player `id`'s meter shows `m`.
fn at_meter(w: &World, id: PlayerId, m: f64) -> Instant {
    let slot = w.slot(id).unwrap();
    let speed = meter_speed(&slot.player.stats(), slot.streak);
    slot.meter_started + Duration::from_secs_f64((1.0 - 2.0 * m).acos() / speed)
}

fn perfect(w: &World, id: PlayerId) -> Instant {
    at_meter(w, id, 1.0)
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

fn index(id: ItemId) -> usize {
    CATALOG.iter().position(|d| d.id == id).unwrap()
}

#[test]
fn rank_thresholds() {
    assert_eq!(Rank::from_lifetime(0.0), Rank::Sub5);
    assert_eq!(Rank::from_lifetime(299.9), Rank::Sub5);
    assert_eq!(Rank::from_lifetime(300.0), Rank::Ltn);
    assert_eq!(Rank::from_lifetime(1_499_999.0), Rank::Ascended);
    assert_eq!(Rank::from_lifetime(13_400_000.0), Rank::SixSeven);
    assert_eq!(Rank::from_lifetime(1e12), Rank::Transcendent);
    assert_eq!(Rank::Ascended.next(), Some(Rank::Sigma));
    assert_eq!(Rank::Transcendent.next(), None);
    assert!(Rank::ALL.windows(2).all(|w| w[0] < w[1] && w[0].threshold() < w[1].threshold()), "ranks climb in order");
}

#[test]
fn item_costs_grow_and_consumables_are_flat() {
    let mewing = &CATALOG[index(ItemId::Mewing)];
    assert!(mewing.cost(1) > mewing.cost(0));
    let drink = &CATALOG[index(ItemId::EnergyDrink)];
    assert_eq!(drink.cost(0), drink.cost(10));
}

#[test]
fn join_rejects_a_second_session_and_leave_announces() {
    let now = Instant::now();
    let mut w = world(2, now);
    assert!(w.join(1, "machine-1".into(), Player::default(), now).is_err());
    assert_eq!(w.online_count(), 2);
    let left = w.leave(2, now).unwrap();
    assert_eq!(left.name, "p2");
    assert!(texts(&w, 1).iter().any(|t| t.contains("p2 saiu")));
    assert!(w.leave(2, now).is_none());
}

#[test]
fn buy_checks_rank_money_and_max_level() {
    let now = Instant::now();
    let mut w = world(1, now);
    w.slot_mut(1).unwrap().player.mp = 1e6;
    w.buy(1, ItemId::MidFade);
    assert_eq!(player(&w, 1).level(ItemId::MidFade), 0, "locked by rank");

    w.slot_mut(1).unwrap().player.mp = 10.0;
    w.buy(1, ItemId::Mewing);
    assert_eq!(player(&w, 1).level(ItemId::Mewing), 0, "not enough MP");

    w.slot_mut(1).unwrap().player.mp = 100.0;
    w.buy(1, ItemId::Mewing);
    assert_eq!(player(&w, 1).level(ItemId::Mewing), 1);
    assert_eq!(player(&w, 1).mp, 75.0);
    assert_eq!(player(&w, 1).purchases, 1);
    assert_eq!(w.purchases.len(), 1);
    assert_eq!(w.purchases[0].cost, 25.0);

    let slot = w.slot_mut(1).unwrap();
    slot.player.lifetime = Rank::Htn.threshold();
    slot.player.mp = 1e12;
    for _ in 0..10 {
        w.buy(1, ItemId::Sunglasses);
    }
    assert_eq!(player(&w, 1).level(ItemId::Sunglasses), CATALOG[index(ItemId::Sunglasses)].max_level);
}

#[test]
fn consumables_are_spent_only_when_they_do_something() {
    let now = Instant::now();
    let mut w = world(1, now);
    w.use_item(1, ItemId::EnergyDrink, now);
    assert!(texts(&w, 1).iter().any(|t| t.contains("Você não tem")));

    give(&mut w, 1, ItemId::EnergyDrink, 2);
    w.use_item(1, ItemId::EnergyDrink, now);
    assert_eq!(player(&w, 1).level(ItemId::EnergyDrink), 2, "not spent when already ready");
    w.mog(1, NPC_TARGET, 0.0, now);
    assert!(w.slot(1).unwrap().cooldown_until > now);
    w.use_item(1, ItemId::EnergyDrink, now);
    assert_eq!(w.slot(1).unwrap().cooldown_until, now);
    assert_eq!(player(&w, 1).level(ItemId::EnergyDrink), 1);

    give(&mut w, 1, ItemId::SigmaShield, 2);
    w.use_item(1, ItemId::SigmaShield, now);
    w.use_item(1, ItemId::SigmaShield, now);
    assert!(player(&w, 1).shield_armed);
    assert_eq!(player(&w, 1).level(ItemId::SigmaShield), 1, "second use while armed is free");

    give(&mut w, 1, ItemId::Mewing, 1);
    w.use_item(1, ItemId::Mewing, now);
    assert_eq!(player(&w, 1).level(ItemId::Mewing), 1, "permanent items can't be used");
}

#[test]
fn every_consumable_has_a_unique_key_and_works() {
    use pmog::items::consumable_for_key;
    let now = Instant::now();
    let mut w = world(1, now);
    let keys: Vec<char> = CATALOG.iter().filter_map(|d| d.usage.map(|(k, _)| k)).collect();
    let mut unique = keys.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(keys.len(), unique.len());
    w.mog(1, NPC_TARGET, 0.0, now);
    for k in keys {
        let id = consumable_for_key(k).unwrap();
        give(&mut w, 1, id, 1);
        w.use_item(1, id, now);
        assert_eq!(player(&w, 1).level(id), 0, "{id:?} was not consumed");
    }
    let p = player(&w, 1);
    assert!(p.shield_armed && p.angle_armed && p.mirror_armed);
    assert!(w.slot(1).unwrap().preworkout_until > now);
}

#[test]
fn mew_is_rate_limited_and_items_raise_it() {
    let now = Instant::now();
    let mut w = world(2, now);
    for _ in 0..50 {
        w.mew(1, now);
    }
    assert_eq!(player(&w, 1).mp, MEW_CAP_PER_SEC as f64);
    w.mew(1, now + Duration::from_secs(1));
    assert_eq!(player(&w, 1).mp, MEW_CAP_PER_SEC as f64 + 1.0, "window resets each second");

    give(&mut w, 2, ItemId::WaterBottle, 5);
    for _ in 0..50 {
        w.mew(2, now);
    }
    let mews = MEW_CAP_PER_SEC + 5;
    assert!((player(&w, 2).mp - mews as f64 * (1.0 + 0.25 * 5.0)).abs() < 1e-9);
}

#[test]
fn perfect_npc_mog_always_wins_and_streak_pays() {
    let now = Instant::now();
    let mut w = world(1, now);
    let base = NPC_REWARD_FLAT + BASE_ATTACK * NPC_REWARD_RATIO;
    let t = perfect(&w, 1);
    w.mog(1, NPC_TARGET, 1.0, t);
    assert!((player(&w, 1).mp - base * REWARD_PERFECT).abs() < 1e-9);
    assert_eq!(w.slot(1).unwrap().streak, 0, "the NPC builds no streak");
    assert_eq!(w.duels.len(), 1);
    assert_eq!(w.duels[0].outcome, "won");
    assert_eq!(w.duels[0].defender, None);

    // A streak brought from players multiplies the NPC reward without growing.
    w.slot_mut(1).unwrap().streak = 1;
    let speed = meter_speed(&player(&w, 1).stats(), 1);
    let period = 2.0 * PI / speed;
    let mut t2 = perfect(&w, 1);
    while t2 < w.slot(1).unwrap().cooldown_until {
        t2 += Duration::from_secs_f64(period);
    }
    let before = player(&w, 1).mp;
    w.mog(1, NPC_TARGET, 1.0, t2);
    let expected = base * REWARD_PERFECT * (1.0 + BASE_STREAK_BONUS);
    assert!((player(&w, 1).mp - before - expected).abs() < 1e-6, "streak multiplies NPC reward");
    assert_eq!(w.slot(1).unwrap().streak, 1);
}

#[test]
fn cooldown_blocks_spam() {
    let now = Instant::now();
    let mut w = world(1, now);
    w.mog(1, NPC_TARGET, 0.0, now);
    w.mog(1, NPC_TARGET, 0.0, now + Duration::from_millis(100));
    assert_eq!(w.duels.len(), 1);
    assert!(texts(&w, 1).iter().any(|t| t.contains("Cooldown")));
    w.mog(1, NPC_TARGET, 0.0, now + Duration::from_secs_f64(BASE_COOLDOWN_SECS));
    assert_eq!(w.duels.len(), 2);
}

#[test]
fn invalid_targets_are_refused() {
    let now = Instant::now();
    let mut w = world(2, now);
    w.mog(1, 1, 0.5, now);
    w.mog(1, 99, 0.5, now);
    w.mog(99, 1, 0.5, now);
    assert!(w.duels.is_empty());
    assert_eq!(w.slot(1).unwrap().cooldown_until, now, "refused mogs cost no cooldown");
}

#[test]
fn duel_attacker_wins_then_target_is_immune() {
    let now = Instant::now();
    let mut w = world(2, now);
    give(&mut w, 1, ItemId::DiamondJaw, 50);
    w.slot_mut(2).unwrap().player.mp = 1000.0;
    w.mog(1, 2, 0.0, now);
    let take = (STAKE_FLAT + 1000.0 * STAKE_RATIO) * REWARD_BAD;
    assert!((player(&w, 2).mp - (1000.0 - take)).abs() < 1e-9, "a BAD hit takes half the stake");
    assert!((player(&w, 1).mp - take).abs() < 1e-9, "the attacker gets exactly what was lost");
    assert_eq!(player(&w, 1).mogs_won, 1);
    assert_eq!(player(&w, 2).defends_lost, 1);
    assert!(texts(&w, 2).iter().any(|t| t.contains("p1 te moggou")));
    assert_eq!(w.duels[0].outcome, "won");
    assert_eq!(w.duels[0].defender, Some(2));

    w.slot_mut(1).unwrap().cooldown_until = now;
    let later = now + Duration::from_millis(10);
    let peer = w.view(1, later).unwrap().players.into_iter().find(|p| p.id == 2).unwrap();
    assert!(peer.immune && peer.immune_ms > 0, "the attacker sees how long the target is immune");
    w.mog(1, 2, 0.0, later);
    assert_eq!(w.duels.len(), 1, "mogging an immune target is refused");
    assert_eq!(w.slot(1).unwrap().cooldown_until, now, "and costs no cooldown");
    assert!(texts(&w, 1).iter().any(|t| t.contains("imune por mais")));
    assert_eq!(player(&w, 1).mogs_won, 1);
}

#[test]
fn duel_defender_holds_and_attacker_pays() {
    let now = Instant::now();
    let mut w = world(2, now);
    give(&mut w, 2, ItemId::DiamondJaw, 50);
    w.slot_mut(1).unwrap().player.mp = 100.0;
    w.mog(1, 2, 0.0, now);
    assert_eq!(player(&w, 2).defends_won, 1);
    assert_eq!(player(&w, 1).mogs_lost, 1);
    assert!((player(&w, 1).mp - (100.0 - 100.0 * LOSS_PENALTY_RATIO)).abs() < 1e-9);
    assert!(player(&w, 2).mp > 0.0, "counter bonus");
    assert_eq!(w.slot(1).unwrap().streak, 0);
}

#[test]
fn loss_never_goes_negative() {
    let now = Instant::now();
    let mut w = world(2, now);
    give(&mut w, 2, ItemId::DiamondJaw, 50);
    w.mog(1, 2, 0.0, now);
    assert_eq!(player(&w, 1).mp, 0.0);
    give(&mut w, 1, ItemId::DiamondJaw, 50);
    w.slot_mut(2).unwrap().player.items.clear();
    w.slot_mut(1).unwrap().cooldown_until = now;
    w.mog(1, 2, 0.0, now);
    assert!(player(&w, 2).mp >= 0.0);
}

#[test]
fn shield_blocks_one_mog() {
    let now = Instant::now();
    let mut w = world(2, now);
    give(&mut w, 1, ItemId::DiamondJaw, 50);
    w.slot_mut(2).unwrap().player.shield_armed = true;
    w.mog(1, 2, 0.0, now);
    assert_eq!(w.duels[0].outcome, "blocked");
    assert!(!player(&w, 2).shield_armed);
    w.slot_mut(1).unwrap().cooldown_until = now;
    w.mog(1, 2, 0.0, now);
    assert_eq!(w.duels[1].outcome, "won");
}

#[test]
fn angle_shot_doubles_power_once() {
    let now = Instant::now();
    let mut w = world(2, now);
    w.slot_mut(1).unwrap().player.angle_armed = true;
    w.mog(1, NPC_TARGET, 0.0, now);
    w.slot_mut(1).unwrap().cooldown_until = now;
    w.slot_mut(1).unwrap().streak = 0;
    w.mog(1, NPC_TARGET, 0.0, now);
    assert!(!player(&w, 1).angle_armed);
    assert!((w.duels[0].power - 2.0 * w.duels[1].power).abs() < 1e-9);
}

#[test]
fn client_meter_is_trusted_only_near_the_server_clock() {
    let now = Instant::now();
    let mut w = world(1, now);
    let low = at_meter(&w, 1, 0.0);
    w.mog(1, NPC_TARGET, 1.0, low);
    assert_eq!(w.duels[0].tier, TimingTier::Bad.label(), "a lying client gets the server reading");

    w.slot_mut(1).unwrap().streak = 0;
    let t = at_meter(&w, 1, PERFECT_THRESHOLD - METER_TOLERANCE / 2.0);
    w.slot_mut(1).unwrap().cooldown_until = t;
    w.mog(1, NPC_TARGET, PERFECT_THRESHOLD, t);
    assert_eq!(w.duels[1].tier, TimingTier::Perfect.label(), "small lag is forgiven");

    w.slot_mut(1).unwrap().cooldown_until = t;
    w.mog(1, NPC_TARGET, f64::NAN, t);
    assert_eq!(w.duels.len(), 3, "garbage meter falls back to the server");
}

#[test]
fn preworkout_halves_cooldown() {
    let now = Instant::now();
    let mut w = world(1, now);
    give(&mut w, 1, ItemId::PreWorkout, 1);
    w.use_item(1, ItemId::PreWorkout, now);
    w.mog(1, NPC_TARGET, 0.0, now);
    assert!((w.slot(1).unwrap().cooldown_total - BASE_COOLDOWN_SECS * PREWORKOUT_FACTOR).abs() < 1e-9);
    let later = now + PREWORKOUT_DURATION + Duration::from_secs(1);
    w.mog(1, NPC_TARGET, 0.0, later);
    assert!((w.slot(1).unwrap().cooldown_total - BASE_COOLDOWN_SECS).abs() < 1e-9);
}

#[test]
fn mirror_triples_one_defense() {
    let now = Instant::now();
    let mut w = world(2, now);
    give(&mut w, 1, ItemId::MidFade, 3); // attack 25 x 0.6 = 15 < 10 x 3 x 0.75
    w.slot_mut(2).unwrap().player.mirror_armed = true;
    w.mog(1, 2, 0.0, at_meter(&w, 1, 0.0));
    assert_eq!(player(&w, 2).defends_won, 1);
    assert!(!player(&w, 2).mirror_armed);
}

#[test]
fn therapy_and_watch_change_defense_outcomes() {
    let now = Instant::now();
    let mut w = world(2, now);
    give(&mut w, 1, ItemId::DiamondJaw, 50);
    give(&mut w, 2, ItemId::Therapy, 5);
    give(&mut w, 2, ItemId::LuxuryWatch, 2);
    w.slot_mut(2).unwrap().player.mp = 1000.0;
    w.mog(1, 2, 0.0, now);
    let take = (STAKE_FLAT + 1000.0 * STAKE_RATIO) * REWARD_BAD;
    assert!((player(&w, 2).mp - (1000.0 - take * 0.5)).abs() < 1e-9, "therapy halves the loss");
    assert!((player(&w, 1).mp - take * 0.5).abs() < 1e-9, "and the attacker only gets what was lost");
    assert!(w.slot(2).unwrap().immune_until >= now + IMMUNITY + Duration::from_secs(2));
}

#[test]
fn better_timing_pays_more_against_players() {
    let mut gains = Vec::new();
    for (m, tier) in [(0.2, TimingTier::Bad), (0.6, TimingTier::Ok), (0.8, TimingTier::Good), (0.99, TimingTier::Perfect)] {
        let now = Instant::now();
        let mut w = world(2, now);
        give(&mut w, 1, ItemId::DiamondJaw, 50);
        w.slot_mut(2).unwrap().player.mp = 1000.0;
        let t = at_meter(&w, 1, m);
        w.mog(1, 2, m, t);
        let stake = STAKE_FLAT + 1000.0 * STAKE_RATIO;
        assert!((player(&w, 1).mp - stake * tier.reward()).abs() < 1e-9, "{tier:?}");
        assert!(texts(&w, 1).iter().any(|l| l.contains(tier.label())), "feed shows the tier");
        gains.push(player(&w, 1).mp);
    }
    assert!(gains.windows(2).all(|w| w[0] < w[1]));
}

#[test]
fn win_bonus_applies() {
    let now = Instant::now();
    let mut w = world(1, now);
    give(&mut w, 1, ItemId::RingLight, 10);
    let t = perfect(&w, 1);
    w.mog(1, NPC_TARGET, 1.0, t);
    let base = NPC_REWARD_FLAT + BASE_ATTACK * NPC_REWARD_RATIO;
    assert!((player(&w, 1).mp - base * REWARD_PERFECT * 2.0).abs() < 1e-9);
}

#[test]
fn passive_income_rank_up_and_achievements_once() {
    let now = Instant::now();
    let mut w = world(1, now);
    give(&mut w, 1, ItemId::Mewing, 1);
    w.tick(now, 0.5);
    assert!((player(&w, 1).mp - 0.25).abs() < 1e-9);
    w.tick(now, 1e9);
    assert!((player(&w, 1).mp - 0.75).abs() < 1e-9, "huge gaps are clamped");
    w.tick(now, f64::NAN);
    assert!(player(&w, 1).mp.is_finite());

    w.slot_mut(1).unwrap().player.gain(Rank::Ltn.threshold());
    w.slot_mut(1).unwrap().player.mogs_won = 1;
    w.tick(now, 0.0);
    assert!(texts(&w, 1).iter().any(|t| t.contains("RANK UP")));
    let unlocked = player(&w, 1).achievements.len();
    assert!(unlocked > 0);
    w.outbox.clear();
    w.tick(now, 0.0);
    assert_eq!(player(&w, 1).achievements.len(), unlocked);
    assert!(texts(&w, 1).is_empty(), "nothing announced twice");
}

#[test]
fn rename_cleans_the_name_and_marks_for_saving() {
    let now = Instant::now();
    let mut w = world(2, now);
    w.take_dirty();
    w.rename(1, "\u{1b}[31mNovo\nNome que é longo demais para caber");
    let name = player(&w, 1).name.clone();
    assert!(!name.chars().any(char::is_control));
    assert!(name.chars().count() <= MAX_NAME_LEN);
    assert!(texts(&w, 2).iter().any(|t| t.contains("agora se chama")));
    let dirty = w.take_dirty();
    assert_eq!(dirty.len(), 1);
    assert_eq!(dirty[0].0, 1);
    w.rename(1, "   ");
    assert_eq!(player(&w, 1).name, name, "blank names are ignored");
}

#[test]
fn view_shows_others_but_not_me() {
    let now = Instant::now();
    let mut w = world(3, now);
    w.mog(1, NPC_TARGET, 0.0, now);
    let view = w.view(1, now).unwrap();
    assert_eq!(view.me.name, "p1");
    assert_eq!(view.players.iter().map(|p| p.id).collect::<Vec<_>>(), vec![2, 3]);
    assert!(view.cooldown_ms > 0 && view.cooldown_total_ms == (BASE_COOLDOWN_SECS * 1000.0) as u64);
    assert!(w.view(9, now).is_none());
}

#[test]
fn every_item_effect_changes_stats() {
    use pmog::items::Effect;
    for def in CATALOG.iter().filter(|d| !d.consumable()) {
        let p = Player::default();
        let (before, after) = (p.stats(), p.stats_after_buying(def.id));
        for e in def.effects {
            let changed = match e {
                Effect::Attack(_) => after.attack > before.attack,
                Effect::Defense(_) => after.defense > before.defense,
                Effect::Passive(_) => after.passive > before.passive,
                Effect::MewGain(_) => after.mew_gain > before.mew_gain,
                Effect::MewCap(_) => after.mew_cap > before.mew_cap,
                Effect::WinBonus(_) => after.win_bonus > before.win_bonus,
                Effect::LossShield(_) => after.loss_shield > before.loss_shield,
                Effect::PerfectZone(_) => after.perfect_threshold < before.perfect_threshold,
                Effect::CooldownCut(_) => after.cooldown < before.cooldown,
                Effect::StreakBonus(_) => after.streak_bonus > before.streak_bonus,
                Effect::ImmunityBonus(_) => after.immunity > before.immunity,
                Effect::CounterBonus(_) => after.counter_bonus > before.counter_bonus,
                Effect::MeterSlow(_) => after.meter_factor < before.meter_factor,
                Effect::Aura(_) => after.attack > before.attack && after.defense > before.defense,
            };
            assert!(changed, "{} effect {e:?} did nothing", def.name);
        }
    }
}

#[test]
fn stat_caps_hold_at_max_levels() {
    let mut p = Player::default();
    for def in &CATALOG {
        p.items.insert(def.id, def.max_level);
    }
    let s = p.stats();
    assert!(s.cooldown >= MIN_COOLDOWN_SECS);
    assert!(s.loss_shield <= MAX_LOSS_SHIELD);
    assert!(s.meter_factor >= MIN_METER_FACTOR);
    assert!(s.perfect_threshold > 0.5);
}

#[test]
fn timing_tiers_cover_the_bar() {
    assert_eq!(TimingTier::from_meter(0.0, PERFECT_THRESHOLD), TimingTier::Bad);
    assert_eq!(TimingTier::from_meter(TIMING_OK_FROM, PERFECT_THRESHOLD), TimingTier::Ok);
    assert_eq!(TimingTier::from_meter(TIMING_GOOD_FROM, PERFECT_THRESHOLD), TimingTier::Good);
    assert_eq!(TimingTier::from_meter(PERFECT_THRESHOLD, PERFECT_THRESHOLD), TimingTier::Perfect);
    let rewards: Vec<f64> = TimingTier::ALL.iter().map(|t| t.reward()).collect();
    assert!(rewards.windows(2).all(|w| w[0] < w[1]));
}

#[test]
fn mogs_between_players_create_no_mp() {
    let now = Instant::now();
    let mut w = world(2, now);
    give(&mut w, 1, ItemId::DiamondJaw, 50);
    w.slot_mut(2).unwrap().player.mp = 1000.0;
    let before = player(&w, 1).mp + player(&w, 2).mp;
    w.mog(1, 2, 1.0, at_meter(&w, 1, 1.0)); // PERFECT timing and a streak still only move MP
    let after = player(&w, 1).mp + player(&w, 2).mp;
    assert_eq!(player(&w, 1).mogs_won, 1);
    assert!((before - after).abs() < 1e-9, "{before} -> {after}");
}

#[test]
fn punching_far_down_pays_little_or_nothing() {
    let now = Instant::now();
    for (lifetime, expect_share, streak) in [(1e12, 0.0, 0), (Rank::ALL[2].threshold(), PUNCH_DOWN_FACTOR, 0), (0.0, 1.0, 1)] {
        let mut w = world(2, now);
        give(&mut w, 1, ItemId::DiamondJaw, 50);
        w.slot_mut(1).unwrap().player.lifetime = lifetime;
        w.slot_mut(2).unwrap().player.mp = 1000.0;
        w.mog(1, 2, 0.0, now);
        let full = (STAKE_FLAT + 1000.0 * STAKE_RATIO) * REWARD_BAD;
        assert!((1000.0 - player(&w, 2).mp - full * expect_share).abs() < 1e-9, "lifetime {lifetime}");
        assert_eq!(w.slot(1).unwrap().streak, streak, "no streak from punching down");
    }
}

#[test]
fn immunity_grows_with_each_mog_in_a_row_and_resets() {
    use pmog::world::escalated_immunity;
    assert_eq!(escalated_immunity(IMMUNITY, 1), IMMUNITY);
    assert_eq!(escalated_immunity(IMMUNITY, 2), IMMUNITY * 3);
    assert_eq!(escalated_immunity(IMMUNITY, 3), IMMUNITY * 9);
    assert_eq!(escalated_immunity(IMMUNITY, 9), IMMUNITY_MAX);

    let mut now = Instant::now();
    let mut w = world(2, now);
    give(&mut w, 1, ItemId::DiamondJaw, 50);
    w.slot_mut(2).unwrap().player.mp = 1000.0;
    let mut expect = Vec::new();
    for _ in 0..3 {
        w.mog(1, 2, 0.0, now);
        expect.push(w.slot(2).unwrap().immune_until - now);
        now = w.slot(2).unwrap().immune_until.max(w.slot(1).unwrap().cooldown_until);
    }
    assert_eq!(expect, vec![IMMUNITY, IMMUNITY * 3, IMMUNITY * 9]);
    now += IMMUNITY_RESET + Duration::from_secs(1);
    w.mog(1, 2, 0.0, now);
    assert_eq!(w.slot(2).unwrap().immune_until - now, IMMUNITY, "a quiet minute resets it");
}

#[test]
fn jaw_tape_saves_mp_and_streak_on_a_failed_mog() {
    let now = Instant::now();
    let mut w = world(2, now);
    give(&mut w, 2, ItemId::DiamondJaw, 50);
    give(&mut w, 1, ItemId::JawTape, 1);
    w.slot_mut(1).unwrap().player.mp = 100.0;
    w.slot_mut(1).unwrap().streak = 4;
    w.use_item(1, ItemId::JawTape, now);
    assert!(player(&w, 1).tape_armed);
    assert_eq!(player(&w, 1).level(ItemId::JawTape), 0, "used up");
    w.mog(1, 2, 0.0, now);
    assert_eq!(w.duels[0].outcome, "lost");
    assert_eq!(player(&w, 1).mp, 100.0, "no MP lost");
    assert_eq!(w.slot(1).unwrap().streak, 4, "streak kept");
    assert!(!player(&w, 1).tape_armed, "only once");
    assert!(texts(&w, 1).iter().any(|t| t.contains("Fita de Mandíbula salvou")));

    w.slot_mut(1).unwrap().cooldown_until = now;
    w.mog(1, 2, 0.0, now);
    assert!(player(&w, 1).mp < 100.0, "the next loss costs again");
    assert_eq!(w.slot(1).unwrap().streak, 0);
}

#[test]
fn cologne_raises_the_take_while_it_lasts() {
    let now = Instant::now();
    let mut w = world(2, now);
    give(&mut w, 1, ItemId::DiamondJaw, 50);
    give(&mut w, 1, ItemId::Cologne, 2);
    w.slot_mut(2).unwrap().player.mp = 1000.0;
    w.use_item(1, ItemId::Cologne, now);
    w.use_item(1, ItemId::Cologne, now);
    assert_eq!(player(&w, 1).level(ItemId::Cologne), 1, "can't stack while active");
    assert!(w.view(1, now).unwrap().cologne_ms > 59_000);
    w.mog(1, 2, 0.0, now);
    let take = (STAKE_FLAT + 1000.0 * STAKE_RATIO) * REWARD_BAD * COLOGNE_MULT;
    assert!((player(&w, 1).mp - take).abs() < 1e-9, "BAD hit takes 1.5x");
    assert!((player(&w, 2).mp - (1000.0 - take)).abs() < 1e-9, "still zero-sum");
    assert_eq!(w.view(1, now + COLOGNE_DURATION).unwrap().cologne_ms, 0);
}

#[test]
fn ice_pack_gives_immunity_and_resets_escalation() {
    let now = Instant::now();
    let mut w = world(2, now);
    give(&mut w, 2, ItemId::IcePack, 2);
    w.use_item(2, ItemId::IcePack, now);
    assert_eq!(w.slot(2).unwrap().immune_until, now + ICE_PACK_IMMUNITY);
    w.slot_mut(2).unwrap().immune_until = now + Duration::from_secs(60);
    w.use_item(2, ItemId::IcePack, now + Duration::from_secs(1));
    assert_eq!(player(&w, 2).level(ItemId::IcePack), 1, "refused while already immune for longer");
    w.mog(1, 2, 0.5, now + Duration::from_secs(2));
    assert!(w.duels.is_empty(), "immune players can't be mogged");
}

#[test]
fn perfect_zone_has_a_floor() {
    let mut p = Player::default();
    p.items.insert(ItemId::Sunglasses, 5);
    p.items.insert(ItemId::Photographer, 5);
    assert!((p.stats().perfect_threshold - MIN_PERFECT_THRESHOLD).abs() < 1e-9);
}

#[test]
fn every_rank_has_new_things_to_buy() {
    for rank in [Rank::Sub5, Rank::Ltn, Rank::Mtn, Rank::Htn, Rank::Chadlite, Rank::Chad, Rank::Gigachad, Rank::Ascended] {
        let wanted = if rank == Rank::Ascended { 2 } else { 3 };
        assert!(CATALOG.iter().filter(|d| d.min_rank == rank).count() >= wanted, "{rank:?}");
    }
    let mut keys: Vec<char> = CATALOG.iter().filter_map(|d| d.usage.map(|(k, _)| k)).collect();
    keys.sort();
    assert_eq!(keys, ('0'..='9').collect::<Vec<_>>(), "consumable keys are 0 to 9, no repeats");
}

#[test]
fn shown_odds_match_what_the_server_rolls() {
    use pmog::game::{Peer, beat_chance};
    let now = Instant::now();
    let mut w = world(2, now);
    give(&mut w, 1, ItemId::MidFade, 6);
    give(&mut w, 2, ItemId::GuaSha, 12);
    let (attack, defense) = (player(&w, 1).stats().attack, player(&w, 2).stats().defense);
    let threshold = player(&w, 1).stats().perfect_threshold;
    let power = attack * PERFECT_MULT;
    w.crit_override = None;
    let beat = beat_chance(power, defense, DEFENSE_ROLL);
    let expected = beat + (1.0 - beat) * crit_chance(attack, defense);
    assert!(expected > 0.1 && expected < 0.9, "pick stats with a real fight: {expected}");
    let trials = 3_000;
    let mut wins = 0;
    for _ in 0..trials {
        let t = perfect(&w, 1);
        w.slot_mut(1).unwrap().cooldown_until = t;
        w.slot_mut(1).unwrap().streak = 0;
        w.slot_mut(2).unwrap().immune_until = t;
        w.slot_mut(2).unwrap().player.mp = 1_000.0;
        w.slot_mut(1).unwrap().player.mp = 1_000.0;
        let before = w.duels.len();
        w.mog(1, 2, threshold.max(0.999), t);
        assert_eq!(w.duels.len(), before + 1);
        wins += (w.duels.last().unwrap().outcome == "won") as u32;
    }
    let rate = wins as f64 / trials as f64;
    assert!((rate - expected).abs() < 0.04, "server {rate:.3} vs shown {expected:.3}");

    let mut game = pmog::game::Game::new(player(&w, 1).clone(), now);
    let peer = Peer { id: 2, name: "p2".into(), mp: 0.0, lifetime: 0.0, attack: 0.0, defense, streak: 0, in_duel: false, immune_until: None, clan_tag: None };
    game.streak = 0;
    let (good, perfect_odds) = game.mog_odds(&peer);
    assert!((perfect_odds - expected).abs() < 1e-9);
    assert!(good < perfect_odds);
    assert_eq!(beat_chance(10.0, 100.0, DEFENSE_ROLL), 0.0);
    assert_eq!(beat_chance(1_000.0, 100.0, DEFENSE_ROLL), 1.0);
}

#[test]
fn failed_mog_says_why_with_numbers() {
    let now = Instant::now();
    let mut w = world(2, now);
    give(&mut w, 2, ItemId::DiamondJaw, 50);
    w.slot_mut(1).unwrap().player.mp = 100.0;
    w.mog(1, 2, 0.0, now);
    let line = texts(&w, 1).into_iter().find(|t| t.contains("falhou")).expect("failure explained");
    assert!(line.starts_with("Seu mog em p2 falhou: a defesa deu "), "{line}");
    assert!(line.contains("contra seu poder de") && line.contains("streak zerado"), "{line}");
}

#[test]
fn every_purchase_gets_a_result_message() {
    let now = Instant::now();
    let mut w = world(1, now);
    w.slot_mut(1).unwrap().player.mp = 40.0;
    w.buy(1, ItemId::GuaSha);
    w.buy(1, ItemId::GuaSha);
    let results: Vec<(bool, String)> = w
        .outbox
        .iter()
        .filter_map(|(to, m)| match m {
            ServerMsg::Purchase { item: ItemId::GuaSha, ok, text } if *to == 1 => Some((*ok, text.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(results.len(), 2);
    assert!(results[0].0 && results[0].1.contains("Comprou Gua Sha"));
    assert!(!results[1].0 && results[1].1.contains("Faltam"), "{:?}", results[1]);
}

#[test]
fn every_tab_has_items_and_every_item_one_tab() {
    use pmog::items::Category;
    let total: usize = Category::ALL.iter().map(|c| c.items().count()).sum();
    assert_eq!(total, CATALOG.len());
    for c in Category::ALL {
        assert!(c.items().count() >= 5, "{c:?} is too thin");
        assert!(c.items().all(|i| CATALOG[i].consumable() == (c == Category::Consumable)), "{c:?}");
    }
    let names: std::collections::HashSet<_> = CATALOG.iter().map(|d| d.name).collect();
    assert_eq!(names.len(), CATALOG.len(), "item names are unique");
}

#[test]
fn combo_six_seven_waits_for_a_mog_that_lands() {
    let now = Instant::now();
    let mut w = world(1, now);
    let slot = w.slot_mut(1).unwrap();
    slot.player.items.insert(ItemId::ComboSixSeven, 2);
    w.use_item(1, ItemId::ComboSixSeven, now);
    assert!(player(&w, 1).combo_armed);
    w.use_item(1, ItemId::ComboSixSeven, now);
    assert_eq!(player(&w, 1).level(ItemId::ComboSixSeven), 1, "can't arm twice");
    // Lands on the NPC eventually; the combo goes with the first hit only.
    let mut t = now;
    for _ in 0..200 {
        if !player(&w, 1).combo_armed {
            break;
        }
        t += Duration::from_secs(30);
        w.mog(1, NPC_TARGET, 1.0, t);
    }
    assert!(!player(&w, 1).combo_armed, "a hit eventually lands");
    assert!(w.outbox.iter().any(|(_, m)| matches!(m, ServerMsg::Event { text, .. } if text.contains("Combo 6-7 +67%"))));
}

#[test]
fn aura_farm_doubles_passive_for_a_while() {
    let now = Instant::now();
    let mut w = world(1, now);
    let slot = w.slot_mut(1).unwrap();
    slot.player.items.insert(ItemId::Mewing, 10);
    slot.player.items.insert(ItemId::AuraFarm, 1);
    let rate = player(&w, 1).stats().passive;
    let mp = |w: &World| player(w, 1).mp;
    let before = mp(&w);
    w.tick(now, 1.0);
    let normal = mp(&w) - before;
    w.use_item(1, ItemId::AuraFarm, now);
    let before = mp(&w);
    w.tick(now, 1.0);
    assert!(((mp(&w) - before) - 2.0 * normal).abs() < 1e-9 && normal > 0.0 && (normal - rate).abs() < 1e-9);
    assert!(w.view(1, now).unwrap().aura_farm_ms > 40_000);
    let later = now + AURA_FARM_DURATION;
    let before = mp(&w);
    w.tick(later, 1.0);
    assert!(((mp(&w) - before) - normal).abs() < 1e-9, "back to normal after 45 s");
}

#[test]
fn new_achievement_counters_add_up() {
    use pmog::game::Achievement;
    let now = Instant::now();
    let mut w = world(1, now);
    w.slot_mut(1).unwrap().player.mp = 67.4;
    w.tick(now, 0.0);
    let p = &w.slot(1).unwrap().player;
    assert!(p.achievements.contains(&Achievement::Exactly67));

    w.mew(1, now);
    w.slot_mut(1).unwrap().player.items.insert(ItemId::SigmaShield, 1);
    w.slot_mut(1).unwrap().player.items.insert(ItemId::JawTape, 1);
    w.slot_mut(1).unwrap().player.items.insert(ItemId::PocketMirror, 1);
    for item in [ItemId::SigmaShield, ItemId::JawTape, ItemId::PocketMirror] {
        w.use_item(1, item, now);
    }
    w.tick(now, 0.0);
    let p = &w.slot(1).unwrap().player;
    assert_eq!((p.mews, p.items_used), (1, 3));
    assert!(p.achievements.contains(&Achievement::Arsenal));
}

fn fxs(w: &World, id: PlayerId) -> Vec<Fx> {
    w.outbox.iter().filter(|(to, _)| *to == id).filter_map(|(_, m)| if let ServerMsg::Fx(f) = m { Some(*f) } else { None }).collect()
}

#[test]
fn shield_animates_both_sides_of_the_impact() {
    let now = Instant::now();
    let mut w = world(2, now);
    w.slot_mut(2).unwrap().player.shield_armed = true;
    w.mog(1, 2, 0.0, now);
    assert_eq!(fxs(&w, 2), [Fx::ShieldBlocked]);
    assert_eq!(fxs(&w, 1), [Fx::ShieldBounced]);
}

#[test]
fn angle_and_pre_workout_animate_the_press() {
    let now = Instant::now();
    let mut w = world(2, now);
    w.slot_mut(1).unwrap().player.angle_armed = true;
    w.slot_mut(1).unwrap().preworkout_until = now + Duration::from_secs(30);
    w.mog(1, 2, 0.0, now);
    assert_eq!(fxs(&w, 1), [Fx::AngleStrike, Fx::RushPress]);
    assert!(fxs(&w, 2).is_empty());
}

#[test]
fn mirror_animates_the_defender_and_matches_the_outcome() {
    let now = Instant::now();
    let mut w = world(2, now);
    give(&mut w, 1, ItemId::DiamondJaw, 50);
    w.slot_mut(2).unwrap().player.mirror_armed = true;
    w.mog(1, 2, 0.0, now);
    let expected = if w.duels[0].outcome == "won" { Fx::MirrorCracked } else { Fx::MirrorHeld };
    assert_eq!(fxs(&w, 2), [expected]);
}

#[test]
fn combo_and_cologne_animate_a_won_mog() {
    let now = Instant::now();
    let mut w = world(1, now);
    w.slot_mut(1).unwrap().player.combo_armed = true;
    w.slot_mut(1).unwrap().cologne_until = now + Duration::from_secs(60);
    for i in 1..=300 {
        let t = now + Duration::from_secs(30 * i);
        w.slot_mut(1).unwrap().cologne_until = t + Duration::from_secs(60);
        w.mog(1, NPC_TARGET, 1.0, t);
        if player(&w, 1).mogs_won > 0 {
            assert_eq!(fxs(&w, 1), [Fx::ComboHit, Fx::CologneHit]);
            return;
        }
    }
    panic!("never won against the NPC");
}

#[test]
fn tape_animates_a_failed_mog() {
    let now = Instant::now();
    let mut w = world(1, now);
    w.slot_mut(1).unwrap().player.tape_armed = true;
    for i in 1..=300 {
        w.slot_mut(1).unwrap().streak = 0;
        w.mog(1, NPC_TARGET, 0.0, now + Duration::from_secs(30 * i));
        if player(&w, 1).mogs_lost > 0 {
            assert_eq!(fxs(&w, 1), [Fx::TapeSaved]);
            return;
        }
    }
    panic!("never lost against the NPC");
}

#[test]
fn ice_pack_bounce_animates_both_but_plain_immunity_does_not() {
    let now = Instant::now();
    let mut w = world(2, now);
    give(&mut w, 2, ItemId::IcePack, 1);
    w.use_item(2, ItemId::IcePack, now);
    w.outbox.clear();
    w.mog(1, 2, 0.0, now);
    assert_eq!(fxs(&w, 1), [Fx::IceBounced]);
    assert_eq!(fxs(&w, 2), [Fx::IceBounced]);
    // Immunity earned by being hit is not ice.
    w.slot_mut(2).unwrap().ice_until = now;
    w.outbox.clear();
    w.mog(1, 2, 0.0, now);
    assert!(fxs(&w, 1).is_empty() && fxs(&w, 2).is_empty());
}

#[test]
fn critical_goes_through_any_defense_and_pays_more() {
    let now = Instant::now();
    let mut w = world(2, now);
    give(&mut w, 2, ItemId::DiamondJaw, 50);
    w.slot_mut(2).unwrap().player.mp = 1000.0;
    w.crit_override = Some(1.0);
    w.mog(1, 2, 0.0, now);
    let take = (STAKE_FLAT + 1000.0 * STAKE_RATIO) * REWARD_BAD * CRIT_TAKE_MULT;
    assert_eq!(w.duels[0].outcome, "won", "a weak attacker crits through a huge defense");
    assert!((player(&w, 1).mp - take).abs() < 1e-9);
    assert!(texts(&w, 1).iter().any(|t| t.contains("CRÍTICO")));
    assert!(texts(&w, 2).iter().any(|t| t.contains("com um CRÍTICO")));
}

#[test]
fn critical_chance_follows_attack_against_defense() {
    assert!((crit_chance(10.0, 10.0) - CRIT_CHANCE_SCALE / 2.0).abs() < 1e-12);
    assert_eq!(crit_chance(1.0, 1_000.0), CRIT_MIN_CHANCE, "never zero");
    assert!(crit_chance(40.0, 10.0) > crit_chance(10.0, 10.0));
    assert!(crit_chance(1e9, 1.0) <= CRIT_CHANCE_SCALE);
}

#[test]
fn breaking_a_streak_pays_a_bounty_and_ends_it() {
    let now = Instant::now();
    let mut w = world(2, now);
    give(&mut w, 1, ItemId::DiamondJaw, 50);
    w.slot_mut(2).unwrap().player.mp = 1000.0;
    w.slot_mut(2).unwrap().streak = 4;
    w.mog(1, 2, 0.0, now);
    let take = (STAKE_FLAT + 1000.0 * STAKE_RATIO) * REWARD_BAD * (1.0 + STREAK_BOUNTY * 4.0);
    assert!((player(&w, 1).mp - take).abs() < 1e-9, "bounty grows with the broken streak");
    assert_eq!(w.slot(2).unwrap().streak, 0, "being mogged ends the streak");
    assert!(texts(&w, 1).iter().any(|t| t.contains("quebrou o streak 4")));
    assert!(texts(&w, 2).iter().any(|t| t.contains("seu streak 4 acabou")));
}
