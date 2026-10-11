use std::collections::BTreeMap;
use std::path::Path;

use hollowrite_protocol::{Archetype, Tile, CELL};
use serde::Deserialize;

use crate::map::{MapDef, MapMeta};

#[derive(Debug, thiserror::Error)]
pub enum ContentError {
    #[error("reading {path}: {source}")]
    Io { path: String, source: std::io::Error },
    #[error("parsing {path}: {source}")]
    Toml { path: String, source: toml::de::Error },
    #[error("invalid content: {0}")]
    Invalid(String),
}

fn invalid(msg: impl Into<String>) -> ContentError {
    ContentError::Invalid(msg.into())
}

#[derive(Debug, Clone, Deserialize)]
pub struct Balance {
    pub tick_hz: u32,
    pub snapshot_every: u32,
    pub interest_radius: i32,
    pub walk_speed: i32,
    pub guard_speed_pct: i32,
    pub heal_speed_pct: i32,
    pub body_radius: i32,
    pub input_buffer_ticks: u16,
    pub move_intent_ttl: u16,
    pub stamina_regen: u32,
    pub stamina_regen_guarding: u32,
    pub stamina_regen_delay: u16,
    pub wick_regen_every: u16,
    pub poise_regen_delay: u16,
    pub stagger_ticks: u16,
    pub enemy_stagger_ticks: u16,
    pub enemy_steady_ticks: u16,
    pub riposte_ticks: u16,
    pub critical_mult_pct: u32,
    pub dead_ticks: u16,
    pub draughts_base: u8,
    pub draught: DraughtDef,
    pub base: BaseStats,
    pub level: LevelCurve,
    pub rolls: BTreeMap<String, RollDef>,
    pub parry: ParryDef,
    pub unarmed_guard_pct: u32,
    pub save_interval_ticks: u32,
    pub enemy_respawn_ticks: u32,
    pub enemy_respawn_clear_radius: i32,
    pub remnant_pickup_radius: i32,
    pub interact_radius: i32,
    pub boss: BossBalance,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DraughtDef {
    pub windup: u16,
    pub apply_at: u16,
    pub heal_pct: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BaseStats {
    pub hp: u32,
    pub hp_per_vit: u32,
    pub stamina: u32,
    pub stamina_per_end: u32,
    pub wick: u32,
    pub wick_per_occ: u32,
    pub unarmed_damage: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LevelCurve {
    pub base_cost: u64,
    pub growth_pct: u64,
    pub per_level: u64,
    pub attr_cap: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RollDef {
    pub ticks: u16,
    pub iframe_from: u16,
    pub iframe_to: u16,
    pub move_ticks: u16,
    pub distance: i32,
    pub stamina: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ParryDef {
    pub window: u16,
    pub total: u16,
    pub stamina: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BossBalance {
    pub coop_hp_pct_per_extra: u32,
    pub coop_poise_pct_per_extra: u32,
    pub join_grace_ticks: u32,
    pub reset_delay_ticks: u32,
    pub repeat_reward_pct: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AttackTiming {
    pub windup: u16,
    pub active: u16,
    pub recovery: u16,
    pub stamina: u32,
    pub poise: u32,
    pub mult_pct: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WeaponDef {
    pub damage: u32,
    pub might_pct: u32,
    pub finesse_pct: u32,
    pub reach: i32,
    pub arc: u32,
    pub light: AttackTiming,
    pub heavy: AttackTiming,
    #[serde(default)]
    pub guard_pct: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ShieldDef {
    pub guard_pct: u32,
    pub guard_stamina_pct: u32,
    pub parry: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpellForm {
    Bolt,
    Toll,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SpellDef {
    pub form: SpellForm,
    pub wick: u32,
    pub cooldown: u16,
    pub windup: u16,
    pub recovery: u16,
    pub damage: u32,
    pub occult_pct: u32,
    pub poise: u32,
    #[serde(default)]
    pub radius: i32,
    #[serde(default)]
    pub speed: i32,
    #[serde(default)]
    pub range: i32,
    #[serde(default)]
    pub occult_req: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ArmorDef {
    pub defense: u32,
    pub poise: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Weapon,
    Shield,
    Spell,
    Armor,
    Consumable,
    Key,
}

impl ItemKind {
    pub fn label(self) -> &'static str {
        match self {
            ItemKind::Weapon => "weapon",
            ItemKind::Shield => "shield",
            ItemKind::Spell => "rite",
            ItemKind::Armor => "armor",
            ItemKind::Consumable => "consumable",
            ItemKind::Key => "key",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case", tag = "effect")]
pub enum ConsumableEffect {
    Echoes { amount: u64 },
    Wick { amount: u32 },
    Cleanse { hp: u32 },
}

#[derive(Debug, Clone, Deserialize)]
pub struct ItemDef {
    #[serde(skip)]
    pub id: String,
    pub name: String,
    pub kind: ItemKind,
    pub desc: String,
    #[serde(default)]
    pub weapon: Option<WeaponDef>,
    #[serde(default)]
    pub shield: Option<ShieldDef>,
    #[serde(default)]
    pub spell: Option<SpellDef>,
    #[serde(default)]
    pub armor: Option<ArmorDef>,
    #[serde(default)]
    pub consumable: Option<ConsumableEffect>,
    #[serde(default)]
    pub max_stack: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ArchetypeDef {
    pub name: String,
    pub desc: String,
    pub attrs: [u32; 5],
    pub roll: String,
    pub main: String,
    #[serde(default)]
    pub off: Option<String>,
    pub armor: String,
    #[serde(default)]
    pub spell1: Option<String>,
    #[serde(default)]
    pub items: BTreeMap<String, u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttackShape {
    Arc,
    Line,
    Circle,
    TargetCircle,
    Projectile,
    Ring,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EnemyAttack {
    pub id: String,
    pub shape: AttackShape,
    pub windup: u16,
    pub active: u16,
    pub recovery: u16,
    pub damage: u32,
    pub poise: u32,
    #[serde(default)]
    pub reach: i32,
    #[serde(default)]
    pub arc: u32,
    #[serde(default)]
    pub radius: i32,
    #[serde(default)]
    pub offset: i32,
    #[serde(default)]
    pub speed: i32,
    #[serde(default = "default_weight")]
    pub weight: u32,
    #[serde(default)]
    pub min_range: i32,
    pub max_range: i32,
    #[serde(default)]
    pub cooldown: u16,
    #[serde(default)]
    pub parryable: bool,
    #[serde(default)]
    pub heavy: bool,
}

fn default_weight() -> u32 {
    1
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Behavior {
    Melee,
    Ranged,
    Occult,
    Boss,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DropDef {
    pub item: String,
    pub chance_pct: u32,
    #[serde(default = "one")]
    pub qty: u32,
}

fn one() -> u32 {
    1
}

#[derive(Debug, Clone, Deserialize)]
pub struct EnemyDef {
    #[serde(skip)]
    pub id: String,
    pub name: String,
    pub behavior: Behavior,
    pub level: u32,
    pub hp: u32,
    pub defense: u32,
    pub poise: u32,
    pub speed: i32,
    pub sight: i32,
    pub leash: i32,
    #[serde(default)]
    pub keep_distance: i32,
    pub echoes: u64,
    pub attacks: Vec<EnemyAttack>,
    #[serde(default)]
    pub drops: Vec<DropDef>,
    #[serde(default)]
    pub radius: Option<i32>,
    #[serde(default)]
    pub blink: Option<i32>,
    #[serde(default)]
    pub phase2: Option<BossPhase2>,
    #[serde(default)]
    pub lore: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BossPhase2 {
    pub threshold_pct: u32,
    pub transition_ticks: u16,
    pub vulnerable_ticks: u16,
    pub vulnerable_mult_pct: u32,
    pub speed: i32,
    pub attacks: Vec<EnemyAttack>,
    pub shockwave_rings: u8,
    pub shockwave_gap: u16,
    pub shockwave_step: u16,
    pub shockwave_damage: u32,
    pub shockwave_max_radius: i32,
    pub dirge_pools: u8,
    pub dirge_radius: i32,
    pub dirge_ticks: u16,
    pub dirge_damage: u32,
    pub dirge_every: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Condition {
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub forbids: Vec<String>,
    #[serde(default)]
    pub has_item: Vec<String>,
}

impl Condition {
    pub fn always() -> Self {
        Condition { requires: vec![], forbids: vec![], has_item: vec![] }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Effects {
    #[serde(default)]
    pub set: Vec<String>,
    #[serde(default)]
    pub give: BTreeMap<String, u32>,
    #[serde(default)]
    pub take: BTreeMap<String, u32>,
    #[serde(default)]
    pub echoes: u64,
    #[serde(default)]
    pub draughts_max: u8,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DialogueChoice {
    pub label: String,
    #[serde(default)]
    pub reply: Vec<String>,
    #[serde(default)]
    pub effects: Effects,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DialogueNode {
    #[serde(flatten)]
    pub when: Condition,
    pub lines: Vec<String>,
    #[serde(default)]
    pub effects: Effects,
    #[serde(default)]
    pub choices: Vec<DialogueChoice>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NpcDef {
    #[serde(skip)]
    pub id: String,
    pub name: String,
    pub title: String,
    pub faction: String,
    pub dialogue: Vec<DialogueNode>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QuestStage {
    pub when: Vec<String>,
    pub note: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QuestDef {
    #[serde(skip)]
    pub id: String,
    pub title: String,
    pub start: String,
    pub done: String,
    pub stages: Vec<QuestStage>,
}

#[derive(Debug)]
pub struct Content {
    pub balance: Balance,
    pub items: BTreeMap<String, ItemDef>,
    pub archetypes: BTreeMap<String, ArchetypeDef>,
    pub enemies: BTreeMap<String, EnemyDef>,
    pub npcs: BTreeMap<String, NpcDef>,
    pub quests: Vec<QuestDef>,
    pub map: MapDef,
}

fn read(root: &Path, rel: &str) -> Result<String, ContentError> {
    let path = root.join(rel);
    std::fs::read_to_string(&path).map_err(|source| ContentError::Io { path: path.display().to_string(), source })
}

fn parse<T: serde::de::DeserializeOwned>(root: &Path, rel: &str) -> Result<T, ContentError> {
    let text = read(root, rel)?;
    toml::from_str(&text).map_err(|source| ContentError::Toml { path: rel.to_string(), source })
}

#[derive(Deserialize)]
struct QuestFile {
    quest: BTreeMap<String, QuestDef>,
}

impl Content {
    pub fn load(root: impl AsRef<Path>) -> Result<Content, ContentError> {
        let root = root.as_ref();
        let balance: Balance = parse(root, "balance.toml")?;
        let mut items: BTreeMap<String, ItemDef> = parse(root, "items/items.toml")?;
        for (id, item) in items.iter_mut() {
            item.id = id.clone();
        }
        let archetypes: BTreeMap<String, ArchetypeDef> = parse(root, "actors/archetypes.toml")?;
        let mut enemies: BTreeMap<String, EnemyDef> = parse(root, "actors/enemies.toml")?;
        for (id, e) in enemies.iter_mut() {
            e.id = id.clone();
        }
        let mut npcs: BTreeMap<String, NpcDef> = parse(root, "dialogue/npcs.toml")?;
        for (id, n) in npcs.iter_mut() {
            n.id = id.clone();
        }
        let quest_file: QuestFile = parse(root, "dialogue/quests.toml")?;
        let quests = quest_file
            .quest
            .into_iter()
            .map(|(id, mut q)| {
                q.id = id;
                q
            })
            .collect();
        let meta: MapMeta = parse(root, "maps/veyr.toml")?;
        let grid = read(root, &format!("maps/{}", meta.grid))?;
        let map = MapDef::build(meta, &grid)?;
        let content = Content { balance, items, archetypes, enemies, npcs, quests, map };
        content.validate()?;
        Ok(content)
    }

    pub fn item(&self, id: &str) -> Option<&ItemDef> {
        self.items.get(id)
    }

    pub fn archetype(&self, a: Archetype) -> &ArchetypeDef {
        &self.archetypes[a.id()]
    }

    pub fn weapon(&self, id: &str) -> Option<&WeaponDef> {
        self.items.get(id).and_then(|i| i.weapon.as_ref())
    }

    pub fn ticks(&self, seconds_x10: u32) -> u32 {
        seconds_x10 * self.balance.tick_hz / 10
    }

    fn validate(&self) -> Result<(), ContentError> {
        let b = &self.balance;
        if b.tick_hz == 0 || b.snapshot_every == 0 {
            return Err(invalid("tick_hz and snapshot_every must be positive"));
        }
        // A body must never travel more than half a cell per tick, otherwise a
        // single-cell wall could be skipped between two collision checks.
        let half = CELL / 2;
        if b.walk_speed >= half {
            return Err(invalid("walk_speed must stay below half a cell per tick"));
        }
        for (name, roll) in &b.rolls {
            if roll.move_ticks == 0 || roll.distance / roll.move_ticks as i32 >= half {
                return Err(invalid(format!("roll {name} moves too far per tick")));
            }
            if roll.iframe_from > roll.iframe_to || roll.iframe_to > roll.ticks || roll.move_ticks > roll.ticks {
                return Err(invalid(format!("roll {name} has inconsistent frame data")));
            }
        }
        if b.parry.window == 0 || b.parry.window > b.parry.total {
            return Err(invalid("parry window must be within the parry duration"));
        }
        if b.draught.apply_at == 0 || b.draught.apply_at > b.draught.windup {
            return Err(invalid("draught apply_at must fall within its windup"));
        }
        for item in self.items.values() {
            let ok = match item.kind {
                ItemKind::Weapon => item.weapon.is_some(),
                ItemKind::Shield => item.shield.is_some(),
                ItemKind::Spell => item.spell.is_some(),
                ItemKind::Armor => item.armor.is_some(),
                ItemKind::Consumable => item.consumable.is_some(),
                ItemKind::Key => true,
            };
            if !ok {
                return Err(invalid(format!("item {} lacks its {:?} block", item.id, item.kind)));
            }
            if let Some(s) = &item.spell {
                if s.form == SpellForm::Bolt && (s.speed <= 0 || s.speed >= half || s.range <= 0) {
                    return Err(invalid(format!("bolt {} needs 0 < speed < half cell and a range", item.id)));
                }
            }
        }
        for a in Archetype::ALL {
            let def = self.archetypes.get(a.id()).ok_or_else(|| invalid(format!("missing archetype {}", a.id())))?;
            if !b.rolls.contains_key(&def.roll) {
                return Err(invalid(format!("archetype {} uses unknown roll {}", a.id(), def.roll)));
            }
            let mut gear = vec![def.main.clone(), def.armor.clone()];
            gear.extend(def.off.clone());
            gear.extend(def.spell1.clone());
            gear.extend(def.items.keys().cloned());
            for g in gear {
                if !self.items.contains_key(&g) {
                    return Err(invalid(format!("archetype {} references unknown item {g}", a.id())));
                }
            }
        }
        for e in self.enemies.values() {
            if e.attacks.is_empty() {
                return Err(invalid(format!("enemy {} has no attacks", e.id)));
            }
            if e.speed >= half {
                return Err(invalid(format!("enemy {} is too fast", e.id)));
            }
            for d in &e.drops {
                if !self.items.contains_key(&d.item) || d.chance_pct > 100 {
                    return Err(invalid(format!("enemy {} has bad drop {}", e.id, d.item)));
                }
            }
            let mut all = e.attacks.iter().collect::<Vec<_>>();
            if let Some(p2) = &e.phase2 {
                all.extend(p2.attacks.iter());
            }
            for atk in all {
                if atk.windup == 0 || atk.active == 0 {
                    return Err(invalid(format!("attack {} of {} needs windup and active frames", atk.id, e.id)));
                }
                if atk.shape == AttackShape::Projectile && (atk.speed <= 0 || atk.speed >= half) {
                    return Err(invalid(format!("projectile {} of {} has a bad speed", atk.id, e.id)));
                }
            }
        }
        for spawn in &self.map.spawns {
            match &spawn.what {
                crate::map::SpawnKind::Enemy(id) => {
                    if !self.enemies.contains_key(id) {
                        return Err(invalid(format!("map spawns unknown enemy {id}")));
                    }
                }
                crate::map::SpawnKind::Npc(id) => {
                    if !self.npcs.contains_key(id) {
                        return Err(invalid(format!("map spawns unknown npc {id}")));
                    }
                }
                crate::map::SpawnKind::Item { item, .. } => {
                    if !self.items.contains_key(item) {
                        return Err(invalid(format!("map places unknown item {item}")));
                    }
                }
            }
        }
        for npc in self.npcs.values() {
            for node in &npc.dialogue {
                let effects = std::iter::once(&node.effects).chain(node.choices.iter().map(|c| &c.effects));
                for fx in effects {
                    for item in fx.give.keys().chain(fx.take.keys()) {
                        if !self.items.contains_key(item) {
                            return Err(invalid(format!("npc {} references unknown item {item}", npc.id)));
                        }
                    }
                }
            }
        }
        for door in &self.map.doors {
            if let crate::map::DoorRule::Key { key } = &door.rule {
                if !self.items.contains_key(key) {
                    return Err(invalid(format!("door {} needs unknown key {key}", door.id)));
                }
            }
            let t = self.map.tile(door.x, door.y);
            if !matches!(t, Tile::Door | Tile::Shortcut | Tile::FogGate | Tile::FalseWall) {
                return Err(invalid(format!("door {} does not sit on a door tile", door.id)));
            }
        }
        Ok(())
    }
}
