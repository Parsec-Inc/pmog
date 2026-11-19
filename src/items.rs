//! Shop catalog: every item, its per-level effects and how to describe them.

use serde::{Deserialize, Serialize};

use crate::constants::{AURA_COST_GROWTH, COST_GROWTH};
use crate::game::Rank;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ItemId {
    Mewing,
    GuaSha,
    WaterBottle,
    MidFade,
    GymPlan,
    Skincare,
    Insoles,
    RingLight,
    Sunglasses,
    Therapy,
    PhonkTrack,
    LuxuryWatch,
    Meditation,
    DiamondJaw,
    PrivateJet,
    InfiniteAura,
    MogCrown,
    EnergyDrink,
    SigmaShield,
    AngleShot,
    PreWorkout,
    PocketMirror,
    Sunscreen,
    Creatine,
    MasticGum,
    NichePerfume,
    Veneers,
    Photographer,
    CanthalTape,
    Mansion,
    BlessedGenes,
    JawTape,
    Cologne,
    IcePack,
    Minoxidil,
    Dermaroller,
    FoxEyes,
    ShoulderPads,
    SleepKit,
    Whey,
    BoatDance,
    DoorLean,
    Jestermaxxing,
    SixSeven,
    SixSevenHeight,
    Capoeira,
    SharkSneakers,
    SahurBat,
    CrocBomber,
    BananaMonkey,
    ComboSixSeven,
    AuraFarm,
    WolfHood,
    AuraScepter,
    Trophy67,
    PhoenixFeather,
    MarbleBust,
    PocketMeteor,
    ThirdEye,
    Underdog,
    WoundedEgo,
    PrisonDlc,
    PullUpBar,
    Crutch,
    CarrotHook,
    MilEar,
    Folder77,
    PeraUniform,
    TowerKey,
    GreenPotion,
    RedPotion,
    BlackPotion,
    Soap,
    CarrotPill,
}

/// Shop tab an item belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Face,
    Body,
    Drip,
    Aura,
    Brainrot,
    Prison,
    Consumable,
}

impl Category {
    pub const ALL: [Category; 7] =
        [Category::Face, Category::Body, Category::Drip, Category::Aura, Category::Brainrot, Category::Prison, Category::Consumable];

    pub fn label(self) -> &'static str {
        match self {
            Category::Face => "Rosto",
            Category::Body => "Físico",
            Category::Drip => "Drip e Status",
            Category::Aura => "Aura",
            Category::Brainrot => "Brainrot e 67",
            Category::Prison => "Cadeia",
            Category::Consumable => "Consumíveis",
        }
    }

    /// Catalog indexes of the items in this tab, in catalog order.
    pub fn items(self) -> impl Iterator<Item = usize> {
        CATALOG.iter().enumerate().filter(move |(_, d)| d.category() == self).map(|(i, _)| i)
    }
}

/// What one level of a permanent item adds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Effect {
    Attack(f64),
    Defense(f64),
    Passive(f64),
    MewGain(f64),
    MewCap(u32),
    WinBonus(f64),
    LossShield(f64),
    PerfectZone(f64),
    CooldownCut(f64),
    StreakBonus(f64),
    ImmunityBonus(f64),
    CounterBonus(f64),
    MeterSlow(f64),
    Aura(f64),
    Underdog(f64),
    Resilience(f64),
}

impl Effect {
    /// Short pt-BR text for one level of this effect.
    pub fn describe(self) -> String {
        match self {
            Effect::Attack(v) => format!("+{} de poder", num(v)),
            Effect::Defense(v) => format!("+{} de defesa", num(v)),
            Effect::Passive(v) => format!("+{} MP/s passivo", num(v)),
            Effect::MewGain(v) => format!("+{} MP por mewada", num(v)),
            Effect::MewCap(v) => format!("+{v} mewada por segundo no limite"),
            Effect::WinBonus(v) => format!("+{}% de MP nas vitórias", pct(v)),
            Effect::LossShield(v) => format!("-{}% de MP perdido", pct(v)),
            Effect::PerfectZone(v) => format!("zona PERFECT {} pontos maior", pct(v)),
            Effect::CooldownCut(v) => format!("-{} s de cooldown", num(v)),
            Effect::StreakBonus(v) => format!("+{}% de poder por streak", pct(v)),
            Effect::ImmunityBonus(v) => format!("+{} s de imunidade", num(v)),
            Effect::CounterBonus(v) => format!("+{}% do que o atacante perde, quando você segura o mog", pct(v)),
            Effect::MeterSlow(v) => format!("cursor {}% mais lento", pct(v)),
            Effect::Aura(v) => format!("+{}% em poder, defesa e MP", pct(v)),
            Effect::Resilience(v) => format!("status ruins duram {}% menos", pct(v)),
            Effect::Underdog(v) => format!("+{}% de crítico a cada vez que a defesa do alvo dobra sobre seu poder", pct(v)),
        }
    }
}

