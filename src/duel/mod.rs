//! PvP duels: six drafted minigames plus sudden death, refereed by the server.

mod contests;

pub use contests::*;

use std::cmp::Ordering;
use std::time::{Duration, Instant};

use rand::Rng;
use rand::seq::IteratorRandom;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

use crate::constants::*;
use crate::protocol::PlayerId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Minigame {
    Staredown,
    Skincare,
    Mewing,
    Angle,
    Typing,
    Rhythm,
    Hunter,
    Macros,
    Jawline,
    Posture,
    FitCheck,
    Memory,
    Hydration,
    Photo,
    Status,
    Hair,
    Brow,
    ReverseMog,
}

impl Minigame {
    pub const ALL: [Minigame; 18] = [
        Minigame::Staredown,
        Minigame::Skincare,
        Minigame::Mewing,
        Minigame::Angle,
        Minigame::Typing,
        Minigame::Rhythm,
        Minigame::Hunter,
        Minigame::Macros,
        Minigame::Jawline,
        Minigame::Posture,
        Minigame::FitCheck,
        Minigame::Memory,
        Minigame::Hydration,
        Minigame::Photo,
        Minigame::Status,
        Minigame::Hair,
        Minigame::Brow,
        Minigame::ReverseMog,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Minigame::Staredown => "Encarada",
            Minigame::Skincare => "Rotina de Skincare",
            Minigame::Mewing => "Corrida de Mewing",
            Minigame::Angle => "Ângulo Perfeito",
            Minigame::Typing => "Batalha de Digitação",
            Minigame::Rhythm => "Treino no Ritmo",
            Minigame::Hunter => "Olhar de Caçador",
            Minigame::Macros => "Conta dos Macros",
            Minigame::Jawline => "Esculpir o Maxilar",
            Minigame::Posture => "Postura Reta",
            Minigame::FitCheck => "Fit Check",
            Minigame::Memory => "Memória de Rotina",
            Minigame::Hydration => "Hidratação",
            Minigame::Photo => "Fotogenia",
            Minigame::Status => "Batalha de Status",
            Minigame::Hair => "Cabelo ao Vento",
            Minigame::Brow => "Sobrancelha Levantada",
            Minigame::ReverseMog => "Mog Reverso",
        }
    }

    /// Only makes sense one against one, so group duels never draw it.
    pub fn duo_only(self) -> bool {
        self == Minigame::ReverseMog
    }

    pub fn how_to(self) -> &'static str {
        match self {
            Minigame::Staredown => {
                "Encare sem piscar. Quando aparecer AGORA, aperte ESPAÇO antes do rival. Apertou antes da hora (ou em menos de 0,1 s, que é chute), perdeu."
            }
            Minigame::Skincare => {
                "Decore a rotina e repita com as setas. A cada nível ela ganha um passo. Errou um passo, saiu do nível."
            }
            Minigame::Mewing => "Alterne ← e → (ou A e D) o mais rápido que puder. Cada troca conta uma mewada.",
            Minigame::Angle => "Pare o cursor em cima do alvo com ESPAÇO. São 3 tentativas e vale a mais perto.",
            Minigame::Typing => {
                "Digite a frase o mais rápido que puder. Letra errada não avança e derruba a precisão. Vale velocidade x precisão², e quem termina primeiro ganha bônus."
            }
            Minigame::Rhythm => {
                "Aperte ESPAÇO em cada batida do metrônomo, quando a nota chega na linha. Quanto mais no tempo, mais pontos. Apertar fora do ritmo tira pontos."
            }
            Minigame::Hunter => {
                "Rostos aparecem por um instante. Aperte ESPAÇO só nos hunter eyes (◣_◢), rápido. Apertar na cara de cachorrinho (◕‿◕) ou no vazio tira pontos."
            }
            Minigame::Macros => {
                "Resolva as contas de macros (gramas) digitando o resultado e Enter. Quem resolver mais vence; empate vai para quem terminou antes."
            }
            Minigame::Jawline => {
                "Deixe o seu maxilar igual ao molde. ← → escolhem o ponto, ↑ ↓ sobem e descem. Vence quem ficar com menos erro; empate vai para quem acertou primeiro."
            }
            Minigame::Posture => {
                "O pescoço vai entortando cada vez mais rápido. Corrija com ← → para mantê-lo dentro da faixa do meio. Vence quem passar mais tempo reto."
            }
            Minigame::FitCheck => {
                "Cada peça troca de cor sem parar, e cada uma mais rápido que a anterior. Aperte ESPAÇO para travar a peça na cor do fit do dia. Mais peças certas vence; empate vai para quem fechou o look antes."
            }
            Minigame::Memory => {
                "Os passos da rotina piscam um de cada vez. Depois, repita digitando os números (1 a 8). A cada nível a rotina ganha um passo; errou, saiu do nível."
            }
            Minigame::Hydration => {
                "Encha a garrafa até a linha da meta: ↑ dá um gole grande (+5), → um golinho (+1), e a garrafa esvazia sozinha com o tempo. ESPAÇO fecha a garrafa. Vence quem fechar mais perto da meta."
            }
            Minigame::Photo => {
                "A contagem 3, 2, 1 vem num ritmo diferente a cada foto e some antes do flash. Aperte ESPAÇO exatamente no clique. Vence quem ficar mais perto do flash, em média."
            }
            Minigame::Status => {
                "Ordene os itens da loja do mais caro para o mais barato (preço inicial), apertando os números deles nessa ordem. Mais posições certas vence; empate vai para quem terminou antes."
            }
            Minigame::Hair => {
                "Fios soltos aparecem de um lado do cabelo. Aperte a seta do lado certo antes de o fio voar. Seta errada ou no vazio tira pontos."
            }
            Minigame::Brow => {
                "Encare o rival. Quando a sobrancelha dele levantar de verdade, aperte ESPAÇO. Tremidinhas são falsas: apertar nelas, antes da hora ou não apertar custa caro. Menor tempo total vence."
            }
            Minigame::ReverseMog => {
                "Pedra, papel e tesoura do mog: Mogar (←) vence Ignorar, Ignorar (→) vence Defender, Defender (↓) vence Mogar. São 5 jogadas, escolha antes do tempo acabar. A escolha do rival só aparece na revelação."
            }
        }
    }
}

/// Keys a duel understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DuelKey {
    Up,
    Down,
    Left,
    Right,
    Action,
    /// A typed character (typing battle only).
    Char(char),
    /// A submitted number (macros only).
    Answer(i64),
    /// A draft pick; `None` lets the server draw one.
    Pick(Option<Minigame>),
}

impl DuelKey {
    pub const ARROWS: [DuelKey; 4] = [DuelKey::Up, DuelKey::Down, DuelKey::Left, DuelKey::Right];

    pub fn arrow(self) -> &'static str {
        match self {
            DuelKey::Up => "↑",
            DuelKey::Down => "↓",
            DuelKey::Left => "←",
            DuelKey::Right => "→",
            DuelKey::Action => "␣",
            DuelKey::Char(_) | DuelKey::Answer(_) | DuelKey::Pick(_) => "·",
        }
    }

    /// Skincare step each arrow stands for.
    pub fn step_name(self) -> &'static str {
        match self {
            DuelKey::Up => "Tônico",
            DuelKey::Down => "Sérum",
            DuelKey::Left => "Hidratante",
            DuelKey::Right => "Protetor",
            DuelKey::Action | DuelKey::Char(_) | DuelKey::Answer(_) | DuelKey::Pick(_) => "",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    /// Players take turns picking the minigames.
    Draft,
    Countdown,
    /// Skincare and typing: the routine or the phrase is on screen before play.
    Show,
    Play,
    RoundOver,
    Finished,
}

