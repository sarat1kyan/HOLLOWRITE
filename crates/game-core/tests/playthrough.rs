//! Walks a character through the whole first-session path using only
//! inputs, dialogue and interaction, proving the authored content chain can
//! be completed. Enemies are removed: combat has its own tests.

mod common;

use common::Sim;
use hollowrite_game_core::world::Encounter;
use hollowrite_game_core::Pos;
use hollowrite_protocol::{Action, ActorState, Archetype, EntityId};

fn cell_of(sim: &Sim, id: EntityId) -> (i32, i32) {
    sim.pos(id).cell()
}

/// Steers along a breadth-first path, re-planning every step.
fn walk_to(sim: &mut Sim, id: EntityId, target: (i32, i32)) {
    for _ in 0..6000 {
        let here = cell_of(sim, id);
        if here == target {
            sim.input(id, None, None);
            sim.world.step();
            return;
        }
        let flags = sim.world.players[&id].data.flags.clone();
        let next = sim
            .world
            .content
            .map
            .next_step(here, target, 50_000, |x, y| sim.world.player_passable(&flags, x, y))
            .unwrap_or_else(|| panic!("no path from {here:?} to {target:?}"));
        let p = sim.pos(id);
        let goal = Pos::cell_center(next.0, next.1);
        let d = p.dir_to(goal).unwrap_or(hollowrite_protocol::Dir8::S);
        sim.input(id, Some(d), None);
        sim.world.step();
    }
    let here = cell_of(sim, id);
    let flags = sim.world.players[&id].data.flags.clone();
    let next = sim.world.content.map.next_step(here, target, 50_000, |x, y| sim.world.player_passable(&flags, x, y));
    panic!("walking to {target:?} took too long; stuck at {:?} pos {:?} next {next:?}", here, sim.pos(id));
}

fn npc(sim: &Sim, def: &str) -> (EntityId, (i32, i32)) {
    let n = sim.world.npcs.values().find(|n| n.def == def).unwrap();
    (n.id, n.pos.cell())
}

fn talk(sim: &mut Sim, id: EntityId, def: &str, choice: Option<u8>) {
    let (npc_id, (x, y)) = npc(sim, def);
    if !sim.world.players[&id].body.pos.within(Pos::cell_center(x, y), 1500) {
        walk_to(sim, id, (x + 1, y));
    }
    sim.world.talk(id, npc_id, None).unwrap_or_else(|e| panic!("talk to {def}: {e:?}"));
    if let Some(c) = choice {
        sim.world.talk(id, npc_id, Some(c)).unwrap_or_else(|e| panic!("choose {c} with {def}: {e:?}"));
    }
}

fn interact(sim: &mut Sim, id: EntityId) {
    sim.press(id, Action::Interact);
    sim.run(3);
    sim.run(10);
}

fn has(sim: &Sim, id: EntityId, flag: &str) -> bool {
    sim.world.players[&id].data.flags.contains(flag)
}

