use std::collections::{BTreeMap, BTreeSet};

use hollowrite_protocol::{
    ActorState, Dir8, EntityId, EntityKind, EntityView, GameEvent, HitKind, Relation, Telegraph, TelegraphKind,
};

use crate::combat::{facing_to, mitigate, velocity, Incoming, Landed};
use crate::content::{AttackShape, Behavior, Content, EnemyAttack};
use crate::geom::{cells_where, in_arc, in_line, Pos};
use crate::world::{Act, ActKind, Body, Encounter, Hazard, LedgerEntry, Loot, Out, Projectile, Ring, Side, World};

const LINE_HALF_WIDTH: i32 = 600;
const LOST_TARGET_TICKS: u16 = 90;
const REPATH_TICKS: u16 = 6;
const DEAD_VISIBLE_TICKS: u32 = 30;
const LOOT_LIFETIME: u64 = 30 * 180;
const BLINK_COOLDOWN: u16 = 90;
const PATH_BUDGET: usize = 1600;
const RETREAT_BUDGET: u16 = 60;
const ATTACK_GAP_MIN: i32 = 18;
const ATTACK_GAP_MAX: i32 = 45;
const HOLD_GROUND_TICKS: u64 = 150;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Idle,
    Chase,
    Return,
    Dead,
}

#[derive(Debug, Clone)]
pub struct Windup {
    pub attack: EnemyAttack,
    pub origin: Pos,
    pub facing: Dir8,
    pub target: Pos,
    pub cells: Vec<[i16; 2]>,
    pub tele: u32,
}

#[derive(Debug, Clone, Default)]
pub struct BossState {
    pub phase: u8,
    pub transition_until: u64,
    pub vulnerable_until: u64,
    pub next_dirge: u64,
    pub peal_pending_until: u64,
    pub threat: BTreeMap<EntityId, u64>,
}

#[derive(Debug, Clone)]
pub struct Enemy {
    pub id: EntityId,
    pub def: String,
    pub home: Pos,
    pub home_facing: Dir8,
    pub body: Body,
    pub mode: Mode,
    pub target: Option<EntityId>,
    pub lost: u16,
    pub cooldowns: BTreeMap<String, u16>,
    pub windup: Option<Windup>,
    pub riposte: u16,
    /// Damage dealt this life, keyed by character id so rewards survive a
    /// reconnect that changes the entity id.
    pub contributors: BTreeMap<String, u32>,
    pub life: u64,
    pub dead_since: u64,
    pub path_timer: u16,
    pub waypoint: Option<(i32, i32)>,
    pub blink_cd: u16,
    pub speed: i32,
    pub boss: Option<BossState>,
    pub pending_death: bool,
    /// After recovering from a stagger an enemy briefly ignores poise damage,
    /// so a stagger is a punish window rather than a lock.
    pub steady_until: u64,
    pub retreat_ticks: u16,
    pub hold_until: u64,
    /// Earliest tick the next attack may start. The gap after each
    /// recovery is the player's window to strike back.
    pub next_attack: u64,
}

impl Enemy {
    pub fn new(id: EntityId, content: &Content, def: &str, home: Pos) -> Enemy {
        let d = &content.enemies[def];
        let boss = (d.behavior == Behavior::Boss).then(|| BossState { phase: 1, ..Default::default() });
        Enemy {
            id,
            def: def.to_string(),
            home,
            home_facing: Dir8::S,
            body: Body {
                pos: home,
                facing: Dir8::S,
                hp: d.hp,
                hp_max: d.hp,
                poise: d.poise,
                poise_max: d.poise,
                poise_timer: 0,
                radius: d.radius.unwrap_or(content.balance.body_radius),
                act: Act::idle(),
            },
            mode: Mode::Idle,
            target: None,
            lost: 0,
            cooldowns: BTreeMap::new(),
            windup: None,
            riposte: 0,
            contributors: BTreeMap::new(),
            life: 1,
            dead_since: 0,
            path_timer: 0,
            waypoint: None,
            blink_cd: 0,
            speed: d.speed,
            boss,
            pending_death: false,
            steady_until: 0,
            retreat_ticks: 0,
            hold_until: 0,
            next_attack: 0,
        }
    }

    pub fn is_boss(&self) -> bool {
        self.boss.is_some()
    }

    pub fn alive(&self) -> bool {
        self.mode != Mode::Dead && self.body.hp > 0
    }

    pub fn visible(&self) -> bool {
        self.mode != Mode::Dead || self.body.act.t < DEAD_VISIBLE_TICKS as u16
    }