/// Cursor position on the angle bar after `elapsed` seconds, bouncing in [0, 1].
pub fn angle_pos(elapsed: f64) -> f64 {
    let x = (elapsed.max(0.0) * ANGLE_SPEED) % 2.0;
    if x <= 1.0 { x } else { 2.0 - x }
}

/// Best (smallest) distance, or a full miss when nothing was tried.
fn best_attempt(attempts: &[f64]) -> f64 {
    attempts.iter().copied().fold(1.0, f64::min)
}

/// Words typing battles are made of: mogging and looksmaxxing slang, no accents so any keyboard works.
pub const TYPING_WORDS: &[&str] = &[
    "mog", "mogger", "mewing", "jawline", "looksmax", "gigachad", "chad", "sigma", "aura", "rizz", "frame",
    "maxxing", "queixo", "maxilar", "simetria", "postura", "olhar", "sorriso", "barba", "cabelo", "creatina",
    "academia", "dieta", "skincare", "protetor", "tonico", "hidratante", "glow", "gua", "sha", "canthal",
    "tilt", "hunter", "eyes", "mandibula", "ascender", "moggado", "frameado", "bonesmash", "pescoco",
];

/// A random phrase of `TYPING_WORD_COUNT` different words.
pub fn typing_text(rng: &mut impl Rng) -> String {
    TYPING_WORDS.choose_multiple(rng, TYPING_WORD_COUNT).copied().collect::<Vec<_>>().join(" ")
}

/// Words per minute and accuracy (0 to 1) for `typed` correct characters and `errors` wrong keys over `ms`.
pub fn typing_stats(typed: usize, errors: u32, ms: f64) -> (f64, f64) {
    let wpm = if ms > 0.0 { typed as f64 / CHARS_PER_WORD / (ms / 60_000.0) } else { 0.0 };
    let keys = typed as f64 + errors as f64;
    let accuracy = if keys > 0.0 { typed as f64 / keys } else { 1.0 };
    (wpm, accuracy)
}

/// One macros sum or difference, in grams.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MacroProblem {
    pub a: i64,
    pub b: i64,
    pub plus: bool,
}

impl MacroProblem {
    fn random(rng: &mut impl Rng) -> MacroProblem {
        let (a, b) = (rng.gen_range(MACROS_OPERAND), rng.gen_range(MACROS_OPERAND));
        let plus = rng.gen_bool(0.5);
        let (a, b) = if plus { (a, b) } else { (a.max(b), a.min(b)) };
        MacroProblem { a, b, plus }
    }

    pub fn answer(self) -> i64 {
        if self.plus { self.a + self.b } else { self.a - self.b }
    }

    pub fn text(self) -> String {
        format!("{}g {} {}g", self.a, if self.plus { "+" } else { "-" }, self.b)
    }
}

/// When beat `i` of the rhythm round falls, in ms after play starts.
pub fn rhythm_beat_ms(i: usize) -> f64 {
    RHYTHM_LEAD_MS + i as f64 * RHYTHM_INTERVAL_MS
}

/// How long a rhythm round plays.
pub fn rhythm_len() -> Duration {
    Duration::from_secs_f64((rhythm_beat_ms(RHYTHM_BEATS - 1) + RHYTHM_WINDOW_MS + RHYTHM_INTERVAL_MS) / 1000.0)
}

/// An empty side of a rhythm round.
pub fn rhythm_fresh() -> RhythmProgress {
    RhythmProgress { hits: 0, perfects: 0, misses: 0, score: 0.0, judged: vec![None; RHYTHM_BEATS] }
}

/// Points for a hit `error_ms` away from its beat.
fn rhythm_points(error_ms: f64) -> f64 {
    1.0 - error_ms.abs() / (2.0 * RHYTHM_WINDOW_MS)
}

/// Hunter eyes schedule: (start in ms after play starts, is it a target).
fn hunter_schedule(rng: &mut impl Rng) -> Vec<(f64, bool)> {
    let mut at = HUNTER_FIRST_MS;
    let mut schedule: Vec<(f64, bool)> = (0..HUNTER_STIMULI)
        .map(|_| {
            let item = (at, rng.gen_bool(HUNTER_TARGET_CHANCE));
            at += HUNTER_SHOW_MS + rng.gen_range(HUNTER_GAP_MS);
            item
        })
        .collect();
    while schedule.iter().filter(|(_, target)| *target).count() < HUNTER_MIN_TARGETS {
        let i = rng.gen_range(0..schedule.len());
        schedule[i].1 = true;
    }
    schedule
}

fn mean(values: &[f64]) -> Option<f64> {
    (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
}

fn percent(distance: f64) -> String {
    format!("{:.1}%", distance * 100.0)
}

/// Tolerance for ranking numbers that only need to differ.
pub(crate) const EXACT: f64 = 1e-9;

/// Each side's ranking key for a round, compared number by number (higher first).
/// Numbers closer than their tolerance count as equal, and NaN never decides.
pub struct Standings {
    keys: Vec<Vec<f64>>,
    ties: Vec<f64>,
}

impl Standings {
    /// `ties[i]` is the tolerance of the `i`-th number; missing ones use `EXACT`.
    pub fn new(keys: Vec<Vec<f64>>, ties: &[f64]) -> Standings {
        Standings { keys, ties: ties.to_vec() }
    }

    /// Whether two values of the `i`-th number are far enough apart to rank.
    fn differs(&self, i: usize, x: f64, y: f64) -> bool {
        (x - y).abs() >= self.ties.get(i).copied().unwrap_or(EXACT)
    }

    fn cmp(&self, a: usize, b: usize) -> Ordering {
        for (i, (x, y)) in self.keys[a].iter().zip(&self.keys[b]).enumerate() {
            if self.differs(i, *x, *y) {
                return x.total_cmp(y);
            }
        }
        Ordering::Equal
    }

    /// Rivals each side beat: its points for the round.
    pub fn points(&self) -> Vec<u32> {
        let n = self.keys.len();
        (0..n).map(|s| (0..n).filter(|o| self.cmp(s, *o) == Ordering::Greater).count() as u32).collect()
    }

    /// The side that beat every other one.
    pub fn winner(&self) -> Option<usize> {
        let n = self.keys.len();
        self.points().iter().position(|p| *p as usize == n - 1)
    }

    /// Whether the first number alone tells the first two sides apart.
    fn by_first(&self) -> bool {
        self.differs(0, self.keys[0][0], self.keys[1][0])
    }

    /// Sides from best to worst with their place (1 is the best); tied sides share a place.
    fn places(&self) -> Vec<(usize, usize)> {
        let n = self.keys.len();
        let mut order: Vec<(usize, usize)> =
            (0..n).map(|s| (s, 1 + (0..n).filter(|o| self.cmp(*o, s) == Ordering::Greater).count())).collect();
        order.sort_by_key(|(s, place)| (*place, *s));
        order
    }

    /// Sides that left go to the bottom.
    fn drop_out(&mut self, out: &[bool]) {
        for (key, _) in self.keys.iter_mut().zip(out).filter(|(_, out)| **out) {
            *key = vec![f64::NEG_INFINITY];
        }
    }
}

/// Staredown keys: a press before the GO (or too fast) ranks below no press at all.
const STAREDOWN_EARLY: f64 = -1e12;
const STAREDOWN_NONE: f64 = -1e9;

enum Round {
    /// `reaction` in ms after the GO; `early` is a press before it (or too fast), in ms; `first` is when the first valid one arrived.
    Staredown { go_at: Instant, reaction: Vec<Option<f64>>, early: Vec<Option<f64>>, first: Option<Instant> },
    /// `digits` is the memory variant: steps are digit keys flashed one at a time.
    /// `out` is the level length and steps of a side knocked out at an earlier level.
    Skincare {
        sequence: Vec<DuelKey>,
        progress: Vec<usize>,
        failed: Vec<bool>,
        done_at: Vec<Option<Instant>>,
        out: Vec<Option<(usize, usize)>>,
        digits: bool,
    },
    /// `last` is the latest counted press; its time breaks ties (who got there first).
    Mewing { count: Vec<u32>, last: Vec<Option<(DuelKey, Instant)>> },
    Angle { target: f64, attempts: Vec<Vec<f64>> },
    /// `judged[side][beat]` is the timing error of the press that hit that beat.
    Rhythm { judged: Vec<Vec<Option<f64>>>, misses: Vec<u32> },
    Hunter { schedule: Vec<(f64, bool)>, reflexes: Reflexes },
    /// `last_ms` is when each side solved its latest problem.
    Macros {
        problems: Vec<MacroProblem>,
        solved: Vec<usize>,
        errors: Vec<u32>,
        last_ms: Vec<Option<f64>>,
        first_done: Option<Instant>,
    },
    /// Times are ms since play started; `first_done` is when the first finisher's key arrived.
    Typing {
        text: Vec<char>,
        typed: Vec<usize>,
        errors: Vec<u32>,
        done_ms: Vec<Option<f64>>,
        first_done: Option<Instant>,
    },
    Contest(Box<dyn Contest>),
}

/// A draft pick as one side sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftPick {
    pub mine: bool,
    pub game: Minigame,
    /// Drawn by the server: asked for or out of time.
    pub random: bool,
}

