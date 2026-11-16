//! Shared game types (ranks, stats, achievements) and the client-side view.

use std::cell::Cell;
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::constants::*;
pub use crate::items::{CATALOG, Category, Effect, ItemDef, ItemId};
use crate::duel::{Board, DuelKey, DuelView, MacrosProgress, Minigame, Phase, TypingProgress, rhythm_fresh, rhythm_len, typing_stats};
use crate::notify::Notice;
use crate::protocol::{ClientMsg, InviteView, NPC_TARGET, PlayerId, ServerMsg, StateView};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Rank {
    Sub5,
    Ltn,
    Mtn,
    Htn,
    Chadlite,
    Chad,
    Gigachad,
    Ascended,
}

impl Rank {
    pub const ALL: [Rank; 8] = [
        Rank::Sub5,
        Rank::Ltn,
        Rank::Mtn,
        Rank::Htn,
        Rank::Chadlite,
        Rank::Chad,
        Rank::Gigachad,
        Rank::Ascended,
    ];

    /// Lifetime points needed to reach this rank.
    pub fn threshold(self) -> f64 {
        match self {
            Rank::Sub5 => 0.0,
            Rank::Ltn => 150.0,
            Rank::Mtn => 750.0,
            Rank::Htn => 3_000.0,
            Rank::Chadlite => 10_000.0,
            Rank::Chad => 30_000.0,
            Rank::Gigachad => 100_000.0,
            Rank::Ascended => 300_000.0,
        }
    }

    pub fn from_lifetime(lifetime: f64) -> Rank {
        Self::ALL
            .into_iter()
            .rev()
            .find(|r| lifetime >= r.threshold())
            .unwrap_or(Rank::Sub5)
    }

    pub fn next(self) -> Option<Rank> {
        Self::ALL.get(self as usize + 1).copied()
    }

    pub fn label(self) -> &'static str {
        match self {
            Rank::Sub5 => "Sub-5",
            Rank::Ltn => "LTN",
            Rank::Mtn => "MTN",
            Rank::Htn => "HTN",
            Rank::Chadlite => "Chadlite",
            Rank::Chad => "Chad",
            Rank::Gigachad => "Gigachad",
            Rank::Ascended => "Ascendido",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Achievement {
    FirstMog,
    TenMogs,
    HundredMogs,
    Streak5,
    Streak10,
    FirstPerfect,
    TwentyPerfects,
    PunchingBag,
    Wall,
    Shopper,
    ReachChad,
    Ascend,
    FirstDuel,
    Duelist,
}

impl Achievement {
    pub const ALL: [Achievement; 14] = [
        Achievement::FirstMog,
        Achievement::TenMogs,
        Achievement::HundredMogs,
        Achievement::Streak5,
        Achievement::Streak10,
        Achievement::FirstPerfect,
        Achievement::TwentyPerfects,
        Achievement::PunchingBag,
        Achievement::Wall,
        Achievement::Shopper,
        Achievement::ReachChad,
        Achievement::Ascend,
        Achievement::FirstDuel,
        Achievement::Duelist,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Achievement::FirstMog => "Primeiro Mog",
            Achievement::TenMogs => "Mogger em Série",
            Achievement::HundredMogs => "Máquina de Mog",
            Achievement::Streak5 => "Em Chamas",
            Achievement::Streak10 => "Imparável",
            Achievement::FirstPerfect => "Ângulo Perfeito",
            Achievement::TwentyPerfects => "Timing de Sniper",
            Achievement::PunchingBag => "Saco de Pancada",
            Achievement::Wall => "Muralha",
            Achievement::Shopper => "Looksmaxxer",
            Achievement::ReachChad => "Virou Chad",
            Achievement::Ascend => "Ascensão",
            Achievement::FirstDuel => "Primeiro Sangue",
            Achievement::Duelist => "Duelista",
        }
    }

    pub fn desc(self) -> &'static str {
        match self {
            Achievement::FirstMog => "Moggue alguém pela primeira vez",
            Achievement::TenMogs => "Vença 10 mogs",
            Achievement::HundredMogs => "Vença 100 mogs",
            Achievement::Streak5 => "Streak de 5",
            Achievement::Streak10 => "Streak de 10",
            Achievement::FirstPerfect => "Acerte um PERFECT",
            Achievement::TwentyPerfects => "Acerte 20 PERFECTs",
            Achievement::PunchingBag => "Seja moggado 10 vezes",
            Achievement::Wall => "Defenda 10 mogs",
            Achievement::Shopper => "Compre 10 itens",
            Achievement::ReachChad => "Chegue ao rank Chad",
            Achievement::Ascend => "Chegue ao rank Ascendido",
            Achievement::FirstDuel => "Vença um duelo PvP valendo MP",
            Achievement::Duelist => "Vença 10 duelos PvP valendo MP",
        }
    }