    pub fn view(&self, content: &Content) -> EntityView {
        let def = &content.enemies[&self.def];
        EntityView {
            id: self.id,
            kind: if self.is_boss() { EntityKind::Boss } else { EntityKind::Enemy },
            def: self.def.clone(),
            name: Some(def.name.clone()),
            x: self.body.pos.x,
            y: self.body.pos.y,
            facing: self.body.facing,
            state: self.body.act.state,
            action: self.windup.as_ref().map(|w| w.attack.id.clone()),
            progress: self.body.act.progress(),
            rel: Relation::Hostile,
            hp: Some([self.body.hp, self.body.hp_max]),
            look: None,
            phase: self.boss.as_ref().map(|b| b.phase),
        }
    }

    pub fn telegraphs(&self, tick: u64) -> Vec<Telegraph> {
        let mut out = Vec::new();
        if let Some(w) = &self.windup {
            if matches!(self.body.act.state, ActorState::Windup | ActorState::Active) {
                let kind = match w.attack.shape {
                    AttackShape::Circle | AttackShape::TargetCircle => TelegraphKind::Area,
                    AttackShape::Ring => TelegraphKind::Shockwave,
                    _ if w.attack.heavy => TelegraphKind::Heavy,
                    _ => TelegraphKind::Strike,
                };
                let left =
                    if self.body.act.state == ActorState::Windup { self.body.act.dur - self.body.act.t } else { 0 };
                out.push(Telegraph {
                    id: w.tele,
                    owner: self.id,
                    kind,
                    cells: w.cells.clone(),
                    left,
                    total: w.attack.windup,
                });
            }
        }
        if let Some(b) = &self.boss {
            if b.vulnerable_until > tick {
                let r = self.body.radius + 1000;
                out.push(Telegraph {
                    id: 0x4000_0000 | self.id,
                    owner: self.id,
                    kind: TelegraphKind::Weak,
                    cells: cells_where(self.body.pos, r, |c| c.within(self.body.pos, r)),
                    left: (b.vulnerable_until - tick) as u16,
                    total: 0,
                });
            }
        }
        out
    }

    fn reset_at_home(&mut self) {
        self.body.pos = self.home;
        self.body.facing = self.home_facing;
        self.body.hp = self.body.hp_max;
        self.body.poise = self.body.poise_max;
        self.body.act = Act::idle();
        self.mode = Mode::Idle;
        self.target = None;
        self.windup = None;
        self.riposte = 0;
        self.contributors.clear();
        self.cooldowns.clear();
        self.waypoint = None;
    }
}

impl World {
    fn enemy_attacks(&self, e: &Enemy) -> Vec<EnemyAttack> {
        let def = &self.content.enemies[&e.def];
        match (&e.boss, &def.phase2) {
            (Some(b), Some(p2)) if b.phase >= 2 => p2.attacks.clone(),
            _ => def.attacks.clone(),
        }
    }

    pub(crate) fn damage_enemy(&mut self, eid: EntityId, attacker: EntityId, character: &str, raw: u32, poise: u32) {
        let content = self.content.clone();
        let tick = self.tick;
        let Some(e) = self.enemies.get_mut(&eid) else { return };
        if !e.alive() || e.mode == Mode::Return {
            return;
        }
        let def = &content.enemies[&e.def];
        let pos = e.body.pos;
        if let Some(b) = &e.boss {
            if b.transition_until > tick {
                self.out.push(Out::Near(
                    pos,
                    GameEvent::Hit { src: attacker, dst: eid, amount: 0, kind: HitKind::Blocked, x: pos.x, y: pos.y },
                ));
                return;
            }
        }
        let mut mult = 100;
        let mut kind = HitKind::Normal;
        if e.riposte > 0 {
            mult = content.balance.critical_mult_pct;
            kind = HitKind::Critical;
            e.riposte = 0;
        }
        if let (Some(b), Some(p2)) = (&e.boss, &def.phase2) {
            if b.vulnerable_until > tick {
                mult = mult * p2.vulnerable_mult_pct / 100;
                kind = HitKind::Critical;
            }
        }
        let dmg = mitigate(raw * mult / 100, def.defense);
        e.body.hp = e.body.hp.saturating_sub(dmg);
        *e.contributors.entry(character.to_string()).or_insert(0) += dmg;
        if let Some(b) = &mut e.boss {
            *b.threat.entry(attacker).or_insert(0) += dmg as u64;
        }
        if e.target.is_none() || e.mode == Mode::Idle {
            e.target = Some(attacker);
            e.mode = Mode::Chase;
        }
        let mut staggered = false;
        if tick >= e.steady_until && e.body.act.state != ActorState::Staggered {
            e.body.poise = e.body.poise.saturating_sub(poise);
            e.body.poise_timer = content.balance.poise_regen_delay;
        }
        if e.body.poise == 0 && e.body.hp > 0 {
            let b = &content.balance;
            e.body.poise = e.body.poise_max;
            e.windup = None;
            e.body.act = Act::new(ActorState::Staggered, ActKind::Enemy, b.enemy_stagger_ticks);
            e.steady_until = tick + (b.enemy_stagger_ticks + b.enemy_steady_ticks) as u64;
            staggered = true;
        }
        if e.body.hp == 0 {
            e.pending_death = true;
        }
        self.out
            .push(Out::Near(pos, GameEvent::Hit { src: attacker, dst: eid, amount: dmg, kind, x: pos.x, y: pos.y }));
        if staggered {
            self.out.push(Out::Near(pos, GameEvent::Staggered { id: eid }));
        }
    }