/// Picks so far, as (side, game, random), and whose turn it is.
struct Draft {
    picks: Vec<(usize, Minigame, bool)>,
    turn: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RoundResult {
    pub winner: Option<usize>,
    pub text: String,
}

/// Minigames a duel of `n` players can draw.
fn pool(n: usize) -> impl Iterator<Item = Minigame> {
    Minigame::ALL.into_iter().filter(move |g| n == 2 || !g.duo_only())
}

pub struct Duel {
    pub players: Vec<PlayerId>,
    pub names: Vec<String>,
    /// What each player bet; the winner takes all of them.
    pub stake: f64,
    /// Points: rivals beaten, summed over the rounds (one per round won in a two-player duel).
    pub wins: Vec<u32>,
    /// Players who gave up; they can't win and their bet stays in the pot.
    pub out: Vec<bool>,
    pub rounds: u32,
    pub game: Minigame,
    pub phase: Phase,
    pub phase_started: Instant,
    pub phase_len: Duration,
    pub last: Option<RoundResult>,
    /// Rounds played to the end (not counting a forfeit).
    pub played: u32,
    /// Set once the duel is decided: `Some(None)` is a draw.
    pub outcome: Option<Option<usize>>,
    settled: bool,
    upcoming: Vec<Minigame>,
    round: Round,
    draft: Option<Draft>,
    /// Every minigame played so far, so sudden death draws a new one.
    played_games: Vec<Minigame>,
    /// Each finished round as (minigame, the side that beat everyone).
    pub history: Vec<(Minigame, Option<usize>)>,
    /// Counts phase changes; clients use it to tell a new phase from a repeat.
    pub seq: u32,
}

impl Duel {
    /// Duel with drawn minigames, or a draft when `draft` (two players only); who picks first is drawn.
    pub fn new(
        players: impl Into<Vec<PlayerId>>,
        names: impl Into<Vec<String>>,
        stake: f64,
        draft: bool,
        now: Instant,
        rng: &mut impl Rng,
    ) -> Duel {
        let mut duel = Duel::blank(players.into(), names.into(), stake, now);
        let n = duel.players.len();
        duel.draft = Some(Draft { picks: Vec::new(), turn: rng.gen_range(0..n) });
        duel.set_phase(Phase::Draft, DRAFT_PICK_TIME, now);
        if !draft || n > 2 {
            while let Some(turn) = duel.draft.as_ref().filter(|d| d.picks.len() < DUEL_ROUNDS as usize).map(|d| d.turn) {
                duel.pick(turn, None, now, rng);
            }
        }
        duel
    }

    /// Duel playing `games` in this order first (then random ones), with no draft.
    pub fn with_games(
        players: impl Into<Vec<PlayerId>>,
        names: impl Into<Vec<String>>,
        stake: f64,
        games: &[Minigame],
        now: Instant,
        rng: &mut impl Rng,
    ) -> Duel {
        let mut duel = Duel::blank(players.into(), names.into(), stake, now);
        duel.upcoming = games.iter().rev().copied().collect();
        duel.next_round(now, rng);
        duel
    }

    fn blank(players: Vec<PlayerId>, names: Vec<String>, stake: f64, now: Instant) -> Duel {
        let n = players.len();
        assert!((2..=DUEL_MAX_PLAYERS).contains(&n) && names.len() == n, "a duel has 2 to {DUEL_MAX_PLAYERS} named players");
        Duel {
            players,
            names,
            stake,
            wins: vec![0; n],
            out: vec![false; n],
            rounds: 0,
            game: Minigame::Staredown,
            phase: Phase::Countdown,
            phase_started: now,
            phase_len: DUEL_COUNTDOWN,
            last: None,
            played: 0,
            outcome: None,
            settled: false,
            upcoming: Vec::new(),
            round: Round::Mewing { count: vec![0; n], last: vec![None; n] },
            draft: None,
            played_games: Vec::new(),
            history: Vec::new(),
            seq: 0,
        }
    }

    fn n(&self) -> usize {
        self.players.len()
    }

    /// Sides still playing.
    fn active(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.n()).filter(|s| !self.out[*s])
    }

    /// The rival `side` sees next to itself: the one with the most points (the first one on a tie).
    pub fn rival(&self, side: usize) -> usize {
        (0..self.n()).filter(|o| *o != side).max_by_key(|o| (self.wins[*o], std::cmp::Reverse(*o))).expect("at least two players")
    }

    /// Everything the pot holds.
    pub fn pot(&self) -> f64 {
        self.stake * self.n() as f64
    }

    /// Changes whenever clients need a fresh state.
    pub fn revision(&self, now: Instant) -> (u32, u32) {
        let stamp = match &self.round {
            Round::Contest(c) if self.phase == Phase::Play => c.stamp(self.elapsed_ms(now)),
            _ => 0,
        };
        (self.seq, stamp)
    }

    fn elapsed_ms(&self, now: Instant) -> f64 {
        now.saturating_duration_since(self.phase_started).as_secs_f64() * 1000.0
    }

