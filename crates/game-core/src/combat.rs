use hollowrite_protocol::{Action, ActorState, Dir8, EntityId, GameEvent, HitKind, Tile, CELL};

use crate::content::{AttackTiming, SpellForm};
use crate::geom::{dir_towards, in_arc, Pos};
use crate::world::{Act, ActKind, Player, Projectile, Side, World};

const UNARMED_REACH: i32 = 1100;
const UNARMED_ARC: u32 = 90;
const EMBER_PIT_DAMAGE: u32 = 14;
const EMBER_PIT_EVERY: u64 = 15;
const HAZARD_EVERY: u64 = 15;
const PROJECTILE_HIT_RADIUS: i32 = 450;

fn unarmed_timing(heavy: bool) -> AttackTiming {
    if heavy {
        AttackTiming { windup: 18, active: 4, recovery: 18, stamina: 26, poise: 30, mult_pct: 150 }
    } else {
        AttackTiming { windup: 8, active: 3, recovery: 12, stamina: 15, poise: 10, mult_pct: 100 }
    }
}

/// Outcome of an attack landing on a player, reported back to the attacker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Landed {
    Ignored,
    Dodged,
    Parried,
    Blocked,
    Hit,
}

pub struct Incoming {
    pub src: EntityId,
    pub src_pos: Pos,
    pub raw: u32,
    pub poise: u32,
    pub parryable: bool,
    pub heavy: bool,
    pub unblockable: bool,
}

impl World {
    fn timing(&self, p: &Player, heavy: bool) -> AttackTiming {
        match p.data.equipment.main.as_deref().and_then(|w| self.content.weapon(w)) {
            Some(w) => {
                if heavy {
                    w.heavy.clone()
                } else {
                    w.light.clone()
                }
            }
            None => unarmed_timing(heavy),
        }
    }

    fn reach_arc(&self, p: &Player) -> (i32, u32) {
        match p.data.equipment.main.as_deref().and_then(|w| self.content.weapon(w)) {
            Some(w) => (w.reach, w.arc),
            None => (UNARMED_REACH, UNARMED_ARC),
        }
    }

    fn spend_stamina(&self, p: &mut Player, cost: u32) {
        p.stamina = p.stamina.saturating_sub(cost);
        p.stamina_delay = self.content.balance.stamina_regen_delay;
    }

    /// Turn toward the nearest enemy in reach unless one is already in the
    /// swing arc. Keeps melee usable on 8-way keyboards without granting
    /// any reach or damage the swing would not otherwise have.
    fn soft_lock(&self, p: &mut Player, reach: i32, arc: u32) {
        let pos = p.body.pos;
        let alive = || self.enemies.values().filter(|e| e.alive());
        if alive().any(|e| in_arc(pos, p.body.facing, reach, arc, e.body.pos, e.body.radius)) {
            return;
        }
        if let Some(e) = alive()
            .filter(|e| e.body.pos.within(pos, reach + e.body.radius + 500))
            .min_by_key(|e| e.body.pos.dist2(pos))
        {
            if let Some(d) = pos.dir_to(e.body.pos) {
                p.body.facing = d;
            }
        }
    }

