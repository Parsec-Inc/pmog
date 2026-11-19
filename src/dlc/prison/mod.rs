//! DLC Cadeia, from the AI series "O sabonete": whoever gets made to pick up the soap gets a number
//! on the forehead. A PERFECT mog (or the Sabonete) does it; potions and pills from the story help.

mod titles;
mod ui;

use std::time::Instant;

use ratatui::text::{Line, Span};
use serde::{Deserialize, Serialize};

pub use titles::PrisonTitle;

use super::{BuffKind, Dlc, DlcAchievement, DlcBuff, DlcId, DlcPublic, DlcSlot, DlcStatus, SpriteAsset, StatusKind, UseFx};
use crate::achievements::{Def, Goal, Group, owns_dlc_set};
use crate::constants::prison::*;
use crate::game::{self, Game, Player, Rank, TimingTier, Tone};
use crate::items::{Effect, ItemDef, ItemId, consumable, permanent};
use crate::protocol::{NPC_TARGET, PlayerId};
use crate::utils::{decimal, format_wait};
use crate::world::{Slot, World};

pub struct Prison;

/// Cadeia fight statuses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    /// Picked up the soap: defense drops while the number is fresh.
    Sabonete,
    /// Drank the Black Potion: can't mog whoever gave it.
    Obedecendo,
    PocaoVerde,
    PocaoVermelha,
}

impl Status {
    const ALL: [Status; 4] = [Status::Sabonete, Status::Obedecendo, Status::PocaoVerde, Status::PocaoVermelha];
}

/// The core status carrying `st`.
fn core(st: Status) -> game::Status {
    game::Status::Dlc(DlcStatus::Prison(st))
}

/// Cadeia consumables waiting for the next mog that lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Armed {
    Soap,
    Pill,
    BlackPotion,
}

impl Armed {
    fn of(item: ItemId) -> Option<Armed> {
        match item {
            ItemId::Soap => Some(Armed::Soap),
            ItemId::CarrotPill => Some(Armed::Pill),
            ItemId::BlackPotion => Some(Armed::BlackPotion),
            _ => None,
        }
    }

    /// Message when arming it, and when it is already armed.
    fn messages(self) -> (&'static str, &'static str) {
        match self {
            Armed::Soap => ("Quem o próximo mog acertar vai pegar o sabonete.", "O Sabonete já está na mão."),
            Armed::Pill => ("Quem o próximo mog acertar vai dormir.", "O Comprimido já está pronto."),
            Armed::BlackPotion => ("Quem o próximo mog acertar vai te obedecer.", "A Poção Negra já está pronta."),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Ach {
    FirstSoap,
    Forehead67,
    FirstMark,
    Marks100,
    ZeroTitle,
    PrisonSet,
}

/// Saved with the player.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    /// Soaps this player had to pick up: the number on the forehead.
    pub forehead: u32,
    /// Soaps this player made others pick up.
    pub marks: u32,
    /// Consumables waiting for the next mog that lands.
    pub armed: Vec<Armed>,
}

impl Profile {
    pub fn title(&self) -> Option<PrisonTitle> {
        PrisonTitle::of(self.marks, self.forehead)
    }
}

/// What everyone sees.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Public {
    pub forehead: u32,
    pub marks: u32,
}

impl Public {
    pub fn title(&self) -> Option<PrisonTitle> {
        PrisonTitle::of(self.marks, self.forehead)
    }
}

/// Server timers for one online player.
#[derive(Debug, Clone)]
pub struct SlotState {
    /// When each status ends, indexed by `Status`.
    until: [Instant; 4],
    /// Who the Black Potion makes this player obey.
    obeys: PlayerId,
}

impl SlotState {
    pub fn new(now: Instant) -> SlotState {
        SlotState { until: [now; 4], obeys: NPC_TARGET }
    }
}

pub fn public(p: &Player) -> Public {
    Public { forehead: p.dlc.prison.forehead, marks: p.dlc.prison.marks }
}

impl StatusKind for Status {
    fn label(self) -> &'static str {
        match self {
            Status::Sabonete => "PEGOU O SABONETE",
            Status::Obedecendo => "OBEDECENDO",
            Status::PocaoVerde => "POÇÃO VERDE",
            Status::PocaoVermelha => "POÇÃO VERMELHA",
        }
    }

    fn effect(self) -> String {
        match self {
            Status::Sabonete => format!("defesa -{:.0}%", SOAP_DEF_CUT * 100.0),
            Status::Obedecendo => "não consegue mogar quem deu a poção".into(),
            Status::PocaoVerde => format!("poder x{}", decimal(GREEN_POTION_MULT)),
            Status::PocaoVermelha => format!("poder +{:.0}%", (RED_POTION_MULT - 1.0) * 100.0),
        }
    }

