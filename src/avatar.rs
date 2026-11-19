//! Player avatar: a pixel-art face stacked from layers bought with MP, reacting to mogs.
//! Layers are 25x28 RGBA PNGs in `assets/avatar/`, symmetric around the center column.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

use crate::constants::avatar::{AVATAR_HEIGHT, AVATAR_WIDTH, FOREHEAD_CENTER, FOREHEAD_MAX, FOREHEAD_TOP};
use crate::dlc::DlcId;
use crate::game::{Player, Rank};
use crate::sprites::Rgb;

/// Which part of the face an option replaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Face,
    Hair,
    Eyes,
    Mouth,
    Accessory,
}

impl Kind {
    pub const ALL: [Kind; 5] = [Kind::Face, Kind::Hair, Kind::Eyes, Kind::Mouth, Kind::Accessory];

    pub fn label(self) -> &'static str {
        match self {
            Kind::Face => "Rosto",
            Kind::Hair => "Cabelo",
            Kind::Eyes => "Olhos",
            Kind::Mouth => "Boca",
            Kind::Accessory => "Acessório",
        }
    }

    /// Options of this kind, the free one first.
    pub fn parts(self) -> impl Iterator<Item = &'static PartDef> {
        PARTS.iter().filter(move |d| d.kind == self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Part {
    RoundFace,
    SquareFace,
    OvalFace,
    ChadJaw,
    MarbleFace,
    // DLC Cadeia.
    ZeroFace,
    MamaoFace,
    PeraFace,
    SixSevenFace,
    CenouraFace,
    NinetyNineFace,
    Bald,
    MidFade,
    Mohawk,
    Fringe,
    Afro,
    // DLC Cadeia.
    ZeroHair,
    Dreads,
    PlainEyes,
    HunterEyes,
    BigEyes,
    StarEyes,
    SigmaEyes,
    PlainMouth,
    Smile,
    MewingMouth,
    Smirk,
    GoldTooth,
    NoAccessory,
    Earrings,
    Scar,
    Cap,
    Shades,
    Crown,
}

/// Embedded layer: PNG bytes and a unique name.
type Layer = (&'static [u8], &'static str);

macro_rules! layer {
    ($n:literal) => {
        Some((include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/avatar/", $n, ".png")).as_slice(), $n))
    };
}

pub struct PartDef {
    pub part: Part,
    pub kind: Kind,
    pub name: &'static str,
    pub cost: f64,
    pub min_rank: Rank,
    /// The DLC needed to buy it.
    pub dlc: Option<DlcId>,
    layer: Option<Layer>,
}

const fn part(part: Part, kind: Kind, name: &'static str, cost: f64, min_rank: Rank, layer: Option<Layer>) -> PartDef {
    PartDef { part, kind, name, cost, min_rank, dlc: None, layer }
}

impl PartDef {
    /// Only for owners of the DLC `dlc`.
    const fn dlc(self, dlc: DlcId) -> PartDef {
        PartDef { dlc: Some(dlc), ..self }
    }
}

/// Every option, grouped by kind with the free one first.
pub const PARTS: [PartDef; 34] = [
    part(Part::RoundFace, Kind::Face, "Redondo", 0.0, Rank::Sub5, layer!("face_round")),
    part(Part::SquareFace, Kind::Face, "Quadrado", 400.0, Rank::Sub5, layer!("face_square")),
    part(Part::OvalFace, Kind::Face, "Oval", 400.0, Rank::Sub5, layer!("face_oval")),
    part(Part::ChadJaw, Kind::Face, "Mandíbula de Chad", 6_000.0, Rank::Htn, layer!("face_chad")),
    part(Part::MarbleFace, Kind::Face, "Esculpido em Mármore", 250_000.0, Rank::Ascended, layer!("face_marble")),
    part(Part::MamaoFace, Kind::Face, "Mamão", 2_000.0, Rank::Ltn, layer!("fruit_mamao")).dlc(DlcId::Prison),
    part(Part::PeraFace, Kind::Face, "Pera, o Chefe", 8_000.0, Rank::Htn, layer!("fruit_pera")).dlc(DlcId::Prison),
    part(Part::SixSevenFace, Kind::Face, "Irmão Six Seven", 15_000.0, Rank::Chadlite, layer!("fruit_sixseven")).dlc(DlcId::Prison),
    part(Part::CenouraFace, Kind::Face, "Cenoura", 30_000.0, Rank::Chad, layer!("fruit_cenoura")).dlc(DlcId::Prison),
    part(Part::NinetyNineFace, Kind::Face, "O 99", 50_000.0, Rank::Chad, layer!("fruit_99")).dlc(DlcId::Prison),
    part(Part::ZeroFace, Kind::Face, "Zero (Lucas)", 67_000.0, Rank::Gigachad, layer!("fruit_zero")).dlc(DlcId::Prison),
    part(Part::Bald, Kind::Hair, "Careca", 0.0, Rank::Sub5, None),
    part(Part::MidFade, Kind::Hair, "Mid Fade", 300.0, Rank::Sub5, layer!("hair_fade")),
    part(Part::Mohawk, Kind::Hair, "Moicano", 1_500.0, Rank::Mtn, layer!("hair_mohawk")),
    part(Part::Fringe, Kind::Hair, "Franja Emo", 4_000.0, Rank::Htn, layer!("hair_fringe")),
    part(Part::Afro, Kind::Hair, "Black Power", 20_000.0, Rank::Chad, layer!("hair_afro")),
    part(Part::ZeroHair, Kind::Hair, "Crespo do Zero", 6_700.0, Rank::Htn, layer!("hair_zero")).dlc(DlcId::Prison),
    part(Part::Dreads, Kind::Hair, "Dreads do Cenoura", 12_000.0, Rank::Chadlite, layer!("hair_dreads")).dlc(DlcId::Prison),
    part(Part::PlainEyes, Kind::Eyes, "Normais", 0.0, Rank::Sub5, layer!("eyes_plain")),
    part(Part::HunterEyes, Kind::Eyes, "Olhar de Caçador", 800.0, Rank::Ltn, layer!("eyes_hunter")),
    part(Part::BigEyes, Kind::Eyes, "Olhos Grandes", 2_500.0, Rank::Mtn, layer!("eyes_big")),
    part(Part::StarEyes, Kind::Eyes, "Olhos Estrelados", 12_000.0, Rank::Chadlite, layer!("eyes_star")),
    part(Part::SigmaEyes, Kind::Eyes, "Olhar Sigma", 60_000.0, Rank::Gigachad, layer!("eyes_sigma")),
    part(Part::PlainMouth, Kind::Mouth, "Neutra", 0.0, Rank::Sub5, layer!("mouth_plain")),
    part(Part::Smile, Kind::Mouth, "Sorriso", 300.0, Rank::Sub5, layer!("mouth_smile")),
    part(Part::MewingMouth, Kind::Mouth, "Mewing", 1_200.0, Rank::Ltn, layer!("mouth_mewing")),
    part(Part::Smirk, Kind::Mouth, "Sorriso de Lado", 5_000.0, Rank::Htn, layer!("mouth_smirk")),
    part(Part::GoldTooth, Kind::Mouth, "Dente de Ouro", 30_000.0, Rank::Chad, layer!("mouth_gold")),
    part(Part::NoAccessory, Kind::Accessory, "Nenhum", 0.0, Rank::Sub5, None),
    part(Part::Earrings, Kind::Accessory, "Brincos", 700.0, Rank::Ltn, layer!("acc_earrings")),
    part(Part::Scar, Kind::Accessory, "Cicatriz", 2_000.0, Rank::Mtn, layer!("acc_scar")),
    part(Part::Cap, Kind::Accessory, "Boné", 3_500.0, Rank::Htn, layer!("acc_cap")),
    part(Part::Shades, Kind::Accessory, "Óculos Escuros", 15_000.0, Rank::Chadlite, layer!("acc_shades")),
    part(Part::Crown, Kind::Accessory, "Coroa do Mog", 400_000.0, Rank::Sigma, layer!("acc_crown")),
];

pub fn def(part: Part) -> &'static PartDef {
    PARTS.iter().find(|d| d.part == part).expect("every Part is in PARTS")
}

impl PartDef {
    pub fn free(&self) -> bool {
        self.cost == 0.0
    }

    /// What stops `player` from buying it, if anything.
    pub fn block(&self, player: &Player) -> Option<Block> {
        if player.owns_part(self.part) {
            Some(Block::Owned)
        } else if let Some(d) = self.dlc.filter(|d| !player.owns_dlc(*d)) {
            Some(Block::Dlc(d))
        } else if player.rank() < self.min_rank {
            Some(Block::Rank)
        } else if player.mp < self.cost {
            Some(Block::Mp)
        } else {
            None
        }
    }

    /// Why `player` can't buy it, as a sentence.
    pub fn purchase_block(&self, player: &Player) -> Option<String> {
        let name = self.name;
        self.block(player).map(|b| match b {
            Block::Owned => format!("{name} já é seu."),
            Block::Dlc(d) => format!("{name} faz parte da DLC {}: compre a DLC primeiro.", crate::dlc::get(d).name()),
            Block::Rank => format!("{name} exige rank {}.", self.min_rank.label()),
            Block::Mp => format!("Faltam {} MP para {name}.", crate::utils::format_mp(self.cost - player.mp)),
        })
    }

    /// Short reason for a list column.
    pub fn block_label(&self, block: Block) -> String {
        match block {
            Block::Owned => "seu".into(),
            Block::Dlc(d) => format!("DLC {}", crate::dlc::get(d).name()),
            Block::Rank => format!("rank {}", self.min_rank.label()),
            Block::Mp => format!("{} MP", crate::utils::format_mp(self.cost)),
        }
    }
}

/// What stops a part from being bought.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Block {
    Owned,
    Dlc(DlcId),
    Rank,
    Mp,
}

/// The parts a player wears.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Avatar {
    pub face: Part,
    pub hair: Part,
    pub eyes: Part,
    pub mouth: Part,
    pub accessory: Part,
}

