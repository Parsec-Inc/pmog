//! Authoritative game rules, run by the server for every connected player.

use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

use rand::Rng;

use crate::constants::*;
use crate::avatar::{self, Mood, Part};
use crate::constants::avatar::MOOD_DURATION;
use crate::dlc::{self, DlcSlot};
use crate::db::{DuelRecord, MatchRecord, PurchaseRecord};
use crate::duel::{Duel, DuelKey, DuelView};
use crate::game::{Achievement, Player, Rank, Status, TimingTier, Tone, crit_chance, meter_value, status_defense_mult, status_power_mult, streak_mult};
use crate::items::{self, ItemId};
use crate::protocol::{Fx, InviteView, NPC_TARGET, PeerView, PlayerId, ServerMsg, StateView};
use crate::utils::{clean_name, decimal, format_mp, format_wait, sane};

/// Runtime state of one online player.
pub struct Slot {
    pub machine_id: String,
    pub player: Player,
    pub streak: u32,
    pub cooldown_until: Instant,
    pub cooldown_total: f64,
    pub immune_until: Instant,
    pub preworkout_until: Instant,
    pub cologne_until: Instant,
    pub aura_farm_until: Instant,
    /// Until when the Ice Pack immunity lasts, to tell it from immunity earned by being hit.
    pub ice_until: Instant,
    abalado_until: Instant,
    /// Until when, and at which rank gap.
    perdeu_aura: (Instant, u8),
    ego_proibido: (Instant, u8),
    /// Each DLC's timers.
    pub dlc: DlcSlot,
    pub meter_started: Instant,
    mew_window: (Instant, u32),
    /// Mogs suffered in a row and when the last one landed; each one lengthens the immunity.
    hits_in_row: u32,
    last_hit: Instant,
    last_rank: Rank,
    /// The face's reaction to the last mog, and until when.
    mood: (Mood, Instant),
    dirty: bool,
}

impl Slot {
    /// The running reaction and ms left, if any.
    fn mood_view(&self, now: Instant) -> Option<(Mood, u64)> {
        let (mood, until) = self.mood;
        (now < until).then(|| (mood, remaining_ms(until, now)))
    }

    fn new(machine_id: String, player: Player, now: Instant) -> Slot {
        let last_rank = player.rank();
        Slot {
            machine_id,
            player,
            streak: 0,
            cooldown_until: now,
            cooldown_total: 0.0,
            immune_until: now,
            preworkout_until: now,
            cologne_until: now,
            aura_farm_until: now,
            ice_until: now,
            abalado_until: now,
            perdeu_aura: (now, 0),
            ego_proibido: (now, 0),
            dlc: DlcSlot::new(now),
            meter_started: now,
            mew_window: (now, 0),
            hits_in_row: 0,
            last_hit: now,
            last_rank,
            mood: (Mood::Neutral, now),
            dirty: false,
        }
    }

    /// Every DLC's statuses with when they end, past ones included.
    fn dlc_statuses(&self) -> Vec<(crate::dlc::DlcStatus, Instant)> {
        let mut out = Vec::new();
        for d in dlc::ALL {
            d.statuses(&self.dlc, &mut out);
        }
        out
    }

    /// Flags the profile to be saved.
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Applies `status` unless the running one lasts longer or Perdeu Aura is still in its grace; true when it took.
    pub(crate) fn set_status(&mut self, status: Status, until: Instant, now: Instant) -> bool {
        let (current, gap) = match status {
            Status::Dlc(st) => return st.dlc().set_status(&mut self.dlc, st, until),
            Status::Abalado => (&mut self.abalado_until, None),
            Status::PerdeuAura(_) if self.perdeu_aura.1 > 0 && now < self.perdeu_aura.0 + PERDEU_AURA_GRACE => return false,
            Status::PerdeuAura(n) => (&mut self.perdeu_aura.0, Some((&mut self.perdeu_aura.1, n))),
            Status::EgoProibido(n) => (&mut self.ego_proibido.0, Some((&mut self.ego_proibido.1, n))),
        };
        if until <= *current {
            return false;
        }
        *current = until;
        if let Some((stored, n)) = gap {
            *stored = n;
        }
        true
    }

    /// Ends every fight status at `now`, grace included.
    pub fn clear_statuses(&mut self, now: Instant) {
        self.abalado_until = now;
        self.perdeu_aura = (now, 0);
        self.ego_proibido = (now, 0);
        for d in dlc::ALL {
            d.clear_statuses(&mut self.dlc, now);
        }
    }

    /// Statuses running at `now`, with the ms left.
    fn active_statuses(&self, now: Instant) -> Vec<(Status, u64)> {
        [
            (Status::Abalado, self.abalado_until),
            (Status::PerdeuAura(self.perdeu_aura.1), self.perdeu_aura.0),
            (Status::EgoProibido(self.ego_proibido.1), self.ego_proibido.0),
        ]
        .into_iter()
        .chain(self.dlc_statuses().into_iter().map(|(st, until)| (Status::Dlc(st), until)))
        .filter(|(_, until)| now < *until)
        .map(|(st, until)| (st, remaining_ms(until, now)))
        .collect()
    }

    /// Power multiplier from fight statuses.
    pub fn atk_mult(&self, now: Instant) -> f64 {
        status_power_mult(self.active_statuses(now).into_iter().map(|(st, _)| st))
    }

    /// Defense multiplier from fight statuses.
    fn def_mult(&self, now: Instant) -> f64 {
        status_defense_mult(self.active_statuses(now).into_iter().map(|(st, _)| st))
    }

    pub fn meter(&self, now: Instant) -> f64 {
        let stats = self.player.stats();
        meter_value(now.saturating_duration_since(self.meter_started).as_secs_f64(), &stats, self.streak)
    }
}

enum Defense {
    /// `broken` is the defender's streak that this mog ended.
    Won { loss: f64, broken: u32 },
    /// `roll` is the defense that beat the attack.
    Held { roll: f64 },
    Blocked,
}

#[derive(Default)]
pub struct World {
    slots: BTreeMap<PlayerId, Slot>,
    /// Messages to deliver, addressed by player.
    pub outbox: Vec<(PlayerId, ServerMsg)>,
    pub duels: Vec<DuelRecord>,
    pub purchases: Vec<PurchaseRecord>,
    pub matches: Vec<MatchRecord>,
    /// Players whose screen changed and should get a state right away.
    pub push: Vec<PlayerId>,
    /// Clan of each online player: (clan id, tag). Kept in sync by the server's clan hub.
    clan_of: HashMap<PlayerId, (i64, String)>,
    active: Vec<Duel>,
    lobbies: Vec<Lobby>,
    /// Players waiting for a duel.
    queue: Vec<Queued>,
    /// Fixed critical chance instead of the attack/defense formula, for exact payouts in tests.
    pub crit_override: Option<f64>,
}

/// A duel challenge from `host`: it starts once nobody is left to answer, with whoever accepted.
struct Lobby {
    host: PlayerId,
    pending: Vec<PlayerId>,
    accepted: Vec<PlayerId>,
    fraction: f64,
    draft: bool,
    expires: Instant,
}

impl Lobby {
    /// The host first, then everyone invited.
    fn members(&self) -> impl Iterator<Item = PlayerId> + '_ {
        std::iter::once(self.host).chain(self.guests())
    }

    fn guests(&self) -> impl Iterator<Item = PlayerId> + '_ {
        self.accepted.iter().chain(&self.pending).copied()
    }
}

/// A player waiting in the duel queue with the bet fraction they accept; `group` waits for a group duel.
#[derive(Clone, Copy)]
struct Queued {
    id: PlayerId,
    fraction: f64,
    draft: bool,
    group: bool,
    since: Instant,
}

const CLAN_BET_REFUSED: &str = "Clãs não apostam entre si: só duelo amistoso.";

fn valid_fraction(fraction: f64) -> bool {
    STAKE_FRACTIONS.iter().any(|f| (f - fraction).abs() < 1e-9)
}