    pub fn reached(self, p: &Player) -> bool {
        match self {
            Achievement::FirstMog => p.mogs_won >= 1,
            Achievement::TenMogs => p.mogs_won >= 10,
            Achievement::HundredMogs => p.mogs_won >= 100,
            Achievement::Streak5 => p.best_streak >= 5,
            Achievement::Streak10 => p.best_streak >= 10,
            Achievement::FirstPerfect => p.perfects >= 1,
            Achievement::TwentyPerfects => p.perfects >= 20,
            Achievement::PunchingBag => p.defends_lost >= 10,
            Achievement::Wall => p.defends_won >= 10,
            Achievement::Shopper => p.purchases >= 10,
            Achievement::ReachChad => p.rank() >= Rank::Chad,
            Achievement::Ascend => p.rank() >= Rank::Ascended,
            Achievement::FirstDuel => p.duels_won >= 1,
            Achievement::Duelist => p.duels_won >= 10,
        }
    }
}

pub struct Stats {
    pub attack: f64,
    pub defense: f64,
    pub passive: f64,
    pub mew_gain: f64,
    pub mew_cap: u32,
    pub cooldown: f64,
    pub perfect_threshold: f64,
    pub streak_bonus: f64,
    pub win_bonus: f64,
    pub loss_shield: f64,
    pub immunity: Duration,
    pub counter_bonus: f64,
    /// Multiplier on the timing meter speed.
    pub meter_factor: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Player {
    pub name: String,
    pub mp: f64,
    pub lifetime: f64,
    pub items: HashMap<ItemId, u32>,
    pub mogs_won: u32,
    pub mogs_lost: u32,
    pub defends_won: u32,
    pub defends_lost: u32,
    pub best_streak: u32,
    pub perfects: u32,
    pub purchases: u32,
    pub duels_won: u32,
    pub duels_lost: u32,
    pub shield_armed: bool,
    pub angle_armed: bool,
    pub mirror_armed: bool,
    pub tape_armed: bool,
    /// Combo 6-7 waiting for the next mog that lands.
    pub combo_armed: bool,
    pub achievements: Vec<Achievement>,
}

impl Player {
    pub fn level(&self, id: ItemId) -> u32 {
        self.items.get(&id).copied().unwrap_or(0)
    }

    pub fn rank(&self) -> Rank {
        Rank::from_lifetime(self.lifetime)
    }

    /// Derives combat and economy stats from owned items.
    pub fn stats(&self) -> Stats {
        let mut s = Stats {
            attack: BASE_ATTACK,
            defense: BASE_DEFENSE,
            passive: 0.0,
            mew_gain: 1.0,
            mew_cap: MEW_CAP_PER_SEC,
            cooldown: BASE_COOLDOWN_SECS,
            perfect_threshold: PERFECT_THRESHOLD,
            streak_bonus: BASE_STREAK_BONUS,
            win_bonus: 0.0,
            loss_shield: 0.0,
            immunity: IMMUNITY,
            counter_bonus: COUNTER_BONUS,
            meter_factor: 1.0,
        };
        let mut aura = 1.0;
        for def in &CATALOG {
            let n = self.level(def.id).min(def.max_level);
            let k = n as f64;
            for effect in def.effects.iter().filter(|_| n > 0) {
                match *effect {
                    Effect::Attack(v) => s.attack += v * k,
                    Effect::Defense(v) => s.defense += v * k,
                    Effect::Passive(v) => s.passive += v * k,
                    Effect::MewGain(v) => s.mew_gain += v * k,
                    Effect::MewCap(v) => s.mew_cap += v * n,
                    Effect::WinBonus(v) => s.win_bonus += v * k,
                    Effect::LossShield(v) => s.loss_shield += v * k,
                    Effect::PerfectZone(v) => s.perfect_threshold -= v * k,
                    Effect::CooldownCut(v) => s.cooldown -= v * k,
                    Effect::StreakBonus(v) => s.streak_bonus += v * k,
                    Effect::ImmunityBonus(v) => s.immunity += Duration::from_secs_f64(v * k),
                    Effect::CounterBonus(v) => s.counter_bonus += v * k,
                    Effect::MeterSlow(v) => s.meter_factor -= v * k,
                    Effect::Aura(v) => aura += v * k,
                }
            }
        }
        s.attack *= aura;
        s.defense *= aura;
        s.passive *= aura;
        s.mew_gain *= aura;
        s.cooldown = s.cooldown.max(MIN_COOLDOWN_SECS);
        s.loss_shield = s.loss_shield.min(MAX_LOSS_SHIELD);
        s.meter_factor = s.meter_factor.max(MIN_METER_FACTOR);
        s.perfect_threshold = s.perfect_threshold.max(MIN_PERFECT_THRESHOLD);
        s
    }

    /// Stats as they would be with one more level of an item.
    pub fn stats_after_buying(&self, id: ItemId) -> Stats {
        let mut next = self.clone();
        *next.items.entry(id).or_insert(0) += 1;
        next.stats()
    }