    fn enemy_dies(&mut self, eid: EntityId) {
        let content = self.content.clone();
        let tick = self.tick;
        let e = self.enemies.get_mut(&eid).unwrap();
        e.pending_death = false;
        e.mode = Mode::Dead;
        e.windup = None;
        e.body.act = Act::new(ActorState::Dead, ActKind::Enemy, 0);
        e.dead_since = tick;
        let pos = e.body.pos;
        let life = e.life;
        let def = content.enemies[&e.def].clone();
        let contributors = std::mem::take(&mut e.contributors);
        let is_boss = e.is_boss();
        self.out.push(Out::Near(pos, GameEvent::Died { id: eid, x: pos.x, y: pos.y }));
        if is_boss {
            self.boss_defeated(eid, &def.name, def.echoes);
            return;
        }
        // Each enemy life pays out exactly once: contributors were taken above,
        // so a second death notification for the same life finds nobody.
        for (character, _) in contributors {
            let Some(pid) = self.player_by_character(&character) else { continue };
            let p = self.players.get_mut(&pid).unwrap();
            if p.body.act.state == ActorState::Dead {
                continue;
            }
            p.data.echoes += def.echoes;
            let total = p.data.echoes;
            p.ledger.push(LedgerEntry {
                kind: "kill",
                delta: def.echoes as i64,
                balance: total,
                reference: format!("{}#{}:{}", def.id, eid, life),
            });
            self.out.push(Out::To(pid, GameEvent::EchoesGained { amount: def.echoes, total }));
            for drop in &def.drops {
                if self.rng.chance_pct(drop.chance_pct) {
                    let id = self.alloc_id();
                    let jitter = Pos::new(pos.x + self.rng.range_i32(-300, 300), pos.y + self.rng.range_i32(-300, 300));
                    self.loot.insert(
                        id,
                        Loot {
                            id,
                            owner: pid,
                            item: drop.item.clone(),
                            qty: drop.qty,
                            pos: jitter,
                            expires: tick + LOOT_LIFETIME,
                        },
                    );
                }
            }
            self.out.push(Out::SheetChanged(pid));
        }
    }

    pub(crate) fn tick_enemy(&mut self, eid: EntityId) {
        let content = self.content.clone();
        let b = &content.balance;
        let tick = self.tick;
        if self.enemies[&eid].pending_death {
            self.enemy_dies(eid);
        }
        let mut e = self.enemies.remove(&eid).unwrap();
        let def = content.enemies[&e.def].clone();

        if e.mode == Mode::Dead {
            e.body.act.t = e.body.act.t.saturating_add(1);
            if !e.is_boss() && tick - e.dead_since >= b.enemy_respawn_ticks as u64 {
                let clear = !self.players.values().any(|p| p.body.pos.within(e.home, b.enemy_respawn_clear_radius));
                if clear {
                    e.reset_at_home();
                    e.life += 1;
                }
            }
            self.enemies.insert(eid, e);
            return;
        }

        for cd in e.cooldowns.values_mut() {
            *cd = cd.saturating_sub(1);
        }
        e.blink_cd = e.blink_cd.saturating_sub(1);
        e.riposte = e.riposte.saturating_sub(1);
        if e.body.poise_timer > 0 {
            e.body.poise_timer -= 1;
        } else {
            e.body.poise = e.body.poise_max;
        }

        if e.is_boss() {
            self.boss_think(&mut e);
        }

        match e.body.act.state {
            ActorState::Staggered => {
                e.body.act.t += 1;
                if e.body.act.t >= e.body.act.dur {
                    e.body.act = Act::idle();
                }
            }
            ActorState::Casting => {
                e.body.act.t += 1;
                if e.body.act.t >= e.body.act.dur {
                    e.body.act = Act::idle();
                }
            }
            ActorState::Windup => {
                e.body.act.t += 1;
                if e.body.act.t >= e.body.act.dur {
                    let w = e.windup.as_ref().unwrap();
                    e.body.act = Act::new(ActorState::Active, ActKind::Enemy, w.attack.active.max(1));
                    let pos = e.body.pos;
                    self.out.push(Out::Near(
                        pos,
                        GameEvent::Swing {
                            src: eid,
                            action: w.attack.id.clone(),
                            facing: w.facing,
                            x: pos.x,
                            y: pos.y,
                        },
                    ));
                    self.enemy_strike(&mut e, true);
                }
            }
            ActorState::Active => {
                self.enemy_strike(&mut e, false);
                e.body.act.t += 1;
                if e.body.act.t >= e.body.act.dur {
                    let rec = e.windup.as_ref().map_or(10, |w| w.attack.recovery);
                    e.body.act = Act::new(ActorState::Recovery, ActKind::Enemy, rec);
                }
            }
            ActorState::Recovery => {
                e.body.act.t += 1;
                if e.body.act.t >= e.body.act.dur {
                    e.body.act = Act::idle();
                    e.windup = None;
                    let (lo, hi) = if e.is_boss() { (8, 20) } else { (ATTACK_GAP_MIN, ATTACK_GAP_MAX) };
                    e.next_attack = tick + self.rng.range_i32(lo, hi) as u64;
                }
            }
            _ => self.enemy_think(&mut e, &def),
        }
        self.enemies.insert(eid, e);
    }

