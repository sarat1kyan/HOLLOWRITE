mod common;

use common::{content, Sim, PLAZA};
use hollowrite_game_core::ai::Mode;
use hollowrite_game_core::world::{Encounter, Out};
use hollowrite_game_core::Pos;
use hollowrite_protocol::{Action, ActorState, Archetype, Attribute, Dir8, GameEvent, CELL};

#[test]
fn diagonal_and_orthogonal_speed_match() {
    let mut sim = Sim::new();
    let a = sim.join("Ortho", Archetype::GraveRunner);
    let b = sim.join("Diago", Archetype::GraveRunner);
    sim.place(a, 64, 22);
    sim.place(b, 64, 22);
    let start = sim.pos(a);
    for _ in 0..20 {
        sim.input(a, Some(Dir8::E), None);
        sim.input(b, Some(Dir8::Se), None);
        sim.world.step();
    }
    let da = sim.pos(a).dist(start);
    let db = sim.pos(b).dist(start);
    assert!(da > 2000, "player should have moved, got {da}");
    assert!((da - db).abs() <= 30, "orthogonal {da} vs diagonal {db}");
}

#[test]
fn walls_cannot_be_tunnelled_by_walking_or_rolling() {
    let mut sim = Sim::new();
    let id = sim.join("Wally", Archetype::GraveRunner);
    // Plaza east wall is at x = 80 (the gate is closed without the key).
    sim.place(id, 78, 24);
    sim.hold(id, Dir8::E, 90);
    for _ in 0..10 {
        sim.input(id, Some(Dir8::E), Some(Action::Roll));
        sim.run(20);
    }
    let (cx, _) = sim.pos(id).cell();
    assert!(cx <= 79, "walked through the locked gate to x={cx}");
    let map = &sim.world.content.map;
    let (x, y) = sim.pos(id).cell();
    assert!(map.tile(x, y).walkable());
}

#[test]
fn movement_is_locked_during_attack_frames() {
    let mut sim = Sim::new();
    let id = sim.join("Locke", Archetype::AshWarden);
    sim.isolate(&[]);
    sim.place(id, PLAZA.0, PLAZA.1);
    sim.input(id, None, Some(Action::Light));
    sim.world.step();
    assert_eq!(sim.world.players[&id].body.act.state, ActorState::Windup);
    let before = sim.pos(id);
    for _ in 0..15 {
        sim.input(id, Some(Dir8::E), None);
        sim.world.step();
        let s = sim.world.players[&id].body.act.state;
        if matches!(s, ActorState::Windup | ActorState::Active | ActorState::Recovery) {
            assert_eq!(sim.pos(id), before, "moved during {s:?}");
        }
    }
}

#[test]
fn actions_need_stamina_and_stamina_never_underflows() {
    let mut sim = Sim::new();
    let id = sim.join("Tired", Archetype::AshWarden);
    sim.isolate(&[]);
    sim.place(id, PLAZA.0, PLAZA.1);
    sim.world.players.get_mut(&id).unwrap().stamina = 5;
    sim.press(id, Action::Heavy);
    sim.world.step();
    assert_eq!(sim.world.players[&id].stamina, 0);
    sim.run(60);
    sim.world.players.get_mut(&id).unwrap().stamina = 0;
    sim.world.players.get_mut(&id).unwrap().stamina_delay = 100;
    sim.press(id, Action::Roll);
    sim.world.step();
    assert_ne!(sim.world.players[&id].body.act.state, ActorState::Rolling, "rolled with zero stamina");
}

#[test]
fn roll_invulnerability_only_on_configured_frames() {
    let mut sim = Sim::new();
    let id = sim.join("Rolly", Archetype::AshWarden);
    sim.isolate(&[]);
    sim.place(id, PLAZA.0, PLAZA.1);
    let roll = content().balance.rolls["standard"].clone();
    sim.press(id, Action::Roll);
    sim.world.step();
    let mut seen = Vec::new();
    for _ in 0..roll.ticks + 2 {
        let p = &sim.world.players[&id];
        if p.body.act.state == ActorState::Rolling {
            seen.push((p.body.act.t, sim.world.player_invulnerable(id)));
        }
        sim.world.step();
    }
    assert!(!seen.is_empty());
    for (t, inv) in seen {
        assert_eq!(inv, t >= roll.iframe_from && t < roll.iframe_to, "tick {t}");
    }
}

