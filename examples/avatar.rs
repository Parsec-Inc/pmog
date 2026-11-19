//! Prints sample avatars with every mood in the terminal (truecolor): `cargo run --example avatar`.

use pmog::avatar::{Avatar, Mood, Part, Pixels, compose};

const PINK: (u8, u8, u8) = (235, 60, 140);

/// Terminal rows of one face, each cell two pixels tall.
fn rows(px: &Pixels) -> Vec<String> {
    let bg = |c: (u8, u8, u8)| format!("\x1b[48;2;{};{};{}m", c.0, c.1, c.2);
    let fg = |c: (u8, u8, u8)| format!("\x1b[38;2;{};{};{}m", c.0, c.1, c.2);
    px.chunks(2)
        .map(|pair| {
            (0..pair[0].len())
                .map(|x| match (pair[0][x], pair.get(1).and_then(|r| r[x])) {
                    (Some(t), Some(b)) => format!("{}{}▀\x1b[0m", fg(t), bg(b)),
                    (Some(t), None) => format!("{}▀\x1b[0m", fg(t)),
                    (None, Some(b)) => format!("{}▄\x1b[0m", fg(b)),
                    (None, None) => " ".into(),
                })
                .collect()
        })
        .collect()
}

fn print_row(faces: &[(String, Vec<String>)]) {
    println!("{}", faces.iter().map(|(l, _)| format!("{l:<28}")).collect::<String>());
    for i in 0..faces[0].1.len() {
        println!("{}", faces.iter().map(|(_, r)| format!("{}   ", r[i])).collect::<String>());
    }
    println!();
}

fn main() {
    let samples = [
        ("Padrão (grátis)", Avatar::default(), Some(0)),
        ("Fade + Caçador + Brincos", Avatar { face: Part::SquareFace, hair: Part::MidFade, eyes: Part::HunterEyes, mouth: Part::Smirk, accessory: Part::Earrings }, Some(3)),
        ("Chad + Boné + Mewing", Avatar { face: Part::ChadJaw, hair: Part::Bald, eyes: Part::BigEyes, mouth: Part::MewingMouth, accessory: Part::Cap }, Some(12)),
        ("Black Power + Óculos", Avatar { face: Part::OvalFace, hair: Part::Afro, eyes: Part::StarEyes, mouth: Part::GoldTooth, accessory: Part::Shades }, Some(67)),
        ("Mármore + Coroa", Avatar { face: Part::MarbleFace, hair: Part::Bald, eyes: Part::SigmaEyes, mouth: Part::Smile, accessory: Part::Crown }, None),
        ("Moicano + Cicatriz", Avatar { face: Part::RoundFace, hair: Part::Mohawk, eyes: Part::PlainEyes, mouth: Part::PlainMouth, accessory: Part::Scar }, Some(1)),
        ("Franja Emo", Avatar { face: Part::OvalFace, hair: Part::Fringe, eyes: Part::PlainEyes, mouth: Part::PlainMouth, accessory: Part::NoAccessory }, Some(248)),
    ];
    println!("\nPeças (número da testa só para quem tem a DLC Cadeia; o Mármore aparece sem):\n");
    let faces: Vec<_> = samples.iter().map(|(l, a, n)| (l.to_string(), rows(&compose(a, Mood::Neutral, n.map(|n| (n, PINK)))))).collect();
    for chunk in faces.chunks(4) {
        print_row(chunk);
    }
    println!("DLC Cadeia:\n");
    let fruits = [
        ("Zero (Lucas)", Part::ZeroFace, Part::ZeroHair, 0),
        ("Cenoura", Part::CenouraFace, Part::Dreads, 2),
        ("Pera, o Chefe", Part::PeraFace, Part::Bald, 5),
        ("Irmão Six Seven", Part::SixSevenFace, Part::Bald, 6),
        ("Mamão", Part::MamaoFace, Part::Bald, 14),
        ("O 99", Part::NinetyNineFace, Part::Bald, 99),
    ];
    let faces: Vec<_> = fruits
        .iter()
        .map(|(l, face, hair, n)| (l.to_string(), rows(&compose(&Avatar { face: *face, hair: *hair, ..Avatar::default() }, Mood::Neutral, Some((*n, PINK))))))
        .collect();
    for chunk in faces.chunks(4) {
        print_row(chunk);
    }
    println!("Expressões:\n");
    let me = samples[1].1;
    let moods: Vec<_> = Mood::ALL.iter().map(|m| (m.label().to_string(), rows(&compose(&me, *m, Some((3, PINK)))))).collect();
    for chunk in moods.chunks(4) {
        print_row(chunk);
    }
}