pub struct ItemDef {
    pub id: ItemId,
    pub name: &'static str,
    /// In-world flavor line.
    pub flavor: &'static str,
    pub min_rank: Rank,
    pub base_cost: f64,
    pub max_level: u32,
    /// Empty for consumables.
    pub effects: &'static [Effect],
    /// Key and what happens on use; `None` for permanent items.
    pub usage: Option<(char, &'static str)>,
}

impl ItemDef {
    pub fn consumable(&self) -> bool {
        self.usage.is_some()
    }

    pub fn category(&self) -> Category {
        use ItemId::*;
        if self.consumable() && !matches!(self.id, GreenPotion | RedPotion | BlackPotion | Soap | CarrotPill) {
            return Category::Consumable;
        }
        match self.id {
            Mewing | Sunscreen | GuaSha | Skincare | Veneers | CanthalTape | DiamondJaw | Minoxidil | Dermaroller | FoxEyes | MarbleBust => {
                Category::Face
            }
            WaterBottle | Creatine | MasticGum | GymPlan | Insoles | ShoulderPads | SleepKit | Whey => Category::Body,
            MidFade | RingLight | NichePerfume | Sunglasses | Photographer | LuxuryWatch | PrivateJet | Mansion | MogCrown | WolfHood | PocketMeteor => {
                Category::Drip
            }
            Therapy | PhonkTrack | Meditation | InfiniteAura | BlessedGenes | BoatDance | DoorLean | Jestermaxxing | AuraScepter | Underdog | PhoenixFeather
            | ThirdEye => Category::Aura,
            SixSeven | SixSevenHeight | Capoeira | SharkSneakers | SahurBat | CrocBomber | BananaMonkey | Trophy67 => Category::Brainrot,
            PrisonDlc | PullUpBar | Crutch | CarrotHook | MilEar | Folder77 | PeraUniform | TowerKey | GreenPotion | RedPotion | BlackPotion | Soap | CarrotPill => {
                Category::Prison
            }
            EnergyDrink | SigmaShield | AngleShot | PreWorkout | PocketMirror | JawTape | Cologne | IcePack | ComboSixSeven | AuraFarm | WoundedEgo => {
                Category::Consumable
            }
        }
    }

    /// Price of the next unit or level.
    pub fn cost(&self, level: u32) -> f64 {
        if self.consumable() {
            self.base_cost
        } else {
            self.base_cost * self.cost_growth().powi(level as i32)
        }
    }

    /// How much each level costs over the previous one.
    pub fn cost_growth(&self) -> f64 {
        if self.id == ItemId::InfiniteAura { AURA_COST_GROWTH } else { COST_GROWTH }
    }

    /// Why `player` can't buy the next unit or level right now, if anything stops it.
    pub fn purchase_block(&self, player: &crate::game::Player) -> Option<String> {
        let level = player.level(self.id);
        if self.category() == Category::Prison && self.id != ItemId::PrisonDlc && !player.has_prison_dlc() {
            Some(format!("{} faz parte da DLC Cadeia: compre a DLC primeiro.", self.name))
        } else if player.rank() < self.min_rank {
            Some(format!("{} exige rank {}.", self.name, self.min_rank.label()))
        } else if level >= self.max_level {
            Some(format!("{} já está no máximo.", self.name))
        } else if player.mp < self.cost(level) {
            Some(format!("Faltam {} MP para {}.", crate::utils::format_mp(self.cost(level) - player.mp), self.name))
        } else {
            None
        }
    }