#[test]
fn light_attack_respects_windup_reach_and_line_of_sight() {
    let mut sim = Sim::new();
    let id = sim.join("Swing", Archetype::AshWarden);
    let k = sim.enemy_of("oathless_knight");
    sim.isolate(&[k]);
    sim.place(id, PLAZA.0, PLAZA.1);
    sim.move_enemy(k, PLAZA.0 + 1, PLAZA.1);
    sim.world.enemies.get_mut(&k).unwrap().body.act.state = ActorState::Staggered;
    sim.world.enemies.get_mut(&k).unwrap().body.act.dur = 500;
    let hp0 = sim.world.enemies[&k].body.hp;
    sim.world.players.get_mut(&id).unwrap().body.facing = Dir8::E;
    sim.press(id, Action::Light);
    let windup = content().items["rustbrand_sword"].weapon.as_ref().unwrap().light.windup;
    for _ in 0..windup {
        sim.world.step();
        assert_eq!(sim.world.enemies[&k].body.hp, hp0, "damage before windup completed");
    }
    sim.run(3);
    assert!(sim.world.enemies[&k].body.hp < hp0, "light attack did not land");

    // Out of reach: nothing happens.
    sim.run(40);
    let hp1 = sim.world.enemies[&k].body.hp;
    sim.move_enemy(k, PLAZA.0 + 4, PLAZA.1);
    sim.world.players.get_mut(&id).unwrap().body.facing = Dir8::E;
    sim.press(id, Action::Light);
    sim.run(40);
    assert_eq!(sim.world.enemies[&k].body.hp, hp1);
}

#[test]
fn attacks_do_not_pass_through_walls() {
    let mut sim = Sim::new();
    let id = sim.join("Blind", Archetype::AshWarden);
    let k = sim.enemy_of("oathless_knight");
    sim.isolate(&[k]);
    // Ossuary gate column x=80 is wall/closed door between the plaza (79) and hall (81).
    sim.place(id, 79, 22);
    sim.move_enemy(k, 81, 22);
    let e = sim.world.enemies.get_mut(&k).unwrap();
    e.body.act.state = ActorState::Staggered;
    e.body.act.dur = 500;
    let hp0 = e.body.hp;
    sim.world.players.get_mut(&id).unwrap().body.facing = Dir8::E;
    sim.press(id, Action::Heavy);
    sim.run(60);
    assert_eq!(sim.world.enemies[&k].body.hp, hp0);
}

#[test]
fn knight_aggroes_telegraphs_then_hits() {
    let mut sim = Sim::new();
    let id = sim.join("Bait", Archetype::AshWarden);
    let k = sim.enemy_of("oathless_knight");
    sim.isolate(&[k]);
    sim.place(id, PLAZA.0, PLAZA.1);
    sim.move_enemy(k, PLAZA.0 + 5, PLAZA.1);
    let hp0 = sim.world.players[&id].body.hp;
    let mut saw_telegraph_tick = None;
    let mut hit_tick = None;
    for t in 0..240 {
        sim.world.step();
        let snap = sim.world.snapshot_for(id, &mut Default::default()).unwrap();
        if saw_telegraph_tick.is_none() && !snap.tele.is_empty() {
            saw_telegraph_tick = Some(t);
        }
        if hit_tick.is_none() && sim.world.players[&id].body.hp < hp0 {
            hit_tick = Some(t);
            break;
        }
    }
    let tele = saw_telegraph_tick.expect("knight never telegraphed");
    let hit = hit_tick.expect("knight never hit");
    assert!(hit - tele >= 15, "hit landed {} ticks after telegraph", hit - tele);
}

