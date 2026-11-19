//! PvP duels: six drafted minigames plus sudden death, refereed by the server.

mod contests;

pub use contests::*;

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

enum Round {
    /// `reaction` in ms after the GO; `first` is when the first valid one arrived.
    Staredown { go_at: Instant, reaction: [Option<f64>; 2], first: Option<Instant> },
    /// `digits` is the memory variant: steps are digit keys flashed one at a time.
    Skincare { sequence: Vec<DuelKey>, progress: [usize; 2], failed: [bool; 2], done_at: [Option<Instant>; 2], digits: bool },
    /// `last` is the latest counted press; its time breaks ties (who got there first).
    Mewing { count: [u32; 2], last: [Option<(DuelKey, Instant)>; 2] },
    Angle { target: f64, attempts: [Vec<f64>; 2] },
    /// `judged[side][beat]` is the timing error of the press that hit that beat.
    Rhythm { judged: [Vec<Option<f64>>; 2], misses: [u32; 2] },
    Hunter { schedule: Vec<(f64, bool)>, reflexes: Reflexes },
    /// `last_ms` is when each side solved its latest problem.
    Macros {
        problems: Vec<MacroProblem>,
        solved: [usize; 2],
        errors: [u32; 2],
        last_ms: [Option<f64>; 2],
        first_done: Option<Instant>,
    },
    /// Times are ms since play started; `first_done` is when the first finisher's key arrived.
    Typing {
        text: Vec<char>,
        typed: [usize; 2],
        errors: [u32; 2],
        done_ms: [Option<f64>; 2],
        last_ms: [Option<f64>; 2],
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

pub struct Duel {
    pub players: [PlayerId; 2],
    pub names: [String; 2],
    pub stake: f64,
    pub wins: [u32; 2],
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
    /// Each finished round as (minigame, winner side).
    pub history: Vec<(Minigame, Option<usize>)>,
    /// Counts phase changes; clients use it to tell a new phase from a repeat.
    pub seq: u32,
}

impl Duel {
    /// Duel with drawn minigames, or a draft when `draft`; who picks first is drawn.
    pub fn new(
        players: [PlayerId; 2],
        names: [String; 2],
        stake: f64,
        draft: bool,
        now: Instant,
        rng: &mut impl Rng,
    ) -> Duel {
        let mut duel = Duel::blank(players, names, stake, now);
        duel.draft = Some(Draft { picks: Vec::new(), turn: rng.gen_range(0..2) });
        duel.set_phase(Phase::Draft, DRAFT_PICK_TIME, now);
        if !draft {
            while let Some(turn) = duel.draft.as_ref().filter(|d| d.picks.len() < DUEL_ROUNDS as usize).map(|d| d.turn) {
                duel.pick(turn, None, now, rng);
            }
        }
        duel
    }

    /// Duel playing `games` in this order first (then random ones), with no draft.
    pub fn with_games(
        players: [PlayerId; 2],
        names: [String; 2],
        stake: f64,
        games: &[Minigame],
        now: Instant,
        rng: &mut impl Rng,
    ) -> Duel {
        let mut duel = Duel::blank(players, names, stake, now);
        duel.upcoming = games.iter().rev().copied().collect();
        duel.next_round(now, rng);
        duel
    }

    fn blank(players: [PlayerId; 2], names: [String; 2], stake: f64, now: Instant) -> Duel {
        Duel {
            players,
            names,
            stake,
            wins: [0, 0],
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
            round: Round::Mewing { count: [0, 0], last: [None, None] },
            draft: None,
            played_games: Vec::new(),
            history: Vec::new(),
            seq: 0,
        }
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
        let Some(draft) = &mut self.draft else { return };
        if side != draft.turn || draft.picks.len() >= DUEL_ROUNDS as usize {
            return;
        }
        let taken = |g: Minigame| draft.picks.iter().any(|p| p.1 == g);
        let (game, random) = match choice {
            Some(g) if taken(g) => return,
            Some(g) => (g, false),
            None => (Minigame::ALL.into_iter().filter(|g| !taken(*g)).choose(rng).expect("more games than picks"), true),
        };
        draft.picks.push((side, game, random));
        draft.turn = 1 - side;
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
        let fresh = |played: &[Minigame], rng: &mut _| {
            Minigame::ALL.into_iter().filter(|g| !played.contains(g)).choose(rng).unwrap_or(Minigame::Staredown)
        };
        self.game = self.upcoming.pop().unwrap_or_else(|| fresh(&self.played_games, rng));
        self.played_games.push(self.game);
        self.rounds += 1;
        self.round = Round::Mewing { count: [0, 0], last: [None, None] };
        self.set_phase(Phase::Countdown, DUEL_COUNTDOWN, now);
    }

    fn start_play(&mut self, now: Instant, rng: &mut impl Rng) {
        match self.game {
            Minigame::Staredown => {
                let wait = Duration::from_secs_f64(rng.gen_range(STAREDOWN_WAIT_SECS));
                self.round = Round::Staredown { go_at: now + wait, reaction: [None, None], first: None };
                self.set_phase(Phase::Play, wait + STAREDOWN_LIMIT, now);
            }
            Minigame::Skincare => {
                let sequence = (0..SKINCARE_START_LEN).map(|_| routine_step(false, rng)).collect();
                self.round = Round::Skincare { sequence, progress: [0, 0], failed: [false, false], done_at: [None, None], digits: false };
                self.show_routine(now);
            }
            Minigame::Memory => {
                let sequence = (0..MEMORY_START_LEN).map(|_| routine_step(true, rng)).collect();
                self.round = Round::Skincare { sequence, progress: [0, 0], failed: [false, false], done_at: [None, None], digits: true };
                self.show_routine(now);
            }
            Minigame::Mewing => {
                self.round = Round::Mewing { count: [0, 0], last: [None, None] };
                self.set_phase(Phase::Play, MEWING_DURATION, now);
            }
            Minigame::Angle => {
                self.round = Round::Angle { target: rng.gen_range(ANGLE_TARGET), attempts: [Vec::new(), Vec::new()] };
                self.set_phase(Phase::Play, ANGLE_LIMIT, now);
            }
            Minigame::Typing => {
                self.round = Round::Typing {
                    text: typing_text(rng).chars().collect(),
                    typed: [0, 0],
                    errors: [0, 0],
                    done_ms: [None, None],
                    last_ms: [None, None],
                    first_done: None,
                };
                self.set_phase(Phase::Show, TYPING_SHOW, now);
            }
            Minigame::Rhythm => {
                self.round = Round::Rhythm { judged: [vec![None; RHYTHM_BEATS], vec![None; RHYTHM_BEATS]], misses: [0, 0] };
                self.set_phase(Phase::Play, rhythm_len(), now);
            }
            Minigame::Hunter => {
                let schedule = hunter_schedule(rng);
                let len = schedule.last().map_or(0.0, |(at, _)| at + HUNTER_SHOW_MS) + RHYTHM_INTERVAL_MS;
                self.round = Round::Hunter { reflexes: Reflexes::new(schedule.len()), schedule };
                self.set_phase(Phase::Play, Duration::from_secs_f64(len / 1000.0), now);
            }
            Minigame::Macros => {
                let problems = (0..MACROS_PROBLEMS).map(|_| MacroProblem::random(rng)).collect();
                self.round =
                    Round::Macros { problems, solved: [0, 0], errors: [0, 0], last_ms: [None, None], first_done: None };
                self.set_phase(Phase::Show, MACROS_SHOW, now);
            }
            game => {
                let contest = contests::create(game, rng).expect("every other minigame is a contest");
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

    fn end_round(&mut self, winner: Option<usize>, text: String, now: Instant) {
        if let Some(w) = winner {
            self.wins[w] += 1;
        }
        self.played += 1;
        self.history.push((self.game, winner));
        self.last = Some(RoundResult { winner, text });
        self.set_phase(Phase::RoundOver, DUEL_ROUND_PAUSE, now);
    }

    /// Ends the duel at once in favour of the other side.
    pub fn forfeit(&mut self, side: usize, now: Instant) {
        if self.outcome.is_some() {
            return;
        }
        let winner = 1 - side;
        self.outcome = Some(Some(winner));
        self.last = Some(RoundResult { winner: Some(winner), text: format!("{} desistiu do duelo.", self.names[side]) });
        self.set_phase(Phase::Finished, DUEL_RESULT_SHOW, now);
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
            Phase::Play if now.saturating_duration_since(self.phase_started) >= self.phase_len + INPUT_GRACE => {
                self.time_up(now, rng)
            }
            Phase::Play => {
                self.decide_staredown(now);
                self.decide_typing(now);
                self.decide_macros(now);
            }
            Phase::RoundOver if over => {
                let [a, b] = self.wins;
                let leader = match a.cmp(&b) {
                    std::cmp::Ordering::Greater => Some(0),
                    std::cmp::Ordering::Less => Some(1),
                    std::cmp::Ordering::Equal => None,
                };
                // Decided once the trailing side can't catch up in the regulation rounds left.
                let left = DUEL_ROUNDS.saturating_sub(self.rounds);
                if leader.is_some() && a.abs_diff(b) > left {
                    self.finish(leader, now);
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
            Round::Staredown { reaction, .. } => match (reaction[0], reaction[1]) {
                (None, None) => self.end_round(None, "Ninguém reagiu a tempo. Empate.".into(), now),
                _ => self.staredown_result(now),
            },
            Round::Skincare { .. } => self.skincare_level_end(now, rng),
            Round::Mewing { count, last } => {
                let [a, b] = *count;
                let reached = |s: usize| last[s].map(|(_, t)| t);
                let (winner, tied) = match a.cmp(&b) {
                    std::cmp::Ordering::Greater => (Some(0), false),
                    std::cmp::Ordering::Less => (Some(1), false),
                    std::cmp::Ordering::Equal => match (reached(0), reached(1)) {
                        (Some(x), Some(y)) if x < y => (Some(0), true),
                        (Some(x), Some(y)) if y < x => (Some(1), true),
                        _ => (None, false),
                    },
                };
                let text = match (winner, tied) {
                    (Some(w), false) => format!("{} mewou mais: {} x {}.", self.names[w], count[w], count[1 - w]),
                    (Some(w), true) => format!("Empate em {a} mewadas; {} chegou lá primeiro.", self.names[w]),
                    (None, _) => format!("Empate na mewada: {a} x {b}."),
                };
                self.end_round(winner, text, now);
            }
            Round::Angle { .. } => self.angle_end(now),
            Round::Typing { .. } => self.typing_end(now),
            Round::Rhythm { .. } => self.rhythm_end(now),
            Round::Hunter { .. } => self.hunter_end(now),
            Round::Macros { .. } => self.macros_end(now),
            Round::Contest(_) => self.contest_end(now),
        }
    }

    fn contest_end(&mut self, now: Instant) {
        let Round::Contest(c) = &self.round else { return };
        let end = self.elapsed_ms(now).min(self.phase_len.as_secs_f64() * 1000.0);
        let (winner, text) = c.result(&self.names, end);
        self.end_round(winner, text, now);
    }

    fn rhythm_end(&mut self, now: Instant) {
        let Round::Rhythm { judged, misses } = &self.round else { return };
        let stats: Vec<(f64, usize)> = (0..2)
            .map(|s| {
                let hits: Vec<f64> = judged[s].iter().flatten().copied().collect();
                let score = hits.iter().map(|e| rhythm_points(*e)).sum::<f64>() - misses[s] as f64 * RHYTHM_MISS_PENALTY;
                (score + 0.0, hits.len())
            })
            .collect();
        let line = |s: usize| format!("{:.1} pontos ({}/{RHYTHM_BEATS} batidas)", stats[s].0, stats[s].1);
        let (winner, text) = if (stats[0].0 - stats[1].0).abs() < 1e-9 {
            (None, format!("Mesmo ritmo: {}. Empate.", line(0)))
        } else {
            let w = if stats[0].0 > stats[1].0 { 0 } else { 1 };
            (Some(w), format!("{} ficou no ritmo: {} x {}.", self.names[w], line(w), line(1 - w)))
        };
        self.end_round(winner, text, now);
    }

    fn hunter_end(&mut self, now: Instant) {
        let Round::Hunter { reflexes, .. } = &self.round else { return };
        let (winner, text) = reflexes.result(&self.names, "tem o olhar de caçador", "Olhares iguais");
        self.end_round(winner, text, now);
    }

    /// Ends the macros round once both finished, or the chase time after the first finisher ran out.
    fn decide_macros(&mut self, now: Instant) {
        let Round::Macros { problems, solved, first_done, .. } = &self.round else { return };
        let both = solved.iter().all(|s| *s == problems.len());
        let chased = first_done.is_some_and(|t| now.saturating_duration_since(t) >= MACROS_CHASE);
        if both || chased {
            self.macros_end(now);
        }
    }

    fn macros_end(&mut self, now: Instant) {
        let Round::Macros { solved, errors, last_ms, .. } = &self.round else { return };
        let key = |s: usize| (solved[s], -last_ms[s].unwrap_or(f64::MAX), -(errors[s] as f64));
        let (a, b) = (key(0), key(1));
        let winner = match a.0.cmp(&b.0) {
            std::cmp::Ordering::Greater => Some(0),
            std::cmp::Ordering::Less => Some(1),
            std::cmp::Ordering::Equal if a.0 == 0 => None,
            std::cmp::Ordering::Equal => match a.1.total_cmp(&b.1).then(a.2.total_cmp(&b.2)) {
                std::cmp::Ordering::Greater => Some(0),
                std::cmp::Ordering::Less => Some(1),
                std::cmp::Ordering::Equal => None,
            },
        };
        let line = |s: usize| format!("{} contas, {} erros", solved[s], errors[s]);
        let text = match winner {
            Some(w) if solved[0] == solved[1] => format!("Mesmas contas ({}); {} terminou antes.", line(w), self.names[w]),
            Some(w) => format!("{} fechou os macros: {} x {}.", self.names[w], line(w), line(1 - w)),
            None => format!("Macros empatados: {}.", line(0)),
        };
        self.end_round(winner, text, now);
    }

    /// Ends the typing battle once both finished, or the chase time after the first finisher ran out.
    fn decide_typing(&mut self, now: Instant) {
        let Round::Typing { done_ms, first_done, .. } = &self.round else { return };
        let both = done_ms.iter().all(Option::is_some);
        let chased = first_done.is_some_and(|t| now.saturating_duration_since(t) >= TYPING_CHASE);
        if both || chased {
            self.typing_end(now);
        }
    }

    fn typing_end(&mut self, now: Instant) {
        let Round::Typing { typed, errors, done_ms, .. } = &self.round else { return };
        let end_ms = now.saturating_duration_since(self.phase_started).min(self.phase_len).as_secs_f64() * 1000.0;
        let first = match (done_ms[0], done_ms[1]) {
            (Some(a), Some(b)) if a <= b => Some(0),
            (Some(_), Some(_)) | (None, Some(_)) => Some(1),
            (Some(_), None) => Some(0),
            (None, None) => None,
        };
        let stats: Vec<(f64, f64, f64)> = (0..2)
            .map(|s| {
                let (wpm, acc) = typing_stats(typed[s], errors[s], done_ms[s].unwrap_or(end_ms));
                let bonus = if first == Some(s) { TYPING_FINISH_BONUS } else { 1.0 };
                (wpm, acc, wpm * acc * acc * bonus)
            })
            .collect();
        let line = |s: usize| format!("{:.0} ppm e {:.0}% de precisão", stats[s].0, stats[s].1 * 100.0);
        let (winner, text) = if typed == &[0, 0] {
            (None, "Ninguém digitou nada. Empate.".to_string())
        } else if (stats[0].2 - stats[1].2).abs() < 1e-9 {
            (None, format!("Mesma pontuação ({}). Empate.", line(0)))
        } else {
            let w = if stats[0].2 > stats[1].2 { 0 } else { 1 };
            let finisher = if first == Some(w) { " e terminou primeiro" } else { "" };
            (Some(w), format!("{} digitou melhor{finisher}: {} x {}.", self.names[w], line(w), line(1 - w)))
        };
        self.end_round(winner, text, now);
    }

    fn angle_end(&mut self, now: Instant) {
        let Round::Angle { attempts, .. } = &self.round else { return };
        // Best attempt first; a tie falls to the next best, and so on.
        let ranked = |s: usize| {
            let mut v = attempts[s].clone();
            v.resize(ANGLE_ATTEMPTS, 1.0);
            v.sort_by(f64::total_cmp);
            v
        };
        let (a, b) = (ranked(0), ranked(1));
        let decider = a.iter().zip(&b).position(|(x, y)| (x - y).abs() >= 1e-9);
        let winner = decider.map(|i| if a[i] < b[i] { 0 } else { 1 });
        let best = [best_attempt(&attempts[0]), best_attempt(&attempts[1])];
        let text = match (winner, decider) {
            (Some(w), Some(0)) => format!("{} achou o ângulo: {} x {} do alvo.", self.names[w], percent(best[w]), percent(best[1 - w])),
            (Some(w), _) => format!("Mesma melhor tentativa ({}); {} foi melhor nas outras.", percent(best[w]), self.names[w]),
            (None, _) => "Mesmas distâncias do alvo. Empate.".to_string(),
        };
        self.end_round(winner, text, now);
    }

    fn skincare_level_end(&mut self, now: Instant, rng: &mut impl Rng) {
        let Round::Skincare { sequence, progress, failed, done_at, digits } = &mut self.round else { return };
        let len = sequence.len();
        let max = if *digits { MEMORY_MAX_LEN } else { SKINCARE_MAX_LEN };
        let passed = [done_at[0].is_some(), done_at[1].is_some()];
        let (winner, text) = match passed {
            [true, true] if len < max => {
                sequence.push(routine_step(*digits, rng));
                *progress = [0, 0];
                *done_at = [None, None];
                *failed = [false, false];
                self.show_routine(now);
                return;
            }
            [true, true] => {
                let (a, b) = (done_at[0].expect("passed"), done_at[1].expect("passed"));
                let w = if a <= b { 0 } else { 1 };
                (Some(w), format!("Rotina completa dos dois; {} foi mais rápido.", self.names[w]))
            }
            [true, false] => (Some(0), format!("{} completou a rotina de {len} passos.", self.names[0])),
            [false, true] => (Some(1), format!("{} completou a rotina de {len} passos.", self.names[1])),
            [false, false] => {
                let [a, b] = *progress;
                match a.cmp(&b) {
                    std::cmp::Ordering::Greater => (Some(0), format!("{} foi mais longe na rotina: {a} x {b} passos.", self.names[0])),
                    std::cmp::Ordering::Less => (Some(1), format!("{} foi mais longe na rotina: {b} x {a} passos.", self.names[1])),
                    std::cmp::Ordering::Equal => (None, format!("Os dois erraram no passo {}. Empate.", a + 1)),
                }
            }
        };
        self.end_round(winner, text, now);
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

    fn staredown_result(&mut self, now: Instant) {
        let Round::Staredown { reaction, .. } = &self.round else { return };
        let [a, b] = *reaction;
        let (winner, text) = match (a, b) {
            (Some(x), Some(y)) if (x - y).abs() < 0.5 => (None, format!("Reação idêntica ({x:.0} ms). Empate.")),
            (Some(x), Some(y)) => {
                let w = if x < y { 0 } else { 1 };
                let (mine, theirs) = if w == 0 { (x, y) } else { (y, x) };
                (Some(w), format!("{} reagiu primeiro: {mine:.0} ms x {theirs:.0} ms.", self.names[w]))
            }
            (Some(x), None) => (Some(0), format!("{} reagiu em {x:.0} ms.", self.names[0])),
            (None, Some(y)) => (Some(1), format!("{} reagiu em {y:.0} ms.", self.names[1])),
            (None, None) => return,
        };
        self.end_round(winner, text, now);
    }

    /// Settles the staredown once no faster press can still be on its way.
    fn decide_staredown(&mut self, now: Instant) {
        let Round::Staredown { reaction, first, .. } = &self.round else { return };
        let both = reaction.iter().all(Option::is_some);
        let waited = first.is_some_and(|t| now.saturating_duration_since(t) >= STAREDOWN_DECIDE_SLACK);
        if both || waited {
            self.staredown_result(now);
        }
    }

    /// Applies one key press from `side`; `at_ms` is the client's own timing (see `press_time`).
    pub fn input(&mut self, side: usize, key: DuelKey, at_ms: Option<f64>, rtt_max_ms: f64, now: Instant, rng: &mut impl Rng) {
        if let (Phase::Draft, DuelKey::Pick(choice), 0..=1) = (self.phase, key, side) {
            return self.pick(side, choice, now, rng);
        }
        if self.phase != Phase::Play || side > 1 {
            return;
        }
        let other = 1 - side;
        let t = self.press_time(at_ms, now, rtt_max_ms);
        if t > self.phase_len.as_secs_f64() * 1000.0 {
            return; // pressed after the phase ended; arrived during the grace period
        }
        if t < 0.0 && !matches!(self.round, Round::Staredown { .. }) {
            return; // pressed before the phase started; only the staredown punishes that
        }
        let at = self.phase_started + Duration::from_secs_f64(t.max(0.0) / 1000.0);
        match &mut self.round {
            Round::Staredown { go_at, reaction, first } => {
                if key != DuelKey::Action || reaction[side].is_some() {
                    return;
                }
                let go_ms = go_at.saturating_duration_since(self.phase_started).as_secs_f64() * 1000.0;
                let r = t - go_ms;
                if r < HUMAN_REACTION_MIN_MS {
                    let text = if r < 0.0 {
                        format!("{} piscou antes da hora!", self.names[side])
                    } else {
                        format!("{} chutou o tempo ({r:.0} ms é rápido demais para um humano).", self.names[side])
                    };
                    self.end_round(Some(other), text, now);
                    return;
                }
                reaction[side] = Some(r);
                first.get_or_insert(now);
                self.decide_staredown(now);
            }
            Round::Skincare { sequence, progress, failed, done_at, digits } => {
                let step = if *digits { matches!(key, DuelKey::Char('1'..='8')) } else { DuelKey::ARROWS.contains(&key) };
                if !step || failed[side] || done_at[side].is_some() {
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
                let finished = |s: usize| failed[s] || done_at[s].is_some();
                if finished(0) && finished(1) {
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
            Round::Typing { text, typed, errors, done_ms, last_ms, first_done } => {
                let DuelKey::Char(c) = key else { return };
                if done_ms[side].is_some() || last_ms[side].is_some_and(|prev| t - prev < TYPING_MIN_GAP_MS) {
                    return;
                }
                last_ms[side] = Some(t);
                if text[typed[side]].eq_ignore_ascii_case(&c) {
                    typed[side] += 1;
                    if typed[side] == text.len() {
                        done_ms[side] = Some(t.max(0.0));
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
                if attempts.iter().all(|a| a.len() >= ANGLE_ATTEMPTS) {
                    self.angle_end(now);
                }
            }
            Round::Contest(c) => {
                c.input(side, key, t);
                if c.settled() {
                    self.contest_end(now);
                }
            }
        }
    }

    /// What `side` sees.
    pub fn view(&self, side: usize, opponent_lifetime: f64, now: Instant) -> DuelView {
        let other = 1 - side;
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
            Round::Contest(c) => c.board(side, if self.phase == Phase::Play { self.elapsed_ms(now) } else { 0.0 }),
            Round::Staredown { go_at, reaction, .. } => Board::Staredown {
                go: self.phase == Phase::Play && now >= *go_at,
                i_pressed: reaction[side].is_some(),
                go_ms: go_at.saturating_duration_since(self.phase_started).as_millis() as u64,
            },
            Round::Skincare { sequence, progress, failed, done_at, .. } => {
                let progress_of = |s: usize| Progress { steps: progress[s], failed: failed[s], done: done_at[s].is_some() };
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
                        score: hits.iter().map(|e| rhythm_points(*e)).sum::<f64>() - misses[s] as f64 * RHYTHM_MISS_PENALTY + 0.0,
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
        DuelView {
            opponent: self.names[other].clone(),
            opponent_lifetime,
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
        }
    }
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
