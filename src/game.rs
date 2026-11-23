//! Shared game types (ranks, stats, achievements) and the client-side view.

use std::cell::Cell;
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::avatar::{self, Avatar, Editor, Mood, Part};
use crate::constants::*;
use crate::constants::avatar::{BLINK_EVERY_MS, BLINK_MS};
pub use crate::achievements::Achievement;
pub use crate::items::{CATALOG, Category, Effect, ItemDef, ItemId};
use crate::dlc::{DlcBuff, DlcId, DlcProfile, DlcPublic, DlcStatus};
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
    Sigma,
    AuraLord,
    SixSeven,
    Mythic,
    GreekGod,
    Cosmic,
    Transcendent,
}

impl Rank {
    pub const ALL: [Rank; 15] = [
        Rank::Sub5,
        Rank::Ltn,
        Rank::Mtn,
        Rank::Htn,
        Rank::Chadlite,
        Rank::Chad,
        Rank::Gigachad,
        Rank::Ascended,
        Rank::Sigma,
        Rank::AuraLord,
        Rank::SixSeven,
        Rank::Mythic,
        Rank::GreekGod,
        Rank::Cosmic,
        Rank::Transcendent,
    ];

    /// Lifetime points needed to reach this rank.
    pub fn threshold(self) -> f64 {
        match self {
            Rank::Sub5 => 0.0,
            Rank::Ltn => 300.0,
            Rank::Mtn => 1_500.0,
            Rank::Htn => 6_000.0,
            Rank::Chadlite => 20_000.0,
            Rank::Chad => 60_000.0,
            Rank::Gigachad => 200_000.0,
            Rank::Ascended => 600_000.0,
            Rank::Sigma => 1_500_000.0,
            Rank::AuraLord => 4_000_000.0,
            Rank::SixSeven => 13_400_000.0,
            Rank::Mythic => 30_000_000.0,
            Rank::GreekGod => 80_000_000.0,
            Rank::Cosmic => 200_000_000.0,
            Rank::Transcendent => 600_000_000.0,
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
            Rank::Sigma => "Sigma",
            Rank::AuraLord => "Lorde da Aura",
            Rank::SixSeven => "Six Seven",
            Rank::Mythic => "Mítico",
            Rank::GreekGod => "Deus Grego",
            Rank::Cosmic => "Mog Cósmico",
            Rank::Transcendent => "Transcendente",
        }
    }

    /// One-line flavor shown in the rank list.
    pub fn desc(self) -> &'static str {
        match self {
            Rank::Sub5 => "Começo de tudo. Ninguém nota quando você entra.",
            Rank::Ltn => "Low-tier normie: já dá para ver algum esforço.",
            Rank::Mtn => "Mid-tier normie: na média, mas subindo.",
            Rank::Htn => "High-tier normie: começa a chamar atenção.",
            Rank::Chadlite => "Quase Chad. Só falta lapidar.",
            Rank::Chad => "O padrão que todo mundo reconhece.",
            Rank::Gigachad => "O meme do rosto esculpido, em pessoa.",
            Rank::Ascended => "Completou a jornada do looksmaxxing.",
            Rank::Sigma => "Não precisa de aprovação. Mogga em silêncio.",
            Rank::AuraLord => "Farmou tanta aura que ela vaza pelo terminal.",
            Rank::SixSeven => "Six seven. Ninguém sabe o que significa, todo mundo respeita.",
            Rank::Mythic => "Os novatos acham que você é lenda urbana.",
            Rank::GreekGod => "Esculpido em mármore, proporção áurea no rosto.",
            Rank::Cosmic => "O mog atravessa galáxias.",
            Rank::Transcendent => "Além do mog. Não sobrou ninguém para moggar.",
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
    /// Share of the attacker's loss the defender gets for holding a mog.
    pub counter_ratio: f64,
    /// Extra crit chance for each doubling of the target's defense over this attack.
    pub underdog: f64,
    /// Multiplier on the timing meter speed.
    pub meter_factor: f64,
    /// Share of a bad status's duration that Terapia removes.
    pub resilience: f64,
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
    pub mews: u64,
    /// Consumables used.
    pub items_used: u32,
    /// Rounds won in duels worth MP.
    pub duel_rounds_won: u32,
    /// Duels worth MP won without losing a round.
    pub flawless_duels: u32,
    pub sudden_death_wins: u32,
    /// Biggest pot taken in a duel.
    pub best_duel_pot: f64,
    /// Minigames with at least one round won in a duel worth MP.
    pub minigames_won: Vec<Minigame>,
    pub shield_armed: bool,
    pub angle_armed: bool,
    pub mirror_armed: bool,
    pub tape_armed: bool,
    /// Combo 6-7 waiting for the next mog that lands.
    pub combo_armed: bool,
    /// Ego Ferido waiting for a mog on `nemesis`.
    pub ego_armed: bool,
    /// Last player who mogged this one.
    pub nemesis: Option<String>,
    pub achievements: Vec<Achievement>,
    /// What each DLC saves.
    pub dlc: DlcProfile,
    /// Face parts worn.
    pub avatar: Avatar,
    /// Face parts bought; free ones are always owned.
    pub avatar_parts: Vec<Part>,
}

impl Player {
    pub fn level(&self, id: ItemId) -> u32 {
        self.items.get(&id).copied().unwrap_or(0)
    }