fn remaining_ms(until: Instant, now: Instant) -> u64 {
    until.saturating_duration_since(now).as_millis() as u64
}

impl World {
    pub fn new() -> World {
        World::default()
    }

    pub fn slot(&self, id: PlayerId) -> Option<&Slot> {
        self.slots.get(&id)
    }

    pub fn slot_mut(&mut self, id: PlayerId) -> Option<&mut Slot> {
        self.slots.get_mut(&id)
    }

    pub fn online(&self) -> impl Iterator<Item = (PlayerId, &Slot)> {
        self.slots.iter().map(|(id, s)| (*id, s))
    }

    pub fn online_count(&self) -> usize {
        self.slots.len()
    }

    pub(crate) fn event(&mut self, to: PlayerId, tone: Tone, text: impl Into<String>, banner: Option<String>, hit: bool) {
        self.outbox.push((to, ServerMsg::Event { tone, text: text.into(), banner, hit }));
    }

    fn fx(&mut self, to: PlayerId, fx: Fx) {
        self.outbox.push((to, ServerMsg::Fx(fx)));
    }

    /// Adds `extra` to `id`'s cooldown and sends them the new state.
    pub(crate) fn add_cooldown(&mut self, id: PlayerId, extra: Duration, now: Instant) {
        let Some(slot) = self.slots.get_mut(&id) else { return };
        slot.cooldown_until = slot.cooldown_until.max(now) + extra;
        slot.cooldown_total = slot.cooldown_until.saturating_duration_since(now).as_secs_f64();
        slot.dirty = true;
        self.push.push(id);
    }

    /// Sends `text` to everyone online but `skip`.
    pub(crate) fn announce(&mut self, skip: &[PlayerId], tone: Tone, text: &str) {
        let others: Vec<PlayerId> = self.slots.keys().copied().filter(|id| !skip.contains(id)).collect();
        for id in others {
            self.event(id, tone, text, None, false);
        }
    }

    /// Brings a player online; refuses a second session for the same computer.
    pub fn join(&mut self, id: PlayerId, machine_id: String, player: Player, now: Instant) -> Result<(), String> {
        if self.slots.contains_key(&id) {
            return Err("Este computador já está conectado neste servidor.".into());
        }
        let name = player.name.clone();
        self.slots.insert(id, Slot::new(machine_id, player, now));
        self.event(id, Tone::Info, format!("Bem-vindo, {name}! Seu progresso fica salvo neste servidor."), None, false);
        self.announce(&[id], Tone::Info, &format!("{name} entrou no servidor. Hora do mog."));
        Ok(())
    }

    /// Takes a player offline and returns the profile to persist.
    pub fn leave(&mut self, id: PlayerId, now: Instant) -> Option<Player> {
        if !self.slots.contains_key(&id) {
            return None;
        }
        self.forfeit(id, now);
        self.drop_invites_of(id, "saiu do servidor");
        self.queue.retain(|q| q.id != id);
        let slot = self.slots.remove(&id)?;
        self.clan_of.remove(&id);
        self.announce(&[id], Tone::Info, &format!("{} saiu do servidor.", slot.player.name));
        Some(slot.player)
    }

    pub fn rename(&mut self, id: PlayerId, name: &str) {
        let name = clean_name(name);
        if name.trim().is_empty() {
            return;
        }
        let Some(slot) = self.slots.get_mut(&id) else { return };
        let old = std::mem::replace(&mut slot.player.name, name.clone());
        slot.dirty = true;
        if old != name {
            self.announce(&[id], Tone::Info, &format!("{old} agora se chama {name}."));
        }
    }

    /// Profiles changed since the last call, for saving.
    pub fn take_dirty(&mut self) -> Vec<(PlayerId, Player)> {
        self.slots
            .iter_mut()
            .filter(|(_, s)| s.dirty)
            .map(|(id, s)| {
                s.dirty = false;
                (*id, s.player.clone())
            })
            .collect()
    }

    pub fn all_profiles(&self) -> Vec<(PlayerId, Player)> {
        self.slots.iter().map(|(id, s)| (*id, s.player.clone())).collect()
    }

    /// Passive income, rank-ups and achievements.
    pub fn tick(&mut self, now: Instant, dt: f64) {
        let dt = sane(dt).min(MAX_TICK_SECS);
        let mut news = Vec::new();
        for (id, slot) in self.slots.iter_mut() {
            let farming = if now < slot.aura_farm_until { AURA_FARM_MULT } else { 1.0 };
            let passive = slot.player.stats().passive * farming;
            if passive > 0.0 && dt > 0.0 {
                slot.player.gain(passive * dt);
                slot.dirty = true;
            }
            let rank = slot.player.rank();
            if rank > slot.last_rank {
                slot.last_rank = rank;
                let unlocks = if items::CATALOG.iter().any(|d| d.min_rank == rank) { "Novos itens na loja." } else { "Veja todos os ranks com P." };
                news.push((*id, Tone::Epic, format!("Você subiu para {}! {unlocks}", rank.label()), Some(format!("RANK UP: {}", rank.label().to_uppercase()))));
            }
            for ach in Achievement::all().iter().copied() {
                if !slot.player.achievements.contains(&ach) && ach.reached(&slot.player) {
                    slot.player.achievements.push(ach);
                    slot.dirty = true;
                    news.push((*id, Tone::Epic, format!("Conquista: {} ({})", ach.name(), ach.desc()), None));
                }
            }
        }
        for (id, tone, text, banner) in news {
            self.event(id, tone, text, banner, false);
        }
        self.expire_invites(now);
        self.matchmake(now);
        self.tick_duels(now);
    }

    /// Clicker action, rate-limited per second.
    pub fn mew(&mut self, id: PlayerId, now: Instant) {
        let Some(slot) = self.slots.get_mut(&id) else { return };
        if now.saturating_duration_since(slot.mew_window.0) >= Duration::from_secs(1) {
            slot.mew_window = (now, 0);
        }
        let stats = slot.player.stats();
        if slot.mew_window.1 >= stats.mew_cap {
            self.event(id, Tone::Info, "Mandíbula cansada, calma aí.", Some("Mandíbula cansada".into()), false);
            return;
        }
        slot.mew_window.1 += 1;
        slot.player.mews += 1;
        slot.player.gain(stats.mew_gain);
        slot.dirty = true;
    }

    pub fn buy(&mut self, id: PlayerId, item: ItemId) {
        let Some(slot) = self.slots.get_mut(&id) else { return };
        let def = items::def(item);
        let p = &mut slot.player;
        let level = p.level(item);
        let result = if let Some(reason) = def.purchase_block(p) {
            Err(reason)
        } else {
            let cost = def.cost(level);
            p.mp -= cost;
            *p.items.entry(item).or_insert(0) += 1;
            p.purchases += 1;
            slot.dirty = true;
            self.purchases.push(PurchaseRecord { player: id, item: format!("{item:?}"), cost });
            Ok(format!("Comprou {} (-{} MP)", def.name, format_mp(cost)))
        };
        let (ok, text) = match result {
            Ok(text) => (true, text),
            Err(text) => (false, text),
        };
        self.event(id, if ok { Tone::Good } else { Tone::Bad }, text.clone(), None, false);
        self.outbox.push((id, ServerMsg::Purchase { item, ok, text }));
    }

    /// Buys a face part and wears it right away.
    pub fn buy_part(&mut self, id: PlayerId, part: Part) {
        let Some(slot) = self.slots.get_mut(&id) else { return };
        let def = avatar::def(part);
        let (tone, text) = match def.purchase_block(&slot.player) {
            Some(reason) => (Tone::Bad, reason),
            None => {
                slot.player.mp -= def.cost;
                slot.player.avatar_parts.push(part);
                slot.player.avatar.set(part);
                slot.dirty = true;
                self.purchases.push(PurchaseRecord { player: id, item: format!("{part:?}"), cost: def.cost });
                (Tone::Good, format!("Comprou {} (-{} MP) e já está usando.", def.name, format_mp(def.cost)))
            }
        };
        self.event(id, tone, text, None, false);
    }

