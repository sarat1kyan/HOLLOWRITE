//! Balance smoke tests: a simple scripted fighter (dodge what is telegraphed
//! at you, strike when the foe is open) must be able to beat each regular
//! enemy at level 1, while a fighter that ignores telegraphs should get hurt.
//! These guard against tuning changes that make the slice unwinnable or
//! trivial; they are not a substitute for playtesting.

mod common;

use common::{Sim, PLAZA};
use hollowrite_game_core::ai::Mode;
use hollowrite_protocol::{Action, ActorState, Archetype, Dir8};

struct Outcome {
    won: bool,
    hp_left_pct: u32,
    seconds: f32,
}

fn duel(enemy: &str, archetype: Archetype, careful: bool, seed_offset: i32) -> Outcome {
    let mut sim = Sim::new();
    let id = sim.join("Duelist", archetype);
    let e = sim.enemy_of(enemy);
    sim.isolate(&[e]);
    sim.place(id, PLAZA.0, PLAZA.1);
    sim.move_enemy(e, PLAZA.0 + 4 + seed_offset % 2, PLAZA.1 - seed_offset % 2);
    for _ in 0..(seed_offset * 7) {
        sim.world.step();
    }
    let start = sim.world.tick;
    let weapon = sim.world.players[&id].data.equipment.main.clone().unwrap();
    let reach = sim.world.content.weapon(&weapon).unwrap().reach;
    for _ in 0..(30 * 120) {
        let p = &sim.world.players[&id];
        if p.body.act.state == ActorState::Dead {
            break;
        }
        let foe = &sim.world.enemies[&e];
        if foe.mode == Mode::Dead {
            break;
        }
        let me = p.body.pos;
        let them = foe.body.pos;
        let toward = me.dir_to(them).unwrap_or(Dir8::E);
        let dist = me.dist(them);
        let my_cell = [me.cell().0 as i16, me.cell().1 as i16];
        let threatened = foe.windup.as_ref().is_some_and(|w| w.cells.contains(&my_cell))
            && foe.body.act.state == ActorState::Windup
            && foe.body.act.dur - foe.body.act.t <= 6;
        let projectile_close = sim.world.projectiles.values().any(|pr| {
            let (dx, dy) = ((me.x - pr.pos.x) as i64, (me.y - pr.pos.y) as i64);
            pr.pos.within(me, 1500) && dx * pr.vx as i64 + dy * pr.vy as i64 > 0
        });
        let stamina = p.stamina;
        let hp_frac = p.body.hp * 100 / p.body.hp_max;
        let free = p.body.act.is_free();
        let (mv, act): (Option<Dir8>, Option<Action>) = if careful && (threatened || projectile_close) && stamina >= 20
        {
            let away = sideways(toward);
            (Some(away), Some(Action::Roll))
        } else if careful && hp_frac < 40 && p.data.draughts > 0 && dist > 2500 && free {
            (None, Some(Action::Heal))
        } else if dist <= reach + foe.body.radius {
            let open = !careful || foe.body.act.state != ActorState::Windup || foe.riposte > 0;
            if open && stamina >= 20 && free {
                (None, Some(Action::Light))
            } else {
                (None, None)
            }
        } else {
            let flags = p.data.flags.clone();
            let step = sim
                .world
                .content
                .map
                .next_step(me.cell(), them.cell(), 4000, |x, y| sim.world.player_passable(&flags, x, y));
            let d = step.and_then(|(x, y)| me.dir_to(hollowrite_game_core::Pos::cell_center(x, y))).unwrap_or(toward);
            (Some(d), None)
        };
        if std::env::var("DUEL_TRACE").is_ok() && sim.world.tick.is_multiple_of(30) {
            eprintln!(
                "t={} d={} me={:?} {:?} st={} hp={} | foe {:?} {:?} {:?} hp={} | mv={mv:?} act={act:?}",
                sim.world.tick,
                dist,
                me.cell(),
                p.body.act.state,
                stamina,
                p.body.hp,
                foe.mode,
                foe.body.act.state,
                them.cell(),
                foe.body.hp
            );
        }
        sim.input(id, mv, act);
        sim.world.step();
    }
    let p = &sim.world.players[&id];
    Outcome {
        won: sim.world.enemies[&e].mode == Mode::Dead && p.body.act.state != ActorState::Dead,
        hp_left_pct: p.body.hp * 100 / p.body.hp_max,
        seconds: (sim.world.tick - start) as f32 / 30.0,
    }
}

fn sideways(d: Dir8) -> Dir8 {
    let i = Dir8::ALL.iter().position(|x| *x == d).unwrap();
    Dir8::ALL[(i + 3) % 8]
}

fn record(label: &str, outs: &[Outcome]) -> (usize, u32) {
    let wins = outs.iter().filter(|o| o.won).count();
    let avg_hp = outs.iter().map(|o| o.hp_left_pct).sum::<u32>() / outs.len() as u32;
    let avg_t = outs.iter().map(|o| o.seconds).sum::<f32>() / outs.len() as f32;
    eprintln!("{label}: {wins}/{} wins, avg hp left {avg_hp}%, avg {avg_t:.1}s", outs.len());
    (wins, avg_hp)
}

fn run(enemy: &str, archetype: Archetype, careful: bool) -> Vec<Outcome> {
    (0..6).map(|s| duel(enemy, archetype, careful, s)).collect()
}

#[test]
fn careful_play_beats_each_regular_enemy() {
    for enemy in ["oathless_knight", "ashen_pilgrim", "starveil_herald"] {
        for arch in [Archetype::AshWarden, Archetype::GraveRunner] {
            let outs = run(enemy, arch, true);
            let (wins, _) = record(&format!("careful {arch:?} vs {enemy}"), &outs);
            assert!(wins >= 4, "{arch:?} should usually beat {enemy} with careful play ({wins}/6)");
        }
    }
}

#[test]
fn standing_still_gets_you_killed() {
    let mut sim = Sim::new();
    let id = sim.join("Statue", Archetype::AshWarden);
    let e = sim.enemy_of("oathless_knight");
    sim.isolate(&[e]);
    sim.place(id, PLAZA.0, PLAZA.1);
    sim.move_enemy(e, PLAZA.0 + 3, PLAZA.1);
    let mut died_at = None;
    for t in 0..(30 * 60) {
        sim.world.step();
        if sim.world.players[&id].body.act.state == ActorState::Dead {
            died_at = Some(t);
            break;
        }
    }
    let t = died_at.expect("a knight must be able to kill an idle player");
    eprintln!("idle warden vs knight: dead after {:.1}s", t as f32 / 30.0);
    assert!(t > 30 * 5, "an idle player should survive a few seconds, not be deleted instantly");
}