    fn valid_target(&self, e: &Enemy, pid: EntityId, sight: i32) -> bool {
        let Some(p) = self.players.get(&pid) else { return false };
        if p.body.act.state == ActorState::Dead {
            return false;
        }
        let (cx, cy) = p.body.pos.cell();
        if self.content.map.zone_at(cx, cy).is_some_and(|z| z.safe) {
            return false;
        }
        if e.is_boss() {
            if let Encounter::Active { participants, .. } = &self.encounter {
                return participants.contains(&pid);
            }
            return false;
        }
        p.body.pos.within(e.body.pos, sight)
    }

    fn sees(&self, e: &Enemy, target: Pos) -> bool {
        self.content.map.line_clear(e.body.pos.cell(), target.cell(), |x, y| self.terrain_opaque(x, y))
    }

    fn enemy_think(&mut self, e: &mut Enemy, def: &crate::content::EnemyDef) {
        let pos = e.body.pos;
        if e.mode == Mode::Return {
            if pos.within(e.home, 600) {
                e.reset_at_home();
                return;
            }
            self.walk_towards(e, e.home);
            return;
        }

        if let Some(t) = e.target {
            if !self.valid_target(e, t, def.sight + 4000) {
                e.target = None;
            }
        }
        if e.target.is_none() {
            let found = self
                .players
                .values()
                .filter(|p| self.valid_target(e, p.id, def.sight) && (e.is_boss() || self.sees(e, p.body.pos)))
                .min_by_key(|p| p.body.pos.dist2(pos))
                .map(|p| p.id);
            if let Some(t) = found {
                e.target = Some(t);
                e.mode = Mode::Chase;
                e.lost = 0;
            }
        }
        let Some(tid) = e.target else {
            if e.mode == Mode::Chase || (!pos.within(e.home, 600) && !e.is_boss()) {
                e.mode = Mode::Return;
            }
            e.body.act.state = ActorState::Idle;
            return;
        };
        if !e.is_boss() && !pos.within(e.home, def.leash) {
            e.mode = Mode::Return;
            e.target = None;
            return;
        }
        let tpos = self.players[&tid].body.pos;
        if e.is_boss() || self.sees(e, tpos) {
            e.lost = 0;
        } else {
            e.lost += 1;
            if e.lost > LOST_TARGET_TICKS {
                e.target = None;
                e.mode = Mode::Return;
                return;
            }
        }
        let dist = pos.dist(tpos);

        if let Some(range) = def.blink {
            if dist < 2200 && e.blink_cd == 0 && self.try_blink(e, tpos, range) {
                return;
            }
        }

        let attacks = self.enemy_attacks(e);
        let sees = e.lost == 0;
        let ready = self.tick >= e.next_attack;
        let candidates: Vec<&EnemyAttack> = attacks
            .iter()
            .filter(|a| ready && sees && dist >= a.min_range && dist <= a.max_range + e.body.radius)
            .filter(|a| e.cooldowns.get(&a.id).copied().unwrap_or(0) == 0)
            .collect();
        if !candidates.is_empty() {
            let total: u32 = candidates.iter().map(|a| a.weight).sum();
            let mut roll = self.rng.below(total);
            let mut chosen = candidates[0];
            for a in &candidates {
                if roll < a.weight {
                    chosen = a;
                    break;
                }
                roll -= a.weight;
            }
            self.begin_enemy_attack(e, chosen.clone(), tpos);
            return;
        }

        // Ranged foes back off, but only for so long: after a couple of
        // seconds of retreating they hold their ground instead of kiting.
        let tick = self.tick;
        if def.keep_distance > 0 && dist < def.keep_distance - 2000 && tick >= e.hold_until {
            e.retreat_ticks += 1;
            if e.retreat_ticks > RETREAT_BUDGET {
                e.retreat_ticks = 0;
                e.hold_until = tick + HOLD_GROUND_TICKS;
            }
            self.step_away(e, tpos);
        } else if def.keep_distance > 0 && (dist < def.keep_distance - 2000 || (dist <= def.keep_distance + 1500 && sees)) {
            e.body.facing = facing_to(pos, tpos, e.body.facing);
            e.body.act.state = ActorState::Idle;
        } else {
            self.walk_towards(e, tpos);
        }
    }