    /// Attempts to begin an action from a free state. Returns true if the
    /// action was accepted.
    fn start_action(&mut self, p: &mut Player, a: Action) -> bool {
        let content = self.content.clone();
        let b = &content.balance;
        if p.stamina == 0 && matches!(a, Action::Light | Action::Heavy | Action::Roll | Action::Parry) {
            return false;
        }
        if let Some(d) = p.move_dir {
            p.body.facing = d;
        }
        match a {
            Action::Light | Action::Heavy => {
                let heavy = a == Action::Heavy;
                let t = self.timing(p, heavy);
                let (reach, arc) = self.reach_arc(p);
                self.soft_lock(p, reach, arc);
                self.spend_stamina(p, t.stamina);
                p.body.act =
                    Act::new(ActorState::Windup, if heavy { ActKind::Heavy } else { ActKind::Light }, t.windup);
                true
            }
            Action::Roll => {
                let roll = &b.rolls[&content.archetype(p.data.archetype).roll];
                let dir = p.move_dir.unwrap_or(p.body.facing);
                p.body.facing = dir;
                self.spend_stamina(p, roll.stamina);
                p.body.act = Act::new(ActorState::Rolling, ActKind::Roll(dir), roll.ticks);
                true
            }
            Action::Parry => {
                let can = p
                    .data
                    .equipment
                    .off
                    .as_deref()
                    .and_then(|s| content.item(s))
                    .and_then(|i| i.shield.as_ref())
                    .is_some_and(|s| s.parry);
                if !can {
                    return false;
                }
                self.spend_stamina(p, b.parry.stamina);
                p.body.act = Act::new(ActorState::Parrying, ActKind::Parry, b.parry.total);
                true
            }
            Action::Heal => {
                if p.data.draughts == 0 {
                    return false;
                }
                p.body.act = Act::new(ActorState::Healing, ActKind::Heal, b.draught.windup);
                true
            }
            Action::Cast1 | Action::Cast2 => {
                let slot = if a == Action::Cast1 { 0 } else { 1 };
                let spell_id = if slot == 0 { &p.data.equipment.spell1 } else { &p.data.equipment.spell2 };
                let Some(spell) = spell_id.as_deref().and_then(|s| content.item(s)).and_then(|i| i.spell.as_ref())
                else {
                    return false;
                };
                if p.cooldowns[slot] > 0 || p.wick < spell.wick {
                    return false;
                }
                if p.data.attr(hollowrite_protocol::Attribute::Occult) < spell.occult_req {
                    return false;
                }
                if spell.form == SpellForm::Bolt {
                    self.soft_lock(p, spell.range, 60);
                }
                p.wick -= spell.wick;
                p.body.act = Act::new(ActorState::Casting, ActKind::Cast(slot as u8), spell.windup);
                true
            }
            Action::Interact | Action::GuardOn | Action::GuardOff => false,
        }
    }

