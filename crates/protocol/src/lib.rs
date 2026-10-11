//! Wire protocol for HOLLOWRITE.
//!
//! Every frame is a single JSON object carried in a WebSocket text message.
//! Messages are internally tagged by `t`. Bump [`PROTOCOL_VERSION`] on any
//! change that an older peer could misinterpret; additive optional fields do
//! not require a bump.

use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 1;

/// Hard cap on a single inbound client frame. The server rejects larger frames.
pub const MAX_CLIENT_FRAME_BYTES: usize = 4 * 1024;
pub const MAX_CHAT_CHARS: usize = 200;
pub const MAX_NAME_CHARS: usize = 16;
pub const MIN_NAME_CHARS: usize = 3;

/// Sub-cell resolution: one map cell is `CELL` world units on each axis.
pub const CELL: i32 = 1000;

pub type EntityId = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dir8 {
    N,
    Ne,
    E,
    Se,
    S,
    Sw,
    W,
    Nw,
}

impl Dir8 {
    pub const ALL: [Dir8; 8] = [Dir8::N, Dir8::Ne, Dir8::E, Dir8::Se, Dir8::S, Dir8::Sw, Dir8::W, Dir8::Nw];

    /// Unit grid offset (dx, dy) with y growing downwards.
    pub fn offset(self) -> (i32, i32) {
        match self {
            Dir8::N => (0, -1),
            Dir8::Ne => (1, -1),
            Dir8::E => (1, 0),
            Dir8::Se => (1, 1),
            Dir8::S => (0, 1),
            Dir8::Sw => (-1, 1),
            Dir8::W => (-1, 0),
            Dir8::Nw => (-1, -1),
        }
    }

    pub fn from_offset(dx: i32, dy: i32) -> Option<Dir8> {
        let d = (dx.signum(), dy.signum());
        Dir8::ALL.into_iter().find(|dir| dir.offset() == d)
    }

    pub fn is_diagonal(self) -> bool {
        let (dx, dy) = self.offset();
        dx != 0 && dy != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Archetype {
    AshWarden,
    VeilSeer,
    GraveRunner,
}

impl Archetype {
    pub const ALL: [Archetype; 3] = [Archetype::AshWarden, Archetype::VeilSeer, Archetype::GraveRunner];

    pub fn id(self) -> &'static str {
        match self {
            Archetype::AshWarden => "ash_warden",
            Archetype::VeilSeer => "veil_seer",
            Archetype::GraveRunner => "grave_runner",
        }
    }

    pub fn from_id(id: &str) -> Option<Archetype> {
        Archetype::ALL.into_iter().find(|a| a.id() == id)
    }

    pub fn label(self) -> &'static str {
        match self {
            Archetype::AshWarden => "Ash Warden",
            Archetype::VeilSeer => "Veil Seer",
            Archetype::GraveRunner => "Grave Runner",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Attribute {
    Vitality,
    Endurance,
    Might,
    Finesse,
    Occult,
}

impl Attribute {
    pub const ALL: [Attribute; 5] =
        [Attribute::Vitality, Attribute::Endurance, Attribute::Might, Attribute::Finesse, Attribute::Occult];
}

/// Player-chosen look. Both values index fixed, readability-checked tables
/// in the shared theme, so a player can never pick an unreadable combination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Appearance {
    pub sigil: u8,
    pub hue: u8,
}

pub const APPEARANCE_SIGILS: u8 = 4;
pub const APPEARANCE_HUES: u8 = 6;

impl Appearance {
    pub fn is_valid(&self) -> bool {
        self.sigil < APPEARANCE_SIGILS && self.hue < APPEARANCE_HUES
    }
}

/// Terrain vocabulary. The single-character codes are what travels in
/// [`MapData::rows`]; clients map them to glyphs through the shared theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tile {
    Void,
    Wall,
    Floor,
    Road,
    Ash,
    Grass,
    Tree,
    Water,
    Bones,
    Rubble,
    Door,
    Shortcut,
    FogGate,
    Lantern,
    Fence,
    Stairs,
    EmberPit,
    Altar,
    FalseWall,
    Grave,
    Pew,
}

impl Tile {
    pub const ALL: [Tile; 21] = [
        Tile::Void,
        Tile::Wall,
        Tile::Floor,
        Tile::Road,
        Tile::Ash,
        Tile::Grass,
        Tile::Tree,
        Tile::Water,
        Tile::Bones,
        Tile::Rubble,
        Tile::Door,
        Tile::Shortcut,
        Tile::FogGate,
        Tile::Lantern,
        Tile::Fence,
        Tile::Stairs,
        Tile::EmberPit,
        Tile::Altar,
        Tile::FalseWall,
        Tile::Grave,
        Tile::Pew,
    ];