    fn try_blink(&mut self, e: &mut Enemy, from: Pos, range: i32) -> bool {
        e.blink_cd = BLINK_COOLDOWN;
        let (hx, hy) = e.body.pos.cell();
        let cells = range / 1000;
        for _ in 0..12 {
            let cx = hx + self.rng.range_i32(-cells, cells);
            let cy = hy + self.rng.range_i32(-cells, cells);
            let dest = Pos::cell_center(cx, cy);
            if !self.enemy_passable(cx, cy) || dest.dist(from) < 3500 || !dest.within(e.home, 9000) {
                continue;
            }
            let old = e.body.pos;
            e.body.pos = dest;
            e.body.facing = facing_to(dest, from, e.body.facing);
            self.out.push(Out::Near(old, GameEvent::Notice { text: "The herald folds into starlight.".into() }));
            return true;
        }
        false
    }

    fn begin_enemy_attack(&mut self, e: &mut Enemy, attack: EnemyAttack, target: Pos) {
        let origin = e.body.pos;
        let facing = facing_to(origin, target, e.body.facing);
        e.body.facing = facing;
        if attack.cooldown > 0 {
            e.cooldowns.insert(attack.id.clone(), attack.cooldown);
        }
        let reach = attack.reach;
        let cells = match attack.shape {
            AttackShape::Arc => cells_where(origin, reach, |c| in_arc(origin, facing, reach, attack.arc, c, 0)),
            AttackShape::Line => cells_where(origin, reach, |c| in_line(origin, facing, reach, LINE_HALF_WIDTH, c)),
            AttackShape::Circle => {
                let center = origin.offset(facing, attack.offset);
                cells_where(center, attack.radius, |c| c.within(center, attack.radius))
            }
            AttackShape::TargetCircle => cells_where(target, attack.radius, |c| c.within(target, attack.radius)),
            AttackShape::Projectile => {
                let (vx, vy) = velocity(origin, target, 500);
                let mut cells = Vec::new();
                let mut p = origin;
                for _ in 0..(reach / 500) {
                    p = Pos::new(p.x + vx, p.y + vy);
                    let (cx, cy) = p.cell();
                    if self.terrain_opaque(cx, cy) {
                        break;
                    }
                    let c = [cx as i16, cy as i16];
                    if cells.last() != Some(&c) {
                        cells.push(c);
                    }
                }
                cells
            }
            AttackShape::Ring => cells_where(origin, 2000, |c| c.within(origin, 2000)),
        };
        let cells: Vec<[i16; 2]> = cells
            .into_iter()
            .filter(|c| {
                let t = self.content.map.tile(c[0] as i32, c[1] as i32);
                !t.opaque() && t != hollowrite_protocol::Tile::Void
            })
            .collect();
        let tele = self.alloc_tele();
        e.body.act = Act::new(ActorState::Windup, ActKind::Enemy, attack.windup);
        if attack.shape == AttackShape::Ring {
            let pos = e.body.pos;
            self.out.push(Out::Near(pos, GameEvent::Notice { text: "The bell draws a long breath...".into() }));
        }
        e.windup = Some(Windup { attack, origin, facing, target, cells, tele });
    }