    pub(crate) fn tick_player(&mut self, id: EntityId) {
        let content = self.content.clone();
        let b = &content.balance;
        let tick = self.tick;
        let Some(mut p) = self.players.remove(&id) else { return };

        if p.body.act.state == ActorState::Dead {
            p.dead_timer = p.dead_timer.saturating_sub(1);
            let done = p.dead_timer == 0;
            self.players.insert(id, p);
            if done {
                self.respawn_player(id);
            }
            return;
        }

        let derived = p.data.derived(&content);
        let busy_for_regen = matches!(p.body.act.state, ActorState::Windup | ActorState::Active | ActorState::Rolling);
        if p.stamina_delay > 0 {
            p.stamina_delay -= 1;
        } else if !busy_for_regen {
            let regen =
                if p.body.act.state == ActorState::Guarding { b.stamina_regen_guarding } else { b.stamina_regen };
            p.stamina = (p.stamina + regen).min(derived.stamina_max);
        }
        p.wick_timer += 1;
        if p.wick_timer >= b.wick_regen_every {
            p.wick_timer = 0;
            p.wick = (p.wick + 1).min(derived.wick_max);
        }
        for c in p.cooldowns.iter_mut() {
            *c = c.saturating_sub(1);
        }
        if p.body.poise_timer > 0 {
            p.body.poise_timer -= 1;
        } else {
            p.body.poise = p.body.poise_max;
        }
        if p.move_ttl > 0 {
            p.move_ttl -= 1;
            if p.move_ttl == 0 {
                p.move_dir = None;
            }
        }

        let pending = p.pending.take();
        let mut interact = false;
        let mut just_started = false;
        if p.body.act.is_free() {
            let buffered = p
                .buffered
                .take()
                .filter(|(_, at)| tick.saturating_sub(*at) <= b.input_buffer_ticks as u64)
                .map(|(a, _)| a);
            if let Some(a) = pending.or(buffered) {
                if a == Action::Interact {
                    interact = true;
                } else {
                    just_started = self.start_action(&mut p, a);
                }
            }
        } else if let Some(a) = pending {
            if a != Action::Interact {
                p.buffered = Some((a, tick));
            }
        }

        let flags = p.data.flags.clone();
        let pass = |x: i32, y: i32| self.player_passable(&flags, x, y);
        let r = p.body.radius;
        let mut swing: Option<&'static str> = None;
        let mut strike = false;
        let mut cast_fire: Option<u8> = None;
        // The tick an action starts on counts as its first frame of zero, so
        // a windup of N ticks gives exactly N ticks of warning before it hits.
        let state = if just_started { None } else { Some(p.body.act.state) };
        match state {
            Some(ActorState::Idle | ActorState::Moving | ActorState::Guarding) => {
                let guarding = p.guard_held;
                if guarding {
                    if p.body.act.state != ActorState::Guarding {
                        p.body.act = Act::new(ActorState::Guarding, ActKind::None, 0);
                    }
                } else if p.body.act.state == ActorState::Guarding {
                    p.body.act = Act::idle();
                }
                match p.move_dir {
                    Some(d) => {
                        let speed = if guarding { b.walk_speed * b.guard_speed_pct / 100 } else { b.walk_speed };
                        let (dx, dy) = crate::geom::step(d, speed);
                        p.body.facing = d;
                        p.body.pos = World::slide(p.body.pos, dx, dy, r, &pass);
                        if !guarding {
                            p.body.act.state = ActorState::Moving;
                        }
                    }
                    None => {
                        if !guarding {
                            p.body.act.state = ActorState::Idle;
                        }
                    }
                }
            }
            Some(ActorState::Windup) => {
                p.body.act.t += 1;
                if p.body.act.t >= p.body.act.dur {
                    let heavy = p.body.act.kind == ActKind::Heavy;
                    let t = self.timing(&p, heavy);
                    p.body.act = Act::new(ActorState::Active, p.body.act.kind, t.active.max(1));
                    swing = Some(if heavy { "heavy" } else { "light" });
                    strike = true;
                }
            }
            Some(ActorState::Active) => {
                strike = true;
                p.body.act.t += 1;
                if p.body.act.t >= p.body.act.dur {
                    let heavy = p.body.act.kind == ActKind::Heavy;
                    let t = self.timing(&p, heavy);
                    p.body.act = Act::new(ActorState::Recovery, p.body.act.kind, t.recovery);
                }
            }
            Some(ActorState::Recovery | ActorState::Parrying | ActorState::Staggered | ActorState::Interacting) => {
                p.body.act.t += 1;
                if p.body.act.t >= p.body.act.dur {
                    p.body.act = Act::idle();
                }
            }
            Some(ActorState::Rolling) => {
                if let ActKind::Roll(d) = p.body.act.kind {
                    let roll = &b.rolls[&content.archetype(p.data.archetype).roll];
                    if p.body.act.t < roll.move_ticks {
                        let per = roll.distance / roll.move_ticks as i32;
                        let (dx, dy) = crate::geom::step(d, per);
                        p.body.pos = World::slide(p.body.pos, dx, dy, r, &pass);
                    }
                }
                p.body.act.t += 1;
                if p.body.act.t >= p.body.act.dur {
                    p.body.act = Act::idle();
                }
            }
            Some(ActorState::Healing) => {
                if let Some(d) = p.move_dir {
                    let (dx, dy) = crate::geom::step(d, b.walk_speed * b.heal_speed_pct / 100);
                    p.body.facing = d;
                    p.body.pos = World::slide(p.body.pos, dx, dy, r, &pass);
                }
                p.body.act.t += 1;
                if p.body.act.t == b.draught.apply_at && p.data.draughts > 0 {
                    p.data.draughts -= 1;
                    p.body.hp = (p.body.hp + p.body.hp_max * b.draught.heal_pct / 100).min(p.body.hp_max);
                }
                if p.body.act.t >= p.body.act.dur {
                    p.body.act = Act::idle();
                }
            }
            Some(ActorState::Casting) => {
                p.body.act.t += 1;
                if p.body.act.t >= p.body.act.dur {
                    if let ActKind::Cast(slot) = p.body.act.kind {
                        cast_fire = Some(slot);
                    }
                }
            }
            Some(ActorState::Dead) | None => {}
        }

        let (cx, cy) = p.body.pos.cell();
        let tile = content.map.tile(cx, cy);
        if p.body.act.is_free() && tile != Tile::EmberPit {
            p.last_safe = p.body.pos;
        }
        let on_pit = tile == Tile::EmberPit && tick.is_multiple_of(EMBER_PIT_EVERY);

        if let Some(name) = swing {
            let pos = p.body.pos;
            self.emit_near(
                pos,
                GameEvent::Swing { src: id, action: name.into(), facing: p.body.facing, x: pos.x, y: pos.y },
            );
        }
        if strike {
            self.player_strike(&mut p);
        }
        if let Some(slot) = cast_fire {
            self.fire_spell(&mut p, slot);
        }
        self.players.insert(id, p);
        if interact {
            self.interact(id);
        }
        if on_pit && !self.player_invulnerable(id) {
            self.hazard_damage(id, 0, EMBER_PIT_DAMAGE);
        }
    }