impl Default for Avatar {
    fn default() -> Avatar {
        Avatar { face: Part::RoundFace, hair: Part::Bald, eyes: Part::PlainEyes, mouth: Part::PlainMouth, accessory: Part::NoAccessory }
    }
}

impl Avatar {
    fn slot_mut(&mut self, kind: Kind) -> &mut Part {
        match kind {
            Kind::Face => &mut self.face,
            Kind::Hair => &mut self.hair,
            Kind::Eyes => &mut self.eyes,
            Kind::Mouth => &mut self.mouth,
            Kind::Accessory => &mut self.accessory,
        }
    }

    pub fn get(&self, kind: Kind) -> Part {
        let mut copy = *self;
        *copy.slot_mut(kind)
    }

    /// Wears `part` in its place.
    pub fn set(&mut self, part: Part) {
        *self.slot_mut(def(part).kind) = part;
    }
}

/// The face's expression right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mood {
    Neutral,
    Blink,
    /// Landed a mog.
    Smug,
    /// Got mogged.
    Mogged,
    /// Own mog failed.
    Sad,
    /// Held a mog.
    Proud,
}

impl Mood {
    pub const ALL: [Mood; 6] = [Mood::Neutral, Mood::Blink, Mood::Smug, Mood::Mogged, Mood::Sad, Mood::Proud];

