//! DLCs: expansions bought once in the shop, each in its own module implementing [`Dlc`].
//!
//! The core game only knows the extension points below; everything else about a DLC (items, rules,
//! statuses, titles, screens, animations, sprites) lives in its module. See `docs/dlc.md` to add one.

pub mod prison;

use std::time::Instant;

use ratatui::text::{Line, Span};
use serde::{Deserialize, Serialize};

use crate::achievements::Def;
use crate::game::{Game, Player, TimingTier};
use crate::items::{ItemDef, ItemId};
use crate::protocol::PlayerId;
use crate::world::{Slot, World};

/// Every DLC, in shop order.
pub const ALL: &[&dyn Dlc] = &[&prison::Prison];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DlcId {
    Prison,
}

/// A DLC's own fight status, carried by the core `Status::Dlc`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DlcStatus {
    Prison(prison::Status),
}

/// A DLC consumable waiting for the next mog, carried by the core `Buff::Dlc`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DlcBuff {
    Prison(prison::Armed),
}

/// A DLC's achievement, carried by the core `Achievement::Dlc`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DlcAchievement {
    Prison(prison::Ach),
}

/// Matches a `DlcStatus`, `DlcBuff` or `DlcAchievement`, binding the DLC's own value and the DLC itself.
/// The one place a new DLC adds an arm for all three.
macro_rules! each_dlc {
    ($enum:ident, $value:expr, ($inner:pat, $dlc:ident) => $body:expr) => {
        match $value {
            $enum::Prison($inner) => {
                let $dlc: &'static dyn Dlc = &prison::Prison;
                $body
            }
        }
    };
}

/// What each DLC saves in a player's profile.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DlcProfile {
    pub prison: prison::Profile,
}

/// What each DLC shows about a player to everyone else.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DlcPublic {
    pub prison: prison::Public,
}

/// What each DLC keeps for an online player on the server (timers, not saved).
#[derive(Debug, Clone)]
pub struct DlcSlot {
    pub prison: prison::SlotState,
}

impl DlcSlot {
    pub fn new(now: Instant) -> DlcSlot {
        DlcSlot { prison: prison::SlotState::new(now) }
    }
}

/// What everyone sees of `player`, for every DLC.
pub fn public(player: &Player) -> DlcPublic {
    DlcPublic { prison: prison::public(player) }
}

/// A fight status a DLC adds: how it reads and what it does to the numbers.
pub trait StatusKind: Copy {
    fn label(self) -> &'static str;
    fn effect(self) -> String;
    fn is_bad(self) -> bool;
    fn power_mult(self) -> f64 {
        1.0
    }
    fn defense_mult(self) -> f64 {
        1.0
    }
}

/// A consumable a DLC arms for the next mog: badge text, color and the item behind it.
pub trait BuffKind: Copy {
    fn label(self) -> &'static str;
    fn effect(self) -> &'static str;
    fn color(self) -> (u8, u8, u8);
    fn item(self) -> ItemId;
}

/// Full-screen animation for a consumable going off (see `ui::consume`).
pub struct UseFx {
    pub color: (u8, u8, u8),
    pub duration_ms: u64,
    pub shake: f64,
    pub paint: Paint,
}

