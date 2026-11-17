//! Shop item pixel art, drawn and animated in Aseprite (`assets/sprites/*.aseprite`) and
//! exported as a horizontal PNG strip plus a JSON file with each frame's duration.
//! Re-export after editing: File > Export Sprite Sheet, "Horizontal Strip", JSON Data (Array).

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::{Mutex, OnceLock};

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

macro_rules! asset {
    ($n:literal) => {
        (
            include_bytes!(concat!("../assets/sprites/", $n, ".png")).as_slice(),
            include_bytes!(concat!("../assets/sprites/", $n, ".json")).as_slice(),
            $n,
        )
    };
}

fn asset(id: ItemId) -> (&'static [u8], &'static [u8], &'static str) {
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
    }
}

fn decode(png: &[u8], json: &[u8]) -> Anim {
    let mut reader = png::Decoder::new(Cursor::new(png)).read_info().expect("sprite png");
    let mut buf = vec![0; reader.output_buffer_size().expect("sprite png size")];
    let info = reader.next_frame(&mut buf).expect("sprite png data");
    assert!(info.color_type == png::ColorType::Rgba && info.bit_depth == png::BitDepth::Eight, "export sprites as 8-bit RGBA");
    let meta: serde_json::Value = serde_json::from_slice(json).expect("sprite json");
    let frames: Vec<Frame> = meta["frames"]
        .as_array()
        .expect("sprite json frames")
        .iter()
        .map(|f| {
            let n = |k: &str| f["frame"][k].as_u64().expect("frame rect") as usize;
            let (x0, y0, w, h) = (n("x"), n("y"), n("w"), n("h"));
            let rows = (y0..y0 + h)
                .map(|y| {
                    (x0..x0 + w)
                        .map(|x| {
                            let p = &buf[(y * info.width as usize + x) * 4..][..4];
                            (p[3] > 127).then_some((p[0], p[1], p[2]))
                        })
                        .collect()
                })
                .collect();
            Frame { rows, ms: f["duration"].as_u64().unwrap_or(100).max(1) }
        })
        .collect();
    let total_ms = frames.iter().map(|f| f.ms).sum();
    Anim { frames, total_ms }
}

/// The frame to show `t_ms` after the game started; loops through the item's animation.
pub fn sprite_at(id: ItemId, t_ms: u64) -> &'static Frame {
    static CACHE: OnceLock<Mutex<HashMap<&'static str, &'static Anim>>> = OnceLock::new();
    let (png, json, name) = asset(id);
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
