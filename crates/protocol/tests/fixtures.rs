//! Keeps `apps/web/src/__fixtures__/protocol.json` in sync with the Rust
//! types. Run with `HOLLOWRITE_WRITE_FIXTURES=1` to regenerate after an
//! intentional protocol change; otherwise the test fails on drift.

use hollowrite_protocol::*;

fn sheet() -> CharacterSheet {
    CharacterSheet {
        id: "c1".into(),
        name: "Alice".into(),
        archetype: Archetype::AshWarden,
        look: Appearance { sigil: 1, hue: 2 },
        level: 3,
        attrs: [13, 12, 12, 8, 6],
        echoes: 420,
        next_level_cost: 160,
        inventory: vec![ItemStack {
            item: "rustbrand_sword".into(),
            name: "Rustbrand Sword".into(),
            qty: 1,
            kind: "weapon".into(),
            desc: "d".into(),
        }],
        equipment: Equipment {
            main: Some("rustbrand_sword".into()),
            off: None,
            armor: None,
            spell1: None,
            spell2: None,
        },
        open_doors: vec!["tunnel_shortcut".into()],
        quests: vec![QuestView { id: "q".into(), title: "A Second Light".into(), note: "n".into(), done: false }],
        checkpoint: "last_lantern".into(),
        lanterns: vec!["last_lantern".into()],
        stats: DerivedStats {
            hp_max: 444,
            stamina_max: 108,
            wick_max: 44,
            defense: 6,
            light_damage: 58,
            heavy_damage: 96,
            spell_power: 0,
        },
    }
}

fn server_messages() -> Vec<ServerMsg> {
    let me = SelfView {
        hp: 400,
        hp_max: 444,
        stamina: 90,
        stamina_max: 108,
        wick: 44,
        wick_max: 44,
        draughts: 2,
        draughts_max: 3,
        echoes: 420,
        state: ActorState::Moving,
        cooldowns: [0, 12],
        zone: "plaza".into(),
        remnant: Some([1500, 2500]),
        boss: Some(BossBar { name: "The Bell-Mother".into(), hp: 1000, hp_max: 2600, phase: 2, vulnerable: true }),
        interact: Some("Rest at Gloam Plaza Lantern".into()),
    };
    let events = vec![
        GameEvent::Hit { src: 1, dst: 2, amount: 30, kind: HitKind::Critical, x: 1, y: 2 },
        GameEvent::Swing { src: 1, action: "heavy".into(), facing: Dir8::Ne, x: 1, y: 2 },
        GameEvent::Staggered { id: 2 },
        GameEvent::Died { id: 2, x: 1, y: 2 },
        GameEvent::Respawned { id: 1, lantern: "last_lantern".into() },
        GameEvent::EchoesGained { amount: 60, total: 480 },
        GameEvent::EchoesLost { amount: 10 },
        GameEvent::EchoesReclaimed { amount: 10 },
        GameEvent::ItemGained { item: "ashen_moth".into(), name: "Ashen Moth".into(), qty: 2 },
        GameEvent::LanternLit { lantern: "gloam_lantern".into(), name: "Gloam Plaza Lantern".into() },
        GameEvent::Rested { lantern: "gloam_lantern".into() },
        GameEvent::LevelUp { level: 4 },
        GameEvent::BossPhase { id: 9, phase: 2 },
        GameEvent::BossDefeated { id: 9, name: "The Bell-Mother".into() },
        GameEvent::BossReset { id: 9 },
        GameEvent::Toll { x: 1, y: 2, radius: 26 },
        GameEvent::DoorOpened { door: "ossuary_gate".into() },
        GameEvent::Notice { text: "hello".into() },
        GameEvent::ZoneEntered { zone: "orchard".into(), name: "Ash Orchard".into() },
    ];
    vec![
        ServerMsg::Welcome {
            protocol: PROTOCOL_VERSION,
            server: "hollowrite".into(),
            tick_hz: 30,
            snapshot_hz: 15,
            account: "alice".into(),
            walk_speed: 120,
            body_radius: 300,
        },
        ServerMsg::Error { code: ErrorCode::NameTaken, message: "taken".into() },
        ServerMsg::Characters {
            list: vec![CharacterSummary {
                id: "c1".into(),
                name: "Alice".into(),
                archetype: Archetype::VeilSeer,
                level: 2,
                look: Appearance { sigil: 0, hue: 3 },
                location: "last_lantern".into(),
            }],
        },
        ServerMsg::EnterWorld {
            you: 7,
            tick: 100,
            map: MapData {
                id: "m".into(),
                name: "M".into(),
                width: 5,
                height: 3,
                rows: vec!["#####".into(), "#.D.#".into(), "#####".into()],
                zones: vec![ZoneInfo { id: "z".into(), name: "Z".into(), x: 0, y: 0, w: 5, h: 3, safe: true }],
                lanterns: vec![LanternInfo { id: "l".into(), name: "L".into(), x: 1, y: 1 }],
                doors: vec![DoorInfo { id: "d".into(), x: 2, y: 1 }],
            },
            sheet: sheet(),
        },
        ServerMsg::Snapshot(Snapshot {
            tick: 101,
            ack: 5,
            me,
            entities: vec![EntityView {
                id: 7,
                kind: EntityKind::Player,
                def: "ash_warden".into(),
                name: Some("Alice".into()),
                x: 1500,
                y: 1500,
                facing: Dir8::E,
                state: ActorState::Windup,
                action: Some("light".into()),
                progress: 40,
                rel: Relation::Me,
                hp: Some([400, 444]),
                look: Some(Appearance { sigil: 1, hue: 2 }),
                phase: None,
            }],
            gone: vec![3],
            tele: vec![Telegraph {
                id: 1,
                owner: 9,
                kind: TelegraphKind::Shockwave,
                cells: vec![[2, 1]],
                left: 10,
                total: 30,
            }],
        }),
        ServerMsg::Sheet { sheet: sheet() },
        ServerMsg::Events { tick: 101, events },
        ServerMsg::Chat { channel: ChatChannel::Party, from: "Bob".into(), text: "hi".into() },
        ServerMsg::Party {
            members: vec![PartyMember { name: "Bob".into(), online: true, leader: true, hp: Some([1, 2]) }],
            invites: vec!["Cy".into()],
        },
        ServerMsg::Dialogue { npc: 3, speaker: "Hesper".into(), lines: vec!["l".into()], choices: vec!["c".into()] },
        ServerMsg::Pong { nonce: 1, tick: 2 },
        ServerMsg::Kicked { reason: "r".into() },
    ]
}

