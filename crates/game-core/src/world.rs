use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::Arc;

use hollowrite_protocol::{
    Action, ActorState, Attribute, BossBar, Dir8, EntityId, EntityKind, EntityView, EquipSlot, GameEvent, Relation,
    SelfView, Snapshot, Telegraph, TelegraphKind, Tile,
};

use crate::character::{attr_index, CharacterData, Remnant};
use crate::content::{Condition, ConsumableEffect, Content, Effects, ItemKind};
use crate::geom::Pos;
use crate::map::{DoorRule, SpawnKind};
use crate::rng::Rng;

pub(crate) const NPC_TALK_LOCK: u16 = 8;

/// Where an outbound event should be delivered.
#[derive(Debug, Clone, PartialEq)]
pub enum Out {
    To(EntityId, GameEvent),
    Near(Pos, GameEvent),
    Dialogue(EntityId, DialogueView),
    SheetChanged(EntityId),
    Save(Box<SaveRequest>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogueView {
    pub npc: EntityId,
    pub speaker: String,
    pub lines: Vec<String>,
    pub choices: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerEntry {
    pub kind: &'static str,
    pub delta: i64,
    pub balance: u64,
    pub reference: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SaveRequest {
    pub data: CharacterData,
    pub reason: &'static str,
    pub ledger: Vec<LedgerEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandError {
    NotFound,
    NotAllowed(&'static str),
}

pub type CmdResult = Result<(), CommandError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActKind {
    None,
    Light,
    Heavy,
    Roll(Dir8),
    Parry,
    Heal,
    Cast(u8),
    Interact,
    Enemy,
}

#[derive(Debug, Clone)]
pub struct Act {
    pub state: ActorState,
    pub kind: ActKind,
    pub t: u16,
    pub dur: u16,
    pub hit: Vec<EntityId>,
}

impl Act {
    pub fn idle() -> Self {
        Act { state: ActorState::Idle, kind: ActKind::None, t: 0, dur: 0, hit: Vec::new() }
    }

    pub fn new(state: ActorState, kind: ActKind, dur: u16) -> Self {
        Act { state, kind, t: 0, dur, hit: Vec::new() }
    }

    pub fn is_free(&self) -> bool {
        matches!(self.state, ActorState::Idle | ActorState::Moving | ActorState::Guarding)
    }

    pub fn progress(&self) -> u8 {
        if self.dur == 0 {
            0
        } else {
            ((self.t as u32 * 100) / self.dur as u32).min(100) as u8
        }
    }
}

#[derive(Debug, Clone)]
pub struct Body {
    pub pos: Pos,
    pub facing: Dir8,
    pub hp: u32,
    pub hp_max: u32,
    pub poise: u32,
    pub poise_max: u32,
    pub poise_timer: u16,
    pub radius: i32,
    pub act: Act,
}

#[derive(Debug, Clone)]
pub struct Player {
    pub id: EntityId,
    pub data: CharacterData,
    pub body: Body,
    pub stamina: u32,
    pub stamina_delay: u16,
    pub wick: u32,
    pub wick_timer: u16,
    pub move_dir: Option<Dir8>,
    pub move_ttl: u16,
    pub guard_held: bool,
    pub pending: Option<Action>,
    pub buffered: Option<(Action, u64)>,
    pub cooldowns: [u16; 2],
    pub last_seq: u32,
    pub zone: String,
    pub party: Option<u64>,
    pub ledger: Vec<LedgerEntry>,
    pub last_save: u64,
    pub dialogue: Option<(EntityId, usize)>,
    pub dead_timer: u16,
    pub last_safe: Pos,
}

#[derive(Debug, Clone)]
pub struct Npc {
    pub id: EntityId,
    pub def: String,
    pub pos: Pos,
}

#[derive(Debug, Clone)]
pub struct Pickup {
    pub id: EntityId,
    pub pickup_id: String,
    pub item: String,
    pub qty: u32,
    pub pos: Pos,
}

#[derive(Debug, Clone)]
pub struct Loot {
    pub id: EntityId,
    pub owner: EntityId,
    pub item: String,
    pub qty: u32,
    pub pos: Pos,
    pub expires: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Players,
    Enemies,
}

#[derive(Debug, Clone)]
pub struct Projectile {
    pub id: EntityId,
    pub def: String,
    pub owner: EntityId,
    pub hits: Side,
    pub pos: Pos,
    pub vx: i32,
    pub vy: i32,
    pub range_left: i32,
    pub damage: u32,
    pub poise: u32,
}

#[derive(Debug, Clone)]
pub struct Hazard {
    pub id: u32,
    pub owner: EntityId,
    pub center: Pos,
    pub radius: i32,
    pub arm_at: u64,
    pub until: u64,
    pub damage: u32,
}

#[derive(Debug, Clone)]
pub struct Ring {
    pub id: u32,
    pub owner: EntityId,
    pub center: Pos,
    pub start: u64,
    pub step: u16,
    pub max_radius: i32,
    pub damage: u32,
    pub hit: Vec<EntityId>,
}

impl Ring {
    pub fn radius(&self, tick: u64) -> Option<i32> {
        if tick < self.start {
            return None;
        }
        let r = 1000 + ((tick - self.start) / self.step as u64) as i32 * 1000;
        (r <= self.max_radius).then_some(r)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Encounter {
    Dormant,
    Active { since: u64, participants: BTreeSet<EntityId>, sealed: bool },
    Resetting { at: u64 },
    Cleared { respawn_at: u64 },
}

pub struct World {
    pub content: Arc<Content>,
    pub tick: u64,
    pub(crate) rng: Rng,
    next_id: EntityId,
    pub(crate) next_tele: u32,
    pub players: BTreeMap<EntityId, Player>,
    pub enemies: BTreeMap<EntityId, crate::ai::Enemy>,
    pub npcs: BTreeMap<EntityId, Npc>,
    pub pickups: BTreeMap<EntityId, Pickup>,
    pub loot: BTreeMap<EntityId, Loot>,
    pub projectiles: BTreeMap<EntityId, Projectile>,
    pub hazards: Vec<Hazard>,
    pub rings: Vec<Ring>,
    pub encounter: Encounter,
    pub boss: Option<EntityId>,
    pub(crate) out: Vec<Out>,
}

impl World {
    pub fn new(content: Arc<Content>, seed: u64) -> World {
        let mut w = World {
            content: content.clone(),
            tick: 0,
            rng: Rng::new(seed),
            next_id: 1,
            next_tele: 1,
            players: BTreeMap::new(),
            enemies: BTreeMap::new(),
            npcs: BTreeMap::new(),
            pickups: BTreeMap::new(),
            loot: BTreeMap::new(),
            projectiles: BTreeMap::new(),
            hazards: Vec::new(),
            rings: Vec::new(),
            encounter: Encounter::Dormant,
            boss: None,
            out: Vec::new(),
        };
        for spawn in &content.map.spawns {
            let pos = Pos::cell_center(spawn.x, spawn.y);
            match &spawn.what {
                SpawnKind::Enemy(def) => {
                    let id = w.alloc_id();
                    let enemy = crate::ai::Enemy::new(id, &content, def, pos);
                    if enemy.is_boss() {
                        w.boss = Some(id);
                    }
                    w.enemies.insert(id, enemy);
                }
                SpawnKind::Npc(def) => {
                    let id = w.alloc_id();
                    w.npcs.insert(id, Npc { id, def: def.clone(), pos });
                }
                SpawnKind::Item { item, qty, pickup_id } => {
                    let id = w.alloc_id();
                    w.pickups
                        .insert(id, Pickup { id, pickup_id: pickup_id.clone(), item: item.clone(), qty: *qty, pos });
                }
            }
        }
        w
    }

    pub(crate) fn alloc_id(&mut self) -> EntityId {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub(crate) fn alloc_tele(&mut self) -> u32 {
        let id = self.next_tele;
        self.next_tele = self.next_tele.wrapping_add(1).max(1);
        id
    }

    pub fn drain(&mut self) -> Vec<Out> {
        std::mem::take(&mut self.out)
    }

    pub(crate) fn emit_near(&mut self, pos: Pos, e: GameEvent) {
        self.out.push(Out::Near(pos, e));
    }

    pub(crate) fn emit_to(&mut self, id: EntityId, e: GameEvent) {
        self.out.push(Out::To(id, e));
    }

    pub fn player_by_character(&self, character_id: &str) -> Option<EntityId> {
        self.players.values().find(|p| p.data.id == character_id).map(|p| p.id)
    }

    // ---- passability -------------------------------------------------

    pub fn door_open_for(&self, flags: &BTreeSet<String>, x: i32, y: i32) -> bool {
        match self.content.map.door_at(x, y) {
            None => true,
            Some(d) => match d.rule {
                DoorRule::Fog => !self.arena_sealed(),
                _ => flags.contains(&format!("door:{}", d.id)),
            },
        }
    }

    pub fn arena_sealed(&self) -> bool {
        matches!(self.encounter, Encounter::Active { sealed: true, .. })
    }

    pub fn player_passable(&self, flags: &BTreeSet<String>, x: i32, y: i32) -> bool {
        let t = self.content.map.tile(x, y);
        if t.walkable() {
            return true;
        }
        match t {
            Tile::Door | Tile::Shortcut | Tile::FogGate | Tile::FalseWall => self.door_open_for(flags, x, y),
            _ => false,
        }
    }

    pub fn player_opaque(&self, flags: &BTreeSet<String>, x: i32, y: i32) -> bool {
        let t = self.content.map.tile(x, y);
        match t {
            Tile::Door | Tile::Shortcut | Tile::FalseWall => !self.door_open_for(flags, x, y),
            Tile::FogGate => true,
            _ => t.opaque(),
        }
    }

    /// Enemies use plain terrain only: they never path through gated doors
    /// and never enter safe zones.
    pub fn enemy_passable(&self, x: i32, y: i32) -> bool {
        let map = &self.content.map;
        let t = map.tile(x, y);
        let ok = t.walkable() || (t == Tile::Door && map.door_at(x, y).is_none());
        ok && !map.zone_at(x, y).is_some_and(|z| z.safe)
    }

    pub fn terrain_opaque(&self, x: i32, y: i32) -> bool {
        let t = self.content.map.tile(x, y);
        match t {
            Tile::Door => self.content.map.door_at(x, y).is_some(),
            _ => t.opaque(),
        }
    }

    pub(crate) fn box_clear(pos: Pos, r: i32, pass: &impl Fn(i32, i32) -> bool) -> bool {
        let corners = [(pos.x - r, pos.y - r), (pos.x + r, pos.y - r), (pos.x - r, pos.y + r), (pos.x + r, pos.y + r)];
        corners
            .iter()
            .all(|&(x, y)| pass(x.div_euclid(hollowrite_protocol::CELL), y.div_euclid(hollowrite_protocol::CELL)))
    }

    /// Axis-separated sliding movement. Per-tick displacement is bounded by
    /// content validation to under half a cell, so a box test at the
    /// destination is sufficient to prevent tunnelling.
    pub(crate) fn slide(pos: Pos, dx: i32, dy: i32, r: i32, pass: &impl Fn(i32, i32) -> bool) -> Pos {
        let cell = hollowrite_protocol::CELL;
        let mut p = pos;
        let nx = Pos::new(p.x + dx, p.y);
        if dx != 0 {
            if Self::box_clear(nx, r, pass) {
                p = nx;
            } else if dy == 0 {
                // Corner assist: pressing straight into a doorway edge eases
                // the body toward the centre of its row instead of stalling.
                let centre = p.y.div_euclid(cell) * cell + cell / 2;
                if centre != p.y && Self::box_clear(Pos::new(p.x + dx, centre), r, pass) {
                    let step = (centre - p.y).signum() * dx.abs().min((centre - p.y).abs());
                    p.y += step;
                }
            }
        }
        let ny = Pos::new(p.x, p.y + dy);
        if dy != 0 {
            if Self::box_clear(ny, r, pass) {
                p = ny;
            } else if dx == 0 {
                let centre = p.x.div_euclid(cell) * cell + cell / 2;
                if centre != p.x && Self::box_clear(Pos::new(centre, p.y + dy), r, pass) {
                    let step = (centre - p.x).signum() * dy.abs().min((centre - p.x).abs());
                    p.x += step;
                }
            }
        }
        p
    }

    // ---- join / leave -----------------------------------------------

    pub fn join(&mut self, mut data: CharacterData) -> EntityId {
        if let Some(existing) = self.player_by_character(&data.id) {
            return existing;
        }
        let content = self.content.clone();
        let derived = data.derived(&content);
        let checkpoint = content
            .map
            .lantern(&data.checkpoint)
            .or_else(|| content.map.lantern(&content.map.start_lantern))
            .expect("start lantern exists");
        let respawn = Pos::cell_center(checkpoint.x + 1, checkpoint.y);
        let (cx, cy) = data.pos.cell();
        let in_arena = content.map.arena().is_some_and(|a| a.contains(cx, cy));
        if in_arena
            || !Self::box_clear(data.pos, content.balance.body_radius, &|x, y| self.player_passable(&data.flags, x, y))
        {
            data.pos = respawn;
        }
        if data.hp == 0 || data.hp > derived.hp_max {
            data.hp = derived.hp_max;
        }
        data.draughts = data.draughts.min(data.draughts_max);
        let id = self.alloc_id();
        let poise = data.poise(&content);
        let zone = self.zone_id_at(data.pos);
        let player = Player {
            id,
            body: Body {
                pos: data.pos,
                facing: Dir8::S,
                hp: data.hp,
                hp_max: derived.hp_max,
                poise,
                poise_max: poise,
                poise_timer: 0,
                radius: content.balance.body_radius,
                act: Act::idle(),
            },
            stamina: derived.stamina_max,
            stamina_delay: 0,
            wick: derived.wick_max,
            wick_timer: 0,
            move_dir: None,
            move_ttl: 0,
            guard_held: false,
            pending: None,
            buffered: None,
            cooldowns: [0, 0],
            last_seq: 0,
            zone,
            party: None,
            ledger: Vec::new(),
            last_save: self.tick,
            dialogue: None,
            dead_timer: 0,
            last_safe: data.pos,
            data,
        };
        self.players.insert(id, player);
        id
    }

    /// Removes the player and returns their final aggregate for saving.
    pub fn leave(&mut self, id: EntityId) -> Option<SaveRequest> {
        let mut p = self.players.remove(&id)?;
        if let Encounter::Active { participants, .. } = &mut self.encounter {
            participants.remove(&id);
        }
        self.loot.retain(|_, l| l.owner != id);
        if p.body.act.state == ActorState::Dead {
            self.finish_respawn_data(&mut p);
        }
        Some(self.build_save(&mut p, "logout"))
    }

    fn finish_respawn_data(&self, p: &mut Player) {
        let lantern = self
            .content
            .map
            .lantern(&p.data.checkpoint)
            .or_else(|| self.content.map.lantern(&self.content.map.start_lantern))
            .expect("start lantern exists");
        p.body.pos = Pos::cell_center(lantern.x + 1, lantern.y);
        p.body.hp = p.body.hp_max;
        p.data.draughts = p.data.draughts_max;
    }

    pub(crate) fn build_save(&self, p: &mut Player, reason: &'static str) -> SaveRequest {
        p.data.pos = p.body.pos;
        p.data.hp = p.body.hp;
        p.data.version += 1;
        p.last_save = self.tick;
        SaveRequest { data: p.data.clone(), reason, ledger: std::mem::take(&mut p.ledger) }
    }

    pub(crate) fn request_save(&mut self, id: EntityId, reason: &'static str) {
        let Some(mut p) = self.players.remove(&id) else { return };
        let req = self.build_save(&mut p, reason);
        self.players.insert(id, p);
        self.out.push(Out::Save(Box::new(req)));
    }

    pub fn save_all(&mut self, reason: &'static str) -> Vec<SaveRequest> {
        let ids: Vec<_> = self.players.keys().copied().collect();
        let mut saves = Vec::new();
        for id in ids {
            let mut p = self.players.remove(&id).unwrap();
            saves.push(self.build_save(&mut p, reason));
            self.players.insert(id, p);
        }
        saves
    }

    pub fn set_party(&mut self, id: EntityId, party: Option<u64>) {
        if let Some(p) = self.players.get_mut(&id) {
            p.party = party;
        }
    }

    // ---- input -------------------------------------------------------

    /// Applies a client input frame. Returns false when the frame is stale
    /// or duplicated and was ignored.
    pub fn input(&mut self, id: EntityId, seq: u32, mv: Option<Dir8>, act: Option<Action>, face: Option<Dir8>) -> bool {
        let ttl = self.content.balance.move_intent_ttl;
        let Some(p) = self.players.get_mut(&id) else { return false };
        if seq <= p.last_seq {
            return false;
        }
        p.last_seq = seq;
        p.move_dir = mv;
        p.move_ttl = if mv.is_some() { ttl } else { 0 };
        if let Some(f) = face {
            if p.body.act.is_free() && mv.is_none() {
                p.body.facing = f;
            }
        }
        match act {
            Some(Action::GuardOn) => p.guard_held = true,
            Some(Action::GuardOff) => p.guard_held = false,
            Some(a) => p.pending = Some(a),
            None => {}
        }
        true
    }

    pub fn last_seq(&self, id: EntityId) -> u32 {
        self.players.get(&id).map_or(0, |p| p.last_seq)
    }

    // ---- commands ----------------------------------------------------

    pub fn equip(&mut self, id: EntityId, slot: EquipSlot, item: Option<String>) -> CmdResult {
        let content = self.content.clone();
        let p = self.players.get_mut(&id).ok_or(CommandError::NotFound)?;
        if !p.body.act.is_free() || p.body.act.state == ActorState::Guarding {
            return Err(CommandError::NotAllowed("finish what you are doing first"));
        }
        if let Some(item) = &item {
            if !p.data.has_item(item) {
                return Err(CommandError::NotAllowed("you do not carry that"));
            }
            let def = content.item(item).ok_or(CommandError::NotFound)?;
            let fits = matches!(
                (slot, def.kind),
                (EquipSlot::Main, ItemKind::Weapon)
                    | (EquipSlot::Off, ItemKind::Shield)
                    | (EquipSlot::Armor, ItemKind::Armor)
                    | (EquipSlot::Spell1, ItemKind::Spell)
                    | (EquipSlot::Spell2, ItemKind::Spell)
            );
            if !fits {
                return Err(CommandError::NotAllowed("that does not go there"));
            }
            if let Some(s) = &def.spell {
                if p.data.attr(Attribute::Occult) < s.occult_req {
                    return Err(CommandError::NotAllowed("your occult is too weak for this rite"));
                }
                let other = if slot == EquipSlot::Spell1 { &p.data.equipment.spell2 } else { &p.data.equipment.spell1 };
                if other.as_deref() == Some(item.as_str()) {
                    return Err(CommandError::NotAllowed("that rite is already prepared"));
                }
            }
        }
        let target = match slot {
            EquipSlot::Main => &mut p.data.equipment.main,
            EquipSlot::Off => &mut p.data.equipment.off,
            EquipSlot::Armor => &mut p.data.equipment.armor,
            EquipSlot::Spell1 => &mut p.data.equipment.spell1,
            EquipSlot::Spell2 => &mut p.data.equipment.spell2,
        };
        *target = item;
        self.refresh_derived(id);
        self.out.push(Out::SheetChanged(id));
        Ok(())
    }

    pub(crate) fn refresh_derived(&mut self, id: EntityId) {
        let content = self.content.clone();
        let Some(p) = self.players.get_mut(&id) else { return };
        let d = p.data.derived(&content);
        p.body.hp_max = d.hp_max;
        p.body.hp = p.body.hp.min(d.hp_max);
        p.body.poise_max = p.data.poise(&content);
        p.stamina = p.stamina.min(d.stamina_max);
        p.wick = p.wick.min(d.wick_max);
    }

    pub fn near_known_lantern(&self, p: &Player) -> Option<String> {
        let r = self.content.balance.interact_radius;
        self.content
            .map
            .lanterns
            .iter()
            .find(|l| p.data.lanterns.contains(&l.id) && p.body.pos.within(Pos::cell_center(l.x, l.y), r))
            .map(|l| l.id.clone())
    }

    pub fn level_up(&mut self, id: EntityId, attr: Attribute) -> CmdResult {
        let content = self.content.clone();
        let p = self.players.get(&id).ok_or(CommandError::NotFound)?;
        if p.body.act.state == ActorState::Dead {
            return Err(CommandError::NotAllowed("the dead cannot grow"));
        }
        if self.near_known_lantern(p).is_none() {
            return Err(CommandError::NotAllowed("you must be at a lit lantern"));
        }
        let p = self.players.get_mut(&id).unwrap();
        let cost = p.data.next_level_cost(&content);
        if p.data.echoes < cost {
            return Err(CommandError::NotAllowed("not enough echoes"));
        }
        let idx = attr_index(attr);
        if p.data.attrs[idx] >= content.balance.level.attr_cap {
            return Err(CommandError::NotAllowed("that attribute is at its limit"));
        }
        p.data.echoes -= cost;
        p.data.attrs[idx] += 1;
        let level = p.data.level(&content);
        let balance = p.data.echoes;
        p.ledger.push(LedgerEntry {
            kind: "level_up",
            delta: -(cost as i64),
            balance,
            reference: format!("{attr:?}->{level}"),
        });
        self.refresh_derived(id);
        self.emit_to(id, GameEvent::LevelUp { level });
        self.out.push(Out::SheetChanged(id));
        self.request_save(id, "level_up");
        Ok(())
    }

    pub fn use_item(&mut self, id: EntityId, item: &str) -> CmdResult {
        let content = self.content.clone();
        let p = self.players.get_mut(&id).ok_or(CommandError::NotFound)?;
        if !p.body.act.is_free() {
            return Err(CommandError::NotAllowed("finish what you are doing first"));
        }
        let def = content.item(item).ok_or(CommandError::NotFound)?;
        let effect = def.consumable.as_ref().ok_or(CommandError::NotAllowed("that cannot be used"))?;
        if !p.data.take_item(item, 1) {
            return Err(CommandError::NotAllowed("you do not carry that"));
        }
        match effect {
            ConsumableEffect::Echoes { amount } => {
                p.data.echoes += amount;
                let total = p.data.echoes;
                p.ledger.push(LedgerEntry {
                    kind: "consumable",
                    delta: *amount as i64,
                    balance: total,
                    reference: item.to_string(),
                });
                self.emit_to(id, GameEvent::EchoesGained { amount: *amount, total });
            }
            ConsumableEffect::Wick { amount } => {
                let max = p.data.derived(&content).wick_max;
                p.wick = (p.wick + amount).min(max);
            }
            ConsumableEffect::Cleanse { hp } => {
                p.body.hp = (p.body.hp + hp).min(p.body.hp_max);
            }
        }
        self.out.push(Out::SheetChanged(id));
        Ok(())
    }

    pub fn talk(&mut self, id: EntityId, npc: EntityId, choice: Option<u8>) -> CmdResult {
        let content = self.content.clone();
        let n = self.npcs.get(&npc).ok_or(CommandError::NotFound)?.clone();
        let p = self.players.get(&id).ok_or(CommandError::NotFound)?;
        if p.body.act.state == ActorState::Dead || !p.body.pos.within(n.pos, content.balance.interact_radius + 500) {
            return Err(CommandError::NotAllowed("too far away"));
        }
        let def = &content.npcs[&n.def];
        match choice {
            None => {
                let idx = def
                    .dialogue
                    .iter()
                    .position(|node| self.condition_holds(&p.data, &node.when))
                    .ok_or(CommandError::NotFound)?;
                let node = &def.dialogue[idx];
                if !self.apply_effects(id, &node.effects, &format!("talk:{}", n.def)) {
                    return Err(CommandError::NotAllowed("you lack what is needed"));
                }
                let p = self.players.get_mut(&id).unwrap();
                p.dialogue = Some((npc, idx));
                self.out.push(Out::Dialogue(
                    id,
                    DialogueView {
                        npc,
                        speaker: format!("{}, {}", def.name, def.title),
                        lines: node.lines.clone(),
                        choices: node.choices.iter().map(|c| c.label.clone()).collect(),
                    },
                ));
                Ok(())
            }
            Some(c) => {
                let (open_npc, idx) = p.dialogue.ok_or(CommandError::NotAllowed("no conversation is open"))?;
                if open_npc != npc {
                    return Err(CommandError::NotAllowed("no conversation is open"));
                }
                let node = &def.dialogue[idx];
                // Re-check the node: the world may have changed since it was shown.
                if !self.condition_holds(&p.data, &node.when) {
                    return Err(CommandError::NotAllowed("the moment has passed"));
                }
                let choice = node.choices.get(c as usize).ok_or(CommandError::NotFound)?;
                if !self.apply_effects(id, &choice.effects, &format!("talk:{}:{c}", n.def)) {
                    return Err(CommandError::NotAllowed("you lack what is needed"));
                }
                let p = self.players.get_mut(&id).unwrap();
                p.dialogue = None;
                self.out.push(Out::Dialogue(
                    id,
                    DialogueView {
                        npc,
                        speaker: format!("{}, {}", def.name, def.title),
                        lines: choice.reply.clone(),
                        choices: vec![],
                    },
                ));
                Ok(())
            }
        }
    }

    pub(crate) fn condition_holds(&self, data: &CharacterData, c: &Condition) -> bool {
        c.requires.iter().all(|f| data.flags.contains(f))
            && !c.forbids.iter().any(|f| data.flags.contains(f))
            && c.has_item.iter().all(|i| data.has_item(i))
    }

    /// Applies dialogue effects atomically: either every take succeeds and
    /// everything is granted, or nothing changes.
    pub(crate) fn apply_effects(&mut self, id: EntityId, fx: &Effects, reference: &str) -> bool {
        let content = self.content.clone();
        let Some(p) = self.players.get_mut(&id) else { return false };
        if fx.take.iter().any(|(item, qty)| p.data.inventory.get(item).copied().unwrap_or(0) < *qty) {
            return false;
        }
        let empty =
            fx.set.is_empty() && fx.give.is_empty() && fx.take.is_empty() && fx.echoes == 0 && fx.draughts_max == 0;
        if empty {
            return true;
        }
        for (item, qty) in &fx.take {
            p.data.take_item(item, *qty);
        }
        for f in &fx.set {
            p.data.flags.insert(f.clone());
        }
        let mut gained = Vec::new();
        for (item, qty) in &fx.give {
            let n = p.data.add_item(&content, item, *qty);
            if n > 0 {
                gained.push((item.clone(), n));
            }
        }
        if fx.echoes > 0 {
            p.data.echoes += fx.echoes;
            let total = p.data.echoes;
            p.ledger.push(LedgerEntry {
                kind: "dialogue",
                delta: fx.echoes as i64,
                balance: total,
                reference: reference.to_string(),
            });
            self.out.push(Out::To(id, GameEvent::EchoesGained { amount: fx.echoes, total }));
        }
        let p = self.players.get_mut(&id).unwrap();
        if fx.draughts_max > 0 {
            p.data.draughts_max = p.data.draughts_max.saturating_add(fx.draughts_max);
            p.data.draughts = p.data.draughts.saturating_add(fx.draughts_max);
        }
        for (item, qty) in gained {
            let name = content.item(&item).map(|d| d.name.clone()).unwrap_or_default();
            self.out.push(Out::To(id, GameEvent::ItemGained { item, name, qty }));
        }
        self.refresh_derived(id);
        self.out.push(Out::SheetChanged(id));
        self.request_save(id, "dialogue");
        true
    }

    // ---- interaction -------------------------------------------------

    pub(crate) fn interact(&mut self, id: EntityId) {
        let content = self.content.clone();
        let r = content.balance.interact_radius;
        let Some(p) = self.players.get(&id) else { return };
        let pos = p.body.pos;
        if let Some(npc) =
            self.npcs.values().filter(|n| n.pos.within(pos, r)).min_by_key(|n| n.pos.dist2(pos)).map(|n| n.id)
        {
            let _ = self.talk(id, npc, None);
            return;
        }
        if let Some(l) = content.map.lanterns.iter().find(|l| Pos::cell_center(l.x, l.y).within(pos, r)) {
            self.use_lantern(id, &l.id);
            return;
        }
        let (cx, cy) = pos.cell();
        for dy in -1..=1 {
            for dx in -1..=1 {
                if let Some(door) = content.map.door_at(cx + dx, cy + dy) {
                    if self.try_open_door(id, door.id.clone(), door.rule.clone(), cx + dx, cy + dy) {
                        return;
                    }
                }
            }
        }
    }

    fn use_lantern(&mut self, id: EntityId, lantern: &str) {
        let content = self.content.clone();
        let name = content.map.lantern(lantern).map(|l| l.name.clone()).unwrap_or_default();
        let p = self.players.get_mut(&id).unwrap();
        if p.data.lanterns.insert(lantern.to_string()) {
            p.data.flags.insert(format!("lit:{lantern}"));
            let pos = p.body.pos;
            self.emit_near(pos, GameEvent::LanternLit { lantern: lantern.to_string(), name });
        }
        let p = self.players.get_mut(&id).unwrap();
        let d = p.data.derived(&content);
        p.body.hp = d.hp_max;
        p.stamina = d.stamina_max;
        p.wick = d.wick_max;
        p.data.draughts = p.data.draughts_max;
        p.data.checkpoint = lantern.to_string();
        p.cooldowns = [0, 0];
        p.body.act = Act::new(ActorState::Interacting, ActKind::Interact, NPC_TALK_LOCK);
        self.emit_to(id, GameEvent::Rested { lantern: lantern.to_string() });
        self.out.push(Out::SheetChanged(id));
        self.request_save(id, "rest");
    }

    fn try_open_door(&mut self, id: EntityId, door: String, rule: DoorRule, dx: i32, dy: i32) -> bool {
        let p = self.players.get_mut(&id).unwrap();
        let flag = format!("door:{door}");
        if p.data.flags.contains(&flag) {
            return false;
        }
        let ok = match &rule {
            DoorRule::Key { key } => {
                if !p.data.has_item(key) {
                    self.emit_to(id, GameEvent::Notice { text: "Locked. Something heavy and iron would fit.".into() });
                    return true;
                }
                true
            }
            DoorRule::Oneway { from } => {
                let (cx, cy) = p.body.pos.cell();
                let (fx, fy) = from.offset();
                let side_ok = (fx == 0 || (cx - dx).signum() == fx) && (fy == 0 || (cy - dy).signum() == fy);
                if !side_ok {
                    self.emit_to(id, GameEvent::Notice { text: "It will not open from this side.".into() });
                    return true;
                }
                true
            }
            DoorRule::Secret => true,
            DoorRule::Fog => false,
        };
        if !ok {
            return false;
        }
        let p = self.players.get_mut(&id).unwrap();
        p.data.flags.insert(flag);
        let pos = p.body.pos;
        if rule == DoorRule::Secret {
            self.emit_to(id, GameEvent::Notice { text: "The wall was only a painted memory. It gives way.".into() });
        }
        self.emit_near(pos, GameEvent::DoorOpened { door });
        self.out.push(Out::SheetChanged(id));
        self.request_save(id, "door");
        true
    }

    pub(crate) fn interact_hint(&self, p: &Player) -> Option<String> {
        let content = &self.content;
        let r = content.balance.interact_radius;
        let pos = p.body.pos;
        if let Some(n) = self.npcs.values().filter(|n| n.pos.within(pos, r)).min_by_key(|n| n.pos.dist2(pos)) {
            return Some(format!("Talk to {}", content.npcs[&n.def].name));
        }
        if let Some(l) = content.map.lanterns.iter().find(|l| Pos::cell_center(l.x, l.y).within(pos, r)) {
            return Some(if p.data.lanterns.contains(&l.id) {
                format!("Rest at {}", l.name)
            } else {
                format!("Light {}", l.name)
            });
        }
        let (cx, cy) = pos.cell();
        for dy in -1..=1 {
            for dx in -1..=1 {
                if let Some(door) = content.map.door_at(cx + dx, cy + dy) {
                    if door.rule != DoorRule::Fog && !p.data.flags.contains(&format!("door:{}", door.id)) {
                        return Some(match door.rule {
                            DoorRule::Secret => "Touch the wall".to_string(),
                            _ => "Open the door".to_string(),
                        });
                    }
                }
            }
        }
        None
    }

    // ---- pickups -----------------------------------------------------

    pub(crate) fn collect_pickups(&mut self, id: EntityId) {
        let content = self.content.clone();
        let Some(p) = self.players.get(&id) else { return };
        if p.body.act.state == ActorState::Dead {
            return;
        }
        let pos = p.body.pos;
        let r = 700;
        let mut taken_world: Vec<(String, String, u32)> = self
            .pickups
            .values()
            .filter(|k| k.pos.within(pos, r) && !p.data.flags.contains(&format!("took:{}", k.pickup_id)))
            .map(|k| (k.pickup_id.clone(), k.item.clone(), k.qty))
            .collect();
        let loot_ids: Vec<EntityId> =
            self.loot.values().filter(|l| l.owner == id && l.pos.within(pos, r)).map(|l| l.id).collect();
        for lid in loot_ids {
            if let Some(l) = self.loot.remove(&lid) {
                taken_world.push((String::new(), l.item, l.qty));
            }
        }
        if taken_world.is_empty() {
            return;
        }
        let p = self.players.get_mut(&id).unwrap();
        let mut events = Vec::new();
        for (pickup_id, item, qty) in taken_world {
            if !pickup_id.is_empty() {
                p.data.flags.insert(format!("took:{pickup_id}"));
            }
            let gained = p.data.add_item(&content, &item, qty);
            let balance = p.data.echoes;
            p.ledger.push(LedgerEntry { kind: "item", delta: 0, balance, reference: format!("{item}x{gained}") });
            if gained > 0 {
                let name = content.item(&item).map(|d| d.name.clone()).unwrap_or_default();
                events.push(GameEvent::ItemGained { item, name, qty: gained });
            }
        }
        for e in events {
            self.emit_to(id, e);
        }
        self.out.push(Out::SheetChanged(id));
        self.request_save(id, "pickup");
    }

    pub(crate) fn reclaim_remnant(&mut self, id: EntityId) {
        let r = self.content.balance.remnant_pickup_radius;
        let Some(p) = self.players.get_mut(&id) else { return };
        if p.body.act.state == ActorState::Dead {
            return;
        }
        let Some(rem) = p.data.remnant else { return };
        if !rem.pos.within(p.body.pos, r) {
            return;
        }
        p.data.remnant = None;
        p.data.echoes += rem.amount;
        let balance = p.data.echoes;
        p.ledger.push(LedgerEntry {
            kind: "remnant_reclaim",
            delta: rem.amount as i64,
            balance,
            reference: String::new(),
        });
        self.emit_to(id, GameEvent::EchoesReclaimed { amount: rem.amount });
        self.out.push(Out::SheetChanged(id));
        self.request_save(id, "reclaim");
    }

    pub(crate) fn zone_id_at(&self, pos: Pos) -> String {
        let (cx, cy) = pos.cell();
        self.content.map.zone_at(cx, cy).map(|z| z.id.clone()).unwrap_or_default()
    }

    // ---- tick --------------------------------------------------------

    pub fn step(&mut self) {
        self.tick += 1;
        let ids: Vec<EntityId> = self.players.keys().copied().collect();
        for id in &ids {
            self.tick_player(*id);
        }
        let eids: Vec<EntityId> = self.enemies.keys().copied().collect();
        for id in eids {
            self.tick_enemy(id);
        }
        self.tick_projectiles();
        self.tick_hazards();
        self.tick_encounter();
        let now = self.tick;
        self.loot.retain(|_, l| l.expires > now);
        let interval = self.content.balance.save_interval_ticks as u64;
        for id in ids {
            if !self.players.contains_key(&id) {
                continue;
            }
            self.collect_pickups(id);
            self.reclaim_remnant(id);
            let p = &self.players[&id];
            let zone = self.zone_id_at(p.body.pos);
            if zone != p.zone {
                let name = self.content.map.zone(&zone).map(|z| z.name.clone()).unwrap_or_default();
                self.players.get_mut(&id).unwrap().zone = zone.clone();
                if !name.is_empty() {
                    self.emit_to(id, GameEvent::ZoneEntered { zone, name });
                }
            }
            if self.tick - self.players[&id].last_save >= interval {
                self.request_save(id, "interval");
            }
        }
    }

    // ---- views -------------------------------------------------------

    pub fn relation(&self, viewer: &Player, other: &Player) -> Relation {
        if viewer.id == other.id {
            Relation::Me
        } else if viewer.party.is_some() && viewer.party == other.party {
            Relation::Party
        } else {
            Relation::Stranger
        }
    }

    pub fn snapshot_for(&self, id: EntityId, known: &mut HashSet<EntityId>) -> Option<Snapshot> {
        let me = self.players.get(&id)?;
        let content = &self.content;
        let r = content.balance.interest_radius;
        let center = me.body.pos;
        let near = |p: Pos| p.chebyshev(center) <= r;
        let mut entities = Vec::new();
        for p in self.players.values() {
            if p.id != id && !near(p.body.pos) {
                continue;
            }
            entities.push(EntityView {
                id: p.id,
                kind: EntityKind::Player,
                def: p.data.archetype.id().to_string(),
                name: Some(p.data.name.clone()),
                x: p.body.pos.x,
                y: p.body.pos.y,
                facing: p.body.facing,
                state: p.body.act.state,
                action: act_label(&p.body.act),
                progress: p.body.act.progress(),
                rel: self.relation(me, p),
                hp: Some([p.body.hp, p.body.hp_max]),
                look: Some(p.data.look),
                phase: None,
            });
        }
        for e in self.enemies.values() {
            if !near(e.body.pos) || !e.visible() {
                continue;
            }
            entities.push(e.view(content));
        }
        for n in self.npcs.values() {
            if !near(n.pos) {
                continue;
            }
            let def = &content.npcs[&n.def];
            entities.push(EntityView {
                id: n.id,
                kind: EntityKind::Npc,
                def: n.def.clone(),
                name: Some(def.name.clone()),
                x: n.pos.x,
                y: n.pos.y,
                facing: Dir8::S,
                state: ActorState::Idle,
                action: None,
                progress: 0,
                rel: Relation::Neutral,
                hp: None,
                look: None,
                phase: None,
            });
        }
        for k in self.pickups.values() {
            if !near(k.pos) || me.data.flags.contains(&format!("took:{}", k.pickup_id)) {
                continue;
            }
            entities.push(item_view(k.id, &k.item, k.pos, content));
        }
        for l in self.loot.values() {
            if l.owner == id && near(l.pos) {
                entities.push(item_view(l.id, &l.item, l.pos, content));
            }
        }
        for pr in self.projectiles.values() {
            if !near(pr.pos) {
                continue;
            }
            let rel = match pr.hits {
                Side::Players => Relation::Hostile,
                Side::Enemies if pr.owner == id => Relation::Me,
                Side::Enemies => Relation::Party,
            };
            entities.push(EntityView {
                id: pr.id,
                kind: EntityKind::Projectile,
                def: pr.def.clone(),
                name: None,
                x: pr.pos.x,
                y: pr.pos.y,
                facing: crate::geom::dir_towards(pr.vx, pr.vy).unwrap_or(Dir8::S),
                state: ActorState::Active,
                action: None,
                progress: 0,
                rel,
                hp: None,
                look: None,
                phase: None,
            });
        }
        if let Some(rem) = me.data.remnant {
            if near(rem.pos) {
                entities.push(EntityView {
                    id: remnant_entity_id(id),
                    kind: EntityKind::Remnant,
                    def: "remnant".into(),
                    name: Some(format!("{} echoes", rem.amount)),
                    x: rem.pos.x,
                    y: rem.pos.y,
                    facing: Dir8::S,
                    state: ActorState::Idle,
                    action: None,
                    progress: 0,
                    rel: Relation::Me,
                    hp: None,
                    look: None,
                    phase: None,
                });
            }
        }
        let current: HashSet<EntityId> = entities.iter().map(|e| e.id).collect();
        let mut gone: Vec<EntityId> = known.difference(&current).copied().collect();
        gone.sort_unstable();
        *known = current;

        let mut tele = Vec::new();
        for e in self.enemies.values() {
            if near(e.body.pos) {
                tele.extend(e.telegraphs(self.tick));
            }
        }
        for h in &self.hazards {
            if !near(h.center) {
                continue;
            }
            let armed = self.tick >= h.arm_at;
            let cells = crate::geom::cells_where(h.center, h.radius, |c| c.within(h.center, h.radius));
            let (left, total) = if armed {
                ((h.until - self.tick) as u16, (h.until - h.arm_at) as u16)
            } else {
                ((h.arm_at - self.tick) as u16, (h.arm_at.saturating_sub(self.tick)) as u16)
            };
            tele.push(Telegraph {
                id: h.id,
                owner: h.owner,
                kind: if armed { TelegraphKind::Hazard } else { TelegraphKind::Area },
                cells,
                left,
                total,
            });
        }
        for ring in &self.rings {
            if !near(ring.center) {
                continue;
            }
            let (r_now, upcoming) = match ring.radius(self.tick) {
                Some(r) => (r, false),
                None => (1000, true),
            };
            let cells = crate::geom::cells_where(ring.center, r_now + 1000, |c| {
                let d = c.dist(ring.center);
                d >= r_now - 500 && d <= r_now + 500
            });
            tele.push(Telegraph {
                id: ring.id,
                owner: ring.owner,
                kind: TelegraphKind::Shockwave,
                cells,
                left: if upcoming { (ring.start - self.tick) as u16 } else { 0 },
                total: ring.step,
            });
        }

        let derived = me.data.derived(content);
        let boss = self.boss_bar_for(me);
        Some(Snapshot {
            tick: self.tick,
            ack: me.last_seq,
            me: SelfView {
                hp: me.body.hp,
                hp_max: me.body.hp_max,
                stamina: me.stamina,
                stamina_max: derived.stamina_max,
                wick: me.wick,
                wick_max: derived.wick_max,
                draughts: me.data.draughts,
                draughts_max: me.data.draughts_max,
                echoes: me.data.echoes,
                state: me.body.act.state,
                cooldowns: me.cooldowns,
                zone: me.zone.clone(),
                remnant: me.data.remnant.map(|r| [r.pos.x, r.pos.y]),
                boss,
                interact: self.interact_hint(me),
            },
            entities,
            gone,
            tele,
        })
    }

    fn boss_bar_for(&self, me: &Player) -> Option<BossBar> {
        let Encounter::Active { participants, .. } = &self.encounter else { return None };
        if !participants.contains(&me.id) {
            return None;
        }
        let boss = self.enemies.get(&self.boss?)?;
        Some(BossBar {
            name: self.content.enemies[&boss.def].name.clone(),
            hp: boss.body.hp,
            hp_max: boss.body.hp_max,
            phase: boss.boss.as_ref().map_or(1, |b| b.phase),
            vulnerable: boss.boss.as_ref().is_some_and(|b| b.vulnerable_until > self.tick),
        })
    }

    pub fn sheet(&self, id: EntityId) -> Option<hollowrite_protocol::CharacterSheet> {
        self.players.get(&id).map(|p| p.data.sheet(&self.content))
    }

    pub fn kill_player(&mut self, id: EntityId) {
        let content = self.content.clone();
        let Some(p) = self.players.get_mut(&id) else { return };
        if p.body.act.state == ActorState::Dead {
            return;
        }
        p.body.hp = 0;
        p.body.act = Act::new(ActorState::Dead, ActKind::None, content.balance.dead_ticks);
        p.dead_timer = content.balance.dead_ticks;
        p.guard_held = false;
        p.pending = None;
        p.buffered = None;
        p.dialogue = None;
        let pos = p.body.pos;
        let drop_at = if World::box_clear(pos, 1, &|x, y| {
            content.map.tile(x, y).walkable() && content.map.tile(x, y) != Tile::EmberPit
        }) {
            pos
        } else {
            p.last_safe
        };
        if let Some(old) = p.data.remnant.take() {
            let balance = p.data.echoes;
            p.ledger.push(LedgerEntry {
                kind: "remnant_lost",
                delta: -(old.amount as i64),
                balance,
                reference: String::new(),
            });
            self.out.push(Out::To(id, GameEvent::EchoesLost { amount: old.amount }));
        }
        let p = self.players.get_mut(&id).unwrap();
        if p.data.echoes > 0 {
            let amount = p.data.echoes;
            p.data.echoes = 0;
            p.data.remnant = Some(Remnant { pos: drop_at, amount });
            p.ledger.push(LedgerEntry {
                kind: "remnant_drop",
                delta: -(amount as i64),
                balance: 0,
                reference: format!("{},{}", drop_at.x, drop_at.y),
            });
        }
        if let Encounter::Active { participants, .. } = &mut self.encounter {
            participants.remove(&id);
        }
        self.emit_near(pos, GameEvent::Died { id, x: pos.x, y: pos.y });
        self.out.push(Out::SheetChanged(id));
        self.request_save(id, "death");
    }

    pub(crate) fn respawn_player(&mut self, id: EntityId) {
        let Some(mut p) = self.players.remove(&id) else { return };
        self.finish_respawn_data(&mut p);
        let content = self.content.clone();
        let d = p.data.derived(&content);
        p.stamina = d.stamina_max;
        p.wick = d.wick_max;
        p.body.act = Act::idle();
        p.body.poise = p.body.poise_max;
        p.cooldowns = [0, 0];
        p.last_safe = p.body.pos;
        let lantern = p.data.checkpoint.clone();
        self.players.insert(id, p);
        self.emit_to(id, GameEvent::Respawned { id, lantern });
        self.out.push(Out::SheetChanged(id));
        self.request_save(id, "respawn");
    }
}

pub fn remnant_entity_id(owner: EntityId) -> EntityId {
    0x8000_0000 | owner
}

fn item_view(id: EntityId, item: &str, pos: Pos, content: &Content) -> EntityView {
    EntityView {
        id,
        kind: EntityKind::Loot,
        def: item.to_string(),
        name: content.item(item).map(|d| d.name.clone()),
        x: pos.x,
        y: pos.y,
        facing: Dir8::S,
        state: ActorState::Idle,
        action: None,
        progress: 0,
        rel: Relation::Me,
        hp: None,
        look: None,
        phase: None,
    }
}

pub(crate) fn act_label(act: &Act) -> Option<String> {
    let s = match act.kind {
        ActKind::None | ActKind::Enemy => return None,
        ActKind::Light => "light",
        ActKind::Heavy => "heavy",
        ActKind::Roll(_) => "roll",
        ActKind::Parry => "parry",
        ActKind::Heal => "heal",
        ActKind::Cast(_) => "cast",
        ActKind::Interact => "interact",
    };
    Some(s.to_string())
}