    pub fn rank(&self) -> Rank {
        Rank::from_lifetime(self.lifetime)
    }

    pub fn owns_part(&self, part: Part) -> bool {
        avatar::def(part).free() || self.avatar_parts.contains(&part)
    }

    /// Bought the DLC `id`.
    pub fn owns_dlc(&self, id: DlcId) -> bool {
        self.level(crate::dlc::get(id).unlock_item()) > 0
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
            counter_ratio: COUNTER_RATIO,
            underdog: 0.0,
            meter_factor: 1.0,
            resilience: 0.0,
        };
        let mut aura = 1.0;
        for def in CATALOG.iter() {
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
                    Effect::CounterBonus(v) => s.counter_ratio += v * k,
                    Effect::Underdog(v) => s.underdog += v * k,
                    Effect::MeterSlow(v) => s.meter_factor -= v * k,
                    Effect::Aura(v) => aura += v * k,
                    Effect::Resilience(v) => s.resilience += v * k,
                }
            }
        }
        s.attack *= aura;
        s.defense *= aura;
        s.passive *= aura;
        s.mew_gain *= aura;
        s.cooldown = s.cooldown.max(MIN_COOLDOWN_SECS);
        s.loss_shield = s.loss_shield.min(MAX_LOSS_SHIELD);
        s.counter_ratio = s.counter_ratio.min(MAX_COUNTER_RATIO);
        s.meter_factor = s.meter_factor.max(MIN_METER_FACTOR);
        s.resilience = s.resilience.min(DEBUFF_CUT_CAP);
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

/// How long after a press the MP that shows up is counted as that mog's prize.
const MOG_PRIZE_WINDOW: Duration = Duration::from_millis(2000);

/// The last MOGAR press, for the screen-shake / wave / aura burst.
#[derive(Debug, Clone, Copy)]
pub struct MogHit {
    pub at: Instant,
    pub tier: TimingTier,
    /// MP before the press.
    pub bank: f64,
    /// MP won by it, once the server says so.
    pub gain: f64,
}

impl MogHit {
    /// How big the burst is: the tier sets the base, the prize relative to your bank adds up to double.
    pub fn intensity(&self) -> f64 {
        let base = match self.tier {
            TimingTier::Bad => 0.4,
            TimingTier::Ok => 0.7,
            TimingTier::Good => 1.0,
            TimingTier::Perfect => 1.4,
        };
        base * (1.0 + (self.gain / self.bank.max(50.0)).clamp(0.0, 1.0))
    }
}

