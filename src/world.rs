//! Authoritative game rules, run by the server for every connected player.

use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

use rand::Rng;

use crate::constants::*;
use crate::db::{DuelRecord, MatchRecord, PurchaseRecord};
use crate::duel::{Duel, DuelKey, DuelView};
use crate::game::{Achievement, Player, Rank, TimingTier, Tone, crit_chance, meter_value, streak_mult};
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
    pub meter_started: Instant,
    mew_window: (Instant, u32),
    /// Mogs suffered in a row and when the last one landed; each one lengthens the immunity.
    hits_in_row: u32,
    last_hit: Instant,
    last_rank: Rank,
    dirty: bool,
}

impl Slot {
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
            meter_started: now,
            mew_window: (now, 0),
            hits_in_row: 0,
            last_hit: now,
            last_rank,
            dirty: false,
        }
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
    invites: Vec<Invite>,
    /// Players waiting for a duel, with the bet fraction they accept.
    queue: Vec<(PlayerId, f64, bool, Instant)>,
    /// Fixed critical chance instead of the attack/defense formula, for exact payouts in tests.
    pub crit_override: Option<f64>,
}

struct Invite {
    from: PlayerId,
    to: PlayerId,
    fraction: f64,
    draft: bool,
    expires: Instant,
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

    fn event(&mut self, to: PlayerId, tone: Tone, text: impl Into<String>, banner: Option<String>, hit: bool) {
        self.outbox.push((to, ServerMsg::Event { tone, text: text.into(), banner, hit }));
    }

    fn fx(&mut self, to: PlayerId, fx: Fx) {
        self.outbox.push((to, ServerMsg::Fx(fx)));
    }

    fn broadcast_except(&mut self, except: PlayerId, tone: Tone, text: &str) {
        let others: Vec<PlayerId> = self.slots.keys().copied().filter(|id| *id != except).collect();
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
        self.broadcast_except(id, Tone::Info, &format!("{name} entrou no servidor. Hora do mog."));
        Ok(())
    }