    /// One-line summary of what the item does.
    pub fn summary(&self) -> String {
        if self.id == ItemId::PrisonDlc {
            return DLC_SUMMARY.into();
        }
        match self.usage {
            Some((key, text)) => format!("[{key}] {text}"),
            None => self.effects.iter().map(|e| e.describe()).collect::<Vec<_>>().join(", "),
        }
    }
}

const DLC_SUMMARY: &str = "libera os itens da Cadeia, o sabonete no PERFECT e os títulos da cela";

pub fn def(id: ItemId) -> &'static ItemDef {
    CATALOG.iter().find(|d| d.id == id).expect("every ItemId is in the catalog")
}

/// Consumable bound to a number key.
pub fn consumable_for_key(key: char) -> Option<ItemId> {
    CATALOG.iter().find(|d| d.usage.is_some_and(|(k, _)| k == key)).map(|d| d.id)
}

fn num(v: f64) -> String {
    let text = if v.fract() == 0.0 { format!("{v:.0}") } else { format!("{v}") };
    text.replace('.', ",")
}

fn pct(v: f64) -> String {
    num((v * 100.0 * 100.0).round() / 100.0)
}

const fn permanent(
    id: ItemId,
    name: &'static str,
    flavor: &'static str,
    min_rank: Rank,
    base_cost: f64,
    max_level: u32,
    effects: &'static [Effect],
) -> ItemDef {
    ItemDef { id, name, flavor, min_rank, base_cost, max_level, effects, usage: None }
}

const fn consumable(
    id: ItemId,
    name: &'static str,
    flavor: &'static str,
    min_rank: Rank,
    base_cost: f64,
    key: char,
    text: &'static str,
) -> ItemDef {
    ItemDef { id, name, flavor, min_rank, base_cost, max_level: 99, effects: &[], usage: Some((key, text)) }
}

