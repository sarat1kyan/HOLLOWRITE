// Wire types mirroring crates/protocol/src/lib.rs. Field names and tags are
// checked against Rust-generated fixtures in protocol.test.ts.

export const PROTOCOL_VERSION = 1;
export const CELL = 1000;
export const MAX_CHAT_CHARS = 200;

export type EntityId = number;
export type Dir8 = "n" | "ne" | "e" | "se" | "s" | "sw" | "w" | "nw";
export const DIRS: Dir8[] = ["n", "ne", "e", "se", "s", "sw", "w", "nw"];
export const DIR_OFFSET: Record<Dir8, [number, number]> = {
  n: [0, -1], ne: [1, -1], e: [1, 0], se: [1, 1], s: [0, 1], sw: [-1, 1], w: [-1, 0], nw: [-1, -1],
};

export function dirFromOffset(dx: number, dy: number): Dir8 | null {
  const sx = Math.sign(dx), sy = Math.sign(dy);
  return DIRS.find((d) => DIR_OFFSET[d][0] === sx && DIR_OFFSET[d][1] === sy) ?? null;
}

export type Archetype = "ash_warden" | "veil_seer" | "grave_runner";
export const ARCHETYPES: { id: Archetype; label: string; blurb: string; stats: string }[] = [
  { id: "ash_warden", label: "Ash Warden", blurb: "Sword and buckler. Sturdy, patient, made for parries.", stats: "VIT 12 · END 12 · MIG 11 · FIN 8 · OCC 6" },
  { id: "veil_seer", label: "Veil Seer", blurb: "Fragile caster. Ember Psalm and a thorn knife for when it gets close.", stats: "VIT 8 · END 9 · MIG 6 · FIN 9 · OCC 15" },
  { id: "grave_runner", label: "Grave Runner", blurb: "Light blade, quicker dodge. Strike, roll, strike again.", stats: "VIT 10 · END 11 · MIG 8 · FIN 14 · OCC 6" },
];

export type Attribute = "vitality" | "endurance" | "might" | "finesse" | "occult";
export const ATTRIBUTES: Attribute[] = ["vitality", "endurance", "might", "finesse", "occult"];

export interface Appearance { sigil: number; hue: number }
export const APPEARANCE_SIGILS = 4;
export const APPEARANCE_HUES = 6;

export type EntityKind = "player" | "npc" | "enemy" | "boss" | "projectile" | "remnant" | "loot";
export type ActorState =
  | "idle" | "moving" | "windup" | "active" | "recovery" | "rolling" | "parrying"
  | "guarding" | "staggered" | "healing" | "casting" | "interacting" | "dead";
export type Relation = "me" | "party" | "stranger" | "neutral" | "hostile";
export type TelegraphKind = "strike" | "heavy" | "area" | "shockwave" | "hazard" | "weak";
export type HitKind = "normal" | "critical" | "blocked" | "parried" | "dodged" | "hazard";
export type ChatChannel = "local" | "party" | "system";
export type EquipSlot = "main" | "off" | "armor" | "spell1" | "spell2";
export type Action =
  | "light" | "heavy" | "roll" | "parry" | "guard_on" | "guard_off" | "heal" | "cast1" | "cast2" | "interact";
export type ErrorCode =
  | "protocol_mismatch" | "bad_request" | "unauthorized" | "name_taken" | "invalid_name"
  | "too_many_characters" | "not_found" | "rate_limited" | "not_allowed" | "busy" | "internal";

export interface ZoneInfo { id: string; name: string; x: number; y: number; w: number; h: number; safe: boolean }
export interface LanternInfo { id: string; name: string; x: number; y: number }
export interface DoorInfo { id: string; x: number; y: number }
export interface MapData {
  id: string; name: string; width: number; height: number; rows: string[];
  zones: ZoneInfo[]; lanterns: LanternInfo[]; doors: DoorInfo[];
}

export interface EntityView {
  id: EntityId; kind: EntityKind; def: string; name?: string; x: number; y: number;
  facing: Dir8; state: ActorState; action?: string; progress: number; rel: Relation;
  hp?: [number, number]; look?: Appearance; phase?: number;
}

export interface Telegraph { id: number; owner: EntityId; kind: TelegraphKind; cells: [number, number][]; left: number; total: number }
export interface BossBar { name: string; hp: number; hp_max: number; phase: number; vulnerable: boolean }

export interface SelfView {
  hp: number; hp_max: number; stamina: number; stamina_max: number; wick: number; wick_max: number;
  draughts: number; draughts_max: number; echoes: number; state: ActorState; cooldowns: [number, number];
  zone: string; remnant?: [number, number]; boss?: BossBar; interact?: string;
}

export interface ItemStack { item: string; name: string; qty: number; kind: string; desc: string }
export interface Equipment { main: string | null; off: string | null; armor: string | null; spell1: string | null; spell2: string | null }
export interface QuestView { id: string; title: string; note: string; done: boolean }
export interface DerivedStats {
  hp_max: number; stamina_max: number; wick_max: number; defense: number;
  light_damage: number; heavy_damage: number; spell_power: number;
}
export interface CharacterSheet {
  id: string; name: string; archetype: Archetype; look: Appearance; level: number;
  attrs: [number, number, number, number, number]; echoes: number; next_level_cost: number;
  inventory: ItemStack[]; equipment: Equipment; open_doors: string[]; quests: QuestView[];
  checkpoint: string; lanterns: string[]; stats: DerivedStats;
}
export interface CharacterSummary { id: string; name: string; archetype: Archetype; level: number; look: Appearance; location: string }
export interface PartyMember { name: string; online: boolean; leader: boolean; hp?: [number, number] }