/// A hit this player took (someone mogged them), for the impact burst.
#[derive(Debug, Clone, Copy)]
pub struct HitTaken {
    pub at: Instant,
    /// MP before the hit.
    pub bank: f64,
    pub loss: f64,
}

impl HitTaken {
    /// 1.0 for a scratch, up to 2.0 for losing the whole bank.
    pub fn intensity(&self) -> f64 {
        1.0 + (self.loss / self.bank.max(50.0)).clamp(0.0, 1.0)
    }
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
    /// Every rank, with the look of each name.
    Ranks,
    Help,
    Server,
    /// Owned items by category, full screen.
    Inventory,
    /// Choosing how much to bet before a challenge or the queue.
    Stake,
    /// Own face: try on, buy and wear parts.
    Avatar,
    /// A player's profile; `None` is the own one.
    Profile(Option<PlayerId>),
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

/// Chance of a critical mog that goes through any defense: proportional to attack against defense, never zero.
pub fn crit_chance(attack: f64, defense: f64, underdog: f64) -> f64 {
    let total = attack + defense;
    let share = if total > 0.0 { attack / total } else { 0.5 };
    let doublings = if attack > 0.0 && defense > attack { (defense / attack).log2() } else { 0.0 };
    ((CRIT_CHANCE_SCALE * share).max(CRIT_MIN_CHANCE) + underdog * doublings).min(CRIT_MAX_CHANCE)
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
    /// Fight statuses and when they end.
    pub statuses: Vec<(Status, Instant)>,
    /// What each DLC shows about them.
    pub dlc: DlcPublic,
    pub avatar: Avatar,
    /// The face's reaction to the last mog, and until when.
    pub mood: Option<(Mood, Instant)>,
}

impl Peer {
    /// The face's reaction at `now`, if one is running.
    pub fn reacting(&self, now: Instant) -> Option<Mood> {
        self.mood.filter(|(_, until)| now < *until).map(|(mood, _)| mood)
    }