/// Client messages exactly as the web client builds them.
const CLIENT_JSON: &[&str] = &[
    r#"{"t":"hello","protocol":1,"client":"web","build":"web"}"#,
    r#"{"t":"list_characters"}"#,
    r#"{"t":"create_character","name":"Alice","archetype":"grave_runner","look":{"sigil":3,"hue":5}}"#,
    r#"{"t":"select_character","id":"c1"}"#,
    r#"{"t":"input","seq":9,"mv":"sw","act":"guard_on"}"#,
    r#"{"t":"input","seq":10}"#,
    r#"{"t":"equip","slot":"spell2","item":"hollow_toll"}"#,
    r#"{"t":"equip","slot":"main","item":null}"#,
    r#"{"t":"use_item","item":"ashen_moth"}"#,
    r#"{"t":"level_up","attr":"occult"}"#,
    r#"{"t":"talk","npc":3,"choice":1}"#,
    r#"{"t":"chat","channel":"party","text":"hi"}"#,
    r#"{"t":"party_invite","name":"Bob"}"#,
    r#"{"t":"party_accept","from":"Bob"}"#,
    r#"{"t":"party_leave"}"#,
    r#"{"t":"mute","name":"Bob","on":true}"#,
    r#"{"t":"report","name":"Bob","reason":"spam"}"#,
    r#"{"t":"ping","nonce":4}"#,
    r#"{"t":"resync"}"#,
    r#"{"t":"leave"}"#,
];

#[test]
fn web_client_messages_deserialize() {
    for json in CLIENT_JSON {
        let msg: ClientMsg = serde_json::from_str(json).unwrap_or_else(|e| panic!("{json}: {e}"));
        let back = serde_json::to_value(&msg).unwrap();
        let orig: serde_json::Value = serde_json::from_str(json).unwrap();
        assert_eq!(back, orig, "round trip changed {json}");
    }
}

#[test]
fn fixtures_match_rust_types() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/web/src/__fixtures__/protocol.json");
    let doc = serde_json::json!({
        "server": server_messages(),
        "client": CLIENT_JSON.iter().map(|j| serde_json::from_str::<serde_json::Value>(j).unwrap()).collect::<Vec<_>>(),
    });
    let text = serde_json::to_string_pretty(&doc).unwrap() + "\n";
    if std::env::var("HOLLOWRITE_WRITE_FIXTURES").is_ok() {
        std::fs::write(&path, &text).unwrap();
        return;
    }
    let committed = std::fs::read_to_string(&path).expect("fixture missing; run with HOLLOWRITE_WRITE_FIXTURES=1");
    assert_eq!(committed, text, "protocol fixtures are stale; regenerate with HOLLOWRITE_WRITE_FIXTURES=1");
}
