//! Achievements: what each one asks for and how far a player is from it.

use serde::{Deserialize, Serialize};

use crate::duel::Minigame;
use crate::game::{Player, Rank};
use crate::items::{CATALOG, Category};
use crate::prison::PrisonTitle;

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
    FirstSoap,
    Forehead67,
    FirstMark,
    Marks100,
    ZeroTitle,
    PrisonSet,
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
    Prison,
    Special,
}

impl Group {
    pub const ALL: [Group; 8] = [Group::Mog, Group::Defense, Group::Shop, Group::Mewing, Group::Ranks, Group::Duels, Group::Prison, Group::Special];

    pub fn label(self) -> &'static str {
        match self {
            Group::Mog => "Mog",
            Group::Defense => "Defesa",
            Group::Shop => "Loja e itens",
            Group::Mewing => "Mewing e MP",
            Group::Ranks => "Ranks",
            Group::Duels => "Duelos",
            Group::Prison => "Cadeia",
            Group::Special => "Especiais",
        }
    }
}

/// What unlocks an achievement: a counter reaching a target, or a yes/no check.
enum Goal {
    Count(fn(&Player) -> u64, u64),
    Check(fn(&Player) -> bool),
}

struct Def {
    group: Group,
    name: &'static str,
    desc: &'static str,
    goal: Goal,
}