    pub fn npc() -> Peer {
        Peer { id: NPC_TARGET, name: NPC_NAME.into(), mp: 0.0, lifetime: 0.0, attack: 0.0, defense: 0.0, streak: 0, in_duel: false, immune_until: None, clan_tag: None, statuses: Vec::new(), dlc: DlcPublic::default(), avatar: Default::default(), mood: None }
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
    /// Last MOGAR press and the tier it hit on the local meter; drives the wave/aura burst.
    pub last_mog: Option<MogHit>,
    /// Last time someone mogged this player, for the impact burst.
    pub last_taken: Option<HitTaken>,
    /// Last consumable used, detected when its count drops in a state update.
    pub last_used: Option<(Instant, ItemId)>,
    /// Last consumable that went off at impact, sent by the server.
    pub last_fx: Option<(Instant, crate::protocol::Fx)>,
    /// Latest MP drop seen in a state update, in case it arrives before the hit event.
    recent_drop: Option<(Instant, f64)>,
    pub overlay: Overlay,
    pub shop_cursor: usize,
    /// Shop item waiting for a yes/no before buying.
    pub shop_confirm: Option<usize>,
    /// Item bought and not yet answered by the server.
    pub shop_pending: Option<ItemId>,
    /// Last purchase outcome shown in the shop: (ok, text, when).
    pub shop_result: Option<(bool, String, Instant)>,
    /// Category shown in the inventory screen.
    pub inventory_tab: Category,
    /// Catalog index of the item picked in the inventory screen.
    pub inventory_cursor: usize,
    pub help_page: usize,
    pub help_scroll: u16,
    /// Scroll of the achievements and ranks lists.
    pub list_scroll: u16,
    /// Last list scroll limit computed by the renderer.
    pub list_max_scroll: Cell<u16>,
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
    /// The queue entry waits for a group duel.
    pub queued_group: bool,
    /// Selected bet in the stake picker, as an index of `STAKE_FRACTIONS`.
    pub stake_cursor: usize,
    /// Who the stake picker challenges; `None` means the queue.
    pub stake_target: Option<PlayerId>,
    /// Focused row of the stake picker: the bet, the minigame mode, then the players.
    pub stake_row: usize,
    /// Each side picks the minigames instead of drawing them.
    pub stake_draft: bool,
    /// Everyone the stake picker challenges (more than one makes a group duel).
    pub stake_invited: Vec<PlayerId>,
    /// Player under the cursor on the players row, as an index of `stake_candidates`.
    pub stake_pick: usize,
    /// The queue waits for a group duel instead of one rival.
    pub stake_group: bool,
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
    /// Fight statuses and when they end.
    pub statuses: Vec<(Status, Instant)>,
    logged: usize,
    pub avatar_editor: Editor,
    /// The own face's reaction to the last mog, and until when.
    mood: Option<(Mood, Instant)>,
}

/// Share of power a rank-gap status changes, for a gap of `n` ranks (0 means none).
pub fn gap_strength(n: u8) -> f64 {
    usize::from(n).checked_sub(1).map_or(0.0, |i| RANK_GAP_STRENGTH[i.min(RANK_GAP_CAP - 1)])
}

/// A timed effect earned in a mog fight, good or bad (see `World::afflict`).
/// The number is the rank gap (1 to `RANK_GAP_CAP`) that sets its strength.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    /// Took a critical hit: defense drops.
    Abalado,
    /// Lost to a lower rank: power drops.
    PerdeuAura(u8),
    /// Beat a higher rank: power rises.
    EgoProibido(u8),
    /// A status from a DLC.
    Dlc(DlcStatus),
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Abalado => "ABALADO",
            Status::PerdeuAura(_) => "PERDEU AURA",
            Status::EgoProibido(_) => "EGO PROIBIDO",
            Status::Dlc(st) => st.label(),
        }
    }

    pub fn effect(self) -> String {
        match self {
            Status::Abalado => format!("defesa -{:.0}%", ABALADO_DEF_CUT * 100.0),
            Status::PerdeuAura(n) => format!("poder -{:.0}%", gap_strength(n) * 100.0),
            Status::EgoProibido(n) => format!("poder +{:.0}%", gap_strength(n) * 100.0),
            Status::Dlc(st) => st.effect(),
        }
    }

    pub fn is_bad(self) -> bool {
        match self {
            Status::EgoProibido(_) => false,
            Status::Dlc(st) => st.is_bad(),
            _ => true,
        }
    }
}

/// Power multiplier from the given active statuses.
pub fn status_power_mult(statuses: impl IntoIterator<Item = Status>) -> f64 {
    statuses.into_iter().fold(1.0, |m, st| match st {
        Status::PerdeuAura(n) => m * (1.0 - gap_strength(n)),
        Status::EgoProibido(n) => m * (1.0 + gap_strength(n)),
        Status::Dlc(st) => m * st.power_mult(),
        Status::Abalado => m,
    })
}