    pub fn gain(&mut self, amount: f64) {
        self.mp += amount;
        self.lifetime += amount;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tone {
    Info,
    Good,
    Bad,
    Epic,
}

pub struct FeedLine {
    pub text: String,
    pub tone: Tone,
}

pub struct Banner {
    pub text: String,
    pub tone: Tone,
    pub until: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overlay {
    None,
    Chat,
    Clan,
    Shop,
    Achievements,
    Help,
    Server,
    /// Choosing how much to bet before a challenge or the queue.
    Stake,
}

/// How well the button press was timed; multiplies the MP reward.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimingTier {
    Bad,
    Ok,
    Good,
    Perfect,
}

impl TimingTier {
    pub const ALL: [TimingTier; 4] = [TimingTier::Bad, TimingTier::Ok, TimingTier::Good, TimingTier::Perfect];

    pub fn from_meter(meter: f64, perfect_threshold: f64) -> TimingTier {
        if meter >= perfect_threshold {
            TimingTier::Perfect
        } else if meter >= TIMING_GOOD_FROM {
            TimingTier::Good
        } else if meter >= TIMING_OK_FROM {
            TimingTier::Ok
        } else {
            TimingTier::Bad
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            TimingTier::Bad => "RUIM",
            TimingTier::Ok => "OK",
            TimingTier::Good => "BOM",
            TimingTier::Perfect => "PERFECT",
        }
    }

    pub fn reward(self) -> f64 {
        match self {
            TimingTier::Bad => REWARD_BAD,
            TimingTier::Ok => REWARD_OK,
            TimingTier::Good => REWARD_GOOD,
            TimingTier::Perfect => REWARD_PERFECT,
        }
    }
}

/// Meter speed in radians per second for these stats and streak.
pub fn meter_speed(stats: &Stats, streak: u32) -> f64 {
    METER_SPEED * stats.meter_factor * (1.0 + METER_STREAK_ACCEL * streak.min(STREAK_CAP) as f64)
}

/// Timing meter position in [0, 1] after `elapsed` seconds.
pub fn meter_value(elapsed: f64, stats: &Stats, streak: u32) -> f64 {
    (1.0 - (elapsed * meter_speed(stats, streak)).cos()) / 2.0
}

/// Odds that `power` beats a defense of `defense` times a uniform pick from `roll`.
pub fn beat_chance(power: f64, defense: f64, roll: std::ops::Range<f64>) -> f64 {
    if defense <= 0.0 {
        return 1.0;
    }
    ((power / defense - roll.start) / (roll.end - roll.start)).clamp(0.0, 1.0)
}

pub fn streak_mult(streak: u32, stats: &Stats) -> f64 {
    1.0 + streak.min(STREAK_CAP) as f64 * stats.streak_bonus
}

/// Someone listed on the left panel; id `NPC_TARGET` is the training bot.
pub struct Peer {
    pub id: PlayerId,
    pub name: String,
    pub mp: f64,
    pub lifetime: f64,
    pub attack: f64,
    pub defense: f64,
    pub streak: u32,
    pub in_duel: bool,
    /// When the player can be mogged again; `None` if not immune.
    pub immune_until: Option<Instant>,
    pub clan_tag: Option<String>,
}

impl Peer {
    pub fn npc() -> Peer {
        Peer { id: NPC_TARGET, name: NPC_NAME.into(), mp: 0.0, lifetime: 0.0, attack: 0.0, defense: 0.0, streak: 0, in_duel: false, immune_until: None, clan_tag: None }
    }

    pub fn is_npc(&self) -> bool {
        self.id == NPC_TARGET
    }

    /// Immunity time left, if any.
    pub fn immune_left(&self, now: Instant) -> Option<Duration> {
        self.immune_until.map(|t| t.saturating_duration_since(now)).filter(|d| !d.is_zero())
    }
}

/// What the client shows: a mirror of the server state plus local UI state.
pub struct Game {
    pub player: Player,
    pub peers: Vec<Peer>,
    pub selected: usize,
    pub streak: u32,
    pub cooldown_until: Instant,
    pub cooldown_total: f64,
    pub immune_until: Instant,
    pub hit_flash_until: Instant,
    pub preworkout_until: Instant,
    pub cologne_until: Instant,
    pub aura_farm_until: Instant,
    pub feed: VecDeque<FeedLine>,
    pub banner: Option<Banner>,
    pub overlay: Overlay,
    pub shop_cursor: usize,
    /// Shop item waiting for a yes/no before buying.
    pub shop_confirm: Option<usize>,
    /// Item bought and not yet answered by the server.
    pub shop_pending: Option<ItemId>,
    /// Last purchase outcome shown in the shop: (ok, text, when).
    pub shop_result: Option<(bool, String, Instant)>,
    pub help_page: usize,
    pub help_scroll: u16,
    /// Last scroll limit computed by the renderer.
    pub help_max_scroll: Cell<u16>,
    /// Start of the timing meter, aligned with the server clock.
    pub started: Instant,
    /// Actions waiting to be sent to the server.
    pub outbox: Vec<ClientMsg>,
    pub duel: Option<DuelView>,
    /// Local start of the current duel phase, aligned with the server clock.
    pub duel_phase_started: Instant,
    pub invites: Vec<InviteView>,
    pub outgoing: Option<InviteView>,
    pub queued: Option<f64>,
    /// Selected bet in the stake picker, as an index of `STAKE_FRACTIONS`.
    pub stake_cursor: usize,
    /// Who the stake picker challenges; `None` means the queue.
    pub stake_target: Option<PlayerId>,
    /// Last measured round trip to the server.
    pub rtt: Duration,
    pub chat: crate::chat::ChatClient,
    pub clan: crate::clan::ClanClient,
    /// Own clan tag, shown next to the name.
    pub clan_tag: Option<String>,
    /// Events worth a desktop notification, drained by the app.
    pub notices: Vec<Notice>,
    /// Typing battle keys already applied here: (round, typed, errors), ahead of the server's echo.
    typing_local: Option<(u32, usize, u32)>,
    /// Macros answers already checked here: (round, solved, errors), ahead of the server's echo.
    macros_local: Option<(u32, usize, u32)>,
    /// Number being typed for the current macros problem.
    pub macros_input: String,
    /// Highlighted option on the draft screen: an index of `Minigame::ALL`, or its length for a random pick.
    pub draft_cursor: usize,
    /// Corrects duel timing for network lag; off only to compare against the old behaviour.
    pub compensate: bool,
    /// Timed buffs active at the last check, to announce when they wear off.
    timed_buffs: Vec<Buff>,
    logged: usize,
}

/// An edge from a consumable that is active right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Buff {
    Shield,
    Angle,
    Mirror,
    Tape,
    PreWorkout,
    Cologne,
    Combo,
    AuraFarm,
}

impl Buff {
    pub fn label(self) -> &'static str {
        match self {
            Buff::Shield => "ESCUDO",
            Buff::Angle => "ÂNGULO",
            Buff::Mirror => "ESPELHO",
            Buff::Tape => "FITA",
            Buff::PreWorkout => "PRÉ-TREINO",
            Buff::Cologne => "COLÔNIA",
            Buff::Combo => "COMBO 6-7",
            Buff::AuraFarm => "FARMANDO AURA",
        }
    }