    fn enemy_strike(&mut self, e: &mut Enemy, first: bool) {
        let Some(w) = e.windup.clone() else { return };
        let a = &w.attack;
        match a.shape {
            AttackShape::Projectile => {
                if first {
                    let (vx, vy) = velocity(w.origin, w.target, a.speed);
                    let id = self.alloc_id();
                    self.projectiles.insert(
                        id,
                        Projectile {
                            id,
                            def: a.id.clone(),
                            owner: e.id,
                            hits: Side::Players,
                            pos: w.origin,
                            vx,
                            vy,
                            range_left: a.reach,
                            damage: a.damage,
                            poise: a.poise,
                        },
                    );
                }
                return;
            }
            AttackShape::Ring => {
                if first {
                    self.launch_peal(e);
                }
                return;
            }
            _ => {}
        }
        let victims: Vec<EntityId> = self
            .players
            .values()
            .filter(|p| p.body.act.state != ActorState::Dead && !e.body.act.hit.contains(&p.id))
            .filter(|p| {
                let pp = p.body.pos;
                let r = p.body.radius;
                let inside = match a.shape {
                    AttackShape::Arc => in_arc(w.origin, w.facing, a.reach, a.arc, pp, r),
                    AttackShape::Line => in_line(w.origin, w.facing, a.reach, LINE_HALF_WIDTH + r, pp),
                    AttackShape::Circle => pp.within(w.origin.offset(w.facing, a.offset), a.radius + r),
                    AttackShape::TargetCircle => pp.within(w.target, a.radius + r),
                    _ => false,
                };
                inside && self.content.map.line_clear(w.origin.cell(), pp.cell(), |x, y| self.terrain_opaque(x, y))
            })
            .map(|p| p.id)
            .collect();
        for v in victims {
            e.body.act.hit.push(v);
            let landed = self.damage_player(
                v,
                Incoming {
                    src: e.id,
                    src_pos: w.origin,
                    raw: a.damage,
                    poise: a.poise,
                    parryable: a.parryable,
                    heavy: a.heavy,
                    unblockable: false,
                },
            );
            if landed == Landed::Parried {
                let b = &self.content.balance;
                e.windup = None;
                e.riposte = b.riposte_ticks;
                e.body.act = Act::new(ActorState::Staggered, ActKind::Enemy, b.riposte_ticks);
                let pos = e.body.pos;
                self.out.push(Out::Near(pos, GameEvent::Staggered { id: e.id }));
                return;
            }
        }
    }

    fn walk_towards(&mut self, e: &mut Enemy, goal: Pos) {
        let pos = e.body.pos;
        let here = pos.cell();
        let goal_cell = goal.cell();
        e.path_timer = e.path_timer.saturating_sub(1);
        let need_path = e.waypoint.is_none() || e.path_timer == 0 || e.waypoint == Some(here);
        if need_path {
            e.path_timer = REPATH_TICKS;
            e.waypoint = if here == goal_cell {
                None
            } else {
                self.content.map.next_step(here, goal_cell, PATH_BUDGET, |x, y| self.enemy_passable(x, y))
            };
        }
        let aim = match e.waypoint {
            Some((x, y)) => Pos::cell_center(x, y),
            None if here == goal_cell => goal,
            None => {
                e.body.act.state = ActorState::Idle;
                return;
            }
        };
        let Some(d) = pos.dir_to(aim) else {
            e.body.act.state = ActorState::Idle;
            return;
        };
        let (dx, dy) = crate::geom::step(d, e.speed);
        let r = e.body.radius.min(400);
        let moved = World::slide(pos, dx, dy, r, &|x, y| self.enemy_passable(x, y));
        e.body.pos = moved;
        e.body.facing = d;
        e.body.act.state = ActorState::Moving;
    }

    fn step_away(&mut self, e: &mut Enemy, threat: Pos) {
        let pos = e.body.pos;
        let r = e.body.radius.min(400);
        let best = Dir8::ALL
            .into_iter()
            .map(|d| {
                let (dx, dy) = crate::geom::step(d, e.speed);
                (d, World::slide(pos, dx, dy, r, &|x, y| self.enemy_passable(x, y)))
            })
            .filter(|(_, p)| *p != pos && p.within(e.home, 9000))
            .max_by_key(|(_, p)| p.dist2(threat));
        if let Some((_, p)) = best {
            e.body.pos = p;
            e.body.facing = facing_to(p, threat, e.body.facing);
            e.body.act.state = ActorState::Moving;
        } else {
            e.body.act.state = ActorState::Idle;
        }
    }

    // ---- boss --------------------------------------------------------