    /// A draft pick by `side`; `None` draws one of the minigames still free.
    fn pick(&mut self, side: usize, choice: Option<Minigame>, now: Instant, rng: &mut impl Rng) {
        let n = self.n();
        let Some(draft) = &mut self.draft else { return };
        if side != draft.turn || draft.picks.len() >= DUEL_ROUNDS as usize {
            return;
        }
        let taken = |g: Minigame| draft.picks.iter().any(|p| p.1 == g);
        let (game, random) = match choice {
            Some(g) if taken(g) || !pool(n).any(|p| p == g) => return,
            Some(g) => (g, false),
            None => (pool(n).filter(|g| !taken(*g)).choose(rng).expect("more games than picks"), true),
        };
        draft.picks.push((side, game, random));
        draft.turn = (side + 1) % n;
        if draft.picks.len() >= DUEL_ROUNDS as usize {
            self.upcoming = draft.picks.iter().rev().map(|p| p.1).collect();
            self.next_round(now, rng);
        } else {
            self.set_phase(Phase::Draft, DRAFT_PICK_TIME, now);
        }
    }

    pub fn side(&self, id: PlayerId) -> Option<usize> {
        self.players.iter().position(|p| *p == id)
    }

    fn set_phase(&mut self, phase: Phase, len: Duration, now: Instant) {
        self.phase = phase;
        self.phase_started = now;
        self.phase_len = len;
        self.seq += 1;
    }

    fn next_round(&mut self, now: Instant, rng: &mut impl Rng) {
        let n = self.n();
        let fresh = |played: &[Minigame], rng: &mut _| pool(n).filter(|g| !played.contains(g)).choose(rng).unwrap_or(Minigame::Staredown);
        self.game = self.upcoming.pop().unwrap_or_else(|| fresh(&self.played_games, rng));
        self.played_games.push(self.game);
        self.rounds += 1;
        self.round = Round::Mewing { count: vec![0; n], last: vec![None; n] };
        self.set_phase(Phase::Countdown, DUEL_COUNTDOWN, now);
    }

    fn start_play(&mut self, now: Instant, rng: &mut impl Rng) {
        let n = self.n();
        match self.game {
            Minigame::Staredown => {
                let wait = Duration::from_secs_f64(rng.gen_range(STAREDOWN_WAIT_SECS));
                self.round = Round::Staredown { go_at: now + wait, reaction: vec![None; n], early: vec![None; n], first: None };
                self.set_phase(Phase::Play, wait + STAREDOWN_LIMIT, now);
            }
            Minigame::Skincare | Minigame::Memory => {
                let digits = self.game == Minigame::Memory;
                let start = if digits { MEMORY_START_LEN } else { SKINCARE_START_LEN };
                let sequence = (0..start).map(|_| routine_step(digits, rng)).collect();
                self.round =
                    Round::Skincare { sequence, progress: vec![0; n], failed: vec![false; n], done_at: vec![None; n], out: vec![None; n], digits };
                self.show_routine(now);
            }
            Minigame::Mewing => {
                self.round = Round::Mewing { count: vec![0; n], last: vec![None; n] };
                self.set_phase(Phase::Play, MEWING_DURATION, now);
            }
            Minigame::Angle => {
                self.round = Round::Angle { target: rng.gen_range(ANGLE_TARGET), attempts: vec![Vec::new(); n] };
                self.set_phase(Phase::Play, ANGLE_LIMIT, now);
            }
            Minigame::Typing => {
                self.round = Round::Typing {
                    text: typing_text(rng).chars().collect(),
                    typed: vec![0; n],
                    errors: vec![0; n],
                    done_ms: vec![None; n],
                    first_done: None,
                };
                self.set_phase(Phase::Show, TYPING_SHOW, now);
            }
            Minigame::Rhythm => {
                self.round = Round::Rhythm { judged: vec![vec![None; RHYTHM_BEATS]; n], misses: vec![0; n] };
                self.set_phase(Phase::Play, rhythm_len(), now);
            }
            Minigame::Hunter => {
                let schedule = hunter_schedule(rng);
                let len = schedule.last().map_or(0.0, |(at, _)| at + HUNTER_SHOW_MS) + RHYTHM_INTERVAL_MS;
                self.round = Round::Hunter { reflexes: Reflexes::new(n, schedule.len()), schedule };
                self.set_phase(Phase::Play, Duration::from_secs_f64(len / 1000.0), now);
            }
            Minigame::Macros => {
                let problems = (0..MACROS_PROBLEMS).map(|_| MacroProblem::random(rng)).collect();
                self.round =
                    Round::Macros { problems, solved: vec![0; n], errors: vec![0; n], last_ms: vec![None; n], first_done: None };
                self.set_phase(Phase::Show, MACROS_SHOW, now);
            }
            game => {
                let contest = contests::create(game, n, rng).expect("every other minigame is a contest");
                let len = contest.play_len();
                self.round = Round::Contest(contest);
                self.set_phase(Phase::Play, len, now);
            }
        }
    }

    /// How long play lasts once the Show phase is over.
    fn play_len_after_show(&self) -> Duration {
        match &self.round {
            Round::Skincare { sequence, digits, .. } => routine_input_len(*digits, sequence.len()),
            Round::Typing { .. } => TYPING_LIMIT,
            Round::Macros { .. } => MACROS_LIMIT,
            _ => Duration::ZERO,
        }
    }

    fn show_routine(&mut self, now: Instant) {
        let Round::Skincare { sequence, digits, .. } = &self.round else { return };
        let len = sequence.len() as u32;
        let show = if *digits { MEMORY_LEAD + MEMORY_FLASH * len } else { SKINCARE_SHOW_BASE + SKINCARE_SHOW_STEP * len };
        self.set_phase(Phase::Show, show, now);
    }

    /// Ends the round, ranking every side by the round's own numbers.
    fn conclude(&mut self, now: Instant) {
        let end = self.elapsed_ms(now).min(self.phase_len.as_secs_f64() * 1000.0);
        let mut standings = self.standings(end);
        standings.drop_out(&self.out);
        let winner = standings.winner();
        let text = if self.n() == 2 {
            self.duo_text(winner, standings.by_first(), end)
        } else {
            let places: Vec<String> =
                standings.places().iter().map(|(s, place)| format!("{place}º {} ({})", self.names[*s], self.line(*s, end))).collect();
            places.join(" · ")
        };
        for (w, p) in self.wins.iter_mut().zip(&standings.points()) {
            *w += p;
        }
        self.played += 1;
        self.history.push((self.game, winner));
        self.last = Some(RoundResult { winner, text });
        self.set_phase(Phase::RoundOver, DUEL_ROUND_PAUSE, now);
    }

    /// Each side's ranking key for the round being played; `end` is when play stopped, in ms.
    fn standings(&self, end: f64) -> Standings {
        let n = self.n();
        let since = |at: Instant| at.saturating_duration_since(self.phase_started).as_secs_f64();
        let keys: Vec<Vec<f64>> = match &self.round {
            Round::Contest(c) => return c.standings(end),
            Round::Hunter { reflexes, .. } => return reflexes.standings(),
            Round::Staredown { reaction, early, .. } => {
                let key = |s: usize| vec![if early[s].is_some() { STAREDOWN_EARLY } else { reaction[s].map_or(STAREDOWN_NONE, |r| -r) }];
                return Standings::new((0..n).map(key).collect(), &[0.5]);
            }
            Round::Skincare { sequence, progress, done_at, out, .. } => (0..n)
                .map(|s| match (out[s], done_at[s]) {
                    (Some((level, steps)), _) => vec![level as f64 - 1.0, steps as f64],
                    (None, Some(at)) => vec![sequence.len() as f64, -since(at)],
                    (None, None) => vec![sequence.len() as f64 - 1.0, progress[s] as f64],
                })
                .collect(),
            Round::Mewing { count, last } => {
                (0..n).map(|s| vec![count[s] as f64, last[s].map_or(f64::NAN, |(_, at)| -since(at))]).collect()
            }
            Round::Angle { attempts, .. } => (0..n)
                .map(|s| {
                    let mut v = attempts[s].clone();
                    v.resize(ANGLE_ATTEMPTS, 1.0);
                    v.sort_by(f64::total_cmp);
                    v.iter().map(|d| -d).collect()
                })
                .collect(),
            Round::Typing { .. } => (0..n).map(|s| vec![self.typing(s, end).2]).collect(),
            Round::Rhythm { .. } => (0..n).map(|s| vec![self.rhythm(s).0]).collect(),
            Round::Macros { solved, errors, last_ms, .. } => (0..n)
                .map(|s| {
                    let solved_some = |v: f64| if solved[s] > 0 { v } else { f64::NAN };
                    vec![solved[s] as f64, solved_some(-last_ms[s].unwrap_or(f64::MAX)), solved_some(-(errors[s] as f64))]
                })
                .collect(),
        };
        Standings::new(keys, &[])
    }