    fn is_bad(self) -> bool {
        matches!(self, Status::Sabonete | Status::Obedecendo)
    }

    fn power_mult(self) -> f64 {
        match self {
            Status::PocaoVerde => GREEN_POTION_MULT,
            Status::PocaoVermelha => RED_POTION_MULT,
            _ => 1.0,
        }
    }

    fn defense_mult(self) -> f64 {
        if self == Status::Sabonete { 1.0 - SOAP_DEF_CUT } else { 1.0 }
    }
}

impl BuffKind for Armed {
    fn label(self) -> &'static str {
        match self {
            Armed::Soap => "SABONETE",
            Armed::Pill => "COMPRIMIDO",
            Armed::BlackPotion => "POÇÃO NEGRA",
        }
    }

    fn effect(self) -> &'static str {
        match self {
            Armed::Soap => "próximo alvo pega o sabonete",
            Armed::Pill => "próximo alvo dorme",
            Armed::BlackPotion => "próximo alvo te obedece",
        }
    }

    fn color(self) -> (u8, u8, u8) {
        match self {
            Armed::Soap => ui::SOAP_RGB,
            Armed::Pill => ui::PILL_RGB,
            Armed::BlackPotion => ui::BLACK_RGB,
        }
    }

    fn item(self) -> ItemId {
        match self {
            Armed::Soap => ItemId::Soap,
            Armed::Pill => ItemId::CarrotPill,
            Armed::BlackPotion => ItemId::BlackPotion,
        }
    }
}

const ITEMS: [ItemDef; 13] = [
    permanent(ItemId::PrisonDlc, "DLC Cadeia", "Expansão baseada em \"O sabonete\". Nunca deixe cair.", Rank::Ltn, 1_000.0, 1, &[]),
    permanent(ItemId::PullUpBar, "Barra do Três", "Um mês atrás o magrelo mal subia nessa barra.", Rank::Ltn, 260.0, 30,
        &[Effect::Attack(4.0), Effect::Defense(2.0)]),
    permanent(ItemId::Crutch, "Muleta do Mamão", "Sem ela ele não consegue nem levantar.", Rank::Mtn, 950.0, 10,
        &[Effect::Defense(10.0), Effect::LossShield(0.02)]),
    permanent(ItemId::CarrotHook, "Gancho do Cenoura", "Quase derrubou o Zero junto com o saco de pancada.", Rank::Htn, 3_200.0, 10,
        &[Effect::Attack(12.0), Effect::WinBonus(0.04)]),
    permanent(ItemId::MilEar, "Ouvido do Mil", "Ouve até tua respiração, guri.", Rank::Chadlite, 9_000.0, 5,
        &[Effect::PerfectZone(0.006), Effect::CounterBonus(0.05)]),
    permanent(ItemId::Folder77, "Pasta 77", "O que estava na torre, ninguém devia saber.", Rank::Chad, 22_000.0, 5,
        &[Effect::Resilience(0.10), Effect::ImmunityBonus(1.0)]),
    permanent(ItemId::PeraUniform, "Farda do Pera", "Quem veste manda na prisão. Por enquanto.", Rank::Gigachad, 90_000.0, 10,
        &[Effect::Passive(35.0), Effect::Defense(40.0)]),
    permanent(ItemId::TowerKey, "Chave da Torre", "A única chave da prisão. Agora quem manda é você.", Rank::Ascended, 300_000.0, 5,
        &[Effect::Attack(120.0), Effect::LossShield(0.04)]),
    consumable(ItemId::Soap, "Sabonete", "Caiu no chão do chuveiro. Alguém vai ter que pegar.", Rank::Mtn, 800.0, 'b',
        "o próximo mog que acertar faz o alvo pegar o sabonete, mesmo sem PERFECT"),
    consumable(ItemId::GreenPotion, "Poção Verde", "Dez minutos de monstro. Aqui, vinte segundos.", Rank::Htn, 2_500.0, 'v',
        "por 20 s, poder x2"),
    consumable(ItemId::CarrotPill, "Comprimido do Cenoura", "Com isso dorme um cavalo.", Rank::Htn, 2_000.0, 'z',
        "o próximo mog que acertar faz o alvo dormir: +10 s de cooldown"),
    consumable(ItemId::RedPotion, "Poção Vermelha", "Do doutor da outra prisão. Ninguém viu a cara dele.", Rank::Chadlite, 6_000.0, 'w',
        "por 2 min, poder +40%"),
    consumable(ItemId::BlackPotion, "Poção Negra", "Não dá força. Faz obedecer.", Rank::Chad, 15_000.0, 'n',
        "quem o próximo mog acertar fica 60 s sem conseguir te mogar"),
];

