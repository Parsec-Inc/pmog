//! Achievements: what each one asks for and how far a player is from it.

use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

use crate::dlc::{self, DlcAchievement, DlcId};
use crate::duel::Minigame;
use crate::game::{Player, Rank};
use crate::items::{CATALOG, Category};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Achievement {
    FirstMog,
    TenMogs,
    HundredMogs,
    FiveHundredMogs,
    ThousandMogs,
    Mogs6767,
    Streak5,
    Streak7,
    Streak10,
    Streak20,
    Streak67,
    FirstPerfect,
    TwentyPerfects,
    HundredPerfects,
    FiveHundredPerfects,
    Wall,
    Wall50,
    Wall200,
    PunchingBag,
    PunchingBag50,
    NeverGiveUp,
    Shopper,
    Shopper50,
    Shopper200,
    Collector,
    FullCatalog,
    Maxed,
    FaceSet,
    BodySet,
    DripSet,
    AuraSet,
    BrainrotSet,
    Consumer,
    Pharmacy,
    Arsenal,
    ComboArmed,
    Mews1k,
    Mews10k,
    Mews67k,
    Savings,
    WealthMog,
    Millionaire,
    Exactly67,
    ReachChad,
    ReachGigachad,
    Ascend,
    ReachSigma,
    ReachAuraLord,
    ReachSixSeven,
    ReachMythic,
    ReachGreekGod,
    ReachCosmic,
    ReachTranscendent,
    FirstDuel,
    Duelist,
    Gladiator,
    ArenaChampion,
    PartOfIt,
    Flawless,
    SuddenDeath,
    HighRoller,
    Rounds50,
    Versatile,
    MasterOfAll,
    Hunter,
    Platinum,
    /// An achievement from a DLC.
    Dlc(DlcAchievement),
}

/// Section an achievement is listed under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Mog,
    Defense,
    Shop,
    Mewing,
    Ranks,
    Duels,
    Dlc(DlcId),
    Special,
}

impl Group {
    /// Sections in order: the base ones, one per DLC, then the special ones.
    pub fn all() -> &'static [Group] {
        static ALL: LazyLock<Vec<Group>> = LazyLock::new(|| {
            let base = [Group::Mog, Group::Defense, Group::Shop, Group::Mewing, Group::Ranks, Group::Duels];
            base.into_iter().chain(dlc::ALL.iter().map(|d| Group::Dlc(d.id()))).chain([Group::Special]).collect()
        });
        &ALL
    }

    pub fn label(self) -> &'static str {
        match self {
            Group::Mog => "Mog",
            Group::Defense => "Defesa",
            Group::Shop => "Loja e itens",
            Group::Mewing => "Mewing e MP",
            Group::Ranks => "Ranks",
            Group::Duels => "Duelos",
            Group::Dlc(id) => dlc::get(id).name(),
            Group::Special => "Especiais",
        }
    }
}

/// What unlocks an achievement: a counter reaching a target, or a yes/no check.
#[derive(Clone, Copy)]
pub enum Goal {
    Count(fn(&Player) -> u64, u64),
    Check(fn(&Player) -> bool),
}

#[derive(Clone, Copy)]
pub struct Def {
    group: Group,
    name: &'static str,
    desc: &'static str,
    goal: Goal,
    secret: bool,
}

impl Def {
    pub const fn new(group: Group, name: &'static str, desc: &'static str, goal: Goal) -> Def {
        Def { group, name, desc, goal, secret: false }
    }

    /// Hidden until unlocked.
    pub const fn secret(self) -> Def {
        Def { secret: true, ..self }
    }
}


fn rank_at_least(p: &Player, rank: Rank) -> bool {
    p.rank() >= rank
}

/// Owns at least one level of every permanent item in `category`, or of all of them with `None`.
fn owns_set(p: &Player, category: Option<Category>) -> bool {
    CATALOG
        .iter()
        .filter(|d| !d.consumable() && category.is_none_or(|c| d.category() == c))
        .all(|d| p.level(d.id) > 0)
}

/// Owns at least one level of every permanent item of the DLC `id`.
pub fn owns_dlc_set(p: &Player, id: DlcId) -> bool {
    owns_set(p, Some(Category::Dlc(id)))
}

fn distinct_items(p: &Player) -> u64 {
    p.items.values().filter(|n| **n > 0).count() as u64
}

fn armed_consumables(p: &Player) -> u64 {
    let base = [p.shield_armed, p.angle_armed, p.mirror_armed, p.tape_armed, p.combo_armed, p.ego_armed].into_iter().filter(|b| *b).count();
    (base + dlc::ALL.iter().map(|d| d.armed(p).len()).sum::<usize>()) as u64
}