    fn boss_think(&mut self, e: &mut Enemy) {
        let content = self.content.clone();
        let tick = self.tick;
        let def = &content.enemies[&e.def];
        let Some(p2) = &def.phase2 else { return };
        let engaged = matches!(self.encounter, Encounter::Active { .. });
        let b = e.boss.as_mut().unwrap();

        if b.phase == 1 && e.body.hp * 100 <= e.body.hp_max * p2.threshold_pct && e.body.hp > 0 {
            b.phase = 2;
            b.transition_until = tick + p2.transition_ticks as u64;
            b.next_dirge = tick + p2.transition_ticks as u64 + p2.dirge_every as u64 / 2;
            e.speed = p2.speed;
            e.windup = None;
            e.cooldowns.clear();
            e.body.act = Act::new(ActorState::Casting, ActKind::Enemy, p2.transition_ticks);
            let pos = e.body.pos;
            self.out.push(Out::Near(pos, GameEvent::BossPhase { id: e.id, phase: 2 }));
            self.out.push(Out::Near(pos, GameEvent::Toll { x: pos.x, y: pos.y, radius: 90 }));
            self.out.push(Out::Near(
                pos,
                GameEvent::Notice { text: "The bell cracks. A hundred names scream at once.".into() },
            ));
            return;
        }
        if b.phase < 2 || !engaged {
            return;
        }
        if b.peal_pending_until != 0 && tick >= b.peal_pending_until {
            b.peal_pending_until = 0;
            b.vulnerable_until = tick + p2.vulnerable_ticks as u64;
            e.windup = None;
            e.body.act = Act::new(ActorState::Staggered, ActKind::Enemy, p2.vulnerable_ticks);
            let pos = e.body.pos;
            self.out
                .push(Out::Near(pos, GameEvent::Notice { text: "The bell hangs open and silent. Strike now.".into() }));
        }
        if tick >= b.next_dirge && b.transition_until <= tick {
            b.next_dirge = tick + p2.dirge_every as u64;
            let targets: Vec<Pos> = match &self.encounter {
                Encounter::Active { participants, .. } => {
                    participants.iter().filter_map(|id| self.players.get(id)).map(|p| p.body.pos).collect()
                }
                _ => vec![],
            };
            for i in 0..p2.dirge_pools as usize {
                if targets.is_empty() {
                    break;
                }
                let t = targets[i % targets.len()];
                let center = Pos::new(t.x + self.rng.range_i32(-1500, 1500), t.y + self.rng.range_i32(-1500, 1500));
                let id = self.alloc_tele();
                self.hazards.push(Hazard {
                    id,
                    owner: e.id,
                    center,
                    radius: p2.dirge_radius,
                    arm_at: tick + 36,
                    until: tick + 36 + p2.dirge_ticks as u64,
                    damage: p2.dirge_damage,
                });
            }
        }
    }

    fn launch_peal(&mut self, e: &mut Enemy) {
        let content = self.content.clone();
        let Some(p2) = &content.enemies[&e.def].phase2 else { return };
        let tick = self.tick;
        let mut last_end = tick;
        for i in 0..p2.shockwave_rings as u64 {
            let start = tick + i * p2.shockwave_gap as u64;
            let id = self.alloc_tele();
            self.rings.push(Ring {
                id,
                owner: e.id,
                center: e.body.pos,
                start,
                step: p2.shockwave_step,
                max_radius: p2.shockwave_max_radius,
                damage: p2.shockwave_damage,
                hit: Vec::new(),
            });
            last_end = start + (p2.shockwave_max_radius / 1000) as u64 * p2.shockwave_step as u64;
        }
        let pos = e.body.pos;
        self.out.push(Out::Near(
            pos,
            GameEvent::Toll { x: pos.x, y: pos.y, radius: (p2.shockwave_max_radius / 100) as u16 },
        ));
        let b = e.boss.as_mut().unwrap();
        b.peal_pending_until = last_end.min(tick + 3 * p2.shockwave_gap as u64 + 20);
    }

    fn boss_defeated(&mut self, eid: EntityId, name: &str, echoes: u64) {
        let content = self.content.clone();
        let tick = self.tick;
        let Encounter::Active { participants, .. } = std::mem::replace(
            &mut self.encounter,
            Encounter::Cleared { respawn_at: tick + content.balance.enemy_respawn_ticks as u64 },
        ) else {
            return;
        };
        self.hazards.clear();
        self.rings.clear();
        let e = self.enemies.get(&eid).unwrap();
        let pos = e.body.pos;
        let def_id = e.def.clone();
        let life = e.life;
        self.out.push(Out::Near(pos, GameEvent::BossDefeated { id: eid, name: name.to_string() }));
        let slain = format!("slain:{def_id}");
        for pid in participants {
            let Some(p) = self.players.get_mut(&pid) else { continue };
            let first = p.data.flags.insert(slain.clone());
            let amount = if first { echoes } else { echoes * content.balance.boss.repeat_reward_pct / 100 };
            p.data.echoes += amount;
            let total = p.data.echoes;
            p.ledger.push(LedgerEntry {
                kind: "boss",
                delta: amount as i64,
                balance: total,
                reference: format!("{def_id}:{life}"),
            });
            let mut gained = Vec::new();
            if first {
                let n = p.data.add_item(&content, "clapper_of_names", 1);
                if n > 0 {
                    gained.push(GameEvent::ItemGained {
                        item: "clapper_of_names".into(),
                        name: content.item("clapper_of_names").map(|i| i.name.clone()).unwrap_or_default(),
                        qty: n,
                    });
                }
            }
            self.out.push(Out::To(pid, GameEvent::EchoesGained { amount, total }));
            for g in gained {
                self.out.push(Out::To(pid, g));
            }
            self.out.push(Out::SheetChanged(pid));
            self.request_save(pid, "boss");
        }
    }