    pub fn code(self) -> char {
        match self {
            Tile::Void => ' ',
            Tile::Wall => '#',
            Tile::Floor => '.',
            Tile::Road => ':',
            Tile::Ash => ',',
            Tile::Grass => '"',
            Tile::Tree => 'T',
            Tile::Water => '~',
            Tile::Bones => 'b',
            Tile::Rubble => '%',
            Tile::Door => '+',
            Tile::Shortcut => 'D',
            Tile::FogGate => 'G',
            Tile::Lantern => 'L',
            Tile::Fence => '|',
            Tile::Stairs => '>',
            Tile::EmberPit => '^',
            Tile::Altar => 'A',
            Tile::FalseWall => 'X',
            Tile::Grave => 't',
            Tile::Pew => '=',
        }
    }

    pub fn from_code(c: char) -> Option<Tile> {
        Tile::ALL.into_iter().find(|t| t.code() == c)
    }

    /// Whether a walker may stand on this tile. Doors and gates are resolved
    /// per character by the server and reported through `open_doors`.
    pub fn walkable(self) -> bool {
        matches!(
            self,
            Tile::Floor
                | Tile::Road
                | Tile::Ash
                | Tile::Grass
                | Tile::Bones
                | Tile::Rubble
                | Tile::Lantern
                | Tile::Stairs
                | Tile::EmberPit
                | Tile::Grave
        )
    }