    /// Makes a player's face react for a while; everyone sees it.
    fn react(&mut self, id: PlayerId, mood: Mood, now: Instant) {
        if let Some(slot) = self.slots.get_mut(&id) {
            slot.mood = (mood, now + MOOD_DURATION);
            slot.dirty = true;
        }
    }

    /// Wears a face part the player owns.
    pub fn wear_part(&mut self, id: PlayerId, part: Part) {
        let Some(slot) = self.slots.get_mut(&id) else { return };
        if !slot.player.owns_part(part) {
            let name = avatar::def(part).name;
            return self.event(id, Tone::Bad, format!("Você ainda não tem {name}."), None, false);
        }
        slot.player.avatar.set(part);
        slot.dirty = true;
        self.push.push(id);
    }

    pub fn use_item(&mut self, id: PlayerId, item: ItemId, now: Instant) {
        let Some(slot) = self.slots.get_mut(&id) else { return };
        let name = items::def(item).name;
        if slot.player.level(item) == 0 {
            self.event(id, Tone::Bad, format!("Você não tem {name}. Compre na loja [L]."), None, false);
            return;
        }
        if let Some(result) = dlc::of_item(item).and_then(|d| d.use_item(slot, item, now)) {
            return self.used(id, item, name, result);
        }
        let p = &mut slot.player;
        let result = match item {
            ItemId::EnergyDrink if now >= slot.cooldown_until => Err("O cooldown já está zerado."),
            ItemId::EnergyDrink => {
                slot.cooldown_until = now;
                Ok("Cooldown zerado!")
            }
            ItemId::SigmaShield if p.shield_armed => Err("O Escudo Sigma já está ativo."),
            ItemId::SigmaShield => {
                p.shield_armed = true;
                Ok("Escudo Sigma ativo.")
            }
            ItemId::AngleShot if p.angle_armed => Err("A Foto no Ângulo já está preparada."),
            ItemId::AngleShot => {
                p.angle_armed = true;
                Ok("Próximo mog com poder x2.")
            }
            ItemId::PreWorkout if now < slot.preworkout_until => Err("O Pré-Treino ainda está fazendo efeito."),
            ItemId::PreWorkout => {
                slot.preworkout_until = now + PREWORKOUT_DURATION;
                Ok("Cooldown pela metade por 30 s.")
            }
            ItemId::PocketMirror if p.mirror_armed => Err("O Espelho de Bolso já está preparado."),
            ItemId::PocketMirror => {
                p.mirror_armed = true;
                Ok("Próxima defesa com defesa x3.")
            }
            ItemId::JawTape if p.tape_armed => Err("A Fita de Mandíbula já está colocada."),
            ItemId::JawTape => {
                p.tape_armed = true;
                Ok("Se o próximo mog der errado, você não perde nada.")
            }
            ItemId::Cologne if now < slot.cologne_until => Err("A Colônia ainda está fazendo efeito."),
            ItemId::Cologne => {
                slot.cologne_until = now + COLOGNE_DURATION;
                Ok("Mogs rendem +50% de MP por 60 s.")
            }
            ItemId::ComboSixSeven if p.combo_armed => Err("O Combo 6-7 já está armado."),
            ItemId::ComboSixSeven => {
                p.combo_armed = true;
                Ok("O próximo mog que acertar rende +67% de MP.")
            }
            ItemId::AuraFarm if now < slot.aura_farm_until => Err("Você ainda está farmando aura."),
            ItemId::AuraFarm => {
                slot.aura_farm_until = now + AURA_FARM_DURATION;
                Ok("MP passivo em dobro por 45 s.")
            }
            ItemId::WoundedEgo if p.ego_armed => Err("O Ego Ferido já está esperando o troco."),
            ItemId::WoundedEgo if p.nemesis.is_none() => Err("Ninguém te moggou ainda: não tem de quem se vingar."),
            ItemId::WoundedEgo => {
                p.ego_armed = true;
                Ok("Seu próximo mog em quem te moggou por último é crítico garantido.")
            }
            ItemId::IcePack if slot.immune_until >= now + ICE_PACK_IMMUNITY => Err("Você já está imune por mais tempo que isso."),
            ItemId::IcePack => {
                slot.immune_until = now + ICE_PACK_IMMUNITY;
                slot.ice_until = slot.immune_until;
                slot.hits_in_row = 0;
                Ok("Imune por 20 s, e a imunidade acumulada voltou ao normal.")
            }
            _ => Err("Esse item não é consumível."),
        };
        self.used(id, item, name, result);
    }

    /// Spends `item` when using it worked and tells the player either way.
    fn used(&mut self, id: PlayerId, item: ItemId, name: &str, result: Result<&str, &str>) {
        match result {
            Ok(text) => {
                let Some(slot) = self.slots.get_mut(&id) else { return };
                *slot.player.items.entry(item).or_insert(1) -= 1;
                slot.player.items_used += 1;
                slot.dirty = true;
                self.event(id, Tone::Good, format!("Usou {name}. {text}"), None, false);
            }
            Err(text) => self.event(id, Tone::Info, text, None, false),
        }
    }