#[test]
fn parry_inside_window_staggers_and_enables_critical() {
    let mut sim = Sim::new();
    let id = sim.join("Parry", Archetype::AshWarden);
    let k = sim.enemy_of("oathless_knight");
    sim.isolate(&[k]);
    sim.place(id, PLAZA.0, PLAZA.1);
    sim.move_enemy(k, PLAZA.0 + 1, PLAZA.1);
    // Wait for a cleave windup to begin, then parry so the window overlaps the active frame.
    let mut parried = false;
    for _ in 0..300 {
        sim.world.step();
        let e = &sim.world.enemies[&k];
        if e.body.act.state == ActorState::Windup {
            let w = e.windup.as_ref().unwrap();
            if w.attack.id == "cleave" && e.body.act.dur - e.body.act.t == 2 {
                sim.world.players.get_mut(&id).unwrap().body.facing = Dir8::E;
                sim.press(id, Action::Parry);
                sim.run(4);
                parried = sim.world.enemies[&k].riposte > 0;
                break;
            }
        }
    }
    assert!(parried, "parry did not register");
    let hp_player = sim.world.players[&id].body.hp;
    assert_eq!(hp_player, sim.world.players[&id].body.hp_max, "parried hit still dealt damage");
    let hp0 = sim.world.enemies[&k].body.hp;
    sim.run(20);
    sim.world.players.get_mut(&id).unwrap().body.facing = Dir8::E;
    sim.press(id, Action::Light);
    sim.run(12);
    let dealt = hp0 - sim.world.enemies[&k].body.hp;
    let normal = content().items["rustbrand_sword"].weapon.as_ref().unwrap().damage;
    assert!(dealt > normal * 2, "riposte dealt only {dealt}");
}

#[test]
fn early_parry_fails() {
    let mut sim = Sim::new();
    let id = sim.join("Early", Archetype::AshWarden);
    let k = sim.enemy_of("oathless_knight");
    sim.isolate(&[k]);
    sim.place(id, PLAZA.0, PLAZA.1);
    sim.move_enemy(k, PLAZA.0 + 1, PLAZA.1);
    let mut hurt = false;
    for _ in 0..300 {
        sim.world.step();
        let e = &sim.world.enemies[&k];
        if e.body.act.state == ActorState::Windup && e.body.act.t == 1 {
            let windup = e.windup.as_ref().unwrap().attack.windup as u32;
            sim.world.players.get_mut(&id).unwrap().body.facing = Dir8::E;
            sim.press(id, Action::Parry);
            sim.run(windup + 3);
            hurt = sim.world.players[&id].body.hp < sim.world.players[&id].body.hp_max;
            break;
        }
    }
    assert!(hurt, "a parry 15 ticks early should not protect");
    assert_eq!(sim.world.enemies[&k].riposte, 0);
}

#[test]
fn enemies_leash_home_and_heal() {
    let mut sim = Sim::new();
    let id = sim.join("Kite", Archetype::GraveRunner);
    let k = sim.enemy_of("oathless_knight");
    sim.isolate(&[k]);
    sim.place(id, PLAZA.0, PLAZA.1);
    sim.move_enemy(k, PLAZA.0 + 4, PLAZA.1);
    sim.run(5);
    assert_eq!(sim.world.enemies[&k].mode, Mode::Chase);
    sim.world.enemies.get_mut(&k).unwrap().body.hp = 50;
    // Teleport the knight far beyond its leash to force the return.
    let e = sim.world.enemies.get_mut(&k).unwrap();
    e.body.pos = Pos::cell_center(50, 23);
    assert!(!e.body.pos.within(e.home, 16 * CELL));
    sim.place(id, 40, 23);
    sim.run(5);
    assert_eq!(sim.world.enemies[&k].mode, Mode::Return);
    sim.run(600);
    let e = &sim.world.enemies[&k];
    assert_eq!(e.mode, Mode::Idle);
    assert_eq!(e.body.hp, e.body.hp_max);
}