const fn def(group: Group, name: &'static str, desc: &'static str, goal: Goal) -> Def {
    Def { group, name, desc, goal }
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

fn distinct_items(p: &Player) -> u64 {
    p.items.values().filter(|n| **n > 0).count() as u64
}

fn armed_consumables(p: &Player) -> u64 {
    [p.shield_armed, p.angle_armed, p.mirror_armed, p.tape_armed, p.combo_armed, p.ego_armed, p.soap_armed, p.pill_armed, p.black_armed].into_iter().filter(|b| *b).count() as u64
}

impl Achievement {
    pub const ALL: [Achievement; 72] = {
        use Achievement::*;
        [
            FirstMog, TenMogs, HundredMogs, FiveHundredMogs, ThousandMogs, Mogs6767, Streak5, Streak7, Streak10, Streak20, Streak67,
            FirstPerfect, TwentyPerfects, HundredPerfects, FiveHundredPerfects, Wall, Wall50, Wall200, PunchingBag, PunchingBag50,
            NeverGiveUp, Shopper, Shopper50, Shopper200, Collector, FullCatalog, Maxed, FaceSet, BodySet, DripSet, AuraSet,
            BrainrotSet, Consumer, Pharmacy, Arsenal, ComboArmed, Mews1k, Mews10k, Mews67k, Savings, WealthMog, Millionaire,
            Exactly67, ReachChad, ReachGigachad, Ascend, ReachSigma, ReachAuraLord, ReachSixSeven, ReachMythic, ReachGreekGod,
            ReachCosmic, ReachTranscendent, FirstDuel, Duelist, Gladiator, ArenaChampion, PartOfIt, Flawless, SuddenDeath,
            HighRoller, Rounds50, Versatile, MasterOfAll, FirstSoap, Forehead67, FirstMark, Marks100, ZeroTitle, PrisonSet, Hunter, Platinum,
        ]
    };

    fn def(self) -> Def {
        use Achievement::*;
        use Goal::{Check, Count};
        use Group::*;
        match self {
            FirstMog => def(Mog, "Primeiro Mog", "Moggue alguém pela primeira vez", Count(|p| p.mogs_won.into(), 1)),
            TenMogs => def(Mog, "Mogger em Série", "Vença 10 mogs", Count(|p| p.mogs_won.into(), 10)),
            HundredMogs => def(Mog, "Máquina de Mog", "Vença 100 mogs", Count(|p| p.mogs_won.into(), 100)),
            FiveHundredMogs => def(Mog, "Lenda do Mog", "Vença 500 mogs", Count(|p| p.mogs_won.into(), 500)),
            ThousandMogs => def(Mog, "Mil Moggadas", "Vença 1.000 mogs", Count(|p| p.mogs_won.into(), 1_000)),
            Mogs6767 => def(Mog, "Seis Mil Seiscentos e Sessenta e Sete", "Vença 6.767 mogs", Count(|p| p.mogs_won.into(), 6_767)),
            Streak5 => def(Mog, "Em Chamas", "Streak de 5", Count(|p| p.best_streak.into(), 5)),
            Streak7 => def(Mog, "Seis... Sete!", "Streak de 7 (passando pelo 6)", Count(|p| p.best_streak.into(), 7)),
            Streak10 => def(Mog, "Imparável", "Streak de 10", Count(|p| p.best_streak.into(), 10)),
            Streak20 => def(Mog, "Lenda Viva", "Streak de 20", Count(|p| p.best_streak.into(), 20)),
            Streak67 => def(Mog, "Inmoggável", "Streak de 67", Count(|p| p.best_streak.into(), 67)),
            FirstPerfect => def(Mog, "Ângulo Perfeito", "Acerte um PERFECT", Count(|p| p.perfects.into(), 1)),
            TwentyPerfects => def(Mog, "Timing de Sniper", "Acerte 20 PERFECTs", Count(|p| p.perfects.into(), 20)),
            HundredPerfects => def(Mog, "Olho Biônico", "Acerte 100 PERFECTs", Count(|p| p.perfects.into(), 100)),
            FiveHundredPerfects => def(Mog, "Frame Perfeito", "Acerte 500 PERFECTs", Count(|p| p.perfects.into(), 500)),
            Wall => def(Defense, "Muralha", "Defenda 10 mogs", Count(|p| p.defends_won.into(), 10)),
            Wall50 => def(Defense, "Fortaleza", "Defenda 50 mogs", Count(|p| p.defends_won.into(), 50)),
            Wall200 => def(Defense, "Inabalável", "Defenda 200 mogs", Count(|p| p.defends_won.into(), 200)),
            PunchingBag => def(Defense, "Saco de Pancada", "Seja moggado 10 vezes", Count(|p| p.defends_lost.into(), 10)),
            PunchingBag50 => def(Defense, "Sobrevivente", "Seja moggado 50 vezes", Count(|p| p.defends_lost.into(), 50)),
            NeverGiveUp => def(Defense, "Nunca Desiste", "Leve 50 reverse mogs", Count(|p| p.mogs_lost.into(), 50)),
            Shopper => def(Shop, "Looksmaxxer", "Compre 10 itens", Count(|p| p.purchases.into(), 10)),
            Shopper50 => def(Shop, "Viciado em Looksmax", "Compre 50 itens", Count(|p| p.purchases.into(), 50)),
            Shopper200 => def(Shop, "Cliente VIP", "Compre 200 itens", Count(|p| p.purchases.into(), 200)),
            Collector => def(Shop, "Colecionador", "Tenha 20 itens diferentes", Count(distinct_items, 20)),
            FullCatalog => def(Shop, "Catálogo Completo", "Tenha todos os itens permanentes", Check(|p| owns_set(p, None))),
            Maxed => def(Shop, "No Talo", "Leve um item ao nível máximo", Check(|p| CATALOG.iter().any(|d| !d.effects.is_empty() && p.level(d.id) >= d.max_level))),
            FaceSet => def(Shop, "Rosto Esculpido", "Tenha todos os itens de Rosto", Check(|p| owns_set(p, Some(Category::Face)))),
            BodySet => def(Shop, "Frame Completo", "Tenha todos os itens de Físico", Check(|p| owns_set(p, Some(Category::Body)))),
            DripSet => def(Shop, "Drip Infinito", "Tenha todos os itens de Drip e Status", Check(|p| owns_set(p, Some(Category::Drip)))),
            AuraSet => def(Shop, "Aura Máxima", "Tenha todos os itens de Aura", Check(|p| owns_set(p, Some(Category::Aura)))),
            BrainrotSet => def(Shop, "Brainrot Terminal", "Tenha todos os itens de Brainrot e 67", Check(|p| owns_set(p, Some(Category::Brainrot)))),
            Consumer => def(Shop, "Consumista", "Use 10 consumíveis", Count(|p| p.items_used.into(), 10)),
            Pharmacy => def(Shop, "Farmácia Ambulante", "Use 100 consumíveis", Count(|p| p.items_used.into(), 100)),
            Arsenal => def(Shop, "Arsenal", "Tenha 3 consumíveis armados ao mesmo tempo", Count(armed_consumables, 3)),
            ComboArmed => def(Shop, "Mãozinha 6-7", "Arme o Combo 6-7", Check(|p| p.combo_armed)),
            Mews1k => def(Mewing, "Língua no Céu", "Mewe 1.000 vezes", Count(|p| p.mews, 1_000)),
            Mews10k => def(Mewing, "Mewing Raiz", "Mewe 10.000 vezes", Count(|p| p.mews, 10_000)),
            Mews67k => def(Mewing, "Mandíbula 67", "Mewe 67.000 vezes", Count(|p| p.mews, 67_000)),
            Savings => def(Mewing, "Cofrinho", "Junte 10 mil MP de uma vez", Count(|p| p.mp.max(0.0) as u64, 10_000)),
            WealthMog => def(Mewing, "Wealth Mog", "Junte 100 mil MP de uma vez", Count(|p| p.mp.max(0.0) as u64, 100_000)),
            Millionaire => def(Mewing, "Milionário do Mog", "Junte 1 milhão de MP de uma vez", Count(|p| p.mp.max(0.0) as u64, 1_000_000)),
            Exactly67 => def(Mewing, "Sessenta e Sete", "Fique com exatamente 67 MP", Check(|p| p.mp.floor() == 67.0)),
            ReachChad => def(Ranks, "Virou Chad", "Chegue ao rank Chad", Check(|p| rank_at_least(p, Rank::Chad))),
            ReachGigachad => def(Ranks, "Gigachad de Verdade", "Chegue ao rank Gigachad", Check(|p| rank_at_least(p, Rank::Gigachad))),
            Ascend => def(Ranks, "Ascensão", "Chegue ao rank Ascendido", Check(|p| rank_at_least(p, Rank::Ascended))),
            ReachSigma => def(Ranks, "Lobo Solitário", "Chegue ao rank Sigma", Check(|p| rank_at_least(p, Rank::Sigma))),
            ReachAuraLord => def(Ranks, "Aura Farmada", "Chegue ao rank Lorde da Aura", Check(|p| rank_at_least(p, Rank::AuraLord))),
            ReachSixSeven => def(Ranks, "Six Seven", "Chegue ao rank Six Seven", Check(|p| rank_at_least(p, Rank::SixSeven))),
            ReachMythic => def(Ranks, "Lenda Urbana", "Chegue ao rank Mítico", Check(|p| rank_at_least(p, Rank::Mythic))),
            ReachGreekGod => def(Ranks, "Esculpido em Mármore", "Chegue ao rank Deus Grego", Check(|p| rank_at_least(p, Rank::GreekGod))),
            ReachCosmic => def(Ranks, "Mog Interestelar", "Chegue ao rank Mog Cósmico", Check(|p| rank_at_least(p, Rank::Cosmic))),
            ReachTranscendent => def(Ranks, "Fim do Jogo", "Chegue ao rank Transcendente", Check(|p| rank_at_least(p, Rank::Transcendent))),
            FirstDuel => def(Duels, "Primeiro Sangue", "Vença um duelo PvP valendo MP", Count(|p| p.duels_won.into(), 1)),
            Duelist => def(Duels, "Duelista", "Vença 10 duelos PvP valendo MP", Count(|p| p.duels_won.into(), 10)),
            Gladiator => def(Duels, "Gladiador", "Vença 25 duelos PvP valendo MP", Count(|p| p.duels_won.into(), 25)),
            ArenaChampion => def(Duels, "Campeão da Arena", "Vença 100 duelos PvP valendo MP", Count(|p| p.duels_won.into(), 100)),
            PartOfIt => def(Duels, "Faz Parte", "Perca 10 duelos valendo MP", Count(|p| p.duels_lost.into(), 10)),
            Flawless => def(Duels, "Sem Chance", "Vença um duelo valendo MP sem perder nenhuma rodada", Count(|p| p.flawless_duels.into(), 1)),
            SuddenDeath => def(Duels, "Morte Súbita", "Vença um duelo valendo MP na morte súbita", Count(|p| p.sudden_death_wins.into(), 1)),
            HighRoller => def(Duels, "Tudo ou Nada", "Leve um pote de 10 mil MP num duelo", Count(|p| p.best_duel_pot.max(0.0) as u64, 10_000)),
            Rounds50 => def(Duels, "Rodada a Rodada", "Vença 50 rodadas em duelos valendo MP", Count(|p| p.duel_rounds_won.into(), 50)),
            Versatile => def(Duels, "Polivalente", "Vença rodadas em 9 minigames diferentes", Count(|p| p.minigames_won.len() as u64, 9)),
            MasterOfAll => def(
                Duels,
                "Mestre de Todos",
                "Vença rodadas em todos os minigames",
                Count(|p| p.minigames_won.len() as u64, Minigame::ALL.len() as u64),
            ),
            FirstSoap => def(Prison, "Abaixou", "Pegue o sabonete pela primeira vez", Count(|p| p.forehead.into(), 1)),
            Forehead67 => def(Prison, "Testa 6-7", "Fique com 67 na testa", Count(|p| p.forehead.into(), 67)),
            FirstMark => def(Prison, "Primeiro Numerinho", "Faça alguém pegar o sabonete", Count(|p| p.marks.into(), 1)),
            Marks100 => def(Prison, "Dono da Cela", "Faça 100 pessoas pegarem o sabonete", Count(|p| p.marks.into(), 100)),
            ZeroTitle => def(Prison, "Nunca Se Abaixou", "Vire o Zero: 67 numerinhos marcados sem nenhum na testa", Check(|p| PrisonTitle::of(p.marks, p.forehead) == Some(PrisonTitle::Zero))),
            PrisonSet => def(Prison, "Kit da Cadeia", "Tenha todos os itens permanentes da Cadeia", Check(|p| owns_set(p, Some(Category::Prison)))),
            Hunter => def(Special, "Caçador de Conquistas", "Desbloqueie 30 conquistas", Count(|p| p.achievements.len() as u64, 30)),
            Platinum => def(
                Special,
                "Platina",
                "Desbloqueie todas as outras conquistas",
                Count(|p| p.achievements.len() as u64, Achievement::ALL.len() as u64 - 1),
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
        matches!(self, Achievement::Exactly67 | Achievement::Streak67 | Achievement::Mews67k | Achievement::Mogs6767 | Achievement::Forehead67)
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
