//! Shop item pixel art, drawn and animated in Aseprite (`assets/sprites/*.aseprite`) and
//! exported as a horizontal PNG strip plus a JSON file with each frame's duration.
//! Re-export after editing: File > Export Sprite Sheet, "Horizontal Strip", JSON Data (Array).

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::{Mutex, OnceLock};

use crate::dlc::{self, SpriteAsset};
use crate::game::ItemId;

pub type Rgb = (u8, u8, u8);

pub struct Frame {
    /// Row-major pixels; `None` is transparent.
    pub rows: Vec<Vec<Option<Rgb>>>,
    pub ms: u64,
}

pub struct Anim {
    pub frames: Vec<Frame>,
    total_ms: u64,
}

/// Embeds `assets/<path>.png` and `.json`, keyed by `path`; usable from any module (DLCs included).
macro_rules! sprite_asset {
    ($path:expr) => {
        (
            include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/", $path, ".png")).as_slice(),
            include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/", $path, ".json")).as_slice(),
            $path,
        )
    };
}
pub(crate) use sprite_asset;

macro_rules! asset {
    ($n:literal) => {
        sprite_asset!(concat!("sprites/", $n))
    };
}

fn asset(id: ItemId) -> SpriteAsset {
    match id {
        ItemId::Mewing => asset!("mewing"),
        ItemId::GuaSha => asset!("gua_sha"),
        ItemId::MidFade => asset!("mid_fade"),
        ItemId::Skincare => asset!("skincare"),
        ItemId::Insoles => asset!("insoles"),
        ItemId::Sunglasses => asset!("sunglasses"),
        ItemId::PhonkTrack => asset!("phonk"),
        ItemId::DiamondJaw => asset!("diamond"),
        ItemId::InfiniteAura => asset!("aura"),
        ItemId::EnergyDrink => asset!("energy_drink"),
        ItemId::SigmaShield => asset!("shield"),
        ItemId::AngleShot => asset!("camera"),
        ItemId::WaterBottle => asset!("water_bottle"),
        ItemId::GymPlan => asset!("dumbbell"),
        ItemId::RingLight => asset!("ring_light"),
        ItemId::Therapy => asset!("couch"),
        ItemId::LuxuryWatch => asset!("watch"),
        ItemId::Meditation => asset!("lotus"),
        ItemId::PrivateJet => asset!("jet"),
        ItemId::MogCrown => asset!("crown"),
        ItemId::PreWorkout => asset!("shaker"),
        ItemId::PocketMirror => asset!("mirror"),
        ItemId::Sunscreen => asset!("sunscreen"),
        ItemId::Creatine => asset!("creatine"),
        ItemId::MasticGum => asset!("gum"),
        ItemId::NichePerfume => asset!("perfume"),
        ItemId::Veneers => asset!("veneers"),
        ItemId::Photographer => asset!("photographer"),
        ItemId::CanthalTape => asset!("canthal"),
        ItemId::Mansion => asset!("mansion"),
        ItemId::BlessedGenes => asset!("dna"),
        ItemId::JawTape => asset!("jaw_tape"),
        ItemId::Cologne => asset!("cologne"),
        ItemId::IcePack => asset!("ice_pack"),
        ItemId::Minoxidil => asset!("minoxidil"),
        ItemId::Dermaroller => asset!("dermaroller"),
        ItemId::FoxEyes => asset!("fox_eyes"),
        ItemId::ShoulderPads => asset!("shoulder_pads"),
        ItemId::SleepKit => asset!("sleep_kit"),
        ItemId::Whey => asset!("whey"),
        ItemId::BoatDance => asset!("boat_dance"),
        ItemId::DoorLean => asset!("door_lean"),
        ItemId::Jestermaxxing => asset!("jester"),
        ItemId::SixSeven => asset!("six_seven"),
        ItemId::SixSevenHeight => asset!("height_67"),
        ItemId::Capoeira => asset!("capoeira"),
        ItemId::SharkSneakers => asset!("shark"),
        ItemId::BananaMonkey => asset!("banana_monkey"),
        ItemId::SahurBat => asset!("sahur_bat"),
        ItemId::CrocBomber => asset!("croc_bomber"),
        ItemId::ComboSixSeven => asset!("combo_67"),
        ItemId::AuraFarm => asset!("aura_farm"),
        ItemId::WolfHood => asset!("wolf_hood"),
        ItemId::AuraScepter => asset!("aura_scepter"),
        ItemId::Trophy67 => asset!("trophy_67"),
        ItemId::PhoenixFeather => asset!("phoenix_feather"),
        ItemId::MarbleBust => asset!("marble_bust"),
        ItemId::PocketMeteor => asset!("pocket_meteor"),
        ItemId::ThirdEye => asset!("third_eye"),
        ItemId::Underdog => asset!("underdog"),
        ItemId::WoundedEgo => asset!("wounded_ego"),
        _ => dlc::of_item(id).and_then(|d| d.sprite(id)).expect("every item has a sprite"),
    }
}