impl Achievement {
    const BASE: [Achievement; 66] = {
        use Achievement::*;
        [
            FirstMog, TenMogs, HundredMogs, FiveHundredMogs, ThousandMogs, Mogs6767, Streak5, Streak7, Streak10, Streak20, Streak67,
            FirstPerfect, TwentyPerfects, HundredPerfects, FiveHundredPerfects, Wall, Wall50, Wall200, PunchingBag, PunchingBag50,
            NeverGiveUp, Shopper, Shopper50, Shopper200, Collector, FullCatalog, Maxed, FaceSet, BodySet, DripSet, AuraSet,
            BrainrotSet, Consumer, Pharmacy, Arsenal, ComboArmed, Mews1k, Mews10k, Mews67k, Savings, WealthMog, Millionaire,
            Exactly67, ReachChad, ReachGigachad, Ascend, ReachSigma, ReachAuraLord, ReachSixSeven, ReachMythic, ReachGreekGod,
            ReachCosmic, ReachTranscendent, FirstDuel, Duelist, Gladiator, ArenaChampion, PartOfIt, Flawless, SuddenDeath,
            HighRoller, Rounds50, Versatile, MasterOfAll, Hunter, Platinum,
        ]
    };

    /// Every achievement: the base game's, then each DLC's.
    pub fn all() -> &'static [Achievement] {
        static ALL: LazyLock<Vec<Achievement>> = LazyLock::new(|| {
            let dlcs = dlc::ALL.iter().flat_map(|d| d.achievements()).map(|(a, _)| Achievement::Dlc(*a));
            Achievement::BASE.into_iter().chain(dlcs).collect()
        });
        &ALL
    }

    fn def(self) -> Def {
        use Achievement::*;
        use Goal::{Check, Count};
        use Group::*;
        match self {
            FirstMog => Def::new(Mog, "Primeiro Mog", "Moggue alguém pela primeira vez", Count(|p| p.mogs_won.into(), 1)),
            TenMogs => Def::new(Mog, "Mogger em Série", "Vença 10 mogs", Count(|p| p.mogs_won.into(), 10)),
            HundredMogs => Def::new(Mog, "Máquina de Mog", "Vença 100 mogs", Count(|p| p.mogs_won.into(), 100)),
            FiveHundredMogs => Def::new(Mog, "Lenda do Mog", "Vença 500 mogs", Count(|p| p.mogs_won.into(), 500)),
            ThousandMogs => Def::new(Mog, "Mil Moggadas", "Vença 1.000 mogs", Count(|p| p.mogs_won.into(), 1_000)),
            Mogs6767 => Def::new(Mog, "Seis Mil Seiscentos e Sessenta e Sete", "Vença 6.767 mogs", Count(|p| p.mogs_won.into(), 6_767)).secret(),
            Streak5 => Def::new(Mog, "Em Chamas", "Streak de 5", Count(|p| p.best_streak.into(), 5)),
            Streak7 => Def::new(Mog, "Seis... Sete!", "Streak de 7 (passando pelo 6)", Count(|p| p.best_streak.into(), 7)),
            Streak10 => Def::new(Mog, "Imparável", "Streak de 10", Count(|p| p.best_streak.into(), 10)),
            Streak20 => Def::new(Mog, "Lenda Viva", "Streak de 20", Count(|p| p.best_streak.into(), 20)),
            Streak67 => Def::new(Mog, "Inmoggável", "Streak de 67", Count(|p| p.best_streak.into(), 67)).secret(),
            FirstPerfect => Def::new(Mog, "Ângulo Perfeito", "Acerte um PERFECT", Count(|p| p.perfects.into(), 1)),
            TwentyPerfects => Def::new(Mog, "Timing de Sniper", "Acerte 20 PERFECTs", Count(|p| p.perfects.into(), 20)),
            HundredPerfects => Def::new(Mog, "Olho Biônico", "Acerte 100 PERFECTs", Count(|p| p.perfects.into(), 100)),
            FiveHundredPerfects => Def::new(Mog, "Frame Perfeito", "Acerte 500 PERFECTs", Count(|p| p.perfects.into(), 500)),
            Wall => Def::new(Defense, "Muralha", "Defenda 10 mogs", Count(|p| p.defends_won.into(), 10)),
            Wall50 => Def::new(Defense, "Fortaleza", "Defenda 50 mogs", Count(|p| p.defends_won.into(), 50)),
            Wall200 => Def::new(Defense, "Inabalável", "Defenda 200 mogs", Count(|p| p.defends_won.into(), 200)),
            PunchingBag => Def::new(Defense, "Saco de Pancada", "Seja moggado 10 vezes", Count(|p| p.defends_lost.into(), 10)),
            PunchingBag50 => Def::new(Defense, "Sobrevivente", "Seja moggado 50 vezes", Count(|p| p.defends_lost.into(), 50)),
            NeverGiveUp => Def::new(Defense, "Nunca Desiste", "Leve 50 reverse mogs", Count(|p| p.mogs_lost.into(), 50)),
            Shopper => Def::new(Shop, "Looksmaxxer", "Compre 10 itens", Count(|p| p.purchases.into(), 10)),
            Shopper50 => Def::new(Shop, "Viciado em Looksmax", "Compre 50 itens", Count(|p| p.purchases.into(), 50)),
            Shopper200 => Def::new(Shop, "Cliente VIP", "Compre 200 itens", Count(|p| p.purchases.into(), 200)),
            Collector => Def::new(Shop, "Colecionador", "Tenha 20 itens diferentes", Count(distinct_items, 20)),
            FullCatalog => Def::new(Shop, "Catálogo Completo", "Tenha todos os itens permanentes", Check(|p| owns_set(p, None))),
            Maxed => Def::new(Shop, "No Talo", "Leve um item ao nível máximo", Check(|p| CATALOG.iter().any(|d| !d.consumable() && !d.is_unlock() && p.level(d.id) >= d.max_level))),
            FaceSet => Def::new(Shop, "Rosto Esculpido", "Tenha todos os itens de Rosto", Check(|p| owns_set(p, Some(Category::Face)))),
            BodySet => Def::new(Shop, "Frame Completo", "Tenha todos os itens de Físico", Check(|p| owns_set(p, Some(Category::Body)))),
            DripSet => Def::new(Shop, "Drip Infinito", "Tenha todos os itens de Drip e Status", Check(|p| owns_set(p, Some(Category::Drip)))),
            AuraSet => Def::new(Shop, "Aura Máxima", "Tenha todos os itens de Aura", Check(|p| owns_set(p, Some(Category::Aura)))),
            BrainrotSet => Def::new(Shop, "Brainrot Terminal", "Tenha todos os itens de Brainrot e 67", Check(|p| owns_set(p, Some(Category::Brainrot)))),
            Consumer => Def::new(Shop, "Consumista", "Use 10 consumíveis", Count(|p| p.items_used.into(), 10)),
            Pharmacy => Def::new(Shop, "Farmácia Ambulante", "Use 100 consumíveis", Count(|p| p.items_used.into(), 100)),
            Arsenal => Def::new(Shop, "Arsenal", "Tenha 3 consumíveis armados ao mesmo tempo", Count(armed_consumables, 3)),
            ComboArmed => Def::new(Shop, "Mãozinha 6-7", "Arme o Combo 6-7", Check(|p| p.combo_armed)),
            Mews1k => Def::new(Mewing, "Língua no Céu", "Mewe 1.000 vezes", Count(|p| p.mews, 1_000)),
            Mews10k => Def::new(Mewing, "Mewing Raiz", "Mewe 10.000 vezes", Count(|p| p.mews, 10_000)),
            Mews67k => Def::new(Mewing, "Mandíbula 67", "Mewe 67.000 vezes", Count(|p| p.mews, 67_000)).secret(),
            Savings => Def::new(Mewing, "Cofrinho", "Junte 10 mil MP de uma vez", Count(|p| p.mp.max(0.0) as u64, 10_000)),
            WealthMog => Def::new(Mewing, "Wealth Mog", "Junte 100 mil MP de uma vez", Count(|p| p.mp.max(0.0) as u64, 100_000)),
            Millionaire => Def::new(Mewing, "Milionário do Mog", "Junte 1 milhão de MP de uma vez", Count(|p| p.mp.max(0.0) as u64, 1_000_000)),
            Exactly67 => Def::new(Mewing, "Sessenta e Sete", "Fique com exatamente 67 MP", Check(|p| p.mp.floor() == 67.0)).secret(),
            ReachChad => Def::new(Ranks, "Virou Chad", "Chegue ao rank Chad", Check(|p| rank_at_least(p, Rank::Chad))),
            ReachGigachad => Def::new(Ranks, "Gigachad de Verdade", "Chegue ao rank Gigachad", Check(|p| rank_at_least(p, Rank::Gigachad))),
            Ascend => Def::new(Ranks, "Ascensão", "Chegue ao rank Ascendido", Check(|p| rank_at_least(p, Rank::Ascended))),
            ReachSigma => Def::new(Ranks, "Lobo Solitário", "Chegue ao rank Sigma", Check(|p| rank_at_least(p, Rank::Sigma))),
            ReachAuraLord => Def::new(Ranks, "Aura Farmada", "Chegue ao rank Lorde da Aura", Check(|p| rank_at_least(p, Rank::AuraLord))),
            ReachSixSeven => Def::new(Ranks, "Six Seven", "Chegue ao rank Six Seven", Check(|p| rank_at_least(p, Rank::SixSeven))),
            ReachMythic => Def::new(Ranks, "Lenda Urbana", "Chegue ao rank Mítico", Check(|p| rank_at_least(p, Rank::Mythic))),
            ReachGreekGod => Def::new(Ranks, "Esculpido em Mármore", "Chegue ao rank Deus Grego", Check(|p| rank_at_least(p, Rank::GreekGod))),
            ReachCosmic => Def::new(Ranks, "Mog Interestelar", "Chegue ao rank Mog Cósmico", Check(|p| rank_at_least(p, Rank::Cosmic))),
            ReachTranscendent => Def::new(Ranks, "Fim do Jogo", "Chegue ao rank Transcendente", Check(|p| rank_at_least(p, Rank::Transcendent))),
            FirstDuel => Def::new(Duels, "Primeiro Sangue", "Vença um duelo PvP valendo MP", Count(|p| p.duels_won.into(), 1)),
            Duelist => Def::new(Duels, "Duelista", "Vença 10 duelos PvP valendo MP", Count(|p| p.duels_won.into(), 10)),
            Gladiator => Def::new(Duels, "Gladiador", "Vença 25 duelos PvP valendo MP", Count(|p| p.duels_won.into(), 25)),
            ArenaChampion => Def::new(Duels, "Campeão da Arena", "Vença 100 duelos PvP valendo MP", Count(|p| p.duels_won.into(), 100)),
            PartOfIt => Def::new(Duels, "Faz Parte", "Perca 10 duelos valendo MP", Count(|p| p.duels_lost.into(), 10)),
            Flawless => Def::new(Duels, "Sem Chance", "Vença um duelo valendo MP sem perder nenhuma rodada", Count(|p| p.flawless_duels.into(), 1)),
            SuddenDeath => Def::new(Duels, "Morte Súbita", "Vença um duelo valendo MP na morte súbita", Count(|p| p.sudden_death_wins.into(), 1)),
            HighRoller => Def::new(Duels, "Tudo ou Nada", "Leve um pote de 10 mil MP num duelo", Count(|p| p.best_duel_pot.max(0.0) as u64, 10_000)),
            Rounds50 => Def::new(Duels, "Rodada a Rodada", "Vença 50 rodadas em duelos valendo MP", Count(|p| p.duel_rounds_won.into(), 50)),
            Versatile => Def::new(Duels, "Polivalente", "Vença rodadas em 9 minigames diferentes", Count(|p| p.minigames_won.len() as u64, 9)),
            MasterOfAll => Def::new(
                Duels,
                "Mestre de Todos",
                "Vença rodadas em todos os minigames",
                Count(|p| p.minigames_won.len() as u64, Minigame::ALL.len() as u64),
            ),
            Achievement::Dlc(a) => a.def(),
            Hunter => Def::new(Special, "Caçador de Conquistas", "Desbloqueie 30 conquistas", Count(|p| p.achievements.len() as u64, 30)),
            Platinum => Def::new(
                Special,
                "Platina",
                "Desbloqueie todas as outras conquistas",
                Count(|p| p.achievements.len() as u64, Achievement::all().len() as u64 - 1),
            ),
        }
    }

    pub fn name(self) -> &'static str {
        self.def().name
    }

    pub fn desc(self) -> &'static str {
        self.def().desc
    }

    pub fn group(self) -> Group {
        self.def().group
    }

    /// Hidden until unlocked.
    pub fn secret(self) -> bool {
        self.def().secret
    }

    /// Progress as (current, target), capped at the target; `None` for yes/no goals.
    pub fn progress(self, p: &Player) -> Option<(u64, u64)> {
        match self.def().goal {
            Goal::Count(count, target) => Some((count(p).min(target), target)),
            Goal::Check(_) => None,
        }
    }

    pub fn reached(self, p: &Player) -> bool {
        match self.def().goal {
            Goal::Count(count, target) => count(p) >= target,
            Goal::Check(check) => check(p),
        }
    }
}