export interface Snapshot {
  tick: number; ack: number; me: SelfView; entities: EntityView[]; gone?: EntityId[]; tele?: Telegraph[];
}

export type GameEvent =
  | { e: "hit"; src: EntityId; dst: EntityId; amount: number; kind: HitKind; x: number; y: number }
  | { e: "swing"; src: EntityId; action: string; facing: Dir8; x: number; y: number }
  | { e: "staggered"; id: EntityId }
  | { e: "died"; id: EntityId; x: number; y: number }
  | { e: "respawned"; id: EntityId; lantern: string }
  | { e: "echoes_gained"; amount: number; total: number }
  | { e: "echoes_lost"; amount: number }
  | { e: "echoes_reclaimed"; amount: number }
  | { e: "item_gained"; item: string; name: string; qty: number }
  | { e: "lantern_lit"; lantern: string; name: string }
  | { e: "rested"; lantern: string }
  | { e: "level_up"; level: number }
  | { e: "boss_phase"; id: EntityId; phase: number }
  | { e: "boss_defeated"; id: EntityId; name: string }
  | { e: "boss_reset"; id: EntityId }
  | { e: "toll"; x: number; y: number; radius: number }
  | { e: "door_opened"; door: string }
  | { e: "notice"; text: string }
  | { e: "zone_entered"; zone: string; name: string };

export type ClientMsg =
  | { t: "hello"; protocol: number; client: "web"; build: string }
  | { t: "list_characters" }
  | { t: "create_character"; name: string; archetype: Archetype; look: Appearance }
  | { t: "select_character"; id: string }
  | { t: "input"; seq: number; mv?: Dir8; act?: Action; face?: Dir8 }
  | { t: "equip"; slot: EquipSlot; item: string | null }
  | { t: "use_item"; item: string }
  | { t: "level_up"; attr: Attribute }
  | { t: "talk"; npc: EntityId; choice?: number }
  | { t: "chat"; channel: ChatChannel; text: string }
  | { t: "party_invite"; name: string }
  | { t: "party_accept"; from: string }
  | { t: "party_leave" }
  | { t: "mute"; name: string; on: boolean }
  | { t: "report"; name: string; reason: string }
  | { t: "ping"; nonce: number }
  | { t: "resync" }
  | { t: "leave" };

export type ServerMsg =
  | { t: "welcome"; protocol: number; server: string; tick_hz: number; snapshot_hz: number; account: string; walk_speed?: number; body_radius?: number }
  | { t: "error"; code: ErrorCode; message: string }
  | { t: "characters"; list: CharacterSummary[] }
  | { t: "enter_world"; you: EntityId; tick: number; map: MapData; sheet: CharacterSheet }
  | ({ t: "snapshot" } & Snapshot)
  | { t: "sheet"; sheet: CharacterSheet }
  | { t: "events"; tick: number; events: GameEvent[] }
  | { t: "chat"; channel: ChatChannel; from: string; text: string }
  | { t: "party"; members: PartyMember[]; invites: string[] }
  | { t: "dialogue"; npc: EntityId; speaker: string; lines: string[]; choices: string[] }
  | { t: "pong"; nonce: number; tick: number }
  | { t: "kicked"; reason: string };

export type Tile =
  | "void" | "wall" | "floor" | "road" | "ash" | "grass" | "tree" | "water" | "bones" | "rubble"
  | "door" | "shortcut" | "fog_gate" | "lantern" | "fence" | "stairs" | "ember_pit" | "altar"
  | "false_wall" | "grave" | "pew";

const TILE_CODES: Record<string, Tile> = {
  " ": "void", "#": "wall", ".": "floor", ":": "road", ",": "ash", '"': "grass", T: "tree", "~": "water",
  b: "bones", "%": "rubble", "+": "door", D: "shortcut", G: "fog_gate", L: "lantern", "|": "fence",
  ">": "stairs", "^": "ember_pit", A: "altar", X: "false_wall", t: "grave", "=": "pew",
};

export function tileFromCode(c: string): Tile {
  return TILE_CODES[c] ?? "void";
}

const WALKABLE = new Set<Tile>(["floor", "road", "ash", "grass", "bones", "rubble", "lantern", "stairs", "ember_pit", "grave"]);
const OPAQUE = new Set<Tile>(["wall", "tree", "door", "shortcut", "fog_gate", "false_wall", "void"]);

export const walkable = (t: Tile) => WALKABLE.has(t);
export const opaque = (t: Tile) => OPAQUE.has(t);

export function validateName(name: string): string | null {
  if (name.length < 3 || name.length > 16) return "name must be 3-16 characters";
  if (!/^[A-Za-z]/.test(name)) return "name must start with a letter";
  if (!/^[A-Za-z][A-Za-z0-9_-]*$/.test(name)) return "name may only contain letters, digits, '-' and '_'";
  return null;
}

/** Defence in depth: drop control and bidi formatting characters before display. */
export function cleanText(s: string): string {
  // eslint-disable-next-line no-control-regex
  return s.replace(/[\u0000-\u001f\u007f-\u009f\u200b-\u200f\u202a-\u202e\u2060-\u206f\ufeff]/g, "");
}