    pub fn effect(self) -> &'static str {
        match self {
            Buff::Shield => "perda de mog bloqueada",
            Buff::Angle => "poder x2",
            Buff::Mirror => "defesa x3",
            Buff::Tape => "mog errado não custa",
            Buff::PreWorkout => "cooldown ÷2",
            Buff::Cologne => "+50% MP",
            Buff::Combo => "+67% MP no próximo mog",
            Buff::AuraFarm => "passivo x2",
        }
    }

    pub fn item_name(self) -> &'static str {
        let id = match self {
            Buff::Shield => ItemId::SigmaShield,
            Buff::Angle => ItemId::AngleShot,
            Buff::Mirror => ItemId::PocketMirror,
            Buff::Tape => ItemId::JawTape,
            Buff::PreWorkout => ItemId::PreWorkout,
            Buff::Cologne => ItemId::Cologne,
            Buff::Combo => ItemId::ComboSixSeven,
            Buff::AuraFarm => ItemId::AuraFarm,
        };
        crate::items::def(id).name
    }
}

impl Game {
    pub fn new(player: Player, now: Instant) -> Game {
        Game {
            player,
            peers: vec![Peer::npc()],
            selected: 0,
            streak: 0,
            cooldown_until: now,
            cooldown_total: 0.0,
            immune_until: now,
            hit_flash_until: now,
            preworkout_until: now,
            cologne_until: now,
            aura_farm_until: now,
            feed: VecDeque::new(),
            banner: None,
            overlay: Overlay::None,
            shop_cursor: 0,
            shop_confirm: None,
            shop_pending: None,
            shop_result: None,
            help_page: 0,
            help_scroll: 0,
            help_max_scroll: Cell::new(0),
            started: now,
            outbox: Vec::new(),
            duel: None,
            duel_phase_started: now,
            invites: Vec::new(),
            outgoing: None,
            queued: None,
            stake_cursor: 1,
            stake_target: None,
            rtt: Duration::ZERO,
            chat: Default::default(),
            clan: Default::default(),
            clan_tag: None,
            notices: Vec::new(),
            typing_local: None,
            macros_local: None,
            macros_input: String::new(),
            draft_cursor: 0,
            compensate: true,
            timed_buffs: Vec::new(),
            logged: 0,
        }
    }

    pub fn log(&mut self, tone: Tone, text: impl Into<String>) {
        self.feed.push_front(FeedLine { text: text.into(), tone });
        self.feed.truncate(FEED_LEN);
        self.logged += 1;
    }