#[test]
fn kill_rewards_each_contributor_exactly_once() {
    let mut sim = Sim::new();
    let a = sim.join("Alice", Archetype::AshWarden);
    let b = sim.join("Bob", Archetype::GraveRunner);
    let k = sim.enemy_of("oathless_knight");
    sim.isolate(&[k]);
    sim.place(a, PLAZA.0, PLAZA.1);
    sim.place(b, PLAZA.0 + 2, PLAZA.1);
    sim.move_enemy(k, PLAZA.0 + 1, PLAZA.1);
    let e = sim.world.enemies.get_mut(&k).unwrap();
    e.body.act.state = ActorState::Staggered;
    e.body.act.dur = 900;
    sim.world.players.get_mut(&a).unwrap().body.facing = Dir8::E;
    sim.world.players.get_mut(&b).unwrap().body.facing = Dir8::W;
    let reward = content().enemies["oathless_knight"].echoes;
    for _ in 0..30 {
        sim.press(a, Action::Light);
        sim.press(b, Action::Light);
        sim.run(25);
        if sim.world.enemies[&k].mode == Mode::Dead {
            break;
        }
    }
    assert_eq!(sim.world.enemies[&k].mode, Mode::Dead);
    sim.run(100);
    assert_eq!(sim.world.players[&a].data.echoes, reward);
    assert_eq!(sim.world.players[&b].data.echoes, reward);
    let kills: usize = sim
        .world
        .drain()
        .iter()
        .filter(|o| matches!(o, Out::Save(s) if s.ledger.iter().any(|l| l.kind == "kill")))
        .count();
    assert!(kills <= 2);
}

#[test]
fn death_drops_remnant_and_reclaim_restores_without_duplication() {
    let mut sim = Sim::new();
    let id = sim.join("Mortal", Archetype::AshWarden);
    sim.isolate(&[]);
    sim.place(id, PLAZA.0, PLAZA.1);
    sim.world.players.get_mut(&id).unwrap().data.echoes = 500;
    sim.world.kill_player(id);
    let p = &sim.world.players[&id];
    assert_eq!(p.data.echoes, 0);
    let rem = p.data.remnant.expect("remnant dropped");
    assert_eq!(rem.amount, 500);
    sim.run(content().balance.dead_ticks as u32 + 2);
    let p = &sim.world.players[&id];
    assert_eq!(p.body.act.state, ActorState::Idle);
    assert_eq!(p.body.hp, p.body.hp_max);
    let lantern = content().map.lantern("last_lantern").unwrap().clone();
    assert_eq!(p.body.pos.cell(), (lantern.x + 1, lantern.y));

    // Walking onto the remnant twice in the same tick pays once.
    let rp = rem.pos;
    sim.world.players.get_mut(&id).unwrap().body.pos = rp;
    sim.world.step();
    sim.world.step();
    assert_eq!(sim.world.players[&id].data.echoes, 500);
    assert!(sim.world.players[&id].data.remnant.is_none());
}

#[test]
fn dying_before_reclaim_loses_the_old_remnant() {
    let mut sim = Sim::new();
    let id = sim.join("Unlucky", Archetype::AshWarden);
    sim.isolate(&[]);
    sim.place(id, PLAZA.0, PLAZA.1);
    sim.world.players.get_mut(&id).unwrap().data.echoes = 300;
    sim.world.kill_player(id);
    sim.run(content().balance.dead_ticks as u32 + 2);
    sim.world.players.get_mut(&id).unwrap().data.echoes = 40;
    sim.world.kill_player(id);
    let p = &sim.world.players[&id];
    assert_eq!(p.data.remnant.unwrap().amount, 40);
    assert_eq!(p.data.echoes, 0);
    let lost: Vec<_> = sim
        .world
        .drain()
        .into_iter()
        .filter(|o| matches!(o, Out::To(_, GameEvent::EchoesLost { amount: 300 })))
        .collect();
    assert_eq!(lost.len(), 1);
}

#[test]
fn stale_and_duplicate_inputs_are_ignored() {
    let mut sim = Sim::new();
    let id = sim.join("Dupe", Archetype::AshWarden);
    assert!(sim.world.input(id, 5, Some(Dir8::E), None, None));
    assert!(!sim.world.input(id, 5, Some(Dir8::E), Some(Action::Heavy), None));
    assert!(!sim.world.input(id, 3, Some(Dir8::W), None, None));
    assert_eq!(sim.world.players[&id].pending, None);
    assert_eq!(sim.world.last_seq(id), 5);
}

