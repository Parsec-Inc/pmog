//! Shop catalog: every item, its per-level effects and how to describe them.

use serde::{Deserialize, Serialize};

use crate::constants::COST_GROWTH;
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
            Effect::CounterBonus(v) => format!("+{} MP ao segurar um mog", num(v)),
            Effect::MeterSlow(v) => format!("cursor {}% mais lento", pct(v)),
            Effect::Aura(v) => format!("+{}% em poder, defesa e MP", pct(v)),
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

    /// Price of the next unit or level.
    pub fn cost(&self, level: u32) -> f64 {
        if self.consumable() {
            self.base_cost
        } else {
            self.base_cost * COST_GROWTH.powi(level as i32)
        }
    }

    /// Why `player` can't buy the next unit or level right now, if anything stops it.
    pub fn purchase_block(&self, player: &crate::game::Player) -> Option<String> {
        let level = player.level(self.id);
        if player.rank() < self.min_rank {
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
        match self.usage {
            Some((key, text)) => format!("[{key}] {text}"),
            None => self.effects.iter().map(|e| e.describe()).collect::<Vec<_>>().join(", "),
        }
    }
}

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

pub const CATALOG: [ItemDef; 34] = [
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
    permanent(ItemId::RingLight, "Ring Light", "Iluminação de estúdio: cada vitória rende mais.", Rank::Mtn, 900.0, 10,
        &[Effect::WinBonus(0.10)]),
    permanent(ItemId::NichePerfume, "Perfume Nicho", "Deixa rastro: quem chega perto já se sente moggado.", Rank::Mtn, 1_100.0, 10,
        &[Effect::WinBonus(0.05), Effect::CounterBonus(5.0)]),
    permanent(ItemId::Sunglasses, "Óculos Escuros", "Ninguém vê seus olhos hesitando.", Rank::Htn, 2_000.0, 5,
        &[Effect::PerfectZone(0.03)]),
    permanent(ItemId::Therapy, "Sessão de Terapia", "Ser moggado dói menos quando você se conhece.", Rank::Htn, 2_500.0, 5,
        &[Effect::LossShield(0.10)]),
    permanent(ItemId::Veneers, "Lentes de Porcelana", "Um sorriso que ofusca de longe.", Rank::Htn, 3_000.0, 10,
        &[Effect::Attack(8.0), Effect::StreakBonus(0.03)]),
    permanent(ItemId::PhonkTrack, "Trilha Phonk", "Toca sozinha quando você entra no ambiente.", Rank::Chadlite, 6_000.0, 10,
        &[Effect::CooldownCut(0.3), Effect::StreakBonus(0.05)]),
    permanent(ItemId::LuxuryWatch, "Relógio de Luxo", "Pulso pesado, postura de quem não se abala.", Rank::Chadlite, 8_000.0, 5,
        &[Effect::ImmunityBonus(1.0), Effect::CounterBonus(10.0)]),
    permanent(ItemId::Photographer, "Fotógrafo Particular", "Ele grita AGORA no momento exato.", Rank::Chadlite, 10_000.0, 5,
        &[Effect::PerfectZone(0.02), Effect::MeterSlow(0.03)]),
    permanent(ItemId::Meditation, "Meditação Sigma", "O tempo desacelera para quem está em paz.", Rank::Chad, 15_000.0, 6,
        &[Effect::MeterSlow(0.05)]),
    permanent(ItemId::DiamondJaw, "Maxilar de Diamante", "Corta vidro. Corta egos.", Rank::Chad, 20_000.0, 50,
        &[Effect::Attack(30.0), Effect::Defense(30.0)]),
    permanent(ItemId::CanthalTape, "Fita de Canthal Tilt", "Olhar de caçador até dormindo.", Rank::Chad, 25_000.0, 10,
        &[Effect::ImmunityBonus(1.0), Effect::LossShield(0.03)]),
    permanent(ItemId::PrivateJet, "Jatinho Particular", "Wealth mog: o MP entra enquanto você voa.", Rank::Gigachad, 60_000.0, 20,
        &[Effect::Passive(20.0)]),
    permanent(ItemId::InfiniteAura, "Aura Infinita", "Multiplica tudo o que você já é.", Rank::Gigachad, 80_000.0, 10,
        &[Effect::Aura(0.5)]),
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
];