    fn player_strike(&mut self, p: &mut Player) {
        let heavy = p.body.act.kind == ActKind::Heavy;
        let t = self.timing(p, heavy);
        let (reach, arc) = self.reach_arc(p);
        let d = p.data.derived(&self.content);
        let dmg = if heavy { d.heavy_damage } else { d.light_damage };
        let origin = p.body.pos;
        let flags = &p.data.flags;
        let targets: Vec<EntityId> = self
            .enemies
            .values()
            .filter(|e| e.alive() && !p.body.act.hit.contains(&e.id))
            .filter(|e| in_arc(origin, p.body.facing, reach, arc, e.body.pos, e.body.radius))
            .filter(|e| {
                self.content.map.line_clear(origin.cell(), e.body.pos.cell(), |x, y| self.player_opaque(flags, x, y))
            })
            .map(|e| e.id)
            .collect();
        for eid in targets {
            p.body.act.hit.push(eid);
            self.damage_enemy(eid, p.id, &p.data.id, dmg, t.poise);
        }
    }

    fn fire_spell(&mut self, p: &mut Player, slot: u8) {
        let content = self.content.clone();
        let spell_id = if slot == 0 { p.data.equipment.spell1.clone() } else { p.data.equipment.spell2.clone() };
        let Some((item, spell)) = spell_id
            .as_deref()
            .and_then(|s| content.item(s))
            .and_then(|i| i.spell.as_ref().map(|sp| (i.id.clone(), sp)))
        else {
            p.body.act = Act::idle();
            return;
        };
        let occ = p.data.attr(hollowrite_protocol::Attribute::Occult);
        let dmg = spell.damage + occ * spell.occult_pct / 100;
        p.cooldowns[slot as usize] = spell.cooldown;
        p.body.act = Act::new(ActorState::Recovery, ActKind::Cast(slot), spell.recovery);
        let pos = p.body.pos;
        self.emit_near(
            pos,
            GameEvent::Swing { src: p.id, action: item.clone(), facing: p.body.facing, x: pos.x, y: pos.y },
        );
        match spell.form {
            SpellForm::Bolt => {
                let target = self
                    .enemies
                    .values()
                    .filter(|e| e.alive() && in_arc(pos, p.body.facing, spell.range, 60, e.body.pos, e.body.radius))
                    .min_by_key(|e| e.body.pos.dist2(pos))
                    .map(|e| e.body.pos);
                let (vx, vy) = match target {
                    Some(t) => velocity(pos, t, spell.speed),
                    None => crate::geom::step(p.body.facing, spell.speed),
                };
                let id = self.alloc_id();
                self.projectiles.insert(
                    id,
                    Projectile {
                        id,
                        def: item,
                        owner: p.id,
                        hits: Side::Enemies,
                        pos: pos.offset(p.body.facing, 400),
                        vx,
                        vy,
                        range_left: spell.range,
                        damage: dmg,
                        poise: spell.poise,
                    },
                );
            }
            SpellForm::Toll => {
                self.emit_near(pos, GameEvent::Toll { x: pos.x, y: pos.y, radius: (spell.radius / 100) as u16 });
                let flags = &p.data.flags;
                let targets: Vec<EntityId> = self
                    .enemies
                    .values()
                    .filter(|e| e.alive() && e.body.pos.within(pos, spell.radius + e.body.radius))
                    .filter(|e| {
                        self.content
                            .map
                            .line_clear(pos.cell(), e.body.pos.cell(), |x, y| self.player_opaque(flags, x, y))
                    })
                    .map(|e| e.id)
                    .collect();
                for eid in targets {
                    self.damage_enemy(eid, p.id, &p.data.id, dmg, spell.poise);
                }
            }
        }
    }