    /// Active buffs and, for timed ones, how long they still last.
    pub fn buffs(&self, now: Instant) -> Vec<(Buff, Option<Duration>)> {
        let p = &self.player;
        let left = |until: Instant| (now < until).then(|| until - now);
        let armed = [(p.shield_armed, Buff::Shield), (p.angle_armed, Buff::Angle), (p.mirror_armed, Buff::Mirror), (p.tape_armed, Buff::Tape), (p.combo_armed, Buff::Combo)];
        let timed = [(Buff::PreWorkout, left(self.preworkout_until)), (Buff::Cologne, left(self.cologne_until)), (Buff::AuraFarm, left(self.aura_farm_until))];
        armed
            .into_iter()
            .filter_map(|(on, b)| on.then_some((b, None)))
            .chain(timed.into_iter().filter_map(|(b, l)| l.map(|l| (b, Some(l)))))
            .collect()
    }

    /// Logs timed buffs that just wore off.
    pub fn note_expired_buffs(&mut self, now: Instant) {
        let active: Vec<Buff> = self.buffs(now).into_iter().filter_map(|(b, left)| left.map(|_| b)).collect();
        for gone in std::mem::replace(&mut self.timed_buffs, active.clone()).into_iter().filter(|b| !active.contains(b)) {
            self.log(Tone::Info, format!("O efeito de {} acabou ({}).", gone.item_name(), gone.effect()));
        }
    }

    /// Number of feed lines ever logged.
    pub fn total_logged(&self) -> usize {
        self.logged
    }

    pub fn flash(&mut self, now: Instant, tone: Tone, text: impl Into<String>) {
        self.banner = Some(Banner { text: text.into(), tone, until: now + BANNER_DURATION });
    }

    /// Chance of mogging `peer` with a mid GOOD hit and with a PERFECT one; shields and mirrors are not visible.
    pub fn mog_odds(&self, peer: &Peer) -> (f64, f64) {
        let stats = self.player.stats();
        let angle = if self.player.angle_armed { 2.0 } else { 1.0 };
        let base = stats.attack * streak_mult(self.streak, &stats) * angle;
        let good = TIMING_FLOOR + TIMING_SPAN * (TIMING_GOOD_FROM + stats.perfect_threshold) / 2.0;
        let (defense, roll) = if peer.is_npc() { (stats.attack, NPC_DEFENSE_ROLL) } else { (peer.defense, DEFENSE_ROLL) };
        (beat_chance(base * good, defense, roll.clone()), beat_chance(base * PERFECT_MULT, defense, roll))
    }

    pub fn meter(&self, now: Instant) -> f64 {
        let elapsed = now.saturating_duration_since(self.started).as_secs_f64();
        meter_value(elapsed, &self.player.stats(), self.streak)
    }

    pub fn streak_mult(&self, stats: &Stats) -> f64 {
        streak_mult(self.streak, stats)
    }

    pub fn is_ready(&self, now: Instant) -> bool {
        now >= self.cooldown_until
    }

    pub fn select_move(&mut self, delta: isize) {
        let len = self.peers.len() as isize;
        self.selected = (self.selected as isize + delta).rem_euclid(len) as usize;
    }

    pub fn help_turn(&mut self, delta: isize) {
        let len = crate::help::PAGES.len() as isize;
        self.help_page = (self.help_page as isize + delta).rem_euclid(len) as usize;
        self.help_scroll = 0;
    }

    pub fn help_scroll_by(&mut self, delta: i32) {
        let next = (self.help_scroll as i32 + delta).max(0) as u16;
        self.help_scroll = next.min(self.help_max_scroll.get());
    }

    /// Moves to the previous or next item of the same shop tab.
    pub fn shop_move(&mut self, delta: isize) {
        let items: Vec<usize> = self.shop_category().items().collect();
        let at = items.iter().position(|i| *i == self.shop_cursor).unwrap_or(0) as isize;
        self.shop_cursor = items[(at + delta).rem_euclid(items.len() as isize) as usize];
    }

    /// Switches to the first item of the previous or next shop tab.
    pub fn shop_tab(&mut self, delta: isize) {
        let tabs = Category::ALL;
        let at = tabs.iter().position(|c| *c == self.shop_category()).unwrap_or(0) as isize;
        let next = tabs[(at + delta).rem_euclid(tabs.len() as isize) as usize];
        self.shop_cursor = next.items().next().unwrap_or(0);
    }

    /// Tab of the item under the shop cursor.
    pub fn shop_category(&self) -> Category {
        CATALOG[self.shop_cursor.min(CATALOG.len() - 1)].category()
    }

