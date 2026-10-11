mod common;

use std::collections::{BTreeSet, HashSet, VecDeque};

use common::content;
use hollowrite_game_core::map::{DoorRule, SpawnKind};
use hollowrite_game_core::world::World;
use hollowrite_protocol::Tile;

fn reachable(world: &World, flags: &BTreeSet<String>, start: (i32, i32)) -> HashSet<(i32, i32)> {
    let mut seen = HashSet::new();
    let mut q = VecDeque::new();
    seen.insert(start);
    q.push_back(start);
    while let Some((x, y)) = q.pop_front() {
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let n = (x + dx, y + dy);
            if !seen.contains(&n) && world.player_passable(flags, n.0, n.1) {
                seen.insert(n);
                q.push_back(n);
            }
        }
    }
    seen
}

fn all_doors_open(world: &World) -> BTreeSet<String> {
    world.content.map.doors.iter().map(|d| format!("door:{}", d.id)).collect()
}

#[test]
fn every_point_of_interest_is_reachable_with_doors_open() {
    let c = content();
    let world = World::new(c.clone(), 1);
    let start = c.map.lantern(&c.map.start_lantern).unwrap();
    let reach = reachable(&world, &all_doors_open(&world), (start.x + 1, start.y));
    for l in &c.map.lanterns {
        let near = [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(dx, dy)| reach.contains(&(l.x + dx, l.y + dy)));
        assert!(near, "lantern {} unreachable", l.id);
    }
    for s in &c.map.spawns {
        assert!(reach.contains(&(s.x, s.y)), "spawn {:?} at {},{} unreachable", s.what, s.x, s.y);
    }
}

#[test]
fn gates_and_secrets_actually_gate() {
    let c = content();
    let world = World::new(c.clone(), 1);
    let start = c.map.lantern(&c.map.start_lantern).unwrap();
    let base = reachable(&world, &BTreeSet::new(), (start.x + 1, start.y));
    let boss = c.map.spawns.iter().find(|s| s.what == SpawnKind::Enemy("bell_mother".into())).unwrap();
    assert!(!base.contains(&(boss.x, boss.y)), "boss reachable without the Ossuary key");
    let moth = c.map.spawns.iter().find(|s| s.what == SpawnKind::Npc("moth".into())).unwrap();
    assert!(!base.contains(&(moth.x, moth.y)), "secret grove reachable without finding the wall");
    let plaza = c.map.lantern("gloam_lantern").unwrap();
    assert!(base.contains(&(plaza.x + 1, plaza.y)), "plaza must be reachable from the start");
}

#[test]
fn tunnel_shortcut_shortens_the_route() {
    let c = content();
    let world = World::new(c.clone(), 1);
    let start = c.map.lantern(&c.map.start_lantern).unwrap();
    let plaza = c.map.lantern("gloam_lantern").unwrap();
    let route = |flags: &BTreeSet<String>| {
        let mut dist = std::collections::HashMap::new();
        let mut q = VecDeque::new();
        let s = (start.x + 1, start.y);
        dist.insert(s, 0);
        q.push_back(s);
        while let Some((x, y)) = q.pop_front() {
            let d = dist[&(x, y)];
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let n = (x + dx, y + dy);
                if !dist.contains_key(&n) && world.player_passable(flags, n.0, n.1) {
                    dist.insert(n, d + 1);
                    q.push_back(n);
                }
            }
        }
        dist[&(plaza.x + 1, plaza.y)]
    };
    let long = route(&BTreeSet::new());
    let mut flags = BTreeSet::new();
    flags.insert("door:tunnel_shortcut".to_string());
    let short = route(&flags);
    assert!(short * 4 < long * 3, "shortcut {short} vs long way {long}");
    let tunnel = c.map.zone("tunnel").unwrap();
    let crossroads = c.map.zone("crossroads").unwrap();
    for s in &c.map.spawns {
        if matches!(s.what, SpawnKind::Enemy(_)) {
            assert!(
                !tunnel.contains(s.x, s.y) && !crossroads.contains(s.x, s.y),
                "the shortcut should be a safe route"
            );
        }
    }
}

#[test]
fn map_has_required_features() {
    let c = content();
    let m = &c.map;
    assert_eq!(m.lanterns.len(), 3);
    assert!(m.arena().is_some());
    let enemy_kinds: BTreeSet<_> = m
        .spawns
        .iter()
        .filter_map(|s| match &s.what {
            SpawnKind::Enemy(e) => Some(e.clone()),
            _ => None,
        })
        .collect();
    for e in ["oathless_knight", "ashen_pilgrim", "starveil_herald", "bell_mother"] {
        assert!(enemy_kinds.contains(e), "missing {e}");
    }
    assert!(m.doors.iter().any(|d| d.rule == DoorRule::Secret));
    assert!(m.doors.iter().any(|d| matches!(d.rule, DoorRule::Oneway { .. })));
    assert!(m.doors.iter().any(|d| d.rule == DoorRule::Fog));
    let safe = m.zones.iter().find(|z| z.safe).unwrap();
    for s in &m.spawns {
        if matches!(s.what, SpawnKind::Enemy(_)) {
            assert!(!safe.contains(s.x, s.y), "enemy spawned in the sanctuary");
        }
    }
    // The map border must be closed so nobody can walk off the edge.
    for x in 0..m.width {
        assert!(!m.tile(x, 0).walkable() && !m.tile(x, m.height - 1).walkable());
    }
    for y in 0..m.height {
        assert!(!m.tile(0, y).walkable() && !m.tile(m.width - 1, y).walkable());
    }
    assert_eq!(m.tile(-1, -1), Tile::Void);
}

#[test]
fn dialogue_effects_cannot_be_farmed() {
    // A node that grants something must forbid a flag it sets, otherwise
    // talking repeatedly would mint items or echoes.
    let c = content();
    for npc in c.npcs.values() {
        for (i, node) in npc.dialogue.iter().enumerate() {
            let fx = &node.effects;
            let grants = !fx.give.is_empty() || fx.echoes > 0 || fx.draughts_max > 0;
            if grants {
                assert!(
                    fx.set.iter().any(|f| node.when.forbids.contains(f)),
                    "{} node {i} grants rewards without a once-only guard",
                    npc.id
                );
            }
            for choice in &node.choices {
                let fx = &choice.effects;
                let grants = !fx.give.is_empty() || fx.echoes > 0 || fx.draughts_max > 0;
                if grants {
                    assert!(
                        fx.set.iter().any(|f| node.when.forbids.contains(f)),
                        "{} node {i} choice '{}' grants rewards without a once-only guard",
                        npc.id,
                        choice.label
                    );
                }
            }
        }
    }
}

#[test]
fn level_costs_grow() {
    let c = content();
    let mut last = 0;
    for lvl in 1..30 {
        let cost = hollowrite_game_core::character::level_cost(&c, lvl);
        assert!(cost > last);
        last = cost;
    }
}
