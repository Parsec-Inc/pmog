//! Cadeia titles: the cell hierarchy from the series, earned by soaps made and kept off the forehead.

use crate::constants::prison::{PRISON_TITLE_MARKS, ZERO_TITLE_MARKS};

/// Prison standing, weakest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PrisonTitle {
    X9,
    Frangolino,
    SixSeven,
    Cenoura,
    Pera,
    Mil,
    Zero,
}

impl PrisonTitle {
    pub const ALL: [PrisonTitle; 7] =
        [PrisonTitle::X9, PrisonTitle::Frangolino, PrisonTitle::SixSeven, PrisonTitle::Cenoura, PrisonTitle::Pera, PrisonTitle::Mil, PrisonTitle::Zero];

    /// Title for `marks` soaps made and `forehead` soaps picked up; `None` before the first mark.
    pub fn of(marks: u32, forehead: u32) -> Option<PrisonTitle> {
        if forehead == 0 && marks >= ZERO_TITLE_MARKS {
            return Some(PrisonTitle::Zero);
        }
        PRISON_TITLE_MARKS.iter().rposition(|need| marks >= *need).map(|i| PrisonTitle::ALL[i])
    }

    pub fn label(self) -> &'static str {
        match self {
            PrisonTitle::X9 => "X9",
            PrisonTitle::Frangolino => "Frangolino",
            PrisonTitle::SixSeven => "Irmão Six Seven",
            PrisonTitle::Cenoura => "Cenoura",
            PrisonTitle::Pera => "Pera, o Chefe",
            PrisonTitle::Mil => "Mil, o Cego",
            PrisonTitle::Zero => "Zero (Lucas)",
        }
    }

    /// What it takes, in pt-BR.
    pub fn requirement(self) -> String {
        match self {
            PrisonTitle::Zero => format!("{ZERO_TITLE_MARKS} numerinhos marcados sem nenhum na testa"),
            other => {
                let need = PRISON_TITLE_MARKS[other as usize];
                if need == 1 { "1 numerinho marcado".into() } else { format!("{need} numerinhos marcados") }
            }
        }
    }

    pub fn flavor(self) -> &'static str {
        match self {
            PrisonTitle::X9 => "Contou pro chefe e ganhou o primeiro numerinho alheio.",
            PrisonTitle::Frangolino => "O magrelo começou a treinar na barra.",
            PrisonTitle::SixSeven => "Os mais fortes de todo o Nordeste, meu irmão.",
            PrisonTitle::Cenoura => "Manda nas prisões por onde passa, parceiro.",
            PrisonTitle::Pera => "Já foi preso aqui. Hoje tem a chave.",
            PrisonTitle::Mil => "Mil noites no poço. Ouve até tua respiração, guri.",
            PrisonTitle::Zero => "O mais aurudo da cela. Nunca se abaixou.",
        }
    }
}