    /// Resolves a press of the MOGAR button.
    pub fn mog(&mut self, id: PlayerId, target: PlayerId, client_meter: f64, now: Instant) {
        let Some(slot) = self.slots.get(&id) else { return };
        if now < slot.cooldown_until {
            self.event(id, Tone::Info, "Cooldown! Respira.", Some("Cooldown! Respira.".into()), false);
            return;
        }
        if target != NPC_TARGET && (target == id || !self.slots.contains_key(&target)) {
            self.event(id, Tone::Info, "Esse alvo não está mais no servidor.", None, false);
            return;
        }
        if self.in_duel(id) {
            self.event(id, Tone::Info, "Termine o duelo primeiro.", None, false);
            return;
        }
        if target != NPC_TARGET && self.same_clan(id, target) {
            self.event(id, Tone::Info, "Não dá pra mogar quem é do seu clã.", None, false);
            return;
        }
        if self.in_duel(target) {
            let name = self.slots[&target].player.name.clone();
            self.event(id, Tone::Info, format!("{name} está num duelo agora."), None, false);
            return;
        }
        if let Some(text) = dlc::ALL.iter().find_map(|d| d.block_mog(self, id, target, now)) {
            self.event(id, Tone::Info, text, None, false);
            return;
        }
        if let Some(defender) = self.slots.get(&target).filter(|s| now < s.immune_until) {
            if now < defender.ice_until {
                self.fx(id, Fx::IceBounced);
                self.fx(target, Fx::IceBounced);
            }
            let defender = &self.slots[&target];
            let left = format_wait(defender.immune_until - now);
            let text = format!("{} ainda está se recuperando: imune por mais {left}.", defender.player.name);
            self.event(id, Tone::Info, text, None, false);
            return;
        }

        // The server clock decides; the client's reading only absorbs network delay.
        let server_meter = slot.meter(now);
        let client_meter = sane(client_meter).min(1.0);
        let meter = if (client_meter - server_meter).abs() <= METER_TOLERANCE { client_meter } else { server_meter };

        let target_defense = self.slots.get(&target).map(|s| s.player.stats().defense);
        let target_name = self.slots.get(&target).map(|s| s.player.name.clone());
        let slot = self.slots.get_mut(&id).expect("checked above");
        let stats = slot.player.stats();
        let tier = TimingTier::from_meter(meter, stats.perfect_threshold);
        let revenge = slot.player.ego_armed && target_name.is_some() && slot.player.nemesis == target_name;
        if revenge {
            slot.player.ego_armed = false;
        }
        let chance = self.crit_override.unwrap_or_else(|| crit_chance(stats.attack, target_defense.unwrap_or(stats.attack), stats.underdog));
        let crit = revenge || rand::thread_rng().gen_bool(chance.clamp(0.0, 1.0));
        let cologne = if now < slot.cologne_until { COLOGNE_MULT } else { 1.0 };
        let combo = if slot.player.combo_armed { COMBO_MULT } else { 1.0 };
        let crit_mult = if crit { CRIT_TAKE_MULT } else { 1.0 };
        let take_mult = tier.reward() * (1.0 + stats.win_bonus) * cologne * combo * crit_mult;
        let timing = if tier == TimingTier::Perfect { PERFECT_MULT } else { TIMING_FLOOR + TIMING_SPAN * meter };
        let mut power = stats.attack * slot.atk_mult(now) * timing * streak_mult(slot.streak, &stats);
        let angle = std::mem::take(&mut slot.player.angle_armed);
        if angle {
            power *= 2.0;
        }
        let rush = now < slot.preworkout_until;
        if tier == TimingTier::Perfect {
            slot.player.perfects += 1;
        }
        let cooldown = if now < slot.preworkout_until { stats.cooldown * PREWORKOUT_FACTOR } else { stats.cooldown };
        slot.cooldown_total = cooldown;
        slot.cooldown_until = now + Duration::from_secs_f64(cooldown);
        slot.dirty = true;
        let attacker_name = slot.player.name.clone();
        let banner = format!("{} · MP x{}{}", tier.label(), decimal(tier.reward()), if crit { " · CRÍTICO" } else { "" });
        self.event(id, tier_tone(tier), String::new(), Some(banner), false);
        if angle {
            self.fx(id, Fx::AngleStrike);
        }
        if rush {
            self.fx(id, Fx::RushPress);
        }

        if target == NPC_TARGET {
            let roll = stats.attack * rand::thread_rng().gen_range(NPC_DEFENSE_ROLL);
            let (outcome, won, lost) = if crit || power > roll {
                let streak = streak_mult(self.slots[&id].streak, &stats);
                let gain = (NPC_REWARD_FLAT + stats.attack * NPC_REWARD_RATIO) * take_mult * streak;
                let won = self.on_win(id, NPC_NAME, gain, tier, crit, false);
                self.react(id, Mood::Smug, now);
                if cologne > 1.0 {
                    self.fx(id, Fx::CologneHit);
                }
                ("won", won, 0.0)
            } else {
                let why = format!("a defesa deu {} contra seu poder de {}", format_mp(roll), format_mp(power));
                self.react(id, Mood::Sad, now);
                ("lost", 0.0, self.on_loss(id, NPC_NAME, &why, 1.0))
            };
            self.duels.push(DuelRecord { attacker: id, defender: None, outcome, tier: tier.label(), power, mp_won: won, mp_lost: lost });
            return;
        }

        let defender_name = self.slots[&target].player.name.clone();
        let punch = self.punch_down(id, target);
        let (up, down) = (self.rank_index(target).saturating_sub(self.rank_index(id)), self.rank_index(id).saturating_sub(self.rank_index(target)));
        let upset = self.rank_ratio(target, id);
        let cap = self.stake_cap(id, target);
        let (outcome, won, lost) = match self.defend(target, &attacker_name, power, take_mult * punch * upset, cap, crit, now) {
            Defense::Won { loss, broken } => {
                if punch < 1.0 {
                    let text = format!("{defender_name} está muito abaixo do seu rank: o mog rendeu pouco e não conta para o streak.");
                    self.event(id, Tone::Info, text, None, false);
                }
                // Between players MP only changes hands: the attacker gets exactly what the defender lost.
                if broken > 0 {
                    let text = format!("Você quebrou o streak {broken} de {defender_name}: +{:.0}% de MP no prêmio.", STREAK_BOUNTY * broken as f64 * 100.0);
                    self.event(id, Tone::Good, text, None, false);
                }
                let won = self.on_win(id, &defender_name, loss, tier, crit, punch >= 1.0);
                self.react(id, Mood::Smug, now);
                self.react(target, Mood::Mogged, now);
                if cologne > 1.0 {
                    self.fx(id, Fx::CologneHit);
                }
                if crit {
                    self.afflict(target, Status::Abalado, ABALADO_DURATION, now);
                }
                if up >= 1 {
                    self.upset(id, target, up, now);
                }
                for d in dlc::ALL {
                    d.on_hit(self, id, target, tier, now);
                }
                ("won", won, loss)
            }
            Defense::Held { roll } => {
                let why = format!("a defesa deu {} contra seu poder de {}", format_mp(roll), format_mp(power));
                let swing = self.rank_ratio(id, target);
                let lost = self.on_loss(id, &defender_name, &why, swing);
                self.react(id, Mood::Sad, now);
                self.react(target, Mood::Proud, now);
                // The holder's prize comes from the plain loss, so the rank swing doesn't pay twice.
                self.on_held(target, &attacker_name, lost / swing);
                if down >= 1 {
                    self.upset(target, id, down, now);
                }
                ("lost", 0.0, lost)
            }
            Defense::Blocked => {
                self.event(id, Tone::Info, format!("{defender_name} bloqueou com o Escudo Sigma."), None, false);
                self.fx(id, Fx::ShieldBounced);
                self.react(id, Mood::Sad, now);
                self.react(target, Mood::Proud, now);
                ("blocked", 0.0, 0.0)
            }
        };
        self.duels.push(DuelRecord { attacker: id, defender: Some(target), outcome, tier: tier.label(), power, mp_won: won, mp_lost: lost });
    }

    /// Weight of `to`'s rank over `from`'s, at least 1: from 1 (same or lower) up to `1 + RANK_WEIGHT_SPAN` over `1`.
    /// Same shape as the Brawlhalla rating divisor, `3 - (3000 - elo) / 800`, which is linear in the rating.
    fn rank_ratio(&self, to: PlayerId, from: PlayerId) -> f64 {
        let weight = |id| 1.0 + RANK_WEIGHT_SPAN * self.rank_index(id) as f64 / (Rank::ALL.len() - 1) as f64;
        (weight(to) / weight(from)).max(1.0)
    }

    /// Applies `status` for `base`, shortened by the target's Terapia if it is a bad one.
    pub(crate) fn afflict(&mut self, id: PlayerId, status: Status, base: Duration, now: Instant) {
        let Some(slot) = self.slots.get_mut(&id) else { return };
        let cut = if status.is_bad() { 1.0 - slot.player.stats().resilience } else { 1.0 };
        if !slot.set_status(status, now + base.mul_f64(cut), now) {
            return;
        }
        slot.dirty = true;
        let tone = if status.is_bad() { Tone::Bad } else { Tone::Good };
        self.event(id, tone, format!("{} ({}).", status.label(), status.effect()), None, false);
    }

    /// A lower rank beat a higher one: Ego Proibido for the winner, Perdeu Aura for the loser, both as strong as the gap (capped).
    fn upset(&mut self, low: PlayerId, high: PlayerId, gap: usize, now: Instant) {
        let n = gap.min(RANK_GAP_CAP) as u8;
        self.afflict(low, Status::EgoProibido(n), UPSET_DURATION, now);
        self.afflict(high, Status::PerdeuAura(n), UPSET_DURATION, now);
    }

    /// Share of the normal take when `attacker` hits someone far below their rank.
    fn punch_down(&self, attacker: PlayerId, defender: PlayerId) -> f64 {
        let gap = self.rank_index(attacker).saturating_sub(self.rank_index(defender));
        if gap >= PUNCH_DOWN_ZERO_GAP {
            0.0
        } else if gap >= PUNCH_DOWN_GAP {
            PUNCH_DOWN_FACTOR
        } else {
            1.0
        }
    }

    /// Credits a win worth `gain` MP; `counts_streak` is false for the NPC and much weaker players.
    fn on_win(&mut self, id: PlayerId, target: &str, gain: f64, tier: TimingTier, crit: bool, counts_streak: bool) -> f64 {
        let slot = self.slots.get_mut(&id).expect("attacker online");
        if counts_streak {
            slot.streak += 1;
        }
        slot.player.best_streak = slot.player.best_streak.max(slot.streak);
        slot.player.mogs_won += 1;
        slot.player.gain(gain);
        let combo = std::mem::take(&mut slot.player.combo_armed);
        let streak = slot.streak;
        self.event(
            id,
            Tone::Good,
            format!(
                "Você moggou {target}! +{} MP ({} x{}{}, streak {streak}){}",
                format_mp(gain),
                tier.label(),
                decimal(tier.reward()),
                if crit { ", CRÍTICO" } else { "" },
                if combo { ", Combo 6-7 +67%" } else { "" }
            ),
            None,
            false,
        );
        if combo {
            self.fx(id, Fx::ComboHit);
        }
        gain
    }