#[test]
fn map_pickups_are_awarded_once_per_character() {
    let mut sim = Sim::new();
    let a = sim.join("Finder", Archetype::AshWarden);
    let b = sim.join("Second", Archetype::AshWarden);
    let pickup = sim.world.pickups.values().find(|p| p.item == "sexton_key").unwrap().clone();
    let (cx, cy) = pickup.pos.cell();
    sim.place(a, cx, cy);
    sim.place(b, cx, cy);
    sim.world.step();
    sim.world.step();
    sim.run(5);
    assert_eq!(sim.world.players[&a].data.inventory.get("sexton_key"), Some(&1));
    assert_eq!(sim.world.players[&b].data.inventory.get("sexton_key"), Some(&1));
    sim.place(a, cx, cy + 1);
    sim.run(2);
    sim.place(a, cx, cy);
    sim.run(2);
    assert_eq!(sim.world.players[&a].data.inventory.get("sexton_key"), Some(&1));
    assert!(sim.world.players[&a].data.flags.contains("got:sexton_key"));
}

#[test]
fn locked_gate_needs_the_key() {
    let mut sim = Sim::new();
    let id = sim.join("Gatekeeper", Archetype::AshWarden);
    sim.isolate(&[]);
    sim.place(id, 79, 24);
    sim.press(id, Action::Interact);
    sim.run(2);
    assert!(!sim.world.players[&id].data.flags.contains("door:ossuary_gate"));
    sim.hold(id, Dir8::E, 30);
    assert!(sim.pos(id).cell().0 <= 79);
    sim.world.players.get_mut(&id).unwrap().data.inventory.insert("sexton_key".into(), 1);
    sim.place(id, 79, 24);
    sim.press(id, Action::Interact);
    sim.run(2);
    assert!(sim.world.players[&id].data.flags.contains("door:ossuary_gate"));
    sim.hold(id, Dir8::E, 30);
    assert!(sim.pos(id).cell().0 >= 81, "still at {:?}", sim.pos(id).cell());
}

#[test]
fn oneway_shortcut_opens_only_from_its_side() {
    let mut sim = Sim::new();
    let id = sim.join("Shorty", Archetype::AshWarden);
    sim.isolate(&[]);
    // West side (crossroads) of the tunnel door at (45, 23).
    sim.place(id, 44, 23);
    sim.press(id, Action::Interact);
    sim.run(2);
    assert!(!sim.world.players[&id].data.flags.contains("door:tunnel_shortcut"));
    sim.place(id, 46, 23);
    sim.press(id, Action::Interact);
    sim.run(2);
    assert!(sim.world.players[&id].data.flags.contains("door:tunnel_shortcut"));
}

#[test]
fn level_up_requires_lantern_and_charges_echoes() {
    let mut sim = Sim::new();
    let id = sim.join("Grower", Archetype::AshWarden);
    sim.place(id, PLAZA.0, PLAZA.1);
    sim.world.players.get_mut(&id).unwrap().data.echoes = 10_000;
    assert!(sim.world.level_up(id, Attribute::Vitality).is_err());
    let l = content().map.lantern("last_lantern").unwrap().clone();
    sim.place(id, l.x + 1, l.y);
    let cost = sim.world.players[&id].data.next_level_cost(&content());
    let hp_before = sim.world.players[&id].body.hp_max;
    sim.world.level_up(id, Attribute::Vitality).unwrap();
    let p = &sim.world.players[&id];
    assert_eq!(p.data.echoes, 10_000 - cost);
    assert_eq!(p.data.level(&content()), 2);
    assert!(p.body.hp_max > hp_before);
    sim.world.players.get_mut(&id).unwrap().data.echoes = 0;
    assert!(sim.world.level_up(id, Attribute::Might).is_err());
}

