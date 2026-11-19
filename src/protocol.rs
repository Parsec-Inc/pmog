//! Wire formats: UDP server beacons for discovery and JSON lines over TCP.

use std::io::{self, BufRead, Write};

use serde::{Deserialize, Serialize};

use crate::constants::MAX_LINE_BYTES;
use crate::duel::{DuelKey, DuelView};
use crate::game::{Player, Tone};
use crate::items::ItemId;

/// Player id assigned by the server; `NPC_TARGET` is the training bot.
pub type PlayerId = i64;
pub const NPC_TARGET: PlayerId = 0;

/// Announced by servers on the LAN.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Beacon {
    pub v: u32,
    pub name: String,
    pub port: u16,
    pub players: u32,
    /// Build the server runs; only players on the same build can join.
    #[serde(default)]
    pub release: u64,
    /// Signed builds this server hands out: (platform, release).
    #[serde(default)]
    pub updates: Vec<(String, u64)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t")]
pub enum ClientMsg {
    /// `release` is the build number; players on one server must all run the same build.
    Hello {
        v: u32,
        machine_id: String,
        name: String,
        #[serde(default)]
        release: u64,
    },
    Mog { target: PlayerId, meter: f64 },
    Mew,
    Buy { item: ItemId },
    Use { item: ItemId },
    Rename { name: String },
    /// Invites `target` to a duel betting `fraction` of the poorer player's MP.
    /// `draft` asks for each side to pick minigames instead of drawing them.
    Challenge {
        target: PlayerId,
        fraction: f64,
        #[serde(default)]
        draft: bool,
    },
    Answer { from: PlayerId, accept: bool },
    CancelChallenge,
    /// Joins the duel queue, or leaves it with `None`.
    Queue {
        fraction: Option<f64>,
        #[serde(default)]
        draft: bool,
    },
    /// `at_ms` is when the key was pressed, in ms since the phase started on this client's aligned clock.
    Duel {
        key: DuelKey,
        #[serde(default)]
        at_ms: Option<f64>,
    },
    Forfeit,
    /// Answer to `ServerMsg::Ping`, sent straight from the reader thread.
    Pong { n: u32 },
    Chat(crate::chat::ChatCmd),
    Clan(crate::clan::ClanCmd),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PeerView {
    pub id: PlayerId,
    pub name: String,
    pub mp: f64,
    pub lifetime: f64,
    pub attack: f64,
    pub defense: f64,
    pub streak: u32,
    pub in_duel: bool,
    pub immune: bool,
    /// How long the player stays immune.
    #[serde(default)]
    pub immune_ms: u64,
    /// Tag of the player's clan, shown next to the name.
    #[serde(default)]
    pub clan_tag: Option<String>,
    /// Fight statuses and ms left, so the odds shown count them.
    #[serde(default)]
    pub statuses: Vec<(crate::game::Status, u64)>,
    /// What each DLC shows about the player.
    #[serde(default)]
    pub dlc: crate::dlc::DlcPublic,
}

/// A duel invitation; `peer` is the other player.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InviteView {
    pub peer: PlayerId,
    pub name: String,
    pub fraction: f64,
    /// MP each side would bet right now.
    pub stake: f64,
    pub expires_ms: u64,
    /// Each side picks minigames instead of drawing them.
    #[serde(default)]
    pub draft: bool,
}

/// Everything a client needs to draw its screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StateView {
    pub me: Player,
    pub streak: u32,
    pub cooldown_ms: u64,
    pub cooldown_total_ms: u64,
    pub immune_ms: u64,
    pub preworkout_ms: u64,
    #[serde(default)]
    pub cologne_ms: u64,
    #[serde(default)]
    pub aura_farm_ms: u64,
    /// Fight statuses (Abalado, Perdeu Aura, Ego Proibido) and ms left.
    #[serde(default)]
    pub statuses: Vec<(crate::game::Status, u64)>,
    /// Time since this player's timing meter started, on the server clock.
    pub meter_ms: u64,
    pub players: Vec<PeerView>,
    pub duel: Option<DuelView>,
    /// Invitations received.
    pub invites: Vec<InviteView>,
    /// Invitation sent and still waiting.
    pub outgoing: Option<InviteView>,
    /// Bet fraction while waiting in the duel queue.
    pub queued: Option<f64>,
    /// Average round trip to the server, in ms.
    #[serde(default)]
    pub rtt_ms: f64,
    /// Tag of this player's own clan.
    #[serde(default)]
    pub clan_tag: Option<String>,
}

/// A consumable going off at the moment of impact; each client animates the ones sent to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Fx {
    /// Defender: the Sigma Shield took the hit.
    ShieldBlocked,
    /// Attacker: the mog bounced off a Sigma Shield.
    ShieldBounced,
    /// Attacker: the Angle Shot doubled this mog.
    AngleStrike,
    /// Defender: the Pocket Mirror reflected the attack.
    MirrorHeld,
    /// Defender: the Pocket Mirror was used but the attack still landed.
    MirrorCracked,
    /// Attacker: the Jaw Tape covered a failed mog.
    TapeSaved,
    /// Attacker: the Combo 6-7 paid out.
    ComboHit,
    /// Attacker: the Cologne boosted a won mog.
    CologneHit,
    /// Attacker: the Pre-Workout sped this mog's cooldown.
    RushPress,
    /// Both: the mog bounced off Ice Pack immunity.
    IceBounced,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t")]
pub enum ServerMsg {
    Welcome { id: PlayerId, server: String },
    Rejected { reason: String },
    State(Box<StateView>),
    /// Outcome of a shop purchase, for the shop screen.
    Purchase { item: crate::items::ItemId, ok: bool, text: String },
    Event { tone: Tone, text: String, banner: Option<String>, hit: bool },
    Fx(Fx),
    Shutdown,
    /// Round-trip probe; the client answers with `ClientMsg::Pong` at once.
    Ping { n: u32 },
    Chat(crate::chat::ChatEvent),
    Clan(crate::clan::ClanEvent),
}

/// Writes one message as a JSON line.
pub fn write_msg<T: Serialize>(out: &mut impl Write, msg: &T) -> io::Result<()> {
    let mut line = serde_json::to_vec(msg).map_err(io::Error::other)?;
    line.push(b'\n');
    out.write_all(&line)?;
    out.flush()
}

/// Reads one raw line; `Ok(None)` on a clean end of stream.
pub fn read_line(input: &mut impl BufRead) -> io::Result<Option<Vec<u8>>> {
    let mut line = Vec::new();
    let read = io::Read::take(&mut *input, MAX_LINE_BYTES as u64 + 1).read_until(b'\n', &mut line)?;
    if read == 0 {
        return Ok(None);
    }
    if line.len() > MAX_LINE_BYTES {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "mensagem grande demais"));
    }
    Ok(Some(line))
}

/// Parses one JSON message.
pub fn parse_msg<T: for<'de> Deserialize<'de>>(line: &[u8]) -> io::Result<T> {
    serde_json::from_slice(line).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Reads one JSON line; `Ok(None)` on a clean end of stream.
pub fn read_msg<T: for<'de> Deserialize<'de>>(input: &mut impl BufRead) -> io::Result<Option<T>> {
    read_line(input)?.map(|line| parse_msg(&line)).transpose()
}