    pub fn player_invulnerable(&self, id: EntityId) -> bool {
        let Some(p) = self.players.get(&id) else { return true };
        if p.body.act.state != ActorState::Rolling {
            return false;
        }
        let roll = &self.content.balance.rolls[&self.content.archetype(p.data.archetype).roll];
        p.body.act.t >= roll.iframe_from && p.body.act.t < roll.iframe_to
    }

    pub(crate) fn hazard_damage(&mut self, id: EntityId, src: EntityId, amount: u32) {
        let Some(p) = self.players.get_mut(&id) else { return };
        if p.body.act.state == ActorState::Dead {
            return;
        }
        p.body.hp = p.body.hp.saturating_sub(amount);
        let pos = p.body.pos;
        let dead = p.body.hp == 0;
        self.emit_near(pos, GameEvent::Hit { src, dst: id, amount, kind: HitKind::Hazard, x: pos.x, y: pos.y });
        if dead {
            self.kill_player(id);
        }
    }

    /// Resolves an enemy attack landing on a player.
    pub(crate) fn damage_player(&mut self, id: EntityId, hit: Incoming) -> Landed {
        let content = self.content.clone();
        let b = &content.balance;
        let invuln = self.player_invulnerable(id);
        let Some(p) = self.players.get_mut(&id) else { return Landed::Ignored };
        if p.body.act.state == ActorState::Dead {
            return Landed::Ignored;
        }
        let pos = p.body.pos;
        if invuln {
            self.emit_near(
                pos,
                GameEvent::Hit { src: hit.src, dst: id, amount: 0, kind: HitKind::Dodged, x: pos.x, y: pos.y },
            );
            return Landed::Dodged;
        }
        let facing_attacker = {
            let (ux, uy) = crate::geom::unit(p.body.facing);
            let dx = (hit.src_pos.x - pos.x) as f64;
            let dy = (hit.src_pos.y - pos.y) as f64;
            dx * ux + dy * uy >= 0.0
        };
        let shield = p.data.equipment.off.as_deref().and_then(|s| content.item(s)).and_then(|i| i.shield.as_ref());
        if p.body.act.state == ActorState::Parrying
            && p.body.act.t < b.parry.window
            && hit.parryable
            && !hit.unblockable
            && facing_attacker
            && shield.is_some_and(|s| s.parry)
        {
            self.emit_near(
                pos,
                GameEvent::Hit { src: hit.src, dst: id, amount: 0, kind: HitKind::Parried, x: pos.x, y: pos.y },
            );
            return Landed::Parried;
        }
        let defense = p.data.derived(&content).defense;
        let mitigated = mitigate(hit.raw, defense);
        if p.body.act.state == ActorState::Guarding && facing_attacker && !hit.unblockable {
            let (guard_pct, stamina_pct) = match shield {
                Some(s) => (s.guard_pct, s.guard_stamina_pct),
                None => {
                    let w = p.data.equipment.main.as_deref().and_then(|w| content.weapon(w));
                    (w.and_then(|w| w.guard_pct).unwrap_or(b.unarmed_guard_pct), 100)
                }
            };
            let mut cost = hit.raw * stamina_pct / 100;
            if hit.heavy {
                cost *= 2;
            }
            let through = mitigated * (100 - guard_pct) / 100;
            if p.stamina >= cost {
                p.stamina -= cost;
                p.stamina_delay = b.stamina_regen_delay;
                p.body.hp = p.body.hp.saturating_sub(through);
                let dead = p.body.hp == 0;
                self.emit_near(
                    pos,
                    GameEvent::Hit {
                        src: hit.src,
                        dst: id,
                        amount: through,
                        kind: HitKind::Blocked,
                        x: pos.x,
                        y: pos.y,
                    },
                );
                if dead {
                    self.kill_player(id);
                }
                return Landed::Blocked;
            }
            p.stamina = 0;
            p.stamina_delay = b.stamina_regen_delay;
            p.body.hp = p.body.hp.saturating_sub(through);
            p.body.act = Act::new(ActorState::Staggered, ActKind::None, b.stagger_ticks);
            let dead = p.body.hp == 0;
            self.emit_near(
                pos,
                GameEvent::Hit { src: hit.src, dst: id, amount: through, kind: HitKind::Blocked, x: pos.x, y: pos.y },
            );
            self.emit_near(pos, GameEvent::Staggered { id });
            if dead {
                self.kill_player(id);
            }
            return Landed::Blocked;
        }
        p.body.hp = p.body.hp.saturating_sub(mitigated);
        p.body.poise = p.body.poise.saturating_sub(hit.poise);
        p.body.poise_timer = b.poise_regen_delay;
        let mut staggered = false;
        if p.body.poise == 0 && p.body.hp > 0 {
            p.body.poise = p.body.poise_max;
            if p.body.act.state != ActorState::Rolling {
                p.body.act = Act::new(ActorState::Staggered, ActKind::None, b.stagger_ticks / 2);
                staggered = true;
            }
        }
        let dead = p.body.hp == 0;
        self.emit_near(
            pos,
            GameEvent::Hit { src: hit.src, dst: id, amount: mitigated, kind: HitKind::Normal, x: pos.x, y: pos.y },
        );
        if staggered {
            self.emit_near(pos, GameEvent::Staggered { id });
        }
        if dead {
            self.kill_player(id);
        }
        Landed::Hit
    }