    /// Typing numbers for `side`: words per minute, accuracy and score (the first finisher gets a bonus).
    fn typing(&self, side: usize, end: f64) -> (f64, f64, f64) {
        let Round::Typing { typed, errors, done_ms, .. } = &self.round else { return (0.0, 0.0, 0.0) };
        let (wpm, acc) = typing_stats(typed[side], errors[side], done_ms[side].unwrap_or(end));
        let bonus = if self.first_typist() == Some(side) { TYPING_FINISH_BONUS } else { 1.0 };
        (wpm, acc, wpm * acc * acc * bonus)
    }

    /// Who finished typing first (the lower side on a tie).
    fn first_typist(&self) -> Option<usize> {
        let Round::Typing { done_ms, .. } = &self.round else { return None };
        (0..self.n()).filter_map(|s| done_ms[s].map(|t| (s, t))).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(s, _)| s)
    }

    /// Rhythm score and beats hit by `side`.
    fn rhythm(&self, side: usize) -> (f64, usize) {
        let Round::Rhythm { judged, misses } = &self.round else { return (0.0, 0) };
        let hits = judged[side].iter().flatten();
        let (points, count) = hits.fold((0.0, 0), |(p, c), e| (p + rhythm_points(*e), c + 1));
        (points - misses[side] as f64 * RHYTHM_MISS_PENALTY + 0.0, count)
    }

    /// One side's numbers in the group ranking.
    fn line(&self, s: usize, end: f64) -> String {
        if self.out[s] {
            return "desistiu".into();
        }
        match &self.round {
            Round::Contest(c) => c.line(s, end),
            Round::Hunter { reflexes, .. } => reflexes.line(s),
            Round::Staredown { reaction, early, .. } => match (early[s], reaction[s]) {
                (Some(_), _) => "antes da hora".into(),
                (None, Some(r)) => format!("{r:.0} ms"),
                (None, None) => "não reagiu".into(),
            },
            Round::Skincare { sequence, progress, done_at, out, .. } => match (out[s], done_at[s]) {
                (Some((level, steps)), _) => format!("caiu na rotina de {level}, {steps} passos"),
                (None, Some(_)) => format!("rotina de {} completa", sequence.len()),
                (None, None) => format!("{} de {} passos", progress[s], sequence.len()),
            },
            Round::Mewing { count, .. } => format!("{} mewadas", count[s]),
            Round::Angle { attempts, .. } => format!("{} do alvo", percent(best_attempt(&attempts[s]))),
            Round::Typing { .. } => {
                let (wpm, acc, _) = self.typing(s, end);
                format!("{wpm:.0} ppm, {:.0}%", acc * 100.0)
            }
            Round::Rhythm { .. } => format!("{:.1} pontos", self.rhythm(s).0),
            Round::Macros { solved, errors, .. } => format!("{} contas, {} erros", solved[s], errors[s]),
        }
    }

    /// Result text of a two-player round; `by_first` when the main number decided it.
    fn duo_text(&self, winner: Option<usize>, by_first: bool, end: f64) -> String {
        let names = &self.names;
        match &self.round {
            Round::Contest(c) => c.duo_text(names, winner, by_first, end),
            Round::Hunter { reflexes, .. } => reflexes.duo_text(names, winner, "tem o olhar de caçador", "Olhares iguais"),
            Round::Staredown { reaction, early, .. } => {
                if let Some((s, r)) = (0..2).find_map(|s| early[s].map(|r| (s, r))) {
                    return if r < 0.0 {
                        format!("{} piscou antes da hora!", names[s])
                    } else {
                        format!("{} chutou o tempo ({r:.0} ms é rápido demais para um humano).", names[s])
                    };
                }
                match (winner, reaction[0], reaction[1]) {
                    (_, None, None) => "Ninguém reagiu a tempo. Empate.".into(),
                    (None, Some(x), _) => format!("Reação idêntica ({x:.0} ms). Empate."),
                    (Some(w), Some(x), Some(y)) => {
                        let (mine, theirs) = if w == 0 { (x, y) } else { (y, x) };
                        format!("{} reagiu primeiro: {mine:.0} ms x {theirs:.0} ms.", names[w])
                    }
                    (Some(w), ..) => format!("{} reagiu em {:.0} ms.", names[w], reaction[w].unwrap_or(0.0)),
                    (None, None, Some(_)) => "Empate.".into(),
                }
            }
            Round::Skincare { sequence, progress, done_at, .. } => {
                let len = sequence.len();
                match (winner, done_at[0].is_some(), done_at[1].is_some()) {
                    (Some(w), true, true) => format!("Rotina completa dos dois; {} foi mais rápido.", names[w]),
                    (None, true, true) => "Rotina completa dos dois, no mesmo tempo. Empate.".into(),
                    (Some(w), true, false) | (Some(w), false, true) => format!("{} completou a rotina de {len} passos.", names[w]),
                    (Some(w), ..) => format!("{} foi mais longe na rotina: {} x {} passos.", names[w], progress[w], progress[1 - w]),
                    (None, ..) => format!("Os dois erraram no passo {}. Empate.", progress[0] + 1),
                }
            }
            Round::Mewing { count, .. } => match (winner, by_first) {
                (Some(w), true) => format!("{} mewou mais: {} x {}.", names[w], count[w], count[1 - w]),
                (Some(w), false) => format!("Empate em {} mewadas; {} chegou lá primeiro.", count[0], names[w]),
                (None, _) => format!("Empate na mewada: {} x {}.", count[0], count[1]),
            },
            Round::Angle { attempts, .. } => {
                let best = [best_attempt(&attempts[0]), best_attempt(&attempts[1])];
                match (winner, by_first) {
                    (Some(w), true) => format!("{} achou o ângulo: {} x {} do alvo.", names[w], percent(best[w]), percent(best[1 - w])),
                    (Some(w), false) => format!("Mesma melhor tentativa ({}); {} foi melhor nas outras.", percent(best[w]), names[w]),
                    (None, _) => "Mesmas distâncias do alvo. Empate.".to_string(),
                }
            }
            Round::Typing { typed, .. } => {
                let line = |s: usize| {
                    let (wpm, acc, _) = self.typing(s, end);
                    format!("{wpm:.0} ppm e {:.0}% de precisão", acc * 100.0)
                };
                match winner {
                    _ if typed.iter().all(|t| *t == 0) => "Ninguém digitou nada. Empate.".to_string(),
                    None => format!("Mesma pontuação ({}). Empate.", line(0)),
                    Some(w) => {
                        let finisher = if self.first_typist() == Some(w) { " e terminou primeiro" } else { "" };
                        format!("{} digitou melhor{finisher}: {} x {}.", names[w], line(w), line(1 - w))
                    }
                }
            }
            Round::Rhythm { .. } => {
                let line = |s: usize| {
                    let (points, hits) = self.rhythm(s);
                    format!("{points:.1} pontos ({hits}/{RHYTHM_BEATS} batidas)")
                };
                match winner {
                    Some(w) => format!("{} ficou no ritmo: {} x {}.", names[w], line(w), line(1 - w)),
                    None => format!("Mesmo ritmo: {}. Empate.", line(0)),
                }
            }
            Round::Macros { solved, errors, .. } => {
                let line = |s: usize| format!("{} contas, {} erros", solved[s], errors[s]);
                match winner {
                    Some(w) if solved[0] == solved[1] => format!("Mesmas contas ({}); {} terminou antes.", line(w), names[w]),
                    Some(w) => format!("{} fechou os macros: {} x {}.", names[w], line(w), line(1 - w)),
                    None => format!("Macros empatados: {}.", line(0)),
                }
            }
        }
    }

    /// Takes `side` out: it can't win and its bet stays in the pot; the last one left wins.
    pub fn forfeit(&mut self, side: usize, now: Instant) {
        if self.outcome.is_some() || self.out[side] {
            return;
        }
        self.out[side] = true;
        let text = format!("{} desistiu do duelo.", self.names[side]);
        let left: Vec<usize> = self.active().collect();
        if let [winner] = left[..] {
            self.outcome = Some(Some(winner));
            self.last = Some(RoundResult { winner: Some(winner), text });
            self.set_phase(Phase::Finished, DUEL_RESULT_SHOW, now);
        } else {
            self.last = Some(RoundResult { winner: None, text });
            self.seq += 1;
        }
    }

    /// Returns the outcome exactly once, when it is time to move the stakes.
    pub fn take_settlement(&mut self) -> Option<Option<usize>> {
        if self.settled {
            return None;
        }
        let outcome = self.outcome?;
        self.settled = true;
        Some(outcome)
    }

    /// Advances timers; true once the result screen is over and the duel can be removed.
    pub fn tick(&mut self, now: Instant, rng: &mut impl Rng) -> bool {
        let over = now.saturating_duration_since(self.phase_started) >= self.phase_len;
        match self.phase {
            Phase::Draft if over => {
                if let Some(turn) = self.draft.as_ref().map(|d| d.turn) {
                    self.pick(turn, None, now, rng);
                }
            }
            Phase::Countdown if over => self.start_play(now, rng),
            Phase::Show if over => self.set_phase(Phase::Play, self.play_len_after_show(), now),
            Phase::Play if now.saturating_duration_since(self.phase_started) >= self.phase_len + INPUT_GRACE => self.time_up(now, rng),
            Phase::Play => {
                self.decide_staredown(now);
                self.decide_typing(now);
                self.decide_macros(now);
            }
            Phase::RoundOver if over => {
                let mut order: Vec<usize> = self.active().collect();
                order.sort_by_key(|s| std::cmp::Reverse(self.wins[*s]));
                let lead = self.wins[order[0]] - self.wins[order[1]];
                // Decided once nobody can catch up in the regulation rounds left (a round moves a gap by up to n - 1).
                let left = DUEL_ROUNDS.saturating_sub(self.rounds) * (self.n() as u32 - 1);
                if lead > left {
                    self.finish(Some(order[0]), now);
                } else if self.rounds >= DUEL_MAX_ROUNDS {
                    self.finish(None, now);
                } else {
                    self.next_round(now, rng);
                }
            }
            Phase::Finished => return over,
            _ => {}
        }
        false
    }

    fn finish(&mut self, winner: Option<usize>, now: Instant) {
        self.outcome = Some(winner);
        self.set_phase(Phase::Finished, DUEL_RESULT_SHOW, now);
    }

    fn time_up(&mut self, now: Instant, rng: &mut impl Rng) {
        match &self.round {
            Round::Skincare { .. } => self.skincare_level_end(now, rng),
            _ => self.conclude(now),
        }
    }

    /// Ends the macros round once everyone finished, or the chase time after the first finisher ran out.
    fn decide_macros(&mut self, now: Instant) {
        let Round::Macros { problems, solved, first_done, .. } = &self.round else { return };
        let all = self.active().all(|s| solved[s] == problems.len());
        let chased = first_done.is_some_and(|t| now.saturating_duration_since(t) >= MACROS_CHASE);
        if all || chased {
            self.conclude(now);
        }
    }

    /// Ends the typing battle once everyone finished, or the chase time after the first finisher ran out.
    fn decide_typing(&mut self, now: Instant) {
        let Round::Typing { done_ms, first_done, .. } = &self.round else { return };
        let all = self.active().all(|s| done_ms[s].is_some());
        let chased = first_done.is_some_and(|t| now.saturating_duration_since(t) >= TYPING_CHASE);
        if all || chased {
            self.conclude(now);
        }
    }

    /// Ends a routine level: the ones who passed go on while at least two did, the others are out.
    fn skincare_level_end(&mut self, now: Instant, rng: &mut impl Rng) {
        let Round::Skincare { out, done_at, .. } = &self.round else { return };
        let passed = self.active().filter(|s| out[*s].is_none() && done_at[*s].is_some()).count();
        let Round::Skincare { sequence, progress, failed, done_at, out, digits } = &mut self.round else { return };
        let len = sequence.len();
        let max = if *digits { MEMORY_MAX_LEN } else { SKINCARE_MAX_LEN };
        if passed >= 2 && len < max {
            for s in 0..out.len() {
                if out[s].is_none() && done_at[s].is_none() {
                    out[s] = Some((len, progress[s]));
                }
            }
            sequence.push(routine_step(*digits, rng));
            progress.fill(0);
            done_at.fill(None);
            failed.fill(false);
            self.show_routine(now);
            return;
        }
        self.conclude(now);
    }

    /// When the press really happened, in ms since the phase started.
    ///
    /// The client's own reading (`at_ms`) is used, but only inside what the server saw:
    /// no later than the packet's arrival, no earlier than arrival minus the player's worst round trip.
    pub fn press_time(&self, at_ms: Option<f64>, now: Instant, rtt_max_ms: f64) -> f64 {
        let seen = now.saturating_duration_since(self.phase_started).as_secs_f64() * 1000.0;
        let low = seen - rtt_max_ms.max(0.0) - CLIENT_TIME_SLACK_MS;
        match at_ms.filter(|v| v.is_finite()) {
            Some(at) => at.clamp(low, seen + CLIENT_TIME_AHEAD_MS),
            None => seen,
        }
    }

    /// Settles the staredown once everyone pressed, only one is still in, or no faster press can still be on its way.
    fn decide_staredown(&mut self, now: Instant) {
        let Round::Staredown { reaction, early, first, .. } = &self.round else { return };
        let all = self.active().all(|s| reaction[s].is_some() || early[s].is_some());
        let alone = self.active().filter(|s| early[*s].is_none()).count() <= 1;
        let waited = first.is_some_and(|t| now.saturating_duration_since(t) >= STAREDOWN_DECIDE_SLACK);
        if all || alone || waited {
            self.conclude(now);
        }
    }

    /// Applies one key press from `side`; `at_ms` is the client's own timing (see `press_time`).
    pub fn input(&mut self, side: usize, key: DuelKey, at_ms: Option<f64>, rtt_max_ms: f64, now: Instant, rng: &mut impl Rng) {
        if side >= self.n() || self.out[side] {
            return;
        }
        if let (Phase::Draft, DuelKey::Pick(choice)) = (self.phase, key) {
            return self.pick(side, choice, now, rng);
        }
        if self.phase != Phase::Play {
            return;
        }
        let t = self.press_time(at_ms, now, rtt_max_ms);
        if t > self.phase_len.as_secs_f64() * 1000.0 {
            return; // pressed after the phase ended; arrived during the grace period
        }
        if t < 0.0 && !matches!(self.round, Round::Staredown { .. }) {
            return; // pressed before the phase started; only the staredown punishes that
        }
        let at = self.phase_started + Duration::from_secs_f64(t.max(0.0) / 1000.0);
        match &mut self.round {
            Round::Staredown { go_at, reaction, early, first } => {
                if key != DuelKey::Action || reaction[side].is_some() || early[side].is_some() {
                    return;
                }
                let go_ms = go_at.saturating_duration_since(self.phase_started).as_secs_f64() * 1000.0;
                let r = t - go_ms;
                if r < HUMAN_REACTION_MIN_MS {
                    early[side] = Some(r);
                } else {
                    reaction[side] = Some(r);
                    first.get_or_insert(now);
                }
                self.decide_staredown(now);
            }
            Round::Skincare { sequence, progress, failed, done_at, out, digits } => {
                let step = if *digits { matches!(key, DuelKey::Char('1'..='8')) } else { DuelKey::ARROWS.contains(&key) };
                if !step || out[side].is_some() || failed[side] || done_at[side].is_some() {
                    return;
                }
                if sequence[progress[side]] == key {
                    progress[side] += 1;
                    if progress[side] == sequence.len() {
                        done_at[side] = Some(at);
                    }
                } else {
                    failed[side] = true;
                }
                let finished = (0..out.len()).all(|s| out[s].is_some() || failed[s] || done_at[s].is_some() || self.out[s]);
                if finished {
                    self.skincare_level_end(now, rng);
                }
            }
            Round::Mewing { count, last } => {
                if !matches!(key, DuelKey::Left | DuelKey::Right) {
                    return;
                }
                let counts = match last[side] {
                    None => true,
                    Some((prev, when)) => prev != key && at.saturating_duration_since(when) >= MEWING_MIN_GAP,
                };
                if counts {
                    count[side] += 1;
                    last[side] = Some((key, at));
                }
            }
            Round::Typing { text, typed, errors, done_ms, first_done } => {
                let DuelKey::Char(c) = key else { return };
                if done_ms[side].is_some() {
                    return;
                }
                if text[typed[side]].eq_ignore_ascii_case(&c) {
                    typed[side] += 1;
                    if typed[side] == text.len() {
                        done_ms[side] = Some(t.max(text.len() as f64 * TYPING_MIN_GAP_MS));
                        first_done.get_or_insert(now);
                    }
                } else {
                    errors[side] += 1;
                }
                self.decide_typing(now);
            }
            Round::Rhythm { judged, misses } => {
                if key != DuelKey::Action {
                    return;
                }
                let nearest = (0..RHYTHM_BEATS)
                    .filter(|i| judged[side][*i].is_none())
                    .map(|i| (i, t - rhythm_beat_ms(i)))
                    .min_by(|a, b| a.1.abs().total_cmp(&b.1.abs()));
                match nearest {
                    Some((i, error)) if error.abs() <= RHYTHM_WINDOW_MS => judged[side][i] = Some(error),
                    _ => misses[side] += 1,
                }
            }
            Round::Hunter { schedule, reflexes } => {
                if key != DuelKey::Action {
                    return;
                }
                match schedule.iter().position(|(at, _)| (*at..at + HUNTER_SHOW_MS).contains(&t)) {
                    Some(i) => {
                        let (at, target) = schedule[i];
                        let penalty = if target { HUNTER_EMPTY_PENALTY } else { HUNTER_DECOY_PENALTY };
                        reflexes.press(side, i, t - at, target, penalty);
                    }
                    None => reflexes.miss(side, HUNTER_EMPTY_PENALTY),
                }
            }
            Round::Macros { problems, solved, errors, last_ms, first_done } => {
                let DuelKey::Answer(value) = key else { return };
                let Some(problem) = problems.get(solved[side]) else { return };
                if problem.answer() == value {
                    solved[side] += 1;
                    last_ms[side] = Some(t.max(0.0));
                    if solved[side] == problems.len() {
                        first_done.get_or_insert(now);
                    }
                } else {
                    errors[side] += 1;
                }
                self.decide_macros(now);
            }
            Round::Angle { target, attempts } => {
                if key != DuelKey::Action || attempts[side].len() >= ANGLE_ATTEMPTS {
                    return;
                }
                let pos = angle_pos(t / 1000.0);
                attempts[side].push((pos - *target).abs());
                let out = &self.out;
                if attempts.iter().enumerate().all(|(s, a)| out[s] || a.len() >= ANGLE_ATTEMPTS) {
                    self.conclude(now);
                }
            }
            Round::Contest(c) => {
                c.input(side, key, t);
                if c.settled() {
                    self.conclude(now);
                }
            }
        }
    }

    /// What `side` sees; `rival_lifetime` is the lifetime MP of the rival shown next to it.
    pub fn view(&self, side: usize, rival_lifetime: f64, now: Instant) -> DuelView {
        let other = self.rival(side);
        let lineup: Vec<DraftPick> = self
            .draft
            .iter()
            .flat_map(|d| &d.picks)
            .map(|(s, game, random)| DraftPick { mine: *s == side, game: *game, random: *random })
            .collect();
        let board = match &self.round {
            _ if self.phase == Phase::Draft => {
                Board::Draft { picks: lineup.clone(), my_turn: self.draft.as_ref().is_some_and(|d| d.turn == side) }
            }
            Round::Contest(c) => c.board(side, other, if self.phase == Phase::Play { self.elapsed_ms(now) } else { 0.0 }),
            Round::Staredown { go_at, reaction, early, .. } => Board::Staredown {
                go: self.phase == Phase::Play && now >= *go_at,
                i_pressed: reaction[side].is_some() || early[side].is_some(),
                go_ms: go_at.saturating_duration_since(self.phase_started).as_millis() as u64,
            },
            Round::Skincare { sequence, progress, failed, done_at, out, .. } => {
                let progress_of = |s: usize| Progress { steps: progress[s], failed: failed[s] || out[s].is_some(), done: done_at[s].is_some() };
                Board::Skincare {
                    level: sequence.len(),
                    sequence: if self.phase == Phase::Show { sequence.clone() } else { Vec::new() },
                    mine: progress_of(side),
                    theirs: progress_of(other),
                }
            }
            Round::Mewing { count, .. } => Board::Mewing { mine: count[side], theirs: count[other] },
            Round::Angle { target, attempts } => {
                Board::Angle { target: *target, mine: attempts[side].clone(), theirs: attempts[other].clone() }
            }
            Round::Typing { text, typed, errors, done_ms, .. } => {
                let elapsed = now.saturating_duration_since(self.phase_started).as_secs_f64() * 1000.0;
                let progress_of = |s: usize| {
                    let live = if self.phase == Phase::Play { done_ms[s].unwrap_or(elapsed) } else { 0.0 };
                    let (wpm, accuracy) = typing_stats(typed[s], errors[s], live);
                    TypingProgress { typed: typed[s], errors: errors[s], done: done_ms[s].is_some(), wpm, accuracy }
                };
                Board::Typing { text: text.iter().collect(), mine: progress_of(side), theirs: progress_of(other) }
            }
            Round::Rhythm { judged, misses } => {
                let progress_of = |s: usize| {
                    let hits: Vec<f64> = judged[s].iter().flatten().copied().collect();
                    RhythmProgress {
                        hits: hits.len() as u32,
                        perfects: hits.iter().filter(|e| e.abs() <= RHYTHM_PERFECT_MS).count() as u32,
                        misses: misses[s],
                        score: self.rhythm(s).0,
                        judged: judged[s].clone(),
                    }
                };
                Board::Rhythm { mine: progress_of(side), theirs: progress_of(other) }
            }
            Round::Hunter { schedule, reflexes } => {
                Board::Hunter { schedule: schedule.clone(), mine: reflexes.progress(side), theirs: reflexes.progress(other) }
            }
            Round::Macros { problems, solved, errors, .. } => {
                let progress_of = |s: usize| MacrosProgress { solved: solved[s], errors: errors[s], done: solved[s] == problems.len() };
                Board::Macros { problems: problems.clone(), mine: progress_of(side), theirs: progress_of(other) }
            }
        };
        let relative = |w: Option<usize>| w.map(|w| w == side);
        let standings = if self.n() > 2 {
            (0..self.n()).map(|s| StandingView { name: self.names[s].clone(), points: self.wins[s], me: s == side, out: self.out[s] }).collect()
        } else {
            Vec::new()
        };
        DuelView {
            opponent: self.names[other].clone(),
            opponent_lifetime: rival_lifetime,
            opponent_tag: None,
            stake: self.stake,
            my_wins: self.wins[side],
            their_wins: self.wins[other],
            round: self.rounds,
            game: self.game,
            phase: self.phase,
            phase_ms: now.saturating_duration_since(self.phase_started).as_millis() as u64,
            phase_len_ms: self.phase_len.as_millis() as u64,
            result: self.last.as_ref().map(|r| r.text.clone()),
            i_won_round: self.last.as_ref().and_then(|r| relative(r.winner)),
            i_won: self.outcome.map(relative),
            board,
            seq: self.seq,
            lineup,
            standings,
            winner: self.outcome.flatten().map(|w| self.names[w].clone()),
        }
    }
}