const GROUP: Group = Group::Dlc(DlcId::Prison);

static ACHIEVEMENTS: [(DlcAchievement, Def); 6] = [
    (
        DlcAchievement::Prison(Ach::FirstSoap),
        Def::new(GROUP, "Abaixou", "Pegue o sabonete pela primeira vez", Goal::Count(|p| p.dlc.prison.forehead as u64, 1)),
    ),
    (
        DlcAchievement::Prison(Ach::Forehead67),
        Def::new(GROUP, "Testa 6-7", "Fique com 67 na testa", Goal::Count(|p| p.dlc.prison.forehead as u64, 67)).secret(),
    ),
    (
        DlcAchievement::Prison(Ach::FirstMark),
        Def::new(GROUP, "Primeiro Numerinho", "Faça alguém pegar o sabonete", Goal::Count(|p| p.dlc.prison.marks as u64, 1)),
    ),
    (
        DlcAchievement::Prison(Ach::Marks100),
        Def::new(GROUP, "Dono da Cela", "Faça 100 pessoas pegarem o sabonete", Goal::Count(|p| p.dlc.prison.marks as u64, 100)),
    ),
    (
        DlcAchievement::Prison(Ach::ZeroTitle),
        Def::new(
            GROUP,
            "Nunca Se Abaixou",
            "Vire o Zero: 67 numerinhos marcados sem nenhum na testa",
            Goal::Check(|p| p.dlc.prison.title() == Some(PrisonTitle::Zero)),
        ),
    ),
    (
        DlcAchievement::Prison(Ach::PrisonSet),
        Def::new(GROUP, "Kit da Cadeia", "Tenha todos os itens permanentes da Cadeia", Goal::Check(|p| owns_dlc_set(p, DlcId::Prison))),
    ),
];

/// Names of who landed the mog and who took it.
struct Hit<'a> {
    attacker: PlayerId,
    target: PlayerId,
    attacker_name: &'a str,
    target_name: &'a str,
}

/// The target picks up the soap: a number on the forehead, a mark for the attacker, and the whole server hears it.
fn soap(world: &mut World, hit: &Hit, now: Instant) {
    let Hit { attacker, target, attacker_name, target_name } = *hit;
    if let Some(s) = world.slot_mut(attacker) {
        s.player.dlc.prison.marks += 1;
        s.mark_dirty();
    }
    let Some(victim) = world.slot_mut(target) else { return };
    victim.player.dlc.prison.forehead += 1;
    victim.mark_dirty();
    let n = victim.player.dlc.prison.forehead;
    world.afflict(target, core(Status::Sabonete), SOAP_DURATION, now);
    world.event(target, Tone::Bad, format!("{attacker_name} te fez pegar o sabonete. Agora você tem {n} na testa."), Some(format!("PEGOU O SABONETE ({n})")), false);
    world.event(attacker, Tone::Good, format!("{target_name} pegou o sabonete pra você: {n} na testa."), None, false);
    world.announce(&[attacker, target], Tone::Info, &format!("{target_name} pegou o sabonete pro {attacker_name} ({n} na testa)."));
}

/// Cenoura's pill: the target sleeps through extra cooldown.
fn pill(world: &mut World, hit: &Hit, now: Instant) {
    let Hit { attacker, target, attacker_name, target_name } = *hit;
    world.add_cooldown(target, PILL_SLEEP, now);
    let secs = PILL_SLEEP.as_secs();
    world.event(target, Tone::Bad, format!("{attacker_name} te deu o Comprimido do Cenoura: você dormiu, +{secs} s de cooldown."), None, false);
    world.event(attacker, Tone::Good, format!("{target_name} tomou o Comprimido e dormiu: +{secs} s de cooldown."), None, false);
}

/// The Black Potion: the target can't mog the attacker for a while.
fn black_potion(world: &mut World, hit: &Hit, now: Instant) {
    let Hit { attacker, target, attacker_name, target_name } = *hit;
    let Some(victim) = world.slot_mut(target) else { return };
    victim.dlc.prison.obeys = attacker;
    world.afflict(target, core(Status::Obedecendo), BLACK_POTION_DURATION, now);
    world.event(target, Tone::Bad, format!("{attacker_name} te fez beber a Poção Negra: você não consegue mogar ele por um tempo."), None, false);
    world.event(attacker, Tone::Good, format!("{target_name} bebeu a Poção Negra e agora te obedece."), None, false);
}

impl Dlc for Prison {
    fn id(&self) -> DlcId {
        DlcId::Prison
    }