    pub fn label(self) -> &'static str {
        match self {
            Mood::Neutral => "neutro",
            Mood::Blink => "piscando",
            Mood::Smug => "mogou",
            Mood::Mogged => "foi moggado",
            Mood::Sad => "mog falhou",
            Mood::Proud => "segurou o mog",
        }
    }

    /// Eye and mouth layers that replace the worn ones.
    fn layers(self) -> (Option<Layer>, Option<Layer>) {
        match self {
            Mood::Neutral => (None, None),
            Mood::Blink => (layer!("mood_blink_eyes"), None),
            Mood::Smug => (layer!("mood_smug_eyes"), layer!("mood_smug_mouth")),
            Mood::Mogged => (layer!("mood_mogged_eyes"), layer!("mood_mogged_mouth")),
            Mood::Sad => (layer!("mood_sad_eyes"), layer!("mood_sad_mouth")),
            Mood::Proud => (layer!("mood_proud_eyes"), layer!("mood_proud_mouth")),
        }
    }
}

/// Square grid of pixels; `None` is transparent.
pub type Pixels = Vec<Vec<Option<Rgb>>>;

fn decode((png, name): Layer) -> &'static Pixels {
    static CACHE: OnceLock<Mutex<HashMap<&'static str, &'static Pixels>>> = OnceLock::new();
    CACHE.get_or_init(Default::default).lock().unwrap().entry(name).or_insert_with(|| {
        let px = crate::sprites::pixels(png);
        assert!(px.len() == AVATAR_HEIGHT && px.iter().all(|r| r.len() == AVATAR_WIDTH), "{name}: 25x28");
        Box::leak(Box::new(px))
    })
}

/// 3x5 digits, one row per byte, high bit on the left.
const DIGITS: [[u8; 5]; 10] = [
    [0b111, 0b101, 0b101, 0b101, 0b111],
    [0b010, 0b110, 0b010, 0b010, 0b111],
    [0b111, 0b001, 0b111, 0b100, 0b111],
    [0b111, 0b001, 0b011, 0b001, 0b111],
    [0b101, 0b101, 0b111, 0b001, 0b001],
    [0b111, 0b100, 0b111, 0b001, 0b111],
    [0b111, 0b100, 0b111, 0b101, 0b111],
    [0b111, 0b001, 0b010, 0b010, 0b010],
    [0b111, 0b101, 0b111, 0b101, 0b111],
    [0b111, 0b101, 0b111, 0b001, 0b111],
];

fn lighten((r, g, b): Rgb) -> Rgb {
    let up = |c: u8| c + (255 - c) / 2;
    (up(r), up(g), up(b))
}