#[test]
fn the_first_session_can_be_completed() {
    let mut sim = Sim::new();
    sim.isolate(&[sim.world.boss.unwrap()]);
    let id = sim.join("Walker", Archetype::AshWarden);
    let map = sim.world.content.map.clone();
    let lantern = |id: &str| {
        let l = map.lantern(id).unwrap();
        (l.x, l.y)
    };

    talk(&mut sim, id, "hesper", None);
    assert!(has(&sim, id, "q:light_plaza"));

    let (gx, gy) = lantern("gloam_lantern");
    walk_to(&mut sim, id, (gx + 1, gy));
    interact(&mut sim, id);
    assert!(has(&sim, id, "lit:gloam_lantern"), "plaza lantern was not lit");
    assert_eq!(sim.world.players[&id].data.checkpoint, "gloam_lantern");

    talk(&mut sim, id, "hesper", None);
    assert!(has(&sim, id, "q:light_plaza_done"));
    talk(&mut sim, id, "ansel", None);
    assert!(has(&sim, id, "q:sexton"));

    let key = sim.world.pickups.values().find(|p| p.item == "sexton_key").unwrap().pos.cell();
    walk_to(&mut sim, id, key);
    sim.run(2);
    assert!(has(&sim, id, "got:sexton_key"), "key not picked up");

    walk_to(&mut sim, id, (79, 24));
    interact(&mut sim, id);
    assert!(has(&sim, id, "door:ossuary_gate"), "gate did not open");

    let (sx, sy) = lantern("sexton_lantern");
    walk_to(&mut sim, id, (sx - 1, sy));
    interact(&mut sim, id);
    assert!(has(&sim, id, "lit:sexton_lantern"));

    // Inner shortcut opens from the lantern side back to the entrance hall.
    walk_to(&mut sim, id, (94, 24));
    interact(&mut sim, id);
    assert!(has(&sim, id, "door:ossuary_shortcut"), "ossuary shortcut did not open");

    let arena = map.arena().unwrap().clone();
    walk_to(&mut sim, id, (arena.x + 1, 24));
    sim.run(2);
    assert!(matches!(sim.world.encounter, Encounter::Active { .. }), "entering the belfry should wake her");
    let boss = sim.world.boss.unwrap();
    let bpos = sim.world.enemies[&boss].body.pos;
    let p = sim.world.players.get_mut(&id).unwrap();
    p.body.pos = bpos.offset(hollowrite_protocol::Dir8::W, 1500);
    p.body.facing = hollowrite_protocol::Dir8::E;
    let e = sim.world.enemies.get_mut(&boss).unwrap();
    e.body.hp = 1;
    e.boss.as_mut().unwrap().phase = 2;
    e.body.act.state = ActorState::Staggered;
    e.body.act.dur = 200;
    sim.press(id, Action::Light);
    sim.run(30);
    assert!(has(&sim, id, "slain:bell_mother"));
    assert_eq!(sim.world.players[&id].data.inventory.get("clapper_of_names"), Some(&1));

    // Rest at the Sexton's lantern, then take the short way home.
    walk_to(&mut sim, id, (sx - 1, sy));
    interact(&mut sim, id);
    talk(&mut sim, id, "ansel", Some(0));
    assert!(has(&sim, id, "clapper_given"));
    assert!(has(&sim, id, "rep:pale_synod"));
    let sheet = sim.world.sheet(id).unwrap();
    assert!(sheet.quests.iter().all(|q| q.done), "quests left open: {:?}", sheet.quests);
}

#[test]
fn the_secret_grove_rewards_the_curious() {
    let mut sim = Sim::new();
    sim.isolate(&[]);
    let id = sim.join("Curious", Archetype::GraveRunner);
    let wall = sim.world.content.map.doors.iter().find(|d| d.id == "grove_wall").unwrap().clone();
    walk_to(&mut sim, id, (wall.x, wall.y - 1));
    interact(&mut sim, id);
    assert!(has(&sim, id, "door:grove_wall"), "false wall did not give way");
    let maul = sim.world.pickups.values().find(|p| p.item == "penitent_maul").unwrap().pos.cell();
    walk_to(&mut sim, id, maul);
    sim.run(2);
    assert_eq!(sim.world.players[&id].data.inventory.get("penitent_maul"), Some(&1));
    let before = sim.world.players[&id].data.draughts_max;
    talk(&mut sim, id, "moth", None);
    assert_eq!(sim.world.players[&id].data.draughts_max, before + 1);
    talk(&mut sim, id, "moth", None);
    assert_eq!(sim.world.players[&id].data.draughts_max, before + 1, "Moth's gift must not repeat");
}

#[test]
fn the_tunnel_shortcut_is_usable_once_opened_from_the_plaza() {
    let mut sim = Sim::new();
    sim.isolate(&[]);
    let id = sim.join("Shortcut", Archetype::GraveRunner);
    walk_to(&mut sim, id, (46, 23));
    interact(&mut sim, id);
    assert!(has(&sim, id, "door:tunnel_shortcut"));
    walk_to(&mut sim, id, (40, 23));
    let start = sim.world.tick;
    walk_to(&mut sim, id, (63, 23));
    let ticks = sim.world.tick - start;
    assert!(ticks < 30 * 10, "the tunnel should take under ten seconds, took {ticks} ticks");
}