/// Glyph and brightness (0..1) for one blank cell of the screen, or nothing.
pub type Paint = fn(&crate::ui::consume::Cell) -> Option<(&'static str, f64)>;

/// Embedded sprite: PNG strip, Aseprite JSON and a unique cache name.
pub type SpriteAsset = (&'static [u8], &'static [u8], &'static str);

/// The base interface every DLC implements. Only the identity methods are required; every hook
/// has a no-op default, so a DLC overrides just the parts of the game it changes.
pub trait Dlc: Sync {
    fn id(&self) -> DlcId;
    /// Name in the shop tab and the help.
    fn name(&self) -> &'static str;
    /// The shop item that unlocks it, bought once; it must be in `items`.
    fn unlock_item(&self) -> ItemId;
    /// What buying it unlocks, completing "libera ...".
    fn unlocks(&self) -> &'static str;
    /// Every item it adds, listed in its own shop tab.
    fn items(&self) -> &'static [ItemDef];
    /// Its achievements with their definitions, in display order.
    fn achievements(&self) -> &'static [(DlcAchievement, Def)] {
        &[]
    }

    // Server rules.

    /// Runs one of its consumables; `None` when `item` is not one of them.
    fn use_item(&self, _slot: &mut Slot, _item: ItemId, _now: Instant) -> Option<Result<&'static str, &'static str>> {
        None
    }
    /// Why `attacker` can't mog `target` right now, if this DLC forbids it.
    fn block_mog(&self, _world: &World, _attacker: PlayerId, _target: PlayerId, _now: Instant) -> Option<String> {
        None
    }
    /// A mog from `attacker` landed on the player `target`.
    fn on_hit(&self, _world: &mut World, _attacker: PlayerId, _target: PlayerId, _tier: TimingTier, _now: Instant) {}
    /// Sets one of its statuses on `slot`; false when a longer one is already running.
    fn set_status(&self, _slot: &mut DlcSlot, _status: DlcStatus, _until: Instant) -> bool {
        false
    }
    /// Appends its statuses on `slot` to `out`, with when they end (past ones included).
    fn statuses(&self, _slot: &DlcSlot, _out: &mut Vec<(DlcStatus, Instant)>) {}
    fn clear_statuses(&self, _slot: &mut DlcSlot, _now: Instant) {}
    /// Its consumables armed on `player`.
    fn armed(&self, _player: &Player) -> Vec<DlcBuff> {
        Vec::new()
    }

    // Screen.

    /// Spans after a player's name, from what everyone can see of them; `fresh` lists its running statuses.
    fn name_tags(&self, _public: &DlcPublic, _fresh: &[DlcStatus]) -> Vec<Span<'static>> {
        Vec::new()
    }
    /// One line in the achievements screen.
    fn stats_line(&self, _player: &Player) -> Option<Line<'static>> {
        None
    }
    /// A section under the ranks list.
    fn rank_lines(&self, _game: &Game) -> Vec<Line<'static>> {
        Vec::new()
    }
    /// Rules shown as bullets in its section of the DLCs help page.
    fn help(&self) -> Vec<String> {
        Vec::new()
    }
    fn use_fx(&self, _item: ItemId) -> Option<UseFx> {
        None
    }
    fn sprite(&self, _item: ItemId) -> Option<SpriteAsset> {
        None
    }
}

pub fn get(id: DlcId) -> &'static dyn Dlc {
    *ALL.iter().find(|d| d.id() == id).expect("every DlcId is registered in ALL")
}

/// The DLC an item belongs to, if any.
pub fn of_item(item: ItemId) -> Option<&'static dyn Dlc> {
    crate::items::def(item).dlc.map(get)
}

impl DlcStatus {
    pub fn dlc(self) -> &'static dyn Dlc {
        each_dlc!(DlcStatus, self, (_, d) => d)
    }
    pub fn label(self) -> &'static str {
        each_dlc!(DlcStatus, self, (s, _d) => s.label())
    }
    pub fn effect(self) -> String {
        each_dlc!(DlcStatus, self, (s, _d) => s.effect())
    }
    pub fn is_bad(self) -> bool {
        each_dlc!(DlcStatus, self, (s, _d) => s.is_bad())
    }
    pub fn power_mult(self) -> f64 {
        each_dlc!(DlcStatus, self, (s, _d) => s.power_mult())
    }
    pub fn defense_mult(self) -> f64 {
        each_dlc!(DlcStatus, self, (s, _d) => s.defense_mult())
    }
}

impl DlcBuff {
    pub fn label(self) -> &'static str {
        each_dlc!(DlcBuff, self, (b, _d) => b.label())
    }
    pub fn effect(self) -> &'static str {
        each_dlc!(DlcBuff, self, (b, _d) => b.effect())
    }
    pub fn color(self) -> (u8, u8, u8) {
        each_dlc!(DlcBuff, self, (b, _d) => b.color())
    }
    pub fn item(self) -> ItemId {
        each_dlc!(DlcBuff, self, (b, _d) => b.item())
    }
}

impl DlcAchievement {
    /// Its definition, from the DLC's own list.
    pub fn def(self) -> Def {
        let dlc = each_dlc!(DlcAchievement, self, (_, d) => d);
        dlc.achievements().iter().find(|(a, _)| *a == self).map(|(_, def)| *def).expect("a DLC lists every achievement it declares")
    }
}