/// Writes `number` centered on the forehead, in `color`.
fn draw_number(out: &mut Pixels, number: u32, color: Rgb) {
    let digits: Vec<usize> = number.min(FOREHEAD_MAX).to_string().bytes().map(|b| usize::from(b - b'0')).collect();
    let width = digits.len() * 4 - 1;
    let left = FOREHEAD_CENTER + 1 - width.div_ceil(2);
    for (i, d) in digits.into_iter().enumerate() {
        for (row, bits) in DIGITS[d].iter().enumerate() {
            for col in (0..3).filter(|c| bits & (0b100 >> c) != 0) {
                let cell = &mut out[FOREHEAD_TOP + row][left + i * 4 + col];
                // Light ink on dark skin, so the number still reads.
                let dark = cell.is_some_and(|(r, g, b)| u32::from(r) + u32::from(g) + u32::from(b) < 300);
                *cell = Some(if dark { lighten(color) } else { color });
            }
        }
    }
}

/// The face's pixels: face, eyes, mouth, hair and accessory, with `mood` swapping eyes and mouth and
/// `forehead` (number and color) written on the forehead.
pub fn compose(avatar: &Avatar, mood: Mood, forehead: Option<(u32, Rgb)>) -> Pixels {
    let (mood_eyes, mood_mouth) = mood.layers();
    let layer = |kind| def(avatar.get(kind)).layer;
    let mut out = vec![vec![None; AVATAR_WIDTH]; AVATAR_HEIGHT];
    let blit = |out: &mut Pixels, layer: Option<Layer>| {
        for (dst, src) in out.iter_mut().flatten().zip(layer.map(decode).into_iter().flatten().flatten()) {
            if src.is_some() {
                *dst = *src;
            }
        }
    };
    blit(&mut out, layer(Kind::Face));
    blit(&mut out, mood_eyes.or(layer(Kind::Eyes)));
    blit(&mut out, mood_mouth.or(layer(Kind::Mouth)));
    blit(&mut out, layer(Kind::Hair));
    // Over the hair, so a fringe never hides it; accessories still go on top.
    if let Some((number, color)) = forehead {
        draw_number(&mut out, number, color);
    }
    blit(&mut out, layer(Kind::Accessory));
    out
}

/// The avatar screen: a tab per kind of part, the option under the cursor tried on in the preview.
#[derive(Debug, Clone, Default)]
pub struct Editor {
    pub tab: usize,
    pub cursor: usize,
    /// Part waiting for a second Enter to be bought.
    pub confirm: Option<Part>,
    /// Part sent to the server to buy, until it answers.
    pub pending: Option<Part>,
}

impl Editor {
    pub fn kind(&self) -> Kind {
        Kind::ALL[self.tab]
    }

    pub fn selected(&self) -> &'static PartDef {
        self.kind().parts().nth(self.cursor).expect("cursor stays inside the tab")
    }

    /// Opens on the first tab, cursor on what is worn.
    pub fn open(&mut self, worn: &Avatar) {
        self.tab = 0;
        self.turn(0, worn);
    }

    /// Switches tab, putting the cursor on the part worn there.
    pub fn turn(&mut self, delta: isize, worn: &Avatar) {
        self.tab = (self.tab as isize + delta).rem_euclid(Kind::ALL.len() as isize) as usize;
        let wearing = worn.get(self.kind());
        self.cursor = self.kind().parts().position(|d| d.part == wearing).unwrap_or(0);
        self.confirm = None;
    }

    pub fn step(&mut self, delta: isize) {
        let len = self.kind().parts().count() as isize;
        self.cursor = (self.cursor as isize + delta).rem_euclid(len) as usize;
        self.confirm = None;
    }

    /// The worn avatar with the option under the cursor tried on.
    pub fn preview(&self, worn: &Avatar) -> Avatar {
        let mut avatar = *worn;
        avatar.set(self.selected().part);
        avatar
    }

    /// Enter on the option under the cursor: wears it, asks to confirm a purchase, or buys it.
    pub fn enter(&mut self, player: &Player) -> Option<crate::protocol::ClientMsg> {
        use crate::protocol::ClientMsg;
        let def = self.selected();
        if player.owns_part(def.part) {
            self.confirm = None;
            return (player.avatar.get(def.kind) != def.part).then_some(ClientMsg::WearPart { part: def.part });
        }
        if self.pending.is_some() || def.block(player).is_some() {
            return None;
        }
        if self.confirm.take() == Some(def.part) {
            self.pending = Some(def.part);
            return Some(ClientMsg::BuyPart { part: def.part });
        }
        self.confirm = Some(def.part);
        None
    }
}