    /// Charges a failed mog; `why` says what beat it.
    /// `swing` grows the penalty when a high rank fails against a lower one.
    fn on_loss(&mut self, id: PlayerId, target: &str, why: &str, swing: f64) -> f64 {
        let slot = self.slots.get_mut(&id).expect("attacker online");
        slot.player.mogs_lost += 1;
        if slot.player.tape_armed {
            slot.player.tape_armed = false;
            self.event(id, Tone::Good, format!("Seu mog em {target} falhou ({why}), mas a Fita de Mandíbula salvou seu MP e seu streak."), None, false);
            self.fx(id, Fx::TapeSaved);
            return 0.0;
        }
        let mp = slot.player.mp.max(0.0);
        let shield = 1.0 - slot.player.stats().loss_shield;
        let penalty = ((mp * LOSS_PENALTY_RATIO).max(LOSS_PENALTY_FLAT) * shield * swing).min(mp);
        slot.player.mp -= penalty;
        slot.streak = 0;
        self.event(
            id,
            Tone::Bad,
            format!("Seu mog em {target} falhou: {why}. -{} MP, streak zerado", format_mp(penalty)),
            Some("MOG FALHOU".into()),
            true,
        );
        penalty
    }

    /// Most a single mog can take before multipliers: grows with the lower rank of the two players.
    fn stake_cap(&self, a: PlayerId, b: PlayerId) -> f64 {
        let lower = self.rank_index(a).min(self.rank_index(b));
        STAKE_CAP.max(Rank::ALL[lower].threshold() * STAKE_CAP_RANK_RATIO)
    }

    /// `take_mult` scales how much of the defender's stake the attacker takes, up to `cap`; a `crit` goes through any defense.
    #[allow(clippy::too_many_arguments)]
    fn defend(&mut self, id: PlayerId, attacker: &str, power: f64, take_mult: f64, cap: f64, crit: bool, now: Instant) -> Defense {
        let slot = self.slots.get_mut(&id).expect("defender online");
        slot.dirty = true;
        if slot.player.shield_armed {
            slot.player.shield_armed = false;
            self.event(id, Tone::Good, format!("Escudo Sigma bloqueou o mog de {attacker}!"), None, false);
            self.fx(id, Fx::ShieldBlocked);
            return Defense::Blocked;
        }
        let stats = slot.player.stats();
        let mirrored = std::mem::take(&mut slot.player.mirror_armed);
        let mirror = if mirrored { MIRROR_MULT } else { 1.0 };
        let roll = stats.defense * slot.def_mult(now) * mirror * rand::thread_rng().gen_range(DEFENSE_ROLL);
        if crit || power > roll {
            let mp = slot.player.mp.max(0.0);
            slot.player.nemesis = Some(attacker.to_string());
            let broken = std::mem::take(&mut slot.streak).min(STREAK_CAP);
            let bounty = 1.0 + STREAK_BOUNTY * broken as f64;
            let stake = (STAKE_FLAT + mp * STAKE_RATIO).min(cap) * take_mult * bounty;
            let loss = (stake * (1.0 - stats.loss_shield)).min(mp);
            slot.player.mp -= loss;
            slot.player.defends_lost += 1;
            if now.saturating_duration_since(slot.last_hit) > IMMUNITY_RESET {
                slot.hits_in_row = 0;
            }
            slot.hits_in_row += 1;
            slot.last_hit = now;
            let immunity = escalated_immunity(stats.immunity, slot.hits_in_row);
            slot.immune_until = now + immunity;
            self.event(
                id,
                Tone::Bad,
                format!(
                    "{attacker} te moggou{}! -{} MP (imune por {} s){}",
                    if crit { " com um CRÍTICO" } else { "" },
                    format_mp(loss),
                    immunity.as_secs(),
                    if broken > 0 { format!(", seu streak {broken} acabou") } else { String::new() }
                ),
                Some(format!("MOGGADO POR {}", attacker.to_uppercase())),
                true,
            );
            if mirrored {
                self.fx(id, Fx::MirrorCracked);
            }
            Defense::Won { loss, broken }
        } else {
            slot.player.defends_won += 1;
            if mirrored {
                self.fx(id, Fx::MirrorHeld);
            }
            Defense::Held { roll }
        }
    }

    /// Pays a defender who held a mog a share of what the attacker lost.
    fn on_held(&mut self, id: PlayerId, attacker: &str, attacker_loss: f64) {
        let Some(slot) = self.slots.get_mut(&id) else { return };
        let gain = attacker_loss * slot.player.stats().counter_ratio;
        slot.player.gain(gain);
        slot.dirty = true;
        self.event(id, Tone::Good, format!("Você segurou o mog de {attacker}. +{} MP", format_mp(gain)), None, false);
    }

    /// Snapshot for one player's screen.
    pub fn view(&self, id: PlayerId, now: Instant) -> Option<StateView> {
        let slot = self.slots.get(&id)?;
        let players = self
            .slots
            .iter()
            .filter(|(other, _)| **other != id)
            .map(|(other, s)| {
                let stats = s.player.stats();
                PeerView {
                    id: *other,
                    name: s.player.name.clone(),
                    mp: s.player.mp,
                    lifetime: s.player.lifetime,
                    attack: stats.attack,
                    defense: stats.defense,
                    streak: s.streak,
                    in_duel: self.in_duel(*other),
                    immune: now < s.immune_until,
                    immune_ms: remaining_ms(s.immune_until, now),
                    clan_tag: self.clan_of.get(other).map(|(_, tag)| tag.clone()),
                    statuses: s.active_statuses(now),
                    dlc: dlc::public(&s.player),
                    avatar: s.player.avatar,
                    mood: s.mood_view(now),
                }
            })
            .collect();
        let duel = self.duel_index(id).map(|i| {
            let d = &self.active[i];
            let side = d.side(id).expect("found by id");
            let rival = d.players[d.rival(side)];
            let lifetime = self.slots.get(&rival).map_or(0.0, |s| s.player.lifetime);
            DuelView { opponent_tag: self.clan_of.get(&rival).map(|(_, tag)| tag.clone()), ..d.view(side, lifetime, now) }
        });
        let invite_view = |l: &Lobby, peer: PlayerId, name: String, others: Vec<String>| InviteView {
            peer,
            name,
            fraction: l.fraction,
            stake: self.stake_for(l.members(), l.fraction),
            expires_ms: remaining_ms(l.expires, now),
            draft: l.draft,
            others,
        };
        let invites = self
            .lobbies
            .iter()
            .filter(|l| l.pending.contains(&id))
            .map(|l| invite_view(l, l.host, self.name_of(l.host), l.guests().filter(|g| *g != id).map(|g| self.name_of(g)).collect()))
            .collect();
        let outgoing = self.lobbies.iter().find(|l| l.host == id).map(|l| {
            let waiting = if l.pending.is_empty() { self.names_of(&l.accepted) } else { self.names_of(&l.pending) };
            invite_view(l, l.guests().next().unwrap_or(id), waiting, l.accepted.iter().map(|g| self.name_of(*g)).collect())
        });
        let queue = self.queue.iter().find(|q| q.id == id);
        let queued = queue.map(|q| q.fraction);
        let queued_group = queue.is_some_and(|q| q.group);
        Some(StateView {
            me: slot.player.clone(),
            streak: slot.streak,
            cooldown_ms: remaining_ms(slot.cooldown_until, now),
            cooldown_total_ms: (slot.cooldown_total * 1000.0) as u64,
            immune_ms: remaining_ms(slot.immune_until, now),
            preworkout_ms: remaining_ms(slot.preworkout_until, now),
            cologne_ms: remaining_ms(slot.cologne_until, now),
            aura_farm_ms: remaining_ms(slot.aura_farm_until, now),
            statuses: slot.active_statuses(now),
            meter_ms: now.saturating_duration_since(slot.meter_started).as_millis() as u64,
            players,
            duel,
            invites,
            outgoing,
            queued,
            queued_group,
            rtt_ms: 0.0,
            clan_tag: self.clan_of.get(&id).map(|(_, tag)| tag.clone()),
            mood: slot.mood_view(now),
        })
    }
}