/// One player in a group duel's scoreboard.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StandingView {
    pub name: String,
    pub points: u32,
    pub me: bool,
    /// Gave up.
    pub out: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    pub steps: usize,
    pub failed: bool,
    pub done: bool,
}

/// One side of a typing battle; `wpm` and `accuracy` are live readings.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TypingProgress {
    pub typed: usize,
    pub errors: u32,
    pub done: bool,
    pub wpm: f64,
    pub accuracy: f64,
}

/// One side of a rhythm round.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RhythmProgress {
    pub hits: u32,
    pub perfects: u32,
    pub misses: u32,
    pub score: f64,
    pub judged: Vec<Option<f64>>,
}

/// One side of a hunter eyes round.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HunterProgress {
    pub hits: u32,
    pub mistakes: u32,
    pub score: f64,
    pub avg_ms: Option<f64>,
    pub answered: Vec<bool>,
}

/// One side of a macros round.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MacrosProgress {
    pub solved: usize,
    pub errors: u32,
    pub done: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Board {
    /// `go_ms` is when the GO shows, in ms after the phase started, so each client can show it on time.
    Staredown { go: bool, i_pressed: bool, go_ms: u64 },
    /// `sequence` is only filled while the routine is on screen.
    Skincare { level: usize, sequence: Vec<DuelKey>, mine: Progress, theirs: Progress },
    Mewing { mine: u32, theirs: u32 },
    Angle { target: f64, mine: Vec<f64>, theirs: Vec<f64> },
    Typing { text: String, mine: TypingProgress, theirs: TypingProgress },
    Rhythm { mine: RhythmProgress, theirs: RhythmProgress },
    /// The whole schedule is sent up front so each client shows the faces on its own clock.
    Hunter { schedule: Vec<(f64, bool)>, mine: HunterProgress, theirs: HunterProgress },
    Macros { problems: Vec<MacroProblem>, mine: MacrosProgress, theirs: MacrosProgress },
    Draft { picks: Vec<DraftPick>, my_turn: bool },
    Jawline { target: Vec<u8>, mine: JawSide, theirs: JawSide },
    /// Each client draws the neck from the drift and the presses on its own clock.
    Posture { drift: PostureWaves, mine: PostureSide, theirs: PostureSide },
    FitCheck { target: Vec<u8>, offsets: Vec<u8>, mine: FitSide, theirs: FitSide },
    Hydration { target: i32, mine: HydrationSide, theirs: HydrationSide },
    Photo { shots: Vec<(f64, f64)>, mine: Vec<Option<f64>>, theirs: Vec<Option<f64>> },
    /// `theirs` is only how many items the rival placed, so the order can't be copied.
    Status { items: Vec<String>, mine: Vec<usize>, theirs: usize },
    Hair { strands: Vec<Strand>, mine: HunterProgress, theirs: HunterProgress },
    Brow { trials: Vec<BrowTrial>, mine: Vec<Option<BrowMark>>, theirs: Vec<Option<BrowMark>> },
    /// `theirs` holds only revealed throws.
    ReverseMog { mine: Vec<Option<Stance>>, theirs: Vec<Option<Stance>>, their_ready: bool },
}