/// Defense multiplier from the given active statuses.
pub fn status_defense_mult(statuses: impl IntoIterator<Item = Status>) -> f64 {
    statuses.into_iter().fold(1.0, |m, st| match st {
        Status::Abalado => m * (1.0 - ABALADO_DEF_CUT),
        Status::Dlc(st) => m * st.defense_mult(),
        _ => m,
    })
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
    Ego,
    /// A consumable from a DLC.
    Dlc(DlcBuff),
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
            Buff::Ego => "EGO FERIDO",
            Buff::AuraFarm => "FARMANDO AURA",
            Buff::Dlc(b) => b.label(),
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
            Buff::Ego => "crítico garantido no troco",
            Buff::AuraFarm => "passivo x2",
            Buff::Dlc(b) => b.effect(),
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
            Buff::Ego => ItemId::WoundedEgo,
            Buff::AuraFarm => ItemId::AuraFarm,
            Buff::Dlc(b) => b.item(),
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
            last_mog: None,
            last_taken: None,
            last_used: None,
            last_fx: None,
            recent_drop: None,
            overlay: Overlay::None,
            shop_cursor: 0,
            shop_confirm: None,
            shop_pending: None,
            shop_result: None,
            inventory_tab: Category::all()[0],
            inventory_cursor: 0,
            help_page: 0,
            help_scroll: 0,
            list_scroll: 0,
            list_max_scroll: Cell::new(0),
            help_max_scroll: Cell::new(0),
            started: now,
            outbox: Vec::new(),
            duel: None,
            duel_phase_started: now,
            invites: Vec::new(),
            outgoing: None,
            queued: None,
            queued_group: false,
            stake_cursor: 1,
            stake_target: None,
            stake_row: 0,
            stake_draft: false,
            stake_invited: Vec::new(),
            stake_pick: 0,
            stake_group: false,
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
            statuses: Vec::new(),
            logged: 0,
            avatar_editor: Default::default(),
            mood: None,
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
        let armed = [(p.shield_armed, Buff::Shield), (p.angle_armed, Buff::Angle), (p.mirror_armed, Buff::Mirror), (p.tape_armed, Buff::Tape), (p.combo_armed, Buff::Combo), (p.ego_armed, Buff::Ego)];
        let timed = [(Buff::PreWorkout, left(self.preworkout_until)), (Buff::Cologne, left(self.cologne_until)), (Buff::AuraFarm, left(self.aura_farm_until))];
        armed
            .into_iter()
            .filter_map(|(on, b)| on.then_some((b, None)))
            .chain(crate::dlc::ALL.iter().flat_map(|d| d.armed(p)).map(|b| (Buff::Dlc(b), None)))
            .chain(timed.into_iter().filter_map(|(b, l)| l.map(|l| (b, Some(l)))))
            .collect()
    }

    /// Fight statuses still running, with the time they have left.
    pub fn active_statuses(&self, now: Instant) -> Vec<(Status, Duration)> {
        self.statuses.iter().filter(|(_, until)| now < *until).map(|(st, until)| (*st, *until - now)).collect()
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
    pub fn mog_odds(&self, peer: &Peer, now: Instant) -> (f64, f64) {
        let stats = self.player.stats();
        let angle = if self.player.angle_armed { 2.0 } else { 1.0 };
        let mine = self.active_statuses(now).into_iter().map(|(st, _)| st);
        let base = stats.attack * status_power_mult(mine) * streak_mult(self.streak, &stats) * angle;
        let good = TIMING_FLOOR + TIMING_SPAN * (TIMING_GOOD_FROM + stats.perfect_threshold) / 2.0;
        let (defense, roll) = if peer.is_npc() { (stats.attack, NPC_DEFENSE_ROLL) } else { (peer.defense, DEFENSE_ROLL) };
        let theirs = peer.statuses.iter().filter(|(_, until)| now < *until).map(|(st, _)| *st);
        let rolled_defense = defense * status_defense_mult(theirs);
        let revenge = self.player.ego_armed && self.player.nemesis.as_deref() == Some(peer.name.as_str());
        let crit = if revenge { 1.0 } else { crit_chance(stats.attack, defense, stats.underdog) };
        let odds = |power: f64| {
            let beat = beat_chance(power, rolled_defense, roll.clone());
            beat + (1.0 - beat) * crit
        };
        (odds(base * good), odds(base * PERFECT_MULT))
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

    /// The own face's expression at `now`: a recent reaction, a blink, or neutral.
    pub fn mood(&self, now: Instant) -> Mood {
        match self.mood {
            Some((mood, until)) if now < until => mood,
            _ => self.idle_mood(now),
        }
    }

    /// A face with nothing to react to: blinking now and then.
    pub fn idle_mood(&self, now: Instant) -> Mood {
        if now.saturating_duration_since(self.started).as_millis() % BLINK_EVERY_MS < BLINK_MS { Mood::Blink } else { Mood::Neutral }
    }

    /// The own player as others see them, for the profile; the client does not know its own id.
    pub fn own_peer(&self, now: Instant) -> Peer {
        let (p, s) = (&self.player, self.player.stats());
        Peer {
            id: NPC_TARGET,
            name: p.name.clone(),
            mp: p.mp,
            lifetime: p.lifetime,
            attack: s.attack,
            defense: s.defense,
            streak: self.streak,
            in_duel: self.duel.is_some(),
            immune_until: None,
            clan_tag: self.clan_tag.clone(),
            statuses: self.active_statuses(now).into_iter().map(|(st, left)| (st, now + left)).collect(),
            dlc: crate::dlc::public(p),
            avatar: p.avatar,
            mood: self.mood,
        }
    }

    pub fn open_avatar(&mut self) {
        self.avatar_editor.open(&self.player.avatar);
        self.overlay = Overlay::Avatar;
    }

    pub fn avatar_enter(&mut self) {
        self.outbox.extend(self.avatar_editor.enter(&self.player));
    }

    /// Opens the profile of the selected player, or the own one when the bot is selected.
    pub fn open_profile(&mut self) {
        let id = self.peers.get(self.selected).filter(|p| !p.is_npc()).map(|p| p.id);
        self.overlay = Overlay::Profile(id);
    }

    /// Moves the open profile to the next player, the own one counting as the first.
    pub fn profile_step(&mut self, delta: isize) {
        let Overlay::Profile(current) = self.overlay else { return };
        let ids: Vec<Option<PlayerId>> = std::iter::once(None).chain(self.peers.iter().filter(|p| !p.is_npc()).map(|p| Some(p.id))).collect();
        let at = ids.iter().position(|id| *id == current).unwrap_or(0) as isize;
        self.overlay = Overlay::Profile(ids[(at + delta).rem_euclid(ids.len() as isize) as usize]);
    }

    /// Opens a scrolling list (achievements or ranks) from the top.
    pub fn open_list(&mut self, overlay: Overlay) {
        self.list_scroll = 0;
        self.overlay = overlay;
    }

    pub fn list_scroll_by(&mut self, delta: i32) {
        let next = (self.list_scroll as i32 + delta).max(0) as u16;
        self.list_scroll = next.min(self.list_max_scroll.get());
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
        let tabs = Category::all();
        let at = tabs.iter().position(|c| *c == self.shop_category()).unwrap_or(0) as isize;
        let next = tabs[(at + delta).rem_euclid(tabs.len() as isize) as usize];
        self.shop_cursor = next.items().next().unwrap_or(0);
    }

    /// Tab of the item under the shop cursor.
    /// Owned items of the open inventory tab, as catalog indexes.
    pub fn inventory_items(&self) -> Vec<usize> {
        self.inventory_tab.items().filter(|i| self.player.level(CATALOG[*i].id) > 0).collect()
    }

    /// Item picked in the inventory; falls back to the tab's first one when the pick ran out.
    pub fn inventory_selected(&self) -> Option<usize> {
        let items = self.inventory_items();
        items.iter().copied().find(|i| *i == self.inventory_cursor).or(items.first().copied())
    }

    pub fn open_inventory(&mut self) {
        self.overlay = Overlay::Inventory;
        if self.inventory_items().is_empty()
            && let Some(tab) = Category::all().iter().copied().find(|c| c.items().any(|i| self.player.level(CATALOG[i].id) > 0))
        {
            self.inventory_tab = tab;
        }
        self.inventory_cursor = self.inventory_selected().unwrap_or(0);
    }

    pub fn inventory_move(&mut self, delta: isize) {
        let items = self.inventory_items();
        if let Some(current) = self.inventory_selected() {
            let at = items.iter().position(|i| *i == current).unwrap_or(0) as isize;
            self.inventory_cursor = items[(at + delta).rem_euclid(items.len() as isize) as usize];
        }
    }

    pub fn inventory_turn(&mut self, delta: isize) {
        let at = Category::all().iter().position(|c| *c == self.inventory_tab).unwrap_or(0) as isize;
        self.inventory_tab = Category::all()[(at + delta).rem_euclid(Category::all().len() as isize) as usize];
        self.inventory_cursor = self.inventory_items().first().copied().unwrap_or(0);
    }

    /// Uses the picked item when it is a consumable.
    pub fn inventory_use(&mut self) {
        if let Some(def) = self.inventory_selected().map(|i| &CATALOG[i]).filter(|d| d.consumable()) {
            self.use_item(def.id);
        }
    }

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
        if target.in_duel {
            let text = format!("{} está num duelo agora.", target.name);
            self.flash(now, Tone::Info, text);
            return;
        }
        let meter = self.meter(now);
        let tier = TimingTier::from_meter(meter, self.player.stats().perfect_threshold);
        self.last_mog = Some(MogHit { at: now, tier, bank: self.player.mp, gain: 0.0 });
        self.outbox.push(ClientMsg::Mog { target: target.id, meter });
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
                self.open_stake(Some(peer.id));
            }
            None => {}
        }
    }

    /// Leaves the queue, or opens the bet picker to join it.
    pub fn toggle_queue(&mut self) {
        if self.queued.is_some() {
            self.outbox.push(ClientMsg::Queue { fraction: None, draft: false, group: false });
        } else {
            self.open_stake(None);
        }
    }

    /// Opens the stake picker on the bet row, with drawn minigames and only `target` invited.
    fn open_stake(&mut self, target: Option<PlayerId>) {
        self.stake_target = target;
        self.stake_invited = target.into_iter().collect();
        self.stake_pick = self.stake_candidates().iter().position(|p| Some(p.id) == target).unwrap_or(0);
        self.stake_row = 0;
        self.stake_draft = false;
        self.stake_group = false;
        self.overlay = Overlay::Stake;
    }

    /// Players who can be invited to the duel being set up.
    pub fn stake_candidates(&self) -> Vec<&Peer> {
        self.peers.iter().filter(|p| !p.is_npc() && !p.in_duel).collect()
    }

    /// A group duel is being set up: several players invited, or the group queue.
    pub fn stake_is_group(&self) -> bool {
        if self.stake_target.is_some() { self.stake_invited.len() > 1 } else { self.stake_group }
    }

    /// Changes the bet, the minigame mode, or the player under the cursor (the queue mode, for the queue).
    pub fn stake_move(&mut self, delta: isize) {
        match self.stake_row {
            0 => {
                let len = STAKE_FRACTIONS.len() as isize;
                self.stake_cursor = (self.stake_cursor as isize + delta).clamp(0, len - 1) as usize;
            }
            1 => self.stake_draft = delta > 0,
            _ if self.stake_target.is_none() => self.stake_group = delta > 0,
            _ => {
                let len = self.stake_candidates().len().max(1) as isize;
                self.stake_pick = (self.stake_pick as isize + delta).rem_euclid(len) as usize;
            }
        }
    }

    /// Moves focus between the bet, minigame mode and players rows.
    pub fn stake_row_move(&mut self, delta: isize) {
        self.stake_row = (self.stake_row as isize + delta).clamp(0, 2) as usize;
    }

    /// Invites or drops the player under the cursor, up to a full group.
    pub fn stake_toggle(&mut self) {
        let Some(id) = self.stake_candidates().get(self.stake_pick).map(|p| p.id) else { return };
        if let Some(i) = self.stake_invited.iter().position(|p| *p == id) {
            self.stake_invited.remove(i);
        } else if self.stake_invited.len() + 1 < DUEL_MAX_PLAYERS {
            self.stake_invited.push(id);
        }
    }

    /// Space: ticks a player on the players row, confirms anywhere else.
    pub fn stake_space(&mut self) {
        if self.stake_row == 2 && self.stake_target.is_some() {
            self.stake_toggle();
        } else {
            self.confirm_stake();
        }
    }

    /// Sends the challenge or joins the queue with the chosen bet.
    pub fn confirm_stake(&mut self) {
        let fraction = STAKE_FRACTIONS[self.stake_cursor.min(STAKE_FRACTIONS.len() - 1)];
        let draft = self.stake_draft && !self.stake_is_group();
        let msg = match self.stake_target {
            Some(_) if self.stake_invited.is_empty() => return self.log(Tone::Info, "Marque ao menos um jogador (ESPAÇO)."),
            Some(_) => ClientMsg::Challenge { targets: self.stake_invited.clone(), fraction, draft },
            None => ClientMsg::Queue { fraction: Some(fraction), draft, group: self.stake_group },
        };
        self.outbox.push(msg);
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
                // A refused part purchase answers with a bad event only.
                if tone == Tone::Bad {
                    self.avatar_editor.pending = None;
                }
                self.notices.extend(Notice::from_event(&text, banner.as_deref(), hit));
                if !text.is_empty() {
                    self.log(tone, text);
                }
                if let Some(b) = banner {
                    self.flash(now, tone, b);
                }
                if hit {
                    self.hit_flash_until = now + HIT_FLASH;
                    let loss = self.recent_drop.filter(|(t, _)| now.saturating_duration_since(*t) < Duration::from_millis(500)).map_or(0.0, |(_, l)| l);
                    self.last_taken = Some(HitTaken { at: now, bank: self.player.mp + loss, loss });
                }
            }
            ServerMsg::Fx(fx) => self.last_fx = Some((now, fx)),
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
        // MP that arrives right after a press is the mog's prize; it scales the burst.
        let delta = view.me.mp - self.player.mp;
        if let Some(hit) = self.last_mog.as_mut().filter(|h| now.saturating_duration_since(h.at) < MOG_PRIZE_WINDOW) {
            hit.gain += delta.max(0.0);
        }
        if delta < 0.0 {
            self.recent_drop = Some((now, -delta));
            if let Some(hit) = self.last_taken.as_mut().filter(|h| now.saturating_duration_since(h.at) < MOG_PRIZE_WINDOW) {
                hit.loss -= delta;
            }
        }
        if let Some(def) = CATALOG.iter().find(|d| d.consumable() && view.me.level(d.id) < self.player.level(d.id)) {
            self.last_used = Some((now, def.id));
        }
        if self.avatar_editor.pending.is_some_and(|part| view.me.owns_part(part)) {
            self.avatar_editor.pending = None;
        }
        self.mood = view.mood.map(|(mood, left)| (mood, now + ms(left)));
        self.player = view.me;
        self.streak = view.streak;
        self.cooldown_until = now + ms(view.cooldown_ms);
        self.cooldown_total = view.cooldown_total_ms as f64 / 1000.0;
        self.immune_until = now + ms(view.immune_ms);
        self.preworkout_until = now + ms(view.preworkout_ms);
        self.cologne_until = now + ms(view.cologne_ms);
        self.aura_farm_until = now + ms(view.aura_farm_ms);
        self.statuses = view.statuses.iter().map(|(st, left)| (*st, now + ms(*left))).collect();
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
        self.queued_group = view.queued_group;
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
            statuses: p.statuses.iter().map(|(st, left)| (*st, now + ms(*left))).collect(),
            dlc: p.dlc,
            avatar: p.avatar,
            mood: p.mood.map(|(mood, left)| (mood, now + ms(left))),
        }));
        self.peers = peers;
        self.selected = selected_id
            .and_then(|id| self.peers.iter().position(|p| p.id == id))
            .unwrap_or(0);
    }
}