impl World {
    /// The duel `id` is in; a player who gave up a group duel is free again (one against one, it ends the duel).
    fn duel_index(&self, id: PlayerId) -> Option<usize> {
        self.active.iter().position(|d| d.side(id).is_some_and(|s| !d.out[s] || d.players.len() == 2))
    }

    /// True while the player is in a duel or looking at its result.
    pub fn in_duel(&self, id: PlayerId) -> bool {
        self.duel_index(id).is_some()
    }

    pub fn duel_count(&self) -> usize {
        self.active.len()
    }

    /// MP each player bets: a share of the poorest player's MP.
    fn stake_for(&self, players: impl IntoIterator<Item = PlayerId>, fraction: f64) -> f64 {
        let mp = |id: PlayerId| self.slots.get(&id).map_or(0.0, |s| s.player.mp.max(0.0));
        (fraction * players.into_iter().map(mp).fold(f64::INFINITY, f64::min)).floor()
    }

    fn name_of(&self, id: PlayerId) -> String {
        self.slots.get(&id).map_or_else(|| "?".into(), |s| s.player.name.clone())
    }

    /// Names joined for a sentence: "A, B e C".
    fn names_of(&self, ids: &[PlayerId]) -> String {
        let names: Vec<String> = ids.iter().map(|id| self.name_of(*id)).collect();
        match names.split_last() {
            Some((last, rest)) if !rest.is_empty() => format!("{} e {last}", rest.join(", ")),
            _ => names.join(""),
        }
    }

    /// Why `id` can't duel `target` right now, if anything.
    fn duel_blocker(&self, id: PlayerId, target: PlayerId) -> Option<String> {
        if target == id || target == NPC_TARGET || !self.slots.contains_key(&target) {
            Some("Escolha um jogador que esteja no servidor.".into())
        } else if self.in_duel(id) {
            Some("Você já está num duelo.".into())
        } else if self.in_duel(target) {
            Some(format!("{} já está num duelo.", self.name_of(target)))
        } else {
            None
        }
    }

    /// Whether any two of `players` share a clan.
    fn clan_clash(&self, players: &[PlayerId]) -> bool {
        players.iter().enumerate().any(|(i, a)| players[i + 1..].iter().any(|b| self.same_clan(*a, *b)))
    }

    /// Invites `targets` to a duel; with more than one it is a group duel with drawn minigames.
    pub fn challenge(&mut self, id: PlayerId, targets: Vec<PlayerId>, fraction: f64, draft: bool, now: Instant) {
        if !self.slots.contains_key(&id) {
            return;
        }
        let mut targets = targets;
        targets.sort_unstable();
        targets.dedup();
        if targets.is_empty() || targets.len() >= DUEL_MAX_PLAYERS {
            self.event(id, Tone::Info, format!("Um duelo tem de 2 a {DUEL_MAX_PLAYERS} jogadores."), None, false);
            return;
        }
        if !valid_fraction(fraction) {
            self.event(id, Tone::Bad, "Aposta inválida.", None, false);
            return;
        }
        if let Some(reason) = targets.iter().find_map(|t| self.duel_blocker(id, *t)) {
            self.event(id, Tone::Info, reason, None, false);
            return;
        }
        let members: Vec<PlayerId> = std::iter::once(id).chain(targets.iter().copied()).collect();
        if fraction > 0.0 && self.clan_clash(&members) {
            self.event(id, Tone::Info, CLAN_BET_REFUSED, None, false);
            return;
        }
        if let [target] = targets[..]
            && self.lobbies.iter().any(|l| l.host == target && l.pending == [id] && l.accepted.is_empty())
        {
            self.event(id, Tone::Info, format!("{} já te desafiou: aceite com A.", self.name_of(target)), None, false);
            return;
        }
        self.cancel_challenge(id);
        self.queue.retain(|q| q.id != id);
        let group = targets.len() > 1;
        let draft = draft && !group;
        let stake = format_mp(self.stake_for(members.iter().copied(), fraction));
        let mode = if draft { ", escolhendo os minigames" } else { "" };
        let me = self.name_of(id);
        self.event(id, Tone::Info, format!("Desafio enviado para {} valendo {stake} MP{mode}. Esperando resposta...", self.names_of(&targets)), None, false);
        for target in &targets {
            let text = if group {
                let others: Vec<PlayerId> = targets.iter().copied().filter(|t| t != target).collect();
                format!("{me} te chamou para um duelo em grupo com {} valendo {stake} MP! A aceita, R recusa.", self.names_of(&others))
            } else {
                format!("{me} te desafiou para um duelo valendo {stake} MP{mode}! A aceita, R recusa.")
            };
            self.event(*target, Tone::Epic, text, Some(format!("DESAFIO DE {}", me.to_uppercase())), false);
        }
        self.push.extend(members);
        self.lobbies.push(Lobby { host: id, pending: targets, accepted: Vec::new(), fraction, draft, expires: now + INVITE_TTL });
    }

    pub fn cancel_challenge(&mut self, id: PlayerId) {
        let Some(pos) = self.lobbies.iter().position(|l| l.host == id) else { return };
        let lobby = self.lobbies.remove(pos);
        let me = self.name_of(id);
        for other in lobby.guests() {
            self.event(other, Tone::Info, format!("{me} cancelou o desafio."), None, false);
            self.push.push(other);
        }
        self.push.push(id);
    }

    pub fn answer(&mut self, id: PlayerId, from: PlayerId, accept: bool, now: Instant) {
        let Some(i) = self.lobbies.iter().position(|l| l.host == from && l.pending.contains(&id)) else {
            self.event(id, Tone::Info, "Esse desafio não existe mais.", None, false);
            return;
        };
        self.lobbies[i].pending.retain(|p| *p != id);
        self.push.extend([id, from]);
        let me = self.name_of(id);
        let members: Vec<PlayerId> = self.lobbies[i].members().chain([id]).collect();
        let blocker = if !accept {
            None
        } else if let Some(reason) = self.duel_blocker(id, from) {
            Some(reason)
        } else {
            (self.lobbies[i].fraction > 0.0 && self.clan_clash(&members)).then(|| CLAN_BET_REFUSED.to_string())
        };
        match (accept, blocker) {
            (true, Some(reason)) => self.event(id, Tone::Info, reason, None, false),
            (true, None) => {
                self.lobbies[i].accepted.push(id);
                if self.lobbies[i].accepted.len() + self.lobbies[i].pending.len() > 1 {
                    self.event(from, Tone::Info, format!("{me} aceitou o duelo."), None, false);
                }
            }
            (false, _) => {
                self.event(from, Tone::Info, format!("{me} recusou o duelo."), None, false);
                self.event(id, Tone::Info, "Desafio recusado.", None, false);
            }
        }
        self.start_ready_lobbies(now);
    }

    /// Starts every challenge nobody still has to answer, with whoever accepted.
    fn start_ready_lobbies(&mut self, now: Instant) {
        while let Some(i) = self.lobbies.iter().position(|l| l.pending.is_empty()) {
            let lobby = self.lobbies.remove(i);
            if lobby.accepted.is_empty() {
                continue;
            }
            let players: Vec<PlayerId> = lobby.members().filter(|p| self.slots.contains_key(p) && !self.in_duel(*p)).collect();
            if players.len() < 2 || players[0] != lobby.host {
                for p in lobby.members() {
                    self.event(p, Tone::Info, "O duelo não pôde começar: faltou gente.", None, false);
                }
                continue;
            }
            self.start_duel(players, lobby.fraction, lobby.draft && lobby.accepted.len() == 1, now);
        }
    }