    /// Presses MOGAR; the server decides the outcome.
    pub fn try_mog(&mut self, now: Instant) {
        if !self.is_ready(now) {
            self.flash(now, Tone::Info, "Cooldown! Respira.");
            return;
        }
        let Some(target) = self.peers.get(self.selected) else { return };
        if let Some(left) = target.immune_left(now) {
            let text = format!("{} se recupera em {}", target.name, crate::utils::format_wait(left));
            self.flash(now, Tone::Info, text);
            return;
        }
        self.outbox.push(ClientMsg::Mog { target: target.id, meter: self.meter(now) });
        // Local guess until the server confirms, so the key can't be spammed.
        let cooldown = self.player.stats().cooldown;
        self.cooldown_until = now + Duration::from_secs_f64(cooldown);
        self.cooldown_total = cooldown;
    }

    pub fn mew(&mut self) {
        self.outbox.push(ClientMsg::Mew);
    }

    /// Enter in the shop: opens the confirmation card, or buys if it is already open.
    pub fn shop_enter(&mut self) {
        self.shop_result = None;
        match self.shop_confirm.take() {
            Some(index) => {
                let blocked = CATALOG.get(index).is_none_or(|def| def.purchase_block(&self.player).is_some());
                if !blocked && self.shop_pending.is_none() {
                    self.shop_pending = Some(CATALOG[index].id);
                    self.buy(index);
                }
            }
            None => self.shop_confirm = Some(self.shop_cursor),
        }
    }

    /// Esc in the shop: closes the card first, then the shop.
    pub fn shop_back(&mut self) {
        if self.shop_confirm.take().is_none() && self.shop_result.take().is_none() {
            self.overlay = Overlay::None;
        }
    }

    pub fn buy(&mut self, index: usize) {
        if let Some(def) = CATALOG.get(index) {
            self.outbox.push(ClientMsg::Buy { item: def.id });
        }
    }

    pub fn use_item(&mut self, item: ItemId) {
        if self.player.level(item) == 0 {
            let name = crate::items::def(item).name;
            self.log(Tone::Bad, format!("Você não tem {name}. Compre na loja [L]."));
            return;
        }
        self.outbox.push(ClientMsg::Use { item });
    }

    /// Opens the bet picker to challenge the selected player.
    pub fn open_challenge(&mut self) {
        match self.peers.get(self.selected) {
            Some(peer) if peer.is_npc() => self.log(Tone::Info, "Escolha um jogador de verdade para duelar (↑↓)."),
            Some(peer) if peer.in_duel => self.log(Tone::Info, format!("{} já está num duelo.", peer.name)),
            Some(peer) => {
                self.stake_target = Some(peer.id);
                self.overlay = Overlay::Stake;
            }
            None => {}
        }
    }

    /// Leaves the queue, or opens the bet picker to join it.
    pub fn toggle_queue(&mut self) {
        if self.queued.is_some() {
            self.outbox.push(ClientMsg::Queue { fraction: None });
        } else {
            self.stake_target = None;
            self.overlay = Overlay::Stake;
        }
    }

    pub fn stake_move(&mut self, delta: isize) {
        let len = STAKE_FRACTIONS.len() as isize;
        self.stake_cursor = (self.stake_cursor as isize + delta).clamp(0, len - 1) as usize;
    }

    /// Sends the challenge or joins the queue with the chosen bet.
    pub fn confirm_stake(&mut self) {
        let fraction = STAKE_FRACTIONS[self.stake_cursor.min(STAKE_FRACTIONS.len() - 1)];
        self.outbox.push(match self.stake_target {
            Some(target) => ClientMsg::Challenge { target, fraction },
            None => ClientMsg::Queue { fraction: Some(fraction) },
        });
        self.overlay = Overlay::None;
    }

    /// Accepts or declines the oldest invitation.
    pub fn answer_invite(&mut self, accept: bool) {
        if let Some(invite) = self.invites.first() {
            self.outbox.push(ClientMsg::Answer { from: invite.peer, accept });
            self.invites.remove(0);
        }
    }

    /// Sends a duel key stamped with when it was pressed, so network lag does not count against the player.
    pub fn duel_key(&mut self, key: DuelKey, now: Instant) {
        self.predict_duel(now);
        self.type_locally(key, now);
        self.answer_locally(key);
        let at_ms = self.compensate.then(|| self.duel_elapsed(now) * 1000.0);
        self.outbox.push(ClientMsg::Duel { key, at_ms });
    }

    /// Applies a typing key on screen right away; the server's count catches up a moment later.
    fn type_locally(&mut self, key: DuelKey, now: Instant) {
        let (DuelKey::Char(c), Some(mine), Some(d)) = (key, self.typing_mine(now), self.duel.as_ref()) else { return };
        let Board::Typing { text, .. } = &d.board else { return };
        if d.phase != Phase::Play || mine.done {
            return;
        }
        let right = text.chars().nth(mine.typed).is_some_and(|want| want.eq_ignore_ascii_case(&c));
        let (typed, errors) = if right { (mine.typed + 1, mine.errors) } else { (mine.typed, mine.errors + 1) };
        self.typing_local = Some((d.round, typed, errors));
    }