/// Every pixel of an 8-bit RGBA PNG, row by row; `None` is transparent.
pub fn pixels(png: &[u8]) -> Vec<Vec<Option<Rgb>>> {
    let mut reader = png::Decoder::new(Cursor::new(png)).read_info().expect("sprite png");
    let mut buf = vec![0; reader.output_buffer_size().expect("sprite png size")];
    let info = reader.next_frame(&mut buf).expect("sprite png data");
    assert!(info.color_type == png::ColorType::Rgba && info.bit_depth == png::BitDepth::Eight, "export sprites as 8-bit RGBA");
    buf.chunks(info.width as usize * 4)
        .take(info.height as usize)
        .map(|row| row.chunks(4).map(|p| (p[3] > 127).then_some((p[0], p[1], p[2]))).collect())
        .collect()
}

fn decode(png: &[u8], json: &[u8]) -> Anim {
    let sheet = pixels(png);
    let meta: serde_json::Value = serde_json::from_slice(json).expect("sprite json");
    let frames: Vec<Frame> = meta["frames"]
        .as_array()
        .expect("sprite json frames")
        .iter()
        .map(|f| {
            let n = |k: &str| f["frame"][k].as_u64().expect("frame rect") as usize;
            let (x0, y0, w, h) = (n("x"), n("y"), n("w"), n("h"));
            let rows = sheet[y0..y0 + h].iter().map(|row| row[x0..x0 + w].to_vec()).collect();
            Frame { rows, ms: f["duration"].as_u64().unwrap_or(100).max(1) }
        })
        .collect();
    let total_ms = frames.iter().map(|f| f.ms).sum();
    Anim { frames, total_ms }
}

/// The frame to show `t_ms` after the game started; loops through the item's animation.
pub fn sprite_at(id: ItemId, t_ms: u64) -> &'static Frame {
    let (png, json, name) = asset(id);
    frame_at(png, json, name, t_ms)
}

/// The lobby mascot's frame at `t_ms`.
pub fn mascot_at(t_ms: u64) -> &'static Frame {
    let (png, json, name) = asset!("mascot");
    frame_at(png, json, name, t_ms)
}

fn frame_at(png: &[u8], json: &[u8], name: &'static str, t_ms: u64) -> &'static Frame {
    static CACHE: OnceLock<Mutex<HashMap<&'static str, &'static Anim>>> = OnceLock::new();
    let anim = *CACHE
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .entry(name)
        .or_insert_with(|| Box::leak(Box::new(decode(png, json))));
    let mut t = t_ms % anim.total_ms;
    for f in &anim.frames {
        if t < f.ms {
            return f;
        }
        t -= f.ms;
    }
    &anim.frames[0]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::CATALOG;

    #[test]
    fn every_item_has_an_animation_with_matching_frames() {
        for def in CATALOG.iter() {
            let (png, json, name) = asset(def.id);
            let a = decode(png, json);
            assert!(!a.frames.is_empty(), "{name}");
            let (w, h) = (a.frames[0].rows[0].len(), a.frames[0].rows.len());
            assert!(a.frames.iter().all(|f| f.rows.len() == h && f.rows[0].len() == w), "{name}: frame sizes differ");
            assert!(a.frames[0].rows.iter().flatten().any(Option::is_some), "{name}: empty");
        }
    }
}