    /// Joins the queue with a bet, or leaves it with `None`; the draft needs both sides asking for it.
    /// `group` waits for 3 to 5 players instead of one rival.
    pub fn queue(&mut self, id: PlayerId, fraction: Option<f64>, draft: bool, group: bool, now: Instant) {
        if !self.slots.contains_key(&id) {
            return;
        }
        self.push.push(id);
        let Some(fraction) = fraction else {
            if self.queue.iter().any(|q| q.id == id) {
                self.queue.retain(|q| q.id != id);
                self.event(id, Tone::Info, "Você saiu da fila de duelo.", None, false);
            }
            return;
        };
        if !valid_fraction(fraction) {
            self.event(id, Tone::Bad, "Aposta inválida.", None, false);
            return;
        }
        if self.in_duel(id) {
            self.event(id, Tone::Info, "Você já está num duelo.", None, false);
            return;
        }
        self.cancel_challenge(id);
        match self.queue.iter_mut().find(|q| q.id == id) {
            Some(entry) => (entry.fraction, entry.draft, entry.group) = (fraction, draft, group),
            None => self.queue.push(Queued { id, fraction, draft, group, since: now }),
        }
        let text = if group {
            format!("Na fila de duelo em grupo. Começa com {DUEL_MAX_PLAYERS} jogadores, ou com {DUEL_GROUP_MIN} depois de {} s.", GROUP_QUEUE_WAIT.as_secs())
        } else {
            "Na fila de duelo. Esperando outro jogador...".into()
        };
        self.event(id, Tone::Info, text, None, false);
        self.matchmake(now);
    }

    fn rank_index(&self, id: PlayerId) -> usize {
        let rank = self.slots.get(&id).map_or(Rank::Sub5, |s| s.player.rank());
        Rank::ALL.iter().position(|r| *r == rank).unwrap_or(0)
    }