    pub(crate) fn tick_projectiles(&mut self) {
        let ids: Vec<EntityId> = self.projectiles.keys().copied().collect();
        for id in ids {
            let mut pr = self.projectiles.remove(&id).unwrap();
            let next = Pos::new(pr.pos.x + pr.vx, pr.pos.y + pr.vy);
            let (cx, cy) = next.cell();
            let speed = ((pr.vx as i64).pow(2) as f64 + (pr.vy as i64).pow(2) as f64).sqrt() as i32;
            pr.range_left -= speed.max(1);
            if self.terrain_opaque(cx, cy) || !self.content.map.in_bounds(cx, cy) || pr.range_left <= 0 {
                continue;
            }
            pr.pos = next;
            match pr.hits {
                Side::Players => {
                    let target = self
                        .players
                        .values()
                        .filter(|p| p.body.act.state != ActorState::Dead)
                        .filter(|p| p.body.pos.within(pr.pos, PROJECTILE_HIT_RADIUS + p.body.radius))
                        .min_by_key(|p| p.body.pos.dist2(pr.pos))
                        .map(|p| p.id);
                    if let Some(t) = target {
                        let src_pos = Pos::new(pr.pos.x - pr.vx * 4, pr.pos.y - pr.vy * 4);
                        let landed = self.damage_player(
                            t,
                            Incoming {
                                src: pr.owner,
                                src_pos,
                                raw: pr.damage,
                                poise: pr.poise,
                                parryable: false,
                                heavy: false,
                                unblockable: false,
                            },
                        );
                        if landed != Landed::Dodged {
                            continue;
                        }
                    }
                }
                Side::Enemies => {
                    let target = self
                        .enemies
                        .values()
                        .filter(|e| e.alive() && e.body.pos.within(pr.pos, PROJECTILE_HIT_RADIUS + e.body.radius))
                        .min_by_key(|e| e.body.pos.dist2(pr.pos))
                        .map(|e| e.id);
                    if let Some(t) = target {
                        let char_id = self.players.get(&pr.owner).map(|p| p.data.id.clone()).unwrap_or_default();
                        self.damage_enemy(t, pr.owner, &char_id, pr.damage, pr.poise);
                        continue;
                    }
                }
            }
            self.projectiles.insert(id, pr);
        }
    }