#[test]
fn healing_is_committed_and_interruptible() {
    let mut sim = Sim::new();
    let id = sim.join("Drinker", Archetype::AshWarden);
    sim.isolate(&[]);
    sim.place(id, PLAZA.0, PLAZA.1);
    let max = sim.world.players[&id].body.hp_max;
    sim.world.players.get_mut(&id).unwrap().body.hp = 100;
    sim.press(id, Action::Heal);
    sim.run(5);
    assert_eq!(sim.world.players[&id].body.act.state, ActorState::Healing);
    assert_eq!(sim.world.players[&id].body.hp, 100, "healed before the drink landed");
    sim.run(40);
    let p = &sim.world.players[&id];
    assert!(p.body.hp > 100 && p.body.hp <= max);
    assert_eq!(p.data.draughts, p.data.draughts_max - 1);
}

#[test]
fn boss_scales_with_participants_changes_phase_and_resets() {
    let mut sim = Sim::new();
    let a = sim.join("Alpha", Archetype::AshWarden);
    let b = sim.join("Beta", Archetype::VeilSeer);
    let boss = sim.world.boss.unwrap();
    let arena = content().map.arena().unwrap().clone();
    let base = content().enemies["bell_mother"].hp;
    sim.place(a, arena.x + 1, arena.y + 11);
    sim.place(b, arena.x + 1, arena.y + 12);
    sim.world.step();
    let expect = hollowrite_game_core::World::scaled_boss_hp(&content(), base, 2);
    assert_eq!(sim.world.enemies[&boss].body.hp_max, expect);
    assert!(expect > base);
    sim.run(content().balance.boss.join_grace_ticks + 2);
    assert!(sim.world.arena_sealed());

    // Drop the boss below the threshold and observe the phase change.
    let e = sim.world.enemies.get_mut(&boss).unwrap();
    e.body.hp = e.body.hp_max / 2 - 1;
    sim.world.step();
    assert_eq!(sim.world.enemies[&boss].boss.as_ref().unwrap().phase, 2);
    let hp = sim.world.enemies[&boss].body.hp;
    sim.world.players.get_mut(&a).unwrap().body.pos = sim.world.enemies[&boss].body.pos.offset(Dir8::W, 1500);
    sim.world.players.get_mut(&a).unwrap().body.facing = Dir8::E;
    sim.press(a, Action::Heavy);
    sim.run(30);
    assert_eq!(sim.world.enemies[&boss].body.hp, hp, "boss took damage during its transition");

    sim.world.kill_player(a);
    sim.world.kill_player(b);
    sim.run(content().balance.boss.reset_delay_ticks + 5);
    assert_eq!(sim.world.encounter, Encounter::Dormant);
    let e = &sim.world.enemies[&boss];
    assert_eq!(e.body.hp, base);
    assert_eq!(e.boss.as_ref().unwrap().phase, 1);
    assert!(!sim.world.arena_sealed());
}

#[test]
fn boss_phase_two_peal_creates_rings_then_vulnerability() {
    let mut sim = Sim::new();
    let a = sim.join("Solo", Archetype::GraveRunner);
    let boss = sim.world.boss.unwrap();
    let arena = content().map.arena().unwrap().clone();
    sim.place(a, arena.x + 1, arena.y + 11);
    sim.world.step();
    let e = sim.world.enemies.get_mut(&boss).unwrap();
    e.body.hp = e.body.hp_max / 2 - 1;
    let p2 = content().enemies["bell_mother"].phase2.clone().unwrap();
    let mut saw_ring = false;
    let mut saw_vulnerable = false;
    for _ in 0..1500 {
        // Keep the player alive so the encounter continues.
        let p = sim.world.players.get_mut(&a).unwrap();
        p.body.hp = p.body.hp_max;
        sim.world.step();
        saw_ring |= !sim.world.rings.is_empty();
        if sim.world.enemies[&boss].boss.as_ref().unwrap().vulnerable_until > sim.world.tick {
            saw_vulnerable = true;
            break;
        }
    }
    assert!(saw_ring, "no shockwave rings appeared");
    assert!(saw_vulnerable, "no vulnerability window after the peal");
    assert!(p2.vulnerable_mult_pct > 100);
}