    /// Pairs queued players of close rank (anyone once someone waited long enough), and fills group duels.
    fn matchmake(&mut self, now: Instant) {
        'search: loop {
            for i in 0..self.queue.len() {
                let a = self.queue[i];
                if a.group {
                    continue;
                }
                let rank_a = self.rank_index(a.id);
                let best = self
                    .queue
                    .iter()
                    .enumerate()
                    .filter(|(j, b)| *j != i && !b.group && !self.same_clan(a.id, b.id))
                    .map(|(j, b)| (j, *b, rank_a.abs_diff(self.rank_index(b.id))))
                    .min_by_key(|(.., gap)| *gap);
                let Some((j, b, gap)) = best else { continue };
                let waited = |since: Instant| now.saturating_duration_since(since) >= QUEUE_RELAX;
                if gap <= QUEUE_RANK_GAP || waited(a.since) || waited(b.since) {
                    let players = if i < j { vec![a.id, b.id] } else { vec![b.id, a.id] };
                    self.start_duel(players, a.fraction.min(b.fraction), a.draft && b.draft, now);
                    continue 'search;
                }
            }
            break;
        }
        // Group queue: the longest waiting first, skipping anyone who shares a clan with a player already in.
        let mut group: Vec<Queued> = Vec::new();
        for q in self.queue.iter().filter(|q| q.group) {
            if group.len() < DUEL_MAX_PLAYERS && group.iter().all(|g| !self.same_clan(g.id, q.id)) {
                group.push(*q);
            }
        }
        let waited = group.first().is_some_and(|q| now.saturating_duration_since(q.since) >= GROUP_QUEUE_WAIT);
        if group.len() == DUEL_MAX_PLAYERS || (group.len() >= DUEL_GROUP_MIN && waited) {
            let fraction = group.iter().map(|q| q.fraction).fold(f64::INFINITY, f64::min);
            self.start_duel(group.iter().map(|q| q.id).collect(), fraction, false, now);
        }
    }

    /// Takes `id` out of every challenge it is in, telling the others why.
    fn drop_invites_of(&mut self, id: PlayerId, why: &str) {
        let name = self.name_of(id);
        let hosted: Vec<Lobby> = self.lobbies.extract_if(.., |l| l.host == id).collect();
        for lobby in hosted {
            for other in lobby.guests() {
                self.event(other, Tone::Info, format!("{name} {why}; o desafio foi cancelado."), None, false);
                self.push.push(other);
            }
        }
        let mut hosts = Vec::new();
        for lobby in self.lobbies.iter_mut().filter(|l| l.pending.contains(&id) || l.accepted.contains(&id)) {
            lobby.pending.retain(|p| *p != id);
            lobby.accepted.retain(|p| *p != id);
            hosts.push(lobby.host);
        }
        for host in hosts {
            self.event(host, Tone::Info, format!("{name} {why}; saiu do desafio."), None, false);
            self.push.push(host);
        }
    }

    /// Times out challenges: the ones with someone who accepted start without the rest.
    fn expire_invites(&mut self, now: Instant) {
        let mut ready = false;
        let expired: Vec<(PlayerId, Vec<PlayerId>, bool)> = self
            .lobbies
            .iter_mut()
            .filter(|l| now >= l.expires && !l.pending.is_empty())
            .map(|l| (l.host, std::mem::take(&mut l.pending), !l.accepted.is_empty()))
            .collect();
        for (host, silent, starts) in expired {
            let from = self.name_of(host);
            self.event(host, Tone::Info, format!("{} não respondeu ao desafio.", self.names_of(&silent)), None, false);
            for p in &silent {
                self.event(*p, Tone::Info, format!("O desafio de {from} expirou."), None, false);
            }
            self.push.push(host);
            self.push.extend(silent);
            ready |= starts;
        }
        self.lobbies.retain(|l| !(l.pending.is_empty() && l.accepted.is_empty()));
        if ready {
            self.start_ready_lobbies(now);
        }
    }

    /// Takes every bet in escrow and starts the duel; `players[0]` is the host or the first in line.
    fn start_duel(&mut self, players: Vec<PlayerId>, fraction: f64, draft: bool, now: Instant) {
        let stake = self.stake_for(players.iter().copied(), fraction);
        for id in &players {
            if let Some(slot) = self.slots.get_mut(id) {
                slot.player.mp -= stake;
                slot.dirty = true;
            }
        }
        for id in &players {
            self.drop_invites_of(*id, "entrou num duelo");
        }
        self.queue.retain(|q| !players.contains(&q.id));
        let names: Vec<String> = players.iter().map(|id| self.name_of(*id)).collect();
        let mode = if draft { format!("cada um escolhe {} minigames", DUEL_ROUNDS / 2) } else { "minigames sorteados".into() };
        for id in &players {
            let others: Vec<PlayerId> = players.iter().copied().filter(|p| p != id).collect();
            let text = if players.len() == 2 {
                format!("Duelo contra {} valendo {} MP! {DUEL_ROUNDS} rodadas, {mode}.", self.names_of(&others), format_mp(stake))
            } else {
                format!(
                    "Duelo em grupo contra {} valendo {} MP cada! {DUEL_ROUNDS} rodadas, {mode}. Pontos por colocação, o vencedor leva tudo.",
                    self.names_of(&others),
                    format_mp(stake)
                )
            };
            self.event(*id, Tone::Epic, text, Some("DUELO!".into()), false);
        }
        self.push.extend(players.iter().copied());
        self.active.push(Duel::new(players, names, stake, draft, now, &mut rand::thread_rng()));
    }

    /// `at_ms` is the client's own timing; `rtt_max_ms` bounds how far back it may reach.
    /// Sets or clears a player's clan; everyone's next state shows the new tag.
    pub fn set_clan(&mut self, player: PlayerId, clan: Option<(i64, String)>) {
        match clan {
            Some(c) => self.clan_of.insert(player, c),
            None => self.clan_of.remove(&player),
        };
        self.push.extend(self.slots.keys().copied());
    }

    /// Clan id and tag of an online player.
    /// Whether two different players belong to the same clan.
    pub fn same_clan(&self, a: PlayerId, b: PlayerId) -> bool {
        a != b && matches!((self.clan_of.get(&a), self.clan_of.get(&b)), (Some(x), Some(y)) if x.0 == y.0)
    }

    pub fn clan_of(&self, player: PlayerId) -> Option<(i64, &str)> {
        self.clan_of.get(&player).map(|(id, tag)| (*id, tag.as_str()))
    }

    /// Online players, with their names, for features outside the game rules.
    pub fn online_names(&self) -> impl Iterator<Item = (PlayerId, &str)> {
        self.slots.iter().map(|(id, slot)| (*id, slot.player.name.as_str()))
    }

    /// Duels in progress, for tools that watch the server.
    pub fn duels(&self) -> &[Duel] {
        &self.active
    }

    /// `at_ms` is the client's own timing; `rtt_max_ms` bounds how far back it may reach.
    pub fn duel_input(&mut self, id: PlayerId, key: DuelKey, at_ms: Option<f64>, rtt_max_ms: f64, now: Instant) {
        let Some(i) = self.duel_index(id) else { return };
        let duel = &mut self.active[i];
        let side = duel.side(id).expect("found by id");
        duel.input(side, key, at_ms, rtt_max_ms, now, &mut rand::thread_rng());
        self.push.extend(duel.players.iter().copied());
        self.settle(i);
    }

    pub fn forfeit(&mut self, id: PlayerId, now: Instant) {
        let Some(i) = self.duel_index(id) else { return };
        let duel = &mut self.active[i];
        let side = duel.side(id).expect("found by id");
        duel.forfeit(side, now);
        self.push.extend(duel.players.iter().copied());
        if duel.outcome.is_none() {
            let lost = format_mp(duel.stake);
            self.event(id, Tone::Bad, format!("Você desistiu do duelo em grupo e deixou {lost} MP no pote."), None, false);
        }
        self.settle(i);
    }

    fn tick_duels(&mut self, now: Instant) {
        let mut rng = rand::thread_rng();
        let mut i = 0;
        while i < self.active.len() {
            let duel = &mut self.active[i];
            let before = duel.revision(now);
            let done = duel.tick(now, &mut rng);
            if duel.revision(now) != before || done {
                self.push.extend(duel.players.iter().copied());
            }
            self.settle(i);
            if done {
                self.active.remove(i);
            } else {
                i += 1;
            }
        }
    }

    /// Updates the duel records behind achievements, for a duel worth MP.
    fn record_duel(&mut self, i: usize, outcome: Option<usize>) {
        let duel = &self.active[i];
        let (players, history, pot, wins, rounds) = (duel.players.clone(), duel.history.clone(), duel.pot(), duel.wins.clone(), duel.rounds);
        let best_others: Vec<u32> = (0..players.len()).map(|side| wins[duel.rival(side)]).collect();
        let rivals = players.len() as u32 - 1;
        for (side, id) in players.iter().enumerate() {
            let Some(slot) = self.slots.get_mut(id) else { continue };
            let p = &mut slot.player;
            for (game, _) in history.iter().filter(|(_, w)| *w == Some(side)) {
                p.duel_rounds_won += 1;
                if !p.minigames_won.contains(game) {
                    p.minigames_won.push(*game);
                }
            }
            let best_other = best_others[side];
            if outcome == Some(side) && wins[side] > best_other {
                p.flawless_duels += (best_other == 0 && wins[side] > DUEL_ROUNDS / 2 * rivals) as u32;
                p.sudden_death_wins += (rounds > DUEL_ROUNDS) as u32;
                p.best_duel_pot = p.best_duel_pot.max(pot);
            }
            slot.dirty = true;
        }
    }

    /// Moves the stakes once a duel is decided.
    fn settle(&mut self, i: usize) {
        let Some(outcome) = self.active[i].take_settlement() else { return };
        let duel = &self.active[i];
        let (players, names, stake, pot, rounds, out) = (duel.players.clone(), duel.names.clone(), duel.stake, duel.pot(), duel.rounds, duel.out.clone());
        // Friendly duels and walkovers before any round was played never count for records or achievements.
        let ranked = stake > 0.0 && duel.played > 0;
        if ranked {
            self.record_duel(i, outcome);
        }
        let unranked_note = if ranked { "" } else { " Não conta para o placar." };
        let (gain, loss, worth) = if stake > 0.0 {
            (format!(" +{} MP", format_mp(pot - stake)), format!(" -{} MP", format_mp(stake)), format!(" valendo {} MP", format_mp(pot)))
        } else {
            (" (amistoso)".to_string(), " (amistoso)".to_string(), " amistoso".to_string())
        };
        match outcome {
            Some(w) => {
                let winner = players[w];
                let losers: Vec<PlayerId> = players.iter().copied().filter(|p| *p != winner).collect();
                if let Some(slot) = self.slots.get_mut(&winner) {
                    // A transfer between players, so it never counts toward the rank.
                    slot.player.mp += pot;
                    slot.player.duels_won += ranked as u32;
                    slot.dirty = true;
                }
                for loser in &losers {
                    if let Some(slot) = self.slots.get_mut(loser) {
                        slot.player.duels_lost += ranked as u32;
                        slot.dirty = true;
                    }
                }
                let (wn, ln) = (&names[w], self.names_of(&losers));
                let won_text = format!("Você venceu o duelo contra {ln}!{gain}{unranked_note}");
                self.event(winner, Tone::Epic, won_text, Some("VITÓRIA NO DUELO".into()), false);
                for loser in &losers {
                    self.event(*loser, Tone::Bad, format!("{wn} venceu o duelo.{loss}{unranked_note}"), Some("DERROTA NO DUELO".into()), true);
                }
                let others: Vec<PlayerId> = if stake >= DUEL_ANNOUNCE_MIN {
                    self.slots.keys().copied().filter(|id| !players.contains(id)).collect()
                } else {
                    Vec::new()
                };
                for other in others {
                    self.event(other, Tone::Info, format!("{wn} venceu {ln} num duelo{worth}."), None, false);
                }
            }
            None => {
                // A draw gives the pot back to whoever stayed; bets left by quitters are shared too.
                let stayed: Vec<PlayerId> = players.iter().zip(&out).filter(|(_, out)| !**out).map(|(p, _)| *p).collect();
                let text = if stayed.len() == players.len() { "Duelo empatado. As apostas voltaram." } else { "Duelo empatado. O pote foi dividido entre quem ficou." };
                for id in &stayed {
                    if let Some(slot) = self.slots.get_mut(id) {
                        slot.player.mp += pot / stayed.len() as f64;
                        slot.dirty = true;
                    }
                    self.event(*id, Tone::Info, text, Some("EMPATE".into()), false);
                }
            }
        }
        self.matches.push(MatchRecord {
            a: players[0],
            b: players[1],
            winner: outcome.map(|w| players[w]),
            stake,
            rounds,
            ranked,
            players: players.len() as u32,
        });
    }

    /// Calls off every running duel and returns the bets (server shutting down).
    pub fn abort_duels(&mut self) {
        for i in 0..self.active.len() {
            if self.active[i].outcome.is_none() {
                self.active[i].outcome = Some(None);
                self.settle(i);
                self.matches.pop(); // called off, not a real draw
            }
        }
        self.active.clear();
    }
}

/// Immunity after the `hits`-th mog in a row: it grows each time, up to a ceiling.
pub fn escalated_immunity(base: Duration, hits: u32) -> Duration {
    let factor = IMMUNITY_GROWTH.powi(hits.saturating_sub(1).min(16) as i32);
    base.mul_f64(factor).min(IMMUNITY_MAX.max(base))
}

fn tier_tone(tier: TimingTier) -> Tone {
    match tier {
        TimingTier::Bad => Tone::Bad,
        TimingTier::Ok => Tone::Info,
        TimingTier::Good => Tone::Good,
        TimingTier::Perfect => Tone::Epic,
    }
}