    pub(crate) fn tick_hazards(&mut self) {
        let now = self.tick;
        self.hazards.retain(|h| h.until > now);
        let pools: Vec<_> =
            self.hazards.iter().filter(|h| now >= h.arm_at).map(|h| (h.owner, h.center, h.radius, h.damage)).collect();
        if now.is_multiple_of(HAZARD_EVERY) {
            for (owner, center, radius, damage) in pools {
                let victims: Vec<EntityId> = self
                    .players
                    .values()
                    .filter(|p| {
                        p.body.act.state != ActorState::Dead && p.body.pos.within(center, radius + p.body.radius)
                    })
                    .map(|p| p.id)
                    .collect();
                for v in victims {
                    self.hazard_damage(v, owner, damage);
                }
            }
        }
        let mut rings = std::mem::take(&mut self.rings);
        rings.retain(|r| now < r.start || r.radius(now).is_some());
        for ring in rings.iter_mut() {
            let Some(r) = ring.radius(now) else { continue };
            let victims: Vec<EntityId> = self
                .players
                .values()
                .filter(|p| p.body.act.state != ActorState::Dead && !ring.hit.contains(&p.id))
                .filter(|p| {
                    let d = p.body.pos.dist(ring.center);
                    d >= r - 600 && d <= r + 400
                })
                .map(|p| p.id)
                .collect();
            for v in victims {
                ring.hit.push(v);
                self.damage_player(
                    v,
                    Incoming {
                        src: ring.owner,
                        src_pos: ring.center,
                        raw: ring.damage,
                        poise: 40,
                        parryable: false,
                        heavy: true,
                        unblockable: true,
                    },
                );
            }
        }
        self.rings = rings;
    }
}

pub fn mitigate(raw: u32, defense: u32) -> u32 {
    (raw * 100 / (100 + 2 * defense)).max(1)
}

pub fn velocity(from: Pos, to: Pos, speed: i32) -> (i32, i32) {
    let dx = (to.x - from.x) as f64;
    let dy = (to.y - from.y) as f64;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1.0 {
        return (0, speed);
    }
    ((dx / len * speed as f64) as i32, (dy / len * speed as f64) as i32)
}

pub fn facing_to(from: Pos, to: Pos, fallback: Dir8) -> Dir8 {
    dir_towards(to.x - from.x, to.y - from.y).unwrap_or(fallback)
}

pub const fn cells(n: i32) -> i32 {
    n * CELL
}