    /// Checks a macros answer on screen right away; the server's count catches up a moment later.
    fn answer_locally(&mut self, key: DuelKey) {
        let (DuelKey::Answer(value), Some(mine), Some(d)) = (key, self.macros_mine(), self.duel.as_ref()) else { return };
        let Board::Macros { problems, .. } = &d.board else { return };
        if d.phase != Phase::Play || mine.done {
            return;
        }
        let right = problems.get(mine.solved).is_some_and(|p| p.answer() == value);
        let (solved, errors) = if right { (mine.solved + 1, mine.errors) } else { (mine.solved, mine.errors + 1) };
        self.macros_local = Some((d.round, solved, errors));
    }

    /// My side of a macros round, counting answers the server has not confirmed yet.
    pub fn macros_mine(&self) -> Option<MacrosProgress> {
        let d = self.duel.as_ref()?;
        let Board::Macros { problems, mine, .. } = &d.board else { return None };
        let (solved, errors) = match self.macros_local {
            Some((round, solved, errors)) if round == d.round => (solved.max(mine.solved), errors.max(mine.errors)),
            _ => (mine.solved, mine.errors),
        };
        Some(MacrosProgress { solved, errors, done: solved >= problems.len() })
    }

    /// Moves the draft cursor by `dx` columns and `dy` rows of the grid.
    pub fn move_draft_cursor(&mut self, dx: i32, dy: i32) {
        let options = Minigame::ALL.len() as i32 + 1;
        let next = self.draft_cursor as i32 + dx + dy * DRAFT_COLUMNS as i32;
        self.draft_cursor = next.clamp(0, options - 1) as usize;
    }

    /// Picks the minigame under the draft cursor, or asks for a random one.
    pub fn pick_minigame(&mut self) {
        let choice = Minigame::ALL.get(self.draft_cursor).copied();
        self.outbox.push(ClientMsg::Duel { key: DuelKey::Pick(choice), at_ms: None });
    }

    /// Sends the typed macros answer, if it is a number.
    pub fn submit_macros(&mut self, now: Instant) {
        let input = std::mem::take(&mut self.macros_input);
        if let Ok(value) = input.trim().parse::<i64>() {
            self.duel_key(DuelKey::Answer(value), now);
        }
    }

    /// My side of a typing battle, counting keys the server has not confirmed yet.
    pub fn typing_mine(&self, now: Instant) -> Option<TypingProgress> {
        let d = self.duel.as_ref()?;
        let Board::Typing { text, mine, .. } = &d.board else { return None };
        let (typed, errors) = match self.typing_local {
            Some((round, typed, errors)) if round == d.round => (typed.max(mine.typed), errors.max(mine.errors)),
            _ => (mine.typed, mine.errors),
        };
        if mine.done {
            return Some(*mine);
        }
        let ms = if d.phase == Phase::Play { self.duel_elapsed(now) * 1000.0 } else { 0.0 };
        let (wpm, accuracy) = typing_stats(typed, errors, ms);
        Some(TypingProgress { typed, errors, done: typed >= text.chars().count(), wpm, accuracy })
    }

    /// Starts play locally once the countdown or memorising time is up, so lag does not delay the first key.
    pub fn predict_duel(&mut self, now: Instant) {
        let Some(d) = self.duel.as_mut() else { return };
        let len = Duration::from_millis(d.phase_len_ms);
        if !self.compensate || now.saturating_duration_since(self.duel_phase_started) < len {
            return;
        }
        let play_len = match (d.phase, d.game, &mut d.board) {
            (Phase::Countdown, Minigame::Mewing, board) => {
                *board = Board::Mewing { mine: 0, theirs: 0 };
                MEWING_DURATION
            }
            (Phase::Show, game @ (Minigame::Skincare | Minigame::Memory), Board::Skincare { sequence, .. }) => {
                let steps = sequence.len();
                sequence.clear();
                crate::duel::routine_input_len(game == Minigame::Memory, steps)
            }
            (Phase::Show, Minigame::Typing, Board::Typing { .. }) => TYPING_LIMIT,
            (Phase::Show, Minigame::Macros, Board::Macros { .. }) => MACROS_LIMIT,
            (Phase::Countdown, Minigame::Rhythm, board) => {
                *board = Board::Rhythm { mine: rhythm_fresh(), theirs: rhythm_fresh() };
                rhythm_len()
            }
            _ => return,
        };
        d.phase = Phase::Play;
        d.seq += 1;
        d.phase_ms = 0;
        d.phase_len_ms = play_len.as_millis() as u64;
        self.duel_phase_started += len;
    }

    /// Whether the staredown GO should show now, timed on this client's own clock.
    pub fn staredown_go(&self, now: Instant) -> bool {
        match self.duel.as_ref().map(|d| (&d.board, d.phase)) {
            Some((Board::Staredown { go, go_ms, .. }, Phase::Play)) => {
                if self.compensate { self.duel_elapsed(now) * 1000.0 >= *go_ms as f64 } else { *go }
            }
            _ => false,
        }
    }

