#![allow(dead_code)]

use std::sync::{Arc, OnceLock};

use hollowrite_game_core::world::World;
use hollowrite_game_core::{CharacterData, Content, Pos};
use hollowrite_protocol::{Action, Appearance, Archetype, Dir8, EntityId};

pub fn content() -> Arc<Content> {
    static CONTENT: OnceLock<Arc<Content>> = OnceLock::new();
    CONTENT
        .get_or_init(|| Arc::new(Content::load(hollowrite_game_core::default_content_dir()).expect("content loads")))
        .clone()
}

pub struct Sim {
    pub world: World,
    seq: u32,
}

impl Sim {
    pub fn new() -> Self {
        Sim { world: World::new(content(), 42), seq: 0 }
    }

    pub fn character(&self, name: &str, archetype: Archetype) -> CharacterData {
        CharacterData::new(
            &self.world.content,
            format!("char-{name}"),
            format!("acct-{name}"),
            name.to_string(),
            archetype,
            Appearance::default(),
        )
    }

    pub fn join(&mut self, name: &str, archetype: Archetype) -> EntityId {
        let data = self.character(name, archetype);
        self.world.join(data)
    }

    pub fn place(&mut self, id: EntityId, cx: i32, cy: i32) {
        let p = self.world.players.get_mut(&id).unwrap();
        p.body.pos = Pos::cell_center(cx, cy);
        p.last_safe = p.body.pos;
    }

    pub fn pos(&self, id: EntityId) -> Pos {
        self.world.players[&id].body.pos
    }

    pub fn input(&mut self, id: EntityId, mv: Option<Dir8>, act: Option<Action>) {
        self.seq += 1;
        assert!(self.world.input(id, self.seq, mv, act, None));
    }

    pub fn hold(&mut self, id: EntityId, mv: Dir8, ticks: u32) {
        for _ in 0..ticks {
            self.input(id, Some(mv), None);
            self.world.step();
        }
    }

    pub fn press(&mut self, id: EntityId, act: Action) {
        self.input(id, None, Some(act));
    }

    pub fn run(&mut self, ticks: u32) {
        for _ in 0..ticks {
            self.world.step();
        }
    }

    pub fn enemy_of(&self, def: &str) -> EntityId {
        *self.world.enemies.iter().find(|(_, e)| e.def == def).map(|(id, _)| id).expect("enemy present")
    }

    /// Detach every enemy except `keep` so a test only deals with one.
    pub fn isolate(&mut self, keep: &[EntityId]) {
        self.world.enemies.retain(|id, _| keep.contains(id));
    }

    pub fn move_enemy(&mut self, id: EntityId, cx: i32, cy: i32) {
        let e = self.world.enemies.get_mut(&id).unwrap();
        e.body.pos = Pos::cell_center(cx, cy);
        e.home = e.body.pos;
    }

    pub fn walkable_run(&self, y: i32, from: i32, len: i32) -> bool {
        (from..from + len).all(|x| self.world.content.map.tile(x, y).walkable())
    }
}

/// An open stretch of the Gloam Plaza used as a neutral test arena.
pub const PLAZA: (i32, i32) = (66, 28);