    pub fn opaque(self) -> bool {
        matches!(
            self,
            Tile::Wall | Tile::Tree | Tile::Door | Tile::Shortcut | Tile::FogGate | Tile::FalseWall | Tile::Void
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapData {
    pub id: String,
    pub name: String,
    pub width: u16,
    pub height: u16,
    /// `height` strings of exactly `width` tile codes each.
    pub rows: Vec<String>,
    pub zones: Vec<ZoneInfo>,
    pub lanterns: Vec<LanternInfo>,
    pub doors: Vec<DoorInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZoneInfo {
    pub id: String,
    pub name: String,
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
    pub safe: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanternInfo {
    pub id: String,
    pub name: String,
    pub x: u16,
    pub y: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoorInfo {
    pub id: String,
    pub x: u16,
    pub y: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    Player,
    Npc,
    Enemy,
    Boss,
    Projectile,
    Remnant,
    Loot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorState {
    Idle,
    Moving,
    Windup,
    Active,
    Recovery,
    Rolling,
    Parrying,
    Guarding,
    Staggered,
    Healing,
    Casting,
    Interacting,
    Dead,
}

/// How a viewer relates to an entity. Computed per recipient.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    Me,
    Party,
    Stranger,
    Neutral,
    Hostile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityView {
    pub id: EntityId,
    pub kind: EntityKind,
    /// Content id: archetype for players, template id for everything else.
    pub def: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub x: i32,
    pub y: i32,
    pub facing: Dir8,
    pub state: ActorState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    /// Progress through the current state, 0..=100.
    pub progress: u8,
    pub rel: Relation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hp: Option<[u32; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub look: Option<Appearance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TelegraphKind {
    Strike,
    Heavy,
    Area,
    Shockwave,
    Hazard,
    Weak,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Telegraph {
    pub id: u32,
    pub owner: EntityId,
    pub kind: TelegraphKind,
    pub cells: Vec<[i16; 2]>,
    pub left: u16,
    pub total: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelfView {
    pub hp: u32,
    pub hp_max: u32,
    pub stamina: u32,
    pub stamina_max: u32,
    pub wick: u32,
    pub wick_max: u32,
    pub draughts: u8,
    pub draughts_max: u8,
    pub echoes: u64,
    pub state: ActorState,
    pub cooldowns: [u16; 2],
    pub zone: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remnant: Option<[i32; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boss: Option<BossBar>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interact: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BossBar {
    pub name: String,
    pub hp: u32,
    pub hp_max: u32,
    pub phase: u8,
    pub vulnerable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemStack {
    pub item: String,
    pub name: String,
    pub qty: u32,
    pub kind: String,
    pub desc: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Equipment {
    pub main: Option<String>,
    pub off: Option<String>,
    pub armor: Option<String>,
    pub spell1: Option<String>,
    pub spell2: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EquipSlot {
    Main,
    Off,
    Armor,
    Spell1,
    Spell2,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestView {
    pub id: String,
    pub title: String,
    pub note: String,
    pub done: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharacterSheet {
    pub id: String,
    pub name: String,
    pub archetype: Archetype,
    pub look: Appearance,
    pub level: u32,
    pub attrs: [u32; 5],
    pub echoes: u64,
    pub next_level_cost: u64,
    pub inventory: Vec<ItemStack>,
    pub equipment: Equipment,
    pub open_doors: Vec<String>,
    pub quests: Vec<QuestView>,
    pub checkpoint: String,
    pub lanterns: Vec<String>,
    pub stats: DerivedStats,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DerivedStats {
    pub hp_max: u32,
    pub stamina_max: u32,
    pub wick_max: u32,
    pub defense: u32,
    pub light_damage: u32,
    pub heavy_damage: u32,
    pub spell_power: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharacterSummary {
    pub id: String,
    pub name: String,
    pub archetype: Archetype,
    pub level: u32,
    pub look: Appearance,
    pub location: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatChannel {
    Local,
    Party,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientKind {
    Web,
    Terminal,
    Bot,
}

/// A discrete action request. Movement is carried separately as held state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Light,
    Heavy,
    Roll,
    Parry,
    GuardOn,
    GuardOff,
    Heal,
    Cast1,
    Cast2,
    Interact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ClientMsg {
    Hello {
        protocol: u32,
        client: ClientKind,
        build: String,
    },
    ListCharacters,
    CreateCharacter {
        name: String,
        archetype: Archetype,
        look: Appearance,
    },
    SelectCharacter {
        id: String,
    },
    /// Held movement plus at most one edge-triggered action. `seq` must be
    /// strictly increasing per connection; stale or duplicate frames are dropped.
    Input {
        seq: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mv: Option<Dir8>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        act: Option<Action>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        face: Option<Dir8>,
    },
    Equip {
        slot: EquipSlot,
        item: Option<String>,
    },
    UseItem {
        item: String,
    },
    LevelUp {
        attr: Attribute,
    },
    Talk {
        npc: EntityId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        choice: Option<u8>,
    },
    Chat {
        channel: ChatChannel,
        text: String,
    },
    PartyInvite {
        name: String,
    },
    PartyAccept {
        from: String,
    },
    PartyLeave,
    Mute {
        name: String,
        on: bool,
    },
    Report {
        name: String,
        reason: String,
    },
    Ping {
        nonce: u32,
    },
    Resync,
    Leave,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ServerMsg {
    Welcome { protocol: u32, server: String, tick_hz: u32, snapshot_hz: u32, account: String },
    Error { code: ErrorCode, message: String },
    Characters { list: Vec<CharacterSummary> },
    EnterWorld { you: EntityId, tick: u64, map: MapData, sheet: CharacterSheet },
    Snapshot(Snapshot),
    Sheet { sheet: CharacterSheet },
    Events { tick: u64, events: Vec<GameEvent> },
    Chat { channel: ChatChannel, from: String, text: String },
    Party { members: Vec<PartyMember>, invites: Vec<String> },
    Dialogue { npc: EntityId, speaker: String, lines: Vec<String>, choices: Vec<String> },
    Pong { nonce: u32, tick: u64 },
    Kicked { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub tick: u64,
    /// Highest input sequence the server has applied for this connection.
    pub ack: u32,
    pub me: SelfView,
    pub entities: Vec<EntityView>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gone: Vec<EntityId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tele: Vec<Telegraph>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartyMember {
    pub name: String,
    pub online: bool,
    pub leader: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hp: Option<[u32; 2]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    ProtocolMismatch,
    BadRequest,
    Unauthorized,
    NameTaken,
    InvalidName,
    TooManyCharacters,
    NotFound,
    RateLimited,
    NotAllowed,
    Busy,
    Internal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HitKind {
    Normal,
    Critical,
    Blocked,
    Parried,
    Dodged,
    Hazard,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "e", rename_all = "snake_case")]
pub enum GameEvent {
    Hit { src: EntityId, dst: EntityId, amount: u32, kind: HitKind, x: i32, y: i32 },
    Swing { src: EntityId, action: String, facing: Dir8, x: i32, y: i32 },
    Staggered { id: EntityId },
    Died { id: EntityId, x: i32, y: i32 },
    Respawned { id: EntityId, lantern: String },
    EchoesGained { amount: u64, total: u64 },
    EchoesLost { amount: u64 },
    EchoesReclaimed { amount: u64 },
    ItemGained { item: String, name: String, qty: u32 },
    LanternLit { lantern: String, name: String },
    Rested { lantern: String },
    LevelUp { level: u32 },
    BossPhase { id: EntityId, phase: u8 },
    BossDefeated { id: EntityId, name: String },
    BossReset { id: EntityId },
    Toll { x: i32, y: i32, radius: u16 },
    DoorOpened { door: String },
    Notice { text: String },
    ZoneEntered { zone: String, name: String },
}

/// Strip anything that could be interpreted as markup or a terminal control
/// sequence, collapse whitespace and clamp length. Both clients must still
/// render text as text; this is defence in depth applied at the server edge.
pub fn sanitize_text(input: &str, max_chars: usize) -> String {
    let mut out = String::with_capacity(input.len().min(max_chars * 4));
    let mut count = 0;
    let mut last_space = true;
    for ch in input.chars() {
        if count >= max_chars {
            break;
        }
        let ch = if ch.is_whitespace() { ' ' } else { ch };
        if ch.is_control() || is_format_char(ch) {
            continue;
        }
        if ch == ' ' {
            if last_space {
                continue;
            }
            last_space = true;
        } else {
            last_space = false;
        }
        out.push(ch);
        count += 1;
    }
    out.trim_end().to_string()
}

fn is_format_char(ch: char) -> bool {
    matches!(ch,
        '\u{200B}'..='\u{200F}'
        | '\u{202A}'..='\u{202E}'
        | '\u{2060}'..='\u{206F}'
        | '\u{FEFF}'
        | '\u{FFF9}'..='\u{FFFB}')
}

/// Character and account names: ASCII letters, digits, `-` and `_`, with a
/// leading letter. Restricting to ASCII sidesteps homoglyph impersonation and
/// guarantees single-width rendering in every terminal.
pub fn validate_name(name: &str) -> Result<(), &'static str> {
    let len = name.chars().count();
    if !(MIN_NAME_CHARS..=MAX_NAME_CHARS).contains(&len) {
        return Err("name must be 3-16 characters");
    }
    let mut chars = name.chars();
    if !chars.next().is_some_and(|c| c.is_ascii_alphabetic()) {
        return Err("name must start with a letter");
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err("name may only contain letters, digits, '-' and '_'");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_codes_round_trip_and_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for t in Tile::ALL {
            assert!(seen.insert(t.code()), "duplicate code for {t:?}");
            assert_eq!(Tile::from_code(t.code()), Some(t));
        }
    }

    #[test]
    fn sanitize_removes_escape_sequences_and_bidi() {
        let s = sanitize_text("hi\x1b[31mred\x1b]0;title\x07 \u{202E}evil\r\nok", 200);
        assert!(!s.contains('\x1b'));
        assert!(!s.contains('\x07'));
        assert!(!s.contains('\u{202E}'));
        assert_eq!(s, "hi[31mred]0;title evil ok");
    }

    #[test]
    fn sanitize_clamps_length() {
        let s = sanitize_text(&"a".repeat(1000), MAX_CHAT_CHARS);
        assert_eq!(s.chars().count(), MAX_CHAT_CHARS);
    }

    #[test]
    fn names_are_validated() {
        assert!(validate_name("Alice").is_ok());
        assert!(validate_name("al_ice-2").is_ok());
        assert!(validate_name("2alice").is_err());
        assert!(validate_name("al").is_err());
        assert!(validate_name("Аlice").is_err());
        assert!(validate_name("alice<script>").is_err());
        assert!(validate_name(&"a".repeat(17)).is_err());
    }

    #[test]
    fn messages_round_trip() {
        let msg = ClientMsg::Input { seq: 7, mv: Some(Dir8::Ne), act: Some(Action::Roll), face: None };
        let json = serde_json::to_string(&msg).unwrap();
        assert_eq!(json, r#"{"t":"input","seq":7,"mv":"ne","act":"roll"}"#);
        assert_eq!(serde_json::from_str::<ClientMsg>(&json).unwrap(), msg);
    }

    #[test]
    fn unknown_variants_are_rejected() {
        assert!(serde_json::from_str::<ClientMsg>(r#"{"t":"grant_echoes","amount":5}"#).is_err());
        assert!(serde_json::from_str::<ClientMsg>(r#"{"t":"input","seq":1,"mv":"up"}"#).is_err());
    }
}