/// A duel from one player's point of view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DuelView {
    pub opponent: String,
    pub opponent_lifetime: f64,
    /// Clan tag of the opponent, shown next to the name.
    #[serde(default)]
    pub opponent_tag: Option<String>,
    pub stake: f64,
    pub my_wins: u32,
    pub their_wins: u32,
    pub round: u32,
    pub game: Minigame,
    pub phase: Phase,
    pub phase_ms: u64,
    pub phase_len_ms: u64,
    pub result: Option<String>,
    /// Last round: `Some(true)` won, `Some(false)` lost, `None` draw or none yet.
    pub i_won_round: Option<bool>,
    /// Whole duel, once decided: `Some(None)` is a draw.
    pub i_won: Option<Option<bool>>,
    pub board: Board,
    /// Phase counter; a new value means a new phase even if it looks the same.
    #[serde(default)]
    pub seq: u32,
    /// Draft picks in play order.
    #[serde(default)]
    pub lineup: Vec<DraftPick>,
    /// Everyone's points in a group duel; empty when it is one against one.
    #[serde(default)]
    pub standings: Vec<StandingView>,
    /// Who won the whole duel, once decided.
    #[serde(default)]
    pub winner: Option<String>,
}

/// A random step for a routine: an arrow, or a digit for the memory variant.
fn routine_step(digits: bool, rng: &mut impl Rng) -> DuelKey {
    if digits {
        DuelKey::Char(char::from(b'1' + rng.gen_range(0..MEMORY_STEPS.len() as u8)))
    } else {
        *DuelKey::ARROWS.choose(rng).expect("non-empty")
    }
}

/// Time to type back a routine of `len` steps.
pub fn routine_input_len(digits: bool, len: usize) -> Duration {
    if digits { MEMORY_INPUT_BASE + MEMORY_INPUT_STEP * len as u32 } else { SKINCARE_INPUT_BASE + SKINCARE_INPUT_STEP * len as u32 }
}