    /// Players inside the arena rectangle.
    fn in_arena(&self) -> BTreeSet<EntityId> {
        let Some(arena) = self.content.map.arena() else { return BTreeSet::new() };
        self.players
            .values()
            .filter(|p| p.body.act.state != ActorState::Dead)
            .filter(|p| {
                let (cx, cy) = p.body.pos.cell();
                arena.contains(cx, cy)
            })
            .map(|p| p.id)
            .collect()
    }

    pub fn scaled_boss_hp(content: &Content, base: u32, participants: usize) -> u32 {
        let extra = participants.saturating_sub(1) as u32;
        base * (100 + extra * content.balance.boss.coop_hp_pct_per_extra) / 100
    }

    pub(crate) fn tick_encounter(&mut self) {
        let Some(bid) = self.boss else { return };
        let content = self.content.clone();
        let tick = self.tick;
        let bb = &content.balance.boss;
        match self.encounter.clone() {
            Encounter::Dormant => {
                let inside = self.in_arena();
                if inside.is_empty() {
                    return;
                }
                let n = inside.len();
                let boss = self.enemies.get_mut(&bid).unwrap();
                let base = content.enemies[&boss.def].hp;
                boss.body.hp_max = Self::scaled_boss_hp(&content, base, n);
                boss.body.hp = boss.body.hp_max;
                let pos = boss.body.pos;
                self.encounter = Encounter::Active { since: tick, participants: inside, sealed: false };
                self.out.push(Out::Near(
                    pos,
                    GameEvent::Notice { text: "The Bell-Mother turns. The belfry door begins to close.".into() },
                ));
            }
            Encounter::Active { since, mut participants, sealed } => {
                let alive_participants: BTreeSet<EntityId> = participants
                    .iter()
                    .copied()
                    .filter(|id| self.players.get(id).is_some_and(|p| p.body.act.state != ActorState::Dead))
                    .collect();
                if !sealed {
                    let joined: Vec<EntityId> =
                        self.in_arena().into_iter().filter(|id| !participants.contains(id)).collect();
                    if !joined.is_empty() {
                        participants.extend(joined);
                        let boss = self.enemies.get_mut(&bid).unwrap();
                        let base = content.enemies[&boss.def].hp;
                        let new_max = Self::scaled_boss_hp(&content, base, participants.len());
                        let missing = boss.body.hp_max - boss.body.hp;
                        boss.body.hp_max = new_max;
                        boss.body.hp = new_max.saturating_sub(missing);
                        let poise = content.enemies[&boss.def].poise;
                        boss.body.poise_max =
                            poise * (100 + (participants.len() as u32 - 1) * bb.coop_poise_pct_per_extra) / 100;
                    }
                }
                participants.retain(|id| self.players.contains_key(id));
                let seal_now = !sealed && tick >= since + bb.join_grace_ticks as u64;
                if seal_now {
                    let pos = self.enemies[&bid].body.pos;
                    self.out
                        .push(Out::Near(pos, GameEvent::Notice { text: "The belfry door seals behind you.".into() }));
                }
                let no_one_left = alive_participants.is_empty() || participants.is_empty();
                if no_one_left {
                    self.encounter = Encounter::Resetting { at: tick + bb.reset_delay_ticks as u64 };
                } else {
                    self.encounter = Encounter::Active { since, participants, sealed: sealed || seal_now };
                }
            }
            Encounter::Resetting { at } => {
                if tick >= at {
                    self.reset_boss(bid);
                    self.encounter = Encounter::Dormant;
                }
            }
            Encounter::Cleared { respawn_at } => {
                if tick >= respawn_at && self.in_arena().is_empty() {
                    self.reset_boss(bid);
                    self.enemies.get_mut(&bid).unwrap().life += 1;
                    self.encounter = Encounter::Dormant;
                }
            }
        }
    }

    fn reset_boss(&mut self, bid: EntityId) {
        let content = self.content.clone();
        self.hazards.clear();
        self.rings.clear();
        let boss = self.enemies.get_mut(&bid).unwrap();
        let def = &content.enemies[&boss.def];
        boss.body.hp_max = def.hp;
        boss.body.poise_max = def.poise;
        boss.speed = def.speed;
        boss.reset_at_home();
        boss.boss = Some(BossState { phase: 1, ..Default::default() });
        let pos = boss.body.pos;
        self.out.push(Out::Near(pos, GameEvent::BossReset { id: bid }));
    }
}