pub const CATALOG: [ItemDef; 74] = [
    permanent(ItemId::Mewing, "Tutorial de Mewing", "Língua no céu da boca, 24 horas por dia.", Rank::Sub5, 25.0, 50,
        &[Effect::Passive(0.5), Effect::MewGain(0.25)]),
    permanent(ItemId::Sunscreen, "Protetor Solar FPS 70", "Envelhecer é para quem foi moggado.", Rank::Sub5, 45.0, 20,
        &[Effect::Defense(2.0), Effect::Passive(0.25)]),
    permanent(ItemId::GuaSha, "Gua Sha", "Pedra de jade para massagear e desinchar o rosto.", Rank::Sub5, 30.0, 50,
        &[Effect::Defense(4.0)]),
    permanent(ItemId::WaterBottle, "Garrafa de 2 Litros", "Hidratação é o primeiro passo do looksmaxxing.", Rank::Sub5, 60.0, 5,
        &[Effect::MewCap(1), Effect::MewGain(0.25)]),
    permanent(ItemId::MidFade, "Corte Mid-Fade", "O degradê que todo mogger respeita.", Rank::Ltn, 120.0, 50,
        &[Effect::Attack(5.0)]),
    permanent(ItemId::Creatine, "Creatina", "Cinco gramas por dia, sem pular nenhum.", Rank::Ltn, 200.0, 30,
        &[Effect::Attack(3.0), Effect::MewGain(0.5)]),
    permanent(ItemId::MasticGum, "Chiclete de Mastique", "Mandíbula de pedra, mastigando o dia inteiro.", Rank::Ltn, 150.0, 5,
        &[Effect::MewCap(1)]),
    permanent(ItemId::GymPlan, "Plano da Academia", "Frame mog começa no supino.", Rank::Ltn, 250.0, 50,
        &[Effect::Attack(4.0), Effect::Defense(4.0)]),
    permanent(ItemId::Skincare, "Skincare Coreana", "Dez passos, pele de vidro.", Rank::Mtn, 500.0, 50,
        &[Effect::Defense(8.0), Effect::Passive(1.0)]),
    permanent(ItemId::Insoles, "Palmilha de 7 cm", "Height mog instantâneo, ninguém precisa saber.", Rank::Mtn, 650.0, 50,
        &[Effect::Attack(12.0)]),
    permanent(ItemId::RingLight, "Ring Light", "Iluminação de estúdio: cada vitória rende mais.", Rank::Mtn, 900.0, 5,
        &[Effect::WinBonus(0.10)]),
    permanent(ItemId::NichePerfume, "Perfume Nicho", "Deixa rastro: quem chega perto já se sente moggado.", Rank::Mtn, 1_100.0, 10,
        &[Effect::WinBonus(0.05), Effect::CounterBonus(0.03)]),
    permanent(ItemId::Sunglasses, "Óculos Escuros", "Ninguém vê seus olhos hesitando.", Rank::Htn, 2_000.0, 5,
        &[Effect::PerfectZone(0.006)]),
    permanent(ItemId::Therapy, "Sessão de Terapia", "Ser moggado dói menos quando você se conhece.", Rank::Htn, 2_500.0, 5,
        &[Effect::LossShield(0.05), Effect::Resilience(0.15)]),
    permanent(ItemId::Veneers, "Lentes de Porcelana", "Um sorriso que ofusca de longe.", Rank::Htn, 3_000.0, 10,
        &[Effect::Attack(8.0), Effect::StreakBonus(0.03)]),
    permanent(ItemId::PhonkTrack, "Trilha Phonk", "Toca sozinha quando você entra no ambiente.", Rank::Chadlite, 6_000.0, 10,
        &[Effect::CooldownCut(0.3), Effect::StreakBonus(0.05)]),
    permanent(ItemId::LuxuryWatch, "Relógio de Luxo", "Pulso pesado, postura de quem não se abala.", Rank::Chadlite, 8_000.0, 5,
        &[Effect::ImmunityBonus(1.0), Effect::CounterBonus(0.06)]),
    permanent(ItemId::Photographer, "Fotógrafo Particular", "Ele grita AGORA no momento exato.", Rank::Chadlite, 10_000.0, 5,
        &[Effect::PerfectZone(0.006), Effect::MeterSlow(0.02)]),
    permanent(ItemId::Meditation, "Meditação Sigma", "O tempo desacelera para quem está em paz.", Rank::Chad, 15_000.0, 6,
        &[Effect::MeterSlow(0.035)]),
    permanent(ItemId::DiamondJaw, "Maxilar de Diamante", "Corta vidro. Corta egos.", Rank::Chad, 20_000.0, 50,
        &[Effect::Attack(30.0), Effect::Defense(30.0)]),
    permanent(ItemId::CanthalTape, "Fita de Canthal Tilt", "Olhar de caçador até dormindo.", Rank::Chad, 25_000.0, 10,
        &[Effect::ImmunityBonus(1.0), Effect::LossShield(0.05)]),
    permanent(ItemId::PrivateJet, "Jatinho Particular", "Wealth mog: o MP entra enquanto você voa.", Rank::Gigachad, 60_000.0, 20,
        &[Effect::Passive(20.0)]),
    permanent(ItemId::InfiniteAura, "Aura Infinita", "Multiplica tudo o que você já é.", Rank::Gigachad, 80_000.0, 10,
        &[Effect::Aura(0.15)]),
    permanent(ItemId::Mansion, "Mansão com Academia", "Supino com vista para o mar.", Rank::Gigachad, 120_000.0, 10,
        &[Effect::Passive(30.0), Effect::Defense(50.0)]),
    permanent(ItemId::MogCrown, "Coroa do Mog", "Só existe uma. Ou cinco.", Rank::Ascended, 250_000.0, 5,
        &[Effect::Attack(100.0), Effect::Defense(100.0), Effect::Passive(50.0)]),
    permanent(ItemId::BlessedGenes, "Genética Abençoada", "Ganhou na loteria do DNA.", Rank::Ascended, 400_000.0, 3,
        &[Effect::Aura(0.3)]),
    consumable(ItemId::EnergyDrink, "Energético", "Cafeína suficiente para mogar de novo agora.", Rank::Ltn, 60.0, '1',
        "zera o cooldown na hora"),
    consumable(ItemId::SigmaShield, "Escudo Sigma", "Fica ativo até alguém tentar te moggar.", Rank::Mtn, 300.0, '2',
        "bloqueia por completo o próximo mog recebido"),
    consumable(ItemId::AngleShot, "Foto no Ângulo", "O ângulo certo muda tudo.", Rank::Htn, 1_200.0, '3',
        "o próximo mog sai com poder x2"),
    consumable(ItemId::PreWorkout, "Pré-Treino", "Formigamento na nuca, foco total.", Rank::Htn, 900.0, '4',
        "por 30 s, o cooldown cai pela metade"),
    consumable(ItemId::PocketMirror, "Espelho de Bolso", "Conferiu o ângulo antes de te atacarem.", Rank::Chadlite, 2_500.0, '5',
        "a próxima defesa conta com defesa x3"),
    consumable(ItemId::JawTape, "Fita de Mandíbula", "Segura a pose mesmo quando dá errado.", Rank::Mtn, 450.0, '6',
        "se o próximo mog der errado, você não perde MP nem o streak"),
    consumable(ItemId::Cologne, "Colônia de Feromônio", "Uma borrifada e todo mundo te nota.", Rank::Htn, 1_500.0, '7',
        "por 60 s, seus mogs rendem +50% de MP"),
    consumable(ItemId::IcePack, "Bolsa de Gelo", "Desincha o rosto depois de uma surra.", Rank::Chadlite, 3_500.0, '8',
        "imune por 20 s agora e zera a imunidade acumulada"),
    permanent(ItemId::Minoxidil, "Minoxidil na Barba", "Barba cheia em 6 meses, ou 7.", Rank::Mtn, 700.0, 20,
        &[Effect::Defense(6.0), Effect::Attack(4.0)]),
    permanent(ItemId::Dermaroller, "Dermaroller", "Mil agulhinhas a favor do seu colágeno.", Rank::Htn, 2_800.0, 10,
        &[Effect::Defense(15.0), Effect::LossShield(0.02)]),
    permanent(ItemId::FoxEyes, "Gel Fox Eyes", "Sobrancelha puxada, olhar de quem já ganhou.", Rank::Chadlite, 7_000.0, 10,
        &[Effect::Attack(18.0), Effect::StreakBonus(0.04)]),
    permanent(ItemId::ShoulderPads, "Ombreira de Frame Mog", "Brutalmente frame-moggado: agora é você que mogga.", Rank::Mtn, 800.0, 20,
        &[Effect::Attack(6.0), Effect::Defense(6.0)]),
    permanent(ItemId::SleepKit, "Kit Sleepmaxxing", "Máscara, fita na boca e 9 horas de sono.", Rank::Ltn, 180.0, 20,
        &[Effect::Passive(1.0), Effect::CooldownCut(0.05)]),
    permanent(ItemId::Whey, "Whey Isolado", "Proteinmaxxing: 2 g por quilo, sem desculpa.", Rank::Ltn, 220.0, 30,
        &[Effect::Attack(2.0), Effect::MewGain(0.5)]),
    permanent(ItemId::BoatDance, "Dança na Proa", "Aura farming de óculos escuros, no bico do barco.", Rank::Htn, 2_200.0, 10,
        &[Effect::WinBonus(0.05), Effect::CounterBonus(0.05)]),
    permanent(ItemId::DoorLean, "Pose Encostado na Porta", "Braço cruzado, olhar no horizonte, nada a provar.", Rank::Mtn, 1_000.0, 10,
        &[Effect::StreakBonus(0.02), Effect::Defense(5.0)]),
    permanent(ItemId::Jestermaxxing, "Jestermaxxing", "Fazer o grupo rir também é mogar.", Rank::Ltn, 300.0, 10,
        &[Effect::CounterBonus(0.02), Effect::Passive(0.5)]),
    permanent(ItemId::SixSeven, "Gesto do 6-7", "Mãos na balança: seis, sete. Ninguém sabe o que significa.", Rank::Ltn, 67.0, 67,
        &[Effect::Attack(0.6), Effect::Defense(0.7)]),
    permanent(ItemId::SixSevenHeight, "Altura de 6'7\"", "Dois metros e um centímetro de height mog.", Rank::Chadlite, 6_767.0, 7,
        &[Effect::Attack(20.0), Effect::Defense(7.0)]),
    permanent(ItemId::Capoeira, "Capoeira Proibida", "Eu sabendo capoeira proibida ☠️ (toca phonk).", Rank::Htn, 3_300.0, 10,
        &[Effect::Attack(10.0), Effect::CounterBonus(0.04)]),
    permanent(ItemId::SharkSneakers, "Tralalero Tralala", "O tubarão de três patas e tênis azul.", Rank::Mtn, 1_200.0, 10,
        &[Effect::CooldownCut(0.1), Effect::Passive(2.0)]),
    permanent(ItemId::BananaMonkey, "Chimpanzini Bananini", "Metade chimpanzé, metade banana. Wa wa wa.", Rank::Ltn, 400.0, 10,
        &[Effect::MewGain(1.0), Effect::MewCap(1)]),
    permanent(ItemId::SahurBat, "Tung Tung Tung Sahur", "O tronco com taco de beisebol que te acorda pra mogar.", Rank::Chad, 18_000.0, 10,
        &[Effect::Attack(40.0), Effect::StreakBonus(0.02)]),
    permanent(ItemId::CrocBomber, "Bombardiro Crocodilo", "Meio crocodilo, meio bombardeiro, todo destruição.", Rank::Gigachad, 70_000.0, 10,
        &[Effect::Attack(80.0), Effect::WinBonus(0.05)]),
    consumable(ItemId::ComboSixSeven, "Combo 6-7", "Seis, sete, e o MP vem junto.", Rank::Mtn, 670.0, '9',
        "o próximo mog que acertar rende +67% de MP"),
    consumable(ItemId::AuraFarm, "Farmar Aura", "Pose, phonk e câmera lenta por 45 s.", Rank::Htn, 2_000.0, '0',
        "por 45 s, o MP passivo vem em dobro"),
    permanent(ItemId::WolfHood, "Capuz do Lobo Solitário", "Ninguém vê sua cara. Ninguém precisa ver.", Rank::Sigma, 600_000.0, 5,
        &[Effect::Defense(200.0), Effect::LossShield(0.05)]),
    permanent(ItemId::AuraScepter, "Cetro da Aura", "Cada aura farmada vira poder.", Rank::AuraLord, 1_500_000.0, 5,
        &[Effect::Aura(0.3), Effect::Passive(100.0)]),
    permanent(ItemId::Trophy67, "Troféu 6-7", "Ninguém sabe por que você ganhou, mas ganhou.", Rank::SixSeven, 6_700_000.0, 7,
        &[Effect::Attack(250.0), Effect::Defense(250.0), Effect::Passive(400.0)]),
    permanent(ItemId::PhoenixFeather, "Pena de Fênix", "Moggado hoje, renascido amanhã.", Rank::Mythic, 12_000_000.0, 5,
        &[Effect::ImmunityBonus(3.0), Effect::LossShield(0.08), Effect::StreakBonus(0.10)]),
    permanent(ItemId::MarbleBust, "Busto de Mármore", "Proporção áurea esculpida em pedra.", Rank::GreekGod, 35_000_000.0, 5,
        &[Effect::Attack(400.0), Effect::Defense(400.0)]),
    permanent(ItemId::PocketMeteor, "Meteoro de Bolso", "Um pedaço do espaço sideral que rende juros.", Rank::Cosmic, 80_000_000.0, 5,
        &[Effect::Passive(1_500.0), Effect::WinBonus(0.10)]),
    permanent(ItemId::ThirdEye, "Terceiro Olho", "Vê o mog antes de ele acontecer.", Rank::Transcendent, 250_000_000.0, 3,
        &[Effect::Aura(0.5), Effect::PerfectZone(0.02), Effect::MeterSlow(0.03)]),
    permanent(ItemId::Underdog, "Mentalidade Underdog", "Quanto maior o gigante, mais forte o tapa.", Rank::Htn, 3_000.0, 10,
        &[Effect::Underdog(0.04)]),
    consumable(ItemId::WoundedEgo, "Ego Ferido", "Ninguém mogga você duas vezes.", Rank::Htn, 2_500.0, 'e',
        "o próximo mog em quem te moggou por último é crítico garantido"),
    // Cadeia: the "O sabonete" series, unlocked by the DLC.
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