    fn name(&self) -> &'static str {
        "Cadeia"
    }

    fn unlock_item(&self) -> ItemId {
        ItemId::PrisonDlc
    }

    fn unlocks(&self) -> &'static str {
        "os itens da Cadeia, o sabonete no PERFECT e os títulos da cela"
    }

    fn items(&self) -> &'static [ItemDef] {
        &ITEMS
    }

    fn achievements(&self) -> &'static [(DlcAchievement, Def)] {
        &ACHIEVEMENTS
    }

    fn use_item(&self, slot: &mut Slot, item: ItemId, now: Instant) -> Option<Result<&'static str, &'static str>> {
        if let Some(a) = Armed::of(item) {
            let (ready, busy) = a.messages();
            let armed = &mut slot.player.dlc.prison.armed;
            if armed.contains(&a) {
                return Some(Err(busy));
            }
            armed.push(a);
            return Some(Ok(ready));
        }
        let (st, duration, ok, busy) = match item {
            ItemId::GreenPotion => (Status::PocaoVerde, GREEN_POTION_DURATION, "Poder x2 por 20 s.", "A Poção Verde ainda está fazendo efeito."),
            ItemId::RedPotion => (Status::PocaoVermelha, RED_POTION_DURATION, "Poder +40% por 2 min.", "A Poção Vermelha ainda está fazendo efeito."),
            _ => return None,
        };
        Some(if slot.set_status(core(st), now + duration, now) { Ok(ok) } else { Err(busy) })
    }

    fn block_mog(&self, world: &World, attacker: PlayerId, target: PlayerId, now: Instant) -> Option<String> {
        let state = &world.slot(attacker)?.dlc.prison;
        let until = state.until[Status::Obedecendo as usize];
        (state.obeys == target && now < until).then(|| {
            let name = world.slot(target).map_or("?", |s| s.player.name.as_str());
            format!("Você está sob a Poção Negra de {name}: não consegue mogar ele por mais {}.", format_wait(until - now))
        })
    }

    fn on_hit(&self, world: &mut World, attacker: PlayerId, target: PlayerId, tier: TimingTier, now: Instant) {
        let (Some(a), Some(t)) = (world.slot(attacker), world.slot(target)) else { return };
        let (attacker_name, target_name) = (a.player.name.clone(), t.player.name.clone());
        let perfect = tier == TimingTier::Perfect && a.player.owns_dlc(DlcId::Prison);
        let Some(slot) = world.slot_mut(attacker) else { return };
        let armed = std::mem::take(&mut slot.player.dlc.prison.armed);
        if !armed.is_empty() {
            slot.mark_dirty();
        }
        let hit = Hit { attacker, target, attacker_name: &attacker_name, target_name: &target_name };
        if perfect || armed.contains(&Armed::Soap) {
            soap(world, &hit, now);
        }
        if armed.contains(&Armed::Pill) {
            pill(world, &hit, now);
        }
        if armed.contains(&Armed::BlackPotion) {
            black_potion(world, &hit, now);
        }
    }

    fn set_status(&self, slot: &mut DlcSlot, status: DlcStatus, until: Instant) -> bool {
        let DlcStatus::Prison(st) = status;
        let current = &mut slot.prison.until[st as usize];
        if until <= *current {
            return false;
        }
        *current = until;
        true
    }

    fn statuses(&self, slot: &DlcSlot, out: &mut Vec<(DlcStatus, Instant)>) {
        out.extend(Status::ALL.map(|st| (DlcStatus::Prison(st), slot.prison.until[st as usize])));
    }

    fn clear_statuses(&self, slot: &mut DlcSlot, now: Instant) {
        slot.prison = SlotState::new(now);
    }

    fn armed(&self, player: &Player) -> Vec<DlcBuff> {
        player.dlc.prison.armed.iter().map(|a| DlcBuff::Prison(*a)).collect()
    }

    fn name_tags(&self, public: &DlcPublic, fresh: &[DlcStatus]) -> Vec<Span<'static>> {
        ui::name_tags(&public.prison, fresh.contains(&DlcStatus::Prison(Status::Sabonete)))
    }

    fn stats_line(&self, player: &Player) -> Option<Line<'static>> {
        Some(ui::stats_line(&player.dlc.prison))
    }

    fn rank_lines(&self, game: &Game) -> Vec<Line<'static>> {
        ui::track_lines(game)
    }

    fn help(&self) -> Vec<String> {
        ui::help()
    }

    fn forehead(&self, public: &DlcPublic) -> Option<(u32, (u8, u8, u8))> {
        Some((public.prison.forehead, ui::FOREHEAD_RGB))
    }

    fn use_fx(&self, item: ItemId) -> Option<UseFx> {
        ui::use_fx(item)
    }

    fn sprite(&self, item: ItemId) -> Option<SpriteAsset> {
        ui::sprite(item)
    }
}