#[test]
fn boss_defeat_rewards_participants_once() {
    let mut sim = Sim::new();
    let a = sim.join("Victor", Archetype::AshWarden);
    let boss = sim.world.boss.unwrap();
    let arena = content().map.arena().unwrap().clone();
    sim.place(a, arena.x + 1, arena.y + 11);
    sim.world.step();
    let pos = sim.world.enemies[&boss].body.pos;
    let p = sim.world.players.get_mut(&a).unwrap();
    p.body.pos = pos.offset(Dir8::W, 1500);
    p.body.facing = Dir8::E;
    let e = sim.world.enemies.get_mut(&boss).unwrap();
    e.body.hp = 1;
    e.boss.as_mut().unwrap().phase = 2;
    e.body.act.state = ActorState::Staggered;
    e.body.act.dur = 300;
    sim.press(a, Action::Light);
    sim.run(30);
    let reward = content().enemies["bell_mother"].echoes;
    let p = &sim.world.players[&a];
    assert_eq!(p.data.echoes, reward);
    assert!(p.data.flags.contains("slain:bell_mother"));
    assert_eq!(p.data.inventory.get("clapper_of_names"), Some(&1));
    assert!(matches!(sim.world.encounter, Encounter::Cleared { .. }));
    sim.run(100);
    assert_eq!(sim.world.players[&a].data.echoes, reward);
}

#[test]
fn equip_validates_slot_and_ownership() {
    let mut sim = Sim::new();
    let id = sim.join("Gear", Archetype::AshWarden);
    use hollowrite_protocol::EquipSlot;
    assert!(sim.world.equip(id, EquipSlot::Main, Some("penitent_maul".into())).is_err());
    assert!(sim.world.equip(id, EquipSlot::Armor, Some("rustbrand_sword".into())).is_err());
    sim.world.players.get_mut(&id).unwrap().data.inventory.insert("penitent_maul".into(), 1);
    sim.world.equip(id, EquipSlot::Main, Some("penitent_maul".into())).unwrap();
    assert_eq!(sim.world.players[&id].data.equipment.main.as_deref(), Some("penitent_maul"));
    sim.world.players.get_mut(&id).unwrap().data.inventory.insert("hollow_toll".into(), 1);
    assert!(sim.world.equip(id, EquipSlot::Spell1, Some("hollow_toll".into())).is_err(), "occult requirement ignored");
}

#[test]
fn snapshot_interest_management_hides_far_entities() {
    let mut sim = Sim::new();
    let a = sim.join("Near", Archetype::AshWarden);
    let b = sim.join("Far", Archetype::AshWarden);
    sim.place(a, 10, 23);
    sim.place(b, 115, 24);
    let mut known = Default::default();
    let snap = sim.world.snapshot_for(a, &mut known).unwrap();
    assert!(snap.entities.iter().any(|e| e.id == a));
    assert!(!snap.entities.iter().any(|e| e.id == b));
    assert!(!snap.entities.iter().any(|e| e.kind == hollowrite_protocol::EntityKind::Boss));
    sim.place(b, 12, 23);
    let snap = sim.world.snapshot_for(a, &mut known).unwrap();
    assert!(snap.entities.iter().any(|e| e.id == b));
    sim.place(b, 115, 24);
    let snap = sim.world.snapshot_for(a, &mut known).unwrap();
    assert!(snap.gone.contains(&b));
}

#[test]
fn simulation_is_deterministic_for_a_seed() {
    let run = || {
        let mut sim = Sim::new();
        let id = sim.join("Det", Archetype::GraveRunner);
        sim.place(id, 40, 10);
        for i in 0..600u32 {
            let d = Dir8::ALL[(i / 37) as usize % 8];
            let act = if i % 23 == 0 {
                Some(Action::Light)
            } else if i % 41 == 0 {
                Some(Action::Roll)
            } else {
                None
            };
            sim.input(id, Some(d), act);
            sim.world.step();
        }
        let p = &sim.world.players[&id];
        let enemies: Vec<_> = sim.world.enemies.values().map(|e| (e.body.pos, e.body.hp)).collect();
        (p.body.pos, p.body.hp, p.data.echoes, enemies)
    };
    assert_eq!(run(), run());
}