    /// Takes a player offline and returns the profile to persist.
    pub fn leave(&mut self, id: PlayerId, now: Instant) -> Option<Player> {
        if !self.slots.contains_key(&id) {
            return None;
        }
        self.forfeit(id, now);
        self.drop_invites_of(id, "saiu do servidor");
        self.queue.retain(|(q, ..)| *q != id);
        let slot = self.slots.remove(&id)?;
        self.clan_of.remove(&id);
        self.broadcast_except(id, Tone::Info, &format!("{} saiu do servidor.", slot.player.name));
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
            self.broadcast_except(id, Tone::Info, &format!("{old} agora se chama {name}."));
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
            for ach in Achievement::ALL {
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

    pub fn use_item(&mut self, id: PlayerId, item: ItemId, now: Instant) {
        let Some(slot) = self.slots.get_mut(&id) else { return };
        let name = items::def(item).name;
        if slot.player.level(item) == 0 {
            self.event(id, Tone::Bad, format!("Você não tem {name}. Compre na loja [L]."), None, false);
            return;
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
        match result {
            Ok(text) => {
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
        let mut power = stats.attack * timing * streak_mult(slot.streak, &stats);
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
                if cologne > 1.0 {
                    self.fx(id, Fx::CologneHit);
                }
                ("won", won, 0.0)
            } else {
                let why = format!("a defesa deu {} contra seu poder de {}", format_mp(roll), format_mp(power));
                ("lost", 0.0, self.on_loss(id, NPC_NAME, &why))
            };
            self.duels.push(DuelRecord { attacker: id, defender: None, outcome, tier: tier.label(), power, mp_won: won, mp_lost: lost });
            return;
        }

        let defender_name = self.slots[&target].player.name.clone();
        let punch = self.punch_down(id, target);
        let (outcome, won, lost) = match self.defend(target, &attacker_name, power, take_mult * punch, crit, now) {
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
                if cologne > 1.0 {
                    self.fx(id, Fx::CologneHit);
                }
                ("won", won, loss)
            }
            Defense::Held { roll } => {
                let why = format!("a defesa deu {} contra seu poder de {}", format_mp(roll), format_mp(power));
                let lost = self.on_loss(id, &defender_name, &why);
                self.on_held(target, &attacker_name, lost);
                ("lost", 0.0, lost)
            }
            Defense::Blocked => {
                self.event(id, Tone::Info, format!("{defender_name} bloqueou com o Escudo Sigma."), None, false);
                self.fx(id, Fx::ShieldBounced);
                ("blocked", 0.0, 0.0)
            }
        };
        self.duels.push(DuelRecord { attacker: id, defender: Some(target), outcome, tier: tier.label(), power, mp_won: won, mp_lost: lost });
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
    fn on_loss(&mut self, id: PlayerId, target: &str, why: &str) -> f64 {
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
        let penalty = ((mp * LOSS_PENALTY_RATIO).max(LOSS_PENALTY_FLAT) * shield).min(mp);
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

    /// `take_mult` scales how much of the defender's stake the attacker takes; a `crit` goes through any defense.
    fn defend(&mut self, id: PlayerId, attacker: &str, power: f64, take_mult: f64, crit: bool, now: Instant) -> Defense {
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
        let roll = stats.defense * mirror * rand::thread_rng().gen_range(DEFENSE_ROLL);
        if crit || power > roll {
            let mp = slot.player.mp.max(0.0);
            slot.player.nemesis = Some(attacker.to_string());
            let broken = std::mem::take(&mut slot.streak).min(STREAK_CAP);
            let bounty = 1.0 + STREAK_BOUNTY * broken as f64;
            let stake = (STAKE_FLAT + mp * STAKE_RATIO).min(STAKE_CAP) * take_mult * bounty;
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
                }
            })
            .collect();
        let duel = self.duel_index(id).map(|i| {
            let d = &self.active[i];
            let side = d.side(id).expect("found by id");
            let opponent = self.slots.get(&d.players[1 - side]).map_or(0.0, |s| s.player.lifetime);
            let other = d.players[1 - side];
            DuelView { opponent_tag: self.clan_of.get(&other).map(|(_, tag)| tag.clone()), ..d.view(side, opponent, now) }
        });
        let invite_view = |inv: &Invite, peer: PlayerId| InviteView {
            peer,
            name: self.slots.get(&peer).map_or(String::new(), |s| s.player.name.clone()),
            fraction: inv.fraction,
            stake: self.stake_for(inv.from, inv.to, inv.fraction),
            expires_ms: remaining_ms(inv.expires, now),
            draft: inv.draft,
        };
        let invites = self.invites.iter().filter(|i| i.to == id).map(|i| invite_view(i, i.from)).collect();
        let outgoing = self.invites.iter().find(|i| i.from == id).map(|i| invite_view(i, i.to));
        let queued = self.queue.iter().find(|(q, ..)| *q == id).map(|(_, f, ..)| *f);
        Some(StateView {
            me: slot.player.clone(),
            streak: slot.streak,
            cooldown_ms: remaining_ms(slot.cooldown_until, now),
            cooldown_total_ms: (slot.cooldown_total * 1000.0) as u64,
            immune_ms: remaining_ms(slot.immune_until, now),
            preworkout_ms: remaining_ms(slot.preworkout_until, now),
            cologne_ms: remaining_ms(slot.cologne_until, now),
            aura_farm_ms: remaining_ms(slot.aura_farm_until, now),
            meter_ms: now.saturating_duration_since(slot.meter_started).as_millis() as u64,
            players,
            duel,
            invites,
            outgoing,
            queued,
            rtt_ms: 0.0,
            clan_tag: self.clan_of.get(&id).map(|(_, tag)| tag.clone()),
        })
    }
}

impl World {
    fn duel_index(&self, id: PlayerId) -> Option<usize> {
        self.active.iter().position(|d| d.side(id).is_some())
    }

    /// True while the player is in a duel or looking at its result.
    pub fn in_duel(&self, id: PlayerId) -> bool {
        self.duel_index(id).is_some()
    }

    pub fn duel_count(&self) -> usize {
        self.active.len()
    }

    /// MP each side bets: a share of the poorer player's MP.
    fn stake_for(&self, a: PlayerId, b: PlayerId, fraction: f64) -> f64 {
        let mp = |id: PlayerId| self.slots.get(&id).map_or(0.0, |s| s.player.mp.max(0.0));
        (fraction * mp(a).min(mp(b))).floor()
    }

    fn name_of(&self, id: PlayerId) -> String {
        self.slots.get(&id).map_or_else(|| "?".into(), |s| s.player.name.clone())
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

    pub fn challenge(&mut self, id: PlayerId, target: PlayerId, fraction: f64, draft: bool, now: Instant) {
        if !self.slots.contains_key(&id) {
            return;
        }
        if !valid_fraction(fraction) {
            self.event(id, Tone::Bad, "Aposta inválida.", None, false);
            return;
        }
        if let Some(reason) = self.duel_blocker(id, target) {
            self.event(id, Tone::Info, reason, None, false);
            return;
        }
        if fraction > 0.0 && self.same_clan(id, target) {
            self.event(id, Tone::Info, CLAN_BET_REFUSED, None, false);
            return;
        }
        if self.invites.iter().any(|i| i.from == target && i.to == id) {
            self.event(id, Tone::Info, format!("{} já te desafiou: aceite com A.", self.name_of(target)), None, false);
            return;
        }
        self.cancel_challenge(id);
        self.queue.retain(|(q, ..)| *q != id);
        self.invites.push(Invite { from: id, to: target, fraction, draft, expires: now + INVITE_TTL });
        let stake = format_mp(self.stake_for(id, target, fraction));
        let (me, them) = (self.name_of(id), self.name_of(target));
        let mode = if draft { ", escolhendo os minigames" } else { "" };
        self.event(id, Tone::Info, format!("Desafio enviado para {them} valendo {stake} MP{mode}. Esperando resposta..."), None, false);
        self.event(
            target,
            Tone::Epic,
            format!("{me} te desafiou para um duelo valendo {stake} MP{mode}! A aceita, R recusa."),
            Some(format!("DESAFIO DE {}", me.to_uppercase())),
            false,
        );
        self.push.extend([id, target]);
    }

    pub fn cancel_challenge(&mut self, id: PlayerId) {
        let Some(pos) = self.invites.iter().position(|i| i.from == id) else { return };
        let invite = self.invites.remove(pos);
        let me = self.name_of(id);
        self.event(invite.to, Tone::Info, format!("{me} cancelou o desafio."), None, false);
        self.push.extend([id, invite.to]);
    }

    pub fn answer(&mut self, id: PlayerId, from: PlayerId, accept: bool, now: Instant) {
        let Some(pos) = self.invites.iter().position(|i| i.from == from && i.to == id) else {
            self.event(id, Tone::Info, "Esse desafio não existe mais.", None, false);
            return;
        };
        let invite = self.invites.remove(pos);
        self.push.extend([id, from]);
        let me = self.name_of(id);
        if !accept {
            self.event(from, Tone::Info, format!("{me} recusou o duelo."), None, false);
            self.event(id, Tone::Info, "Desafio recusado.", None, false);
            return;
        }
        if let Some(reason) = self.duel_blocker(id, from) {
            self.event(id, Tone::Info, reason, None, false);
            return;
        }
        if invite.fraction > 0.0 && self.same_clan(id, from) {
            self.event(id, Tone::Info, CLAN_BET_REFUSED, None, false);
            return;
        }
        self.start_duel(from, id, invite.fraction, invite.draft, now);
    }

    /// Joins the queue with a bet, or leaves it with `None`; the draft needs both sides asking for it.
    pub fn queue(&mut self, id: PlayerId, fraction: Option<f64>, draft: bool, now: Instant) {
        if !self.slots.contains_key(&id) {
            return;
        }
        self.push.push(id);
        let Some(fraction) = fraction else {
            if self.queue.iter().any(|(q, ..)| *q == id) {
                self.queue.retain(|(q, ..)| *q != id);
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
        match self.queue.iter_mut().find(|(q, ..)| *q == id) {
            Some(entry) => (entry.1, entry.2) = (fraction, draft),
            None => self.queue.push((id, fraction, draft, now)),
        }
        self.event(id, Tone::Info, "Na fila de duelo. Esperando outro jogador...", None, false);
        self.matchmake(now);
    }

    fn rank_index(&self, id: PlayerId) -> usize {
        let rank = self.slots.get(&id).map_or(Rank::Sub5, |s| s.player.rank());
        Rank::ALL.iter().position(|r| *r == rank).unwrap_or(0)
    }

    /// Pairs queued players of close rank; anyone goes once someone waited long enough.
    fn matchmake(&mut self, now: Instant) {
        'search: loop {
            for i in 0..self.queue.len() {
                let (a, fa, da, since_a) = self.queue[i];
                let best = self
                    .queue
                    .iter()
                    .enumerate()
                    .filter(|(j, (b, ..))| *j != i && !self.same_clan(a, *b))
                    .map(|(j, &(b, fb, db, since_b))| (j, b, fb, db, since_b, self.rank_index(a).abs_diff(self.rank_index(b))))
                    .min_by_key(|(.., gap)| *gap);
                let Some((j, b, fb, db, since_b, gap)) = best else { continue };
                let waited = |since: Instant| now.saturating_duration_since(since) >= QUEUE_RELAX;
                if gap <= QUEUE_RANK_GAP || waited(since_a) || waited(since_b) {
                    let (first, second) = if i < j { (a, b) } else { (b, a) };
                    self.queue.retain(|(q, ..)| *q != a && *q != b);
                    self.start_duel(first, second, fa.min(fb), da && db, now);
                    continue 'search;
                }
            }
            break;
        }
    }

    fn drop_invites_of(&mut self, id: PlayerId, why: &str) {
        let name = self.name_of(id);
        let gone: Vec<Invite> = self.invites.extract_if(.., |i| i.from == id || i.to == id).collect();
        for invite in gone {
            let other = if invite.from == id { invite.to } else { invite.from };
            self.event(other, Tone::Info, format!("{name} {why}; o desafio foi cancelado."), None, false);
            self.push.push(other);
        }
    }

    fn expire_invites(&mut self, now: Instant) {
        let expired: Vec<Invite> = self.invites.extract_if(.., |i| now >= i.expires).collect();
        for invite in expired {
            let (from, to) = (self.name_of(invite.from), self.name_of(invite.to));
            self.event(invite.from, Tone::Info, format!("{to} não respondeu ao desafio."), None, false);
            self.event(invite.to, Tone::Info, format!("O desafio de {from} expirou."), None, false);
            self.push.extend([invite.from, invite.to]);
        }
    }

    /// Takes both bets in escrow and starts the duel.
    fn start_duel(&mut self, a: PlayerId, b: PlayerId, fraction: f64, draft: bool, now: Instant) {
        let stake = self.stake_for(a, b, fraction);
        for id in [a, b] {
            if let Some(slot) = self.slots.get_mut(&id) {
                slot.player.mp -= stake;
                slot.dirty = true;
            }
        }
        self.drop_invites_of(a, "entrou num duelo");
        self.drop_invites_of(b, "entrou num duelo");
        self.queue.retain(|(q, ..)| *q != a && *q != b);
        let names = [self.name_of(a), self.name_of(b)];
        let mode = if draft { format!("cada um escolhe {} minigames", DUEL_ROUNDS / 2) } else { "minigames sorteados".into() };
        for (me, other) in [(a, &names[1]), (b, &names[0])] {
            let text = format!("Duelo contra {other} valendo {} MP! {DUEL_ROUNDS} rodadas, {mode}.", format_mp(stake));
            self.event(me, Tone::Epic, text, Some("DUELO!".into()), false);
        }
        self.active.push(Duel::new([a, b], names, stake, draft, now, &mut rand::thread_rng()));
        self.push.extend([a, b]);
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

    pub fn duel_input(&mut self, id: PlayerId, key: DuelKey, at_ms: Option<f64>, rtt_max_ms: f64, now: Instant) {
        let Some(i) = self.duel_index(id) else { return };
        let duel = &mut self.active[i];
        let side = duel.side(id).expect("found by id");
        duel.input(side, key, at_ms, rtt_max_ms, now, &mut rand::thread_rng());
        self.push.extend(duel.players);
        self.settle(i);
    }

    pub fn forfeit(&mut self, id: PlayerId, now: Instant) {
        let Some(i) = self.duel_index(id) else { return };
        let duel = &mut self.active[i];
        let side = duel.side(id).expect("found by id");
        duel.forfeit(side, now);
        self.push.extend(duel.players);
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
                self.push.extend(duel.players);
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
        let (players, history, stake, wins, rounds) = (duel.players, duel.history.clone(), duel.stake, duel.wins, duel.rounds);
        for side in 0..2 {
            let Some(slot) = self.slots.get_mut(&players[side]) else { continue };
            let p = &mut slot.player;
            for (game, _) in history.iter().filter(|(_, w)| *w == Some(side)) {
                p.duel_rounds_won += 1;
                if !p.minigames_won.contains(game) {
                    p.minigames_won.push(*game);
                }
            }
            if outcome == Some(side) && wins[side] > wins[1 - side] {
                p.flawless_duels += (wins[1 - side] == 0 && wins[side] > DUEL_ROUNDS / 2) as u32;
                p.sudden_death_wins += (rounds > DUEL_ROUNDS) as u32;
                p.best_duel_pot = p.best_duel_pot.max(2.0 * stake);
            }
            slot.dirty = true;
        }
    }

    /// Moves the stakes once a duel is decided.
    fn settle(&mut self, i: usize) {
        let Some(outcome) = self.active[i].take_settlement() else { return };
        let duel = &self.active[i];
        let (players, names, stake, rounds) = (duel.players, duel.names.clone(), duel.stake, duel.rounds);
        // Friendly duels and walkovers before any round was played never count for records or achievements.
        let ranked = stake > 0.0 && duel.played > 0;
        if ranked {
            self.record_duel(i, outcome);
        }
        let unranked_note = if ranked { "" } else { " Não conta para o placar." };
        let (gain, loss, worth) = if stake > 0.0 {
            let pot = format_mp(stake);
            (format!(" +{pot} MP"), format!(" -{pot} MP"), format!(" valendo {pot} MP"))
        } else {
            (" (amistoso)".to_string(), " (amistoso)".to_string(), " amistoso".to_string())
        };
        match outcome {
            Some(w) => {
                let (winner, loser) = (players[w], players[1 - w]);
                if let Some(slot) = self.slots.get_mut(&winner) {
                    // A transfer between players, so it never counts toward the rank.
                    slot.player.mp += 2.0 * stake;
                    slot.player.duels_won += ranked as u32;
                    slot.dirty = true;
                }
                if let Some(slot) = self.slots.get_mut(&loser) {
                    slot.player.duels_lost += ranked as u32;
                    slot.dirty = true;
                }
                let (wn, ln) = (&names[w], &names[1 - w]);
                let won_text = format!("Você venceu o duelo contra {ln}!{gain}{unranked_note}");
                self.event(winner, Tone::Epic, won_text, Some("VITÓRIA NO DUELO".into()), false);
                self.event(loser, Tone::Bad, format!("{wn} venceu o duelo.{loss}{unranked_note}"), Some("DERROTA NO DUELO".into()), true);
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
                for id in players {
                    if let Some(slot) = self.slots.get_mut(&id) {
                        slot.player.mp += stake;
                        slot.dirty = true;
                    }
                    self.event(id, Tone::Info, "Duelo empatado. As apostas voltaram.", Some("EMPATE".into()), false);
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