    /// How long the UI may sleep before something time-critical must be drawn.
    pub fn next_wake(&self, now: Instant) -> Option<Duration> {
        match self.duel.as_ref().map(|d| (&d.board, d.phase)) {
            Some((Board::Staredown { go_ms, i_pressed: false, .. }, Phase::Play)) if !self.staredown_go(now) => {
                let left = *go_ms as f64 / 1000.0 - self.duel_elapsed(now);
                Some(Duration::from_secs_f64(left.max(0.0)))
            }
            _ => None,
        }
    }

    /// Seconds since the current duel phase started.
    pub fn duel_elapsed(&self, now: Instant) -> f64 {
        now.saturating_duration_since(self.duel_phase_started).as_secs_f64()
    }

    /// Applies a message from the server.
    pub fn apply(&mut self, msg: ServerMsg, now: Instant) {
        match msg {
            ServerMsg::State(view) => self.apply_state(*view, now),
            ServerMsg::Event { tone, text, banner, hit } => {
                self.notices.extend(Notice::from_event(&text, banner.as_deref(), hit));
                if !text.is_empty() {
                    self.log(tone, text);
                }
                if let Some(b) = banner {
                    self.flash(now, tone, b);
                }
                if hit {
                    self.hit_flash_until = now + HIT_FLASH;
                }
            }
            ServerMsg::Purchase { item, ok, text } => {
                if self.shop_pending == Some(item) {
                    self.shop_pending = None;
                }
                self.shop_result = Some((ok, text, now));
            }
            ServerMsg::Chat(event) => {
                self.chat.apply(event, now, &mut self.notices);
                self.outbox.extend(self.chat.take_pending().into_iter().map(ClientMsg::Chat));
            }
            ServerMsg::Clan(event) => self.clan.apply(event, now, &mut self.notices),
            ServerMsg::Welcome { .. } | ServerMsg::Rejected { .. } | ServerMsg::Shutdown | ServerMsg::Ping { .. } => {}
        }
        crate::notify::bound(&mut self.notices);
    }

    fn apply_state(&mut self, view: StateView, now: Instant) {
        let ms = Duration::from_millis;
        self.player = view.me;
        self.streak = view.streak;
        self.cooldown_until = now + ms(view.cooldown_ms);
        self.cooldown_total = view.cooldown_total_ms as f64 / 1000.0;
        self.immune_until = now + ms(view.immune_ms);
        self.preworkout_until = now + ms(view.preworkout_ms);
        self.cologne_until = now + ms(view.cologne_ms);
        self.aura_farm_until = now + ms(view.aura_farm_ms);
        self.rtt = Duration::from_secs_f64(view.rtt_ms.max(0.0) / 1000.0);
        // The state left the server half a round trip ago.
        let travel = if self.compensate { self.rtt / 2 } else { Duration::ZERO };
        self.started = now.checked_sub(ms(view.meter_ms) + travel).unwrap_or(now);
        if let Some(duel) = &view.duel {
            let start = now.checked_sub(ms(duel.phase_ms) + travel).unwrap_or(now);
            // Within a phase, the earliest estimate came from the least delayed packet.
            let same = self.duel.as_ref().is_some_and(|d| d.seq == duel.seq);
            self.duel_phase_started = if same && self.compensate { start.min(self.duel_phase_started) } else { start };
            if self.duel.is_none() {
                self.overlay = Overlay::None;
                self.draft_cursor = 0;
            }
        }
        self.duel = view.duel;
        // Local typing only lives as long as the typing board it was typed on.
        if !self.duel.as_ref().is_some_and(|d| matches!(d.board, Board::Typing { .. })) {
            self.typing_local = None;
        }
        if !self.duel.as_ref().is_some_and(|d| matches!(d.board, Board::Macros { .. })) {
            self.macros_local = None;
            self.macros_input.clear();
        }
        for invite in view.invites.iter().filter(|new| !self.invites.iter().any(|old| old.peer == new.peer)) {
            self.notices.push(Notice::invite(&invite.name, invite.stake));
        }
        self.invites = view.invites;
        self.outgoing = view.outgoing;
        self.queued = view.queued;
        self.clan_tag = view.clan_tag;

        let selected_id = self.peers.get(self.selected).map(|p| p.id);
        let mut peers = vec![Peer::npc()];
        peers.extend(view.players.into_iter().map(|p| Peer {
            id: p.id,
            name: crate::utils::clean_name(&p.name),
            mp: p.mp,
            lifetime: p.lifetime,
            attack: p.attack,
            defense: p.defense,
            streak: p.streak,
            in_duel: p.in_duel,
            immune_until: p.immune.then(|| now + ms(p.immune_ms.max(1))),
            clan_tag: p.clan_tag,
        }));
        self.peers = peers;
        self.selected = selected_id
            .and_then(|id| self.peers.iter().position(|p| p.id == id))
            .unwrap_or(0);
    }
}
