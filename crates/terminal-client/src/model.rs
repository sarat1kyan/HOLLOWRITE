//! Client-side view of the world: interpolation, local movement prediction,
//! field of view and transient effects. Nothing here is authoritative.

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

use hollowrite_protocol::*;

pub const INTERP_DELAY: Duration = Duration::from_millis(110);
pub const FOV_RADIUS: i32 = 18;
const CHAT_KEEP: usize = 200;

pub struct Tracked {
    pub view: EntityView,
    samples: VecDeque<(Instant, i32, i32)>,
    pub flash_until: Option<Instant>,
}

impl Tracked {
    fn push(&mut self, at: Instant, x: i32, y: i32) {
        self.samples.push_back((at, x, y));
        while self.samples.len() > 8 {
            self.samples.pop_front();
        }
    }

    /// Position at `t`, linearly interpolated between buffered snapshots.
    pub fn pos_at(&self, t: Instant) -> (i32, i32) {
        let mut prev = None;
        for &(at, x, y) in &self.samples {
            if at >= t {
                return match prev {
                    Some((pa, px, py)) => {
                        let span = at.duration_since(pa).as_secs_f32().max(0.001);
                        let k = (t.duration_since(pa).as_secs_f32() / span).clamp(0.0, 1.0);
                        (lerp(px, x, k), lerp(py, y, k))
                    }
                    None => (x, y),
                };
            }
            prev = Some((at, x, y));
        }
        prev.map(|(_, x, y)| (x, y)).unwrap_or((self.view.x, self.view.y))
    }
}

fn lerp(a: i32, b: i32, k: f32) -> i32 {
    a + ((b - a) as f32 * k) as i32
}

pub enum FxKind {
    Number { text: String, color: &'static str },
    Swing { facing: Dir8, heavy: bool },
    Burst { color: &'static str },
    Ring { radius: f32, color: &'static str },
}

pub struct Effect {
    pub kind: FxKind,
    pub x: i32,
    pub y: i32,
    pub born: Instant,
    pub life: Duration,
}

impl Effect {
    pub fn age(&self, now: Instant) -> f32 {
        (now.duration_since(self.born).as_secs_f32() / self.life.as_secs_f32()).clamp(0.0, 1.0)
    }
}

pub struct Banner {
    pub text: String,
    pub sub: String,
    pub color: &'static str,
    pub until: Instant,
}

pub struct ChatLine {
    pub channel: ChatChannel,
    pub from: String,
    pub text: String,
}

pub struct Dialogue {
    pub npc: EntityId,
    pub speaker: String,
    pub lines: Vec<String>,
    pub choices: Vec<String>,
}

pub struct Prediction {
    pub x: f32,
    pub y: f32,
    history: VecDeque<(Instant, f32, f32)>,
}

pub struct Model {
    pub me: EntityId,
    pub map: MapData,
    tiles: Vec<Tile>,
    pub width: i32,
    pub height: i32,
    doors: HashMap<(i32, i32), String>,
    pub sheet: CharacterSheet,
    pub me_view: Option<SelfView>,
    pub entities: HashMap<EntityId, Tracked>,
    pub tele: Vec<(Telegraph, Instant)>,
    pub tick: u64,
    pub ack: u32,
    pub last_snapshot: Instant,
    pub explored: Vec<bool>,
    pub visible: Vec<bool>,
    fov_from: Option<(i32, i32)>,
    fov_doors: usize,
    pub effects: Vec<Effect>,
    pub toasts: VecDeque<(String, Instant)>,
    pub banner: Option<Banner>,
    pub chat: VecDeque<ChatLine>,
    pub party: Vec<PartyMember>,
    pub invites: Vec<String>,
    pub dialogue: Option<Dialogue>,
    pub prediction: Option<Prediction>,
    pub rtt: Option<Duration>,
    pub walk_speed: i32,
    pub body_radius: i32,
    pub tick_hz: u32,
    pub zone_name: String,
}

pub fn sanitize_display(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).collect()
}

impl Model {
    pub fn new(
        me: EntityId,
        map: MapData,
        sheet: CharacterSheet,
        walk_speed: i32,
        body_radius: i32,
        tick_hz: u32,
    ) -> Model {
        let width = map.width as i32;
        let height = map.height as i32;
        let tiles = map.rows.iter().flat_map(|r| r.chars().map(|c| Tile::from_code(c).unwrap_or(Tile::Void))).collect();
        let doors = map.doors.iter().map(|d| ((d.x as i32, d.y as i32), d.id.clone())).collect();
        let n = (width * height) as usize;
        Model {
            me,
            map,
            tiles,
            width,
            height,
            doors,
            sheet,
            me_view: None,
            entities: HashMap::new(),
            tele: Vec::new(),
            tick: 0,
            ack: 0,
            last_snapshot: Instant::now(),
            explored: vec![false; n],
            visible: vec![false; n],
            fov_from: None,
            fov_doors: usize::MAX,
            effects: Vec::new(),
            toasts: VecDeque::new(),
            banner: None,
            chat: VecDeque::new(),
            party: Vec::new(),
            invites: Vec::new(),
            dialogue: None,
            prediction: None,
            rtt: None,
            walk_speed: if walk_speed > 0 { walk_speed } else { 120 },
            body_radius: if body_radius > 0 { body_radius } else { 300 },
            tick_hz: tick_hz.max(1),
            zone_name: String::new(),
        }
    }

    pub fn tile(&self, x: i32, y: i32) -> Tile {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return Tile::Void;
        }
        self.tiles[(y * self.width + x) as usize]
    }

    pub fn idx(&self, x: i32, y: i32) -> Option<usize> {
        (x >= 0 && y >= 0 && x < self.width && y < self.height).then(|| (y * self.width + x) as usize)
    }

    /// A gated door this character has not opened. Unlisted doors are open
    /// doorways.
    pub fn door_closed(&self, x: i32, y: i32) -> bool {
        match self.doors.get(&(x, y)) {
            Some(id) => !self.sheet.open_doors.iter().any(|d| d == id),
            None => false,
        }
    }

    pub fn fog_sealed(&self) -> bool {
        self.me_view.as_ref().is_some_and(|m| m.boss.is_some())
    }

    pub fn passable(&self, x: i32, y: i32) -> bool {
        let t = self.tile(x, y);
        match t {
            Tile::Door | Tile::Shortcut | Tile::FalseWall => !self.door_closed(x, y),
            Tile::FogGate => !self.fog_sealed(),
            _ => t.walkable(),
        }
    }

    pub fn opaque(&self, x: i32, y: i32) -> bool {
        let t = self.tile(x, y);
        match t {
            Tile::Door | Tile::Shortcut | Tile::FalseWall => self.door_closed(x, y),
            _ => t.opaque(),
        }
    }

    pub fn my_state(&self) -> ActorState {
        self.me_view.as_ref().map_or(ActorState::Idle, |m| m.state)
    }

    pub fn apply_snapshot(&mut self, snap: Snapshot) {
        let now = Instant::now();
        self.tick = snap.tick;
        self.ack = snap.ack;
        self.last_snapshot = now;
        for id in &snap.gone {
            self.entities.remove(id);
        }
        let mut present = HashSet::new();
        for e in snap.entities {
            present.insert(e.id);
            let (x, y) = (e.x, e.y);
            let tracked = self.entities.entry(e.id).or_insert_with(|| Tracked {
                view: e.clone(),
                samples: VecDeque::new(),
                flash_until: None,
            });
            tracked.view = e;
            tracked.push(now, x, y);
        }
        self.tele = snap.tele.into_iter().map(|t| (t, now)).collect();
        let zone = snap.me.zone.clone();
        if let Some(z) = self.map.zones.iter().rev().find(|z| z.id == zone) {
            self.zone_name = z.name.clone();
        }
        self.me_view = Some(snap.me);
        self.reconcile(now);
    }

    fn reconcile(&mut self, now: Instant) {
        let Some(me) = self.entities.get(&self.me) else { return };
        let (sx, sy) = (me.view.x as f32, me.view.y as f32);
        let free = matches!(
            self.my_state(),
            ActorState::Idle | ActorState::Moving | ActorState::Guarding | ActorState::Healing
        );
        if !free {
            self.prediction = None;
            return;
        }
        let one_way = self.rtt.map_or(Duration::from_millis(40), |r| r / 2);
        let Some(p) = &mut self.prediction else {
            self.prediction = Some(Prediction { x: sx, y: sy, history: VecDeque::new() });
            return;
        };
        let target = now.checked_sub(one_way + Duration::from_millis(33)).unwrap_or(now);
        let past = p.history.iter().rev().find(|(t, _, _)| *t <= target).map(|&(_, x, y)| (x, y)).unwrap_or((p.x, p.y));
        let (ex, ey) = (sx - past.0, sy - past.1);
        let err = (ex * ex + ey * ey).sqrt();
        if err > 1800.0 {
            p.x = sx;
            p.y = sy;
            p.history.clear();
        } else {
            // Ease towards the authoritative path rather than snapping.
            let k = if err > 400.0 { 0.5 } else { 0.2 };
            p.x += ex * k;
            p.y += ey * k;
            for h in p.history.iter_mut() {
                h.1 += ex * k;
                h.2 += ey * k;
            }
        }
    }

    /// Advances local prediction by `dt` given the currently held direction.
    pub fn predict(&mut self, now: Instant, dt: Duration, held: Option<Dir8>) {
        let state = self.my_state();
        let speed_pct = match state {
            ActorState::Idle | ActorState::Moving => 100,
            ActorState::Guarding => 50,
            ActorState::Healing => 40,
            _ => 0,
        };
        let r = self.body_radius;
        let Some(mut p) = self.prediction.take() else { return };
        if let Some(d) = held.filter(|_| speed_pct > 0) {
            let per_sec = self.walk_speed as f32 * self.tick_hz as f32 * speed_pct as f32 / 100.0;
            let dist = per_sec * dt.as_secs_f32().min(0.1);
            let (dx, dy) = d.offset();
            let norm = if dx != 0 && dy != 0 { std::f32::consts::FRAC_1_SQRT_2 } else { 1.0 };
            let (mx, my) = (dx as f32 * dist * norm, dy as f32 * dist * norm);
            let cell = CELL as f32;
            if mx != 0.0 {
                if self.box_clear(p.x + mx, p.y, r) {
                    p.x += mx;
                } else if my == 0.0 {
                    let centre = (p.y / cell).floor() * cell + cell / 2.0;
                    if self.box_clear(p.x + mx, centre, r) {
                        p.y += (centre - p.y).signum() * mx.abs().min((centre - p.y).abs());
                    }
                }
            }
            if my != 0.0 {
                if self.box_clear(p.x, p.y + my, r) {
                    p.y += my;
                } else if mx == 0.0 {
                    let centre = (p.x / cell).floor() * cell + cell / 2.0;
                    if self.box_clear(centre, p.y + my, r) {
                        p.x += (centre - p.x).signum() * my.abs().min((centre - p.x).abs());
                    }
                }
            }
        }
        p.history.push_back((now, p.x, p.y));
        while p.history.len() > 120 {
            p.history.pop_front();
        }
        self.prediction = Some(p);
    }

    fn box_clear(&self, x: f32, y: f32, r: i32) -> bool {
        let r = r as f32;
        [(x - r, y - r), (x + r, y - r), (x - r, y + r), (x + r, y + r)]
            .iter()
            .all(|&(cx, cy)| self.passable((cx / CELL as f32).floor() as i32, (cy / CELL as f32).floor() as i32))
    }

    /// Where to draw an entity right now.
    pub fn render_pos(&self, id: EntityId, now: Instant) -> Option<(i32, i32)> {
        if id == self.me {
            if let Some(p) = &self.prediction {
                return Some((p.x as i32, p.y as i32));
            }
            return self.entities.get(&id).map(|e| e.pos_at(now.checked_sub(Duration::from_millis(30)).unwrap_or(now)));
        }
        self.entities.get(&id).map(|e| e.pos_at(now.checked_sub(INTERP_DELAY).unwrap_or(now)))
    }

    pub fn my_cell(&self, now: Instant) -> Option<(i32, i32)> {
        self.render_pos(self.me, now).map(|(x, y)| (x.div_euclid(CELL), y.div_euclid(CELL)))
    }

    /// Recomputes the field of view by casting rays to the edge of a square.
    pub fn update_fov(&mut self, from: (i32, i32)) {
        let doors = self.sheet.open_doors.len();
        if self.fov_from == Some(from) && self.fov_doors == doors {
            return;
        }
        self.fov_from = Some(from);
        self.fov_doors = doors;
        self.visible.iter_mut().for_each(|v| *v = false);
        let r = FOV_RADIUS;
        let mut targets = Vec::new();
        for i in -r..=r {
            targets.push((from.0 + i, from.1 - r));
            targets.push((from.0 + i, from.1 + r));
            targets.push((from.0 - r, from.1 + i));
            targets.push((from.0 + r, from.1 + i));
        }
        for t in targets {
            self.cast(from, t, r);
        }
    }

    fn cast(&mut self, a: (i32, i32), b: (i32, i32), r: i32) {
        let (mut x, mut y) = a;
        let dx = (b.0 - a.0).abs();
        let dy = -(b.1 - a.1).abs();
        let sx = if a.0 < b.0 { 1 } else { -1 };
        let sy = if a.1 < b.1 { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            let ddx = x - a.0;
            let ddy = y - a.1;
            if ddx * ddx + ddy * ddy > r * r {
                return;
            }
            if let Some(i) = self.idx(x, y) {
                self.visible[i] = true;
                self.explored[i] = true;
            } else {
                return;
            }
            if (x, y) != a && self.opaque(x, y) {
                return;
            }
            if (x, y) == b {
                return;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    pub fn is_visible(&self, x: i32, y: i32) -> bool {
        self.idx(x, y).is_some_and(|i| self.visible[i])
    }

    pub fn is_explored(&self, x: i32, y: i32) -> bool {
        self.idx(x, y).is_some_and(|i| self.explored[i])
    }

    pub fn toast(&mut self, text: impl Into<String>) {
        self.toasts.push_back((text.into(), Instant::now()));
        while self.toasts.len() > 5 {
            self.toasts.pop_front();
        }
    }

    pub fn banner(&mut self, text: &str, sub: &str, color: &'static str, secs: f32) {
        self.banner = Some(Banner {
            text: text.to_string(),
            sub: sub.to_string(),
            color,
            until: Instant::now() + Duration::from_secs_f32(secs),
        });
    }

    pub fn push_chat(&mut self, channel: ChatChannel, from: String, text: String) {
        self.chat.push_back(ChatLine { channel, from: sanitize_display(&from), text: sanitize_display(&text) });
        while self.chat.len() > CHAT_KEEP {
            self.chat.pop_front();
        }
    }

    fn fx(&mut self, kind: FxKind, x: i32, y: i32, ms: u64) {
        self.effects.push(Effect { kind, x, y, born: Instant::now(), life: Duration::from_millis(ms) });
    }

    pub fn apply_events(&mut self, events: Vec<GameEvent>) {
        for e in events {
            match e {
                GameEvent::Hit { dst, amount, kind, x, y, .. } => {
                    let (text, color) = match kind {
                        HitKind::Dodged => ("dodge".to_string(), "fog_hi"),
                        HitKind::Parried => ("PARRY".to_string(), "amber_hi"),
                        HitKind::Blocked => (format!("{amount}"), "stone"),
                        HitKind::Critical => (format!("{amount}!"), "amber_hi"),
                        HitKind::Hazard => (format!("{amount}"), "sickly_hi"),
                        HitKind::Normal => (format!("{amount}"), if dst == self.me { "blood_hi" } else { "bone" }),
                    };
                    self.fx(FxKind::Number { text, color }, x, y, 900);
                    if let Some(t) = self.entities.get_mut(&dst) {
                        if amount > 0 {
                            t.flash_until = Some(Instant::now() + Duration::from_millis(120));
                        }
                    }
                }
                GameEvent::Swing { action, facing, x, y, .. } => {
                    let heavy = action == "heavy" || action == "slam" || action == "lunge";
                    self.fx(FxKind::Swing { facing, heavy }, x, y, if heavy { 260 } else { 180 });
                }
                GameEvent::Staggered { id } => {
                    if let Some((x, y)) = self.entities.get(&id).map(|t| (t.view.x, t.view.y)) {
                        self.fx(FxKind::Number { text: "staggered".into(), color: "amber" }, x, y - 600, 700);
                    }
                }
                GameEvent::Died { id, x, y } => {
                    self.fx(FxKind::Burst { color: "bone" }, x, y, 700);
                    if id == self.me {
                        self.banner("UNTHREADED", "your echoes stay where you fell", "blood_hi", 3.5);
                    }
                }
                GameEvent::Respawned { id, .. } if id == self.me => {
                    self.prediction = None;
                    self.banner("THE LANTERN REMEMBERS YOU", "", "amber", 2.2);
                }
                GameEvent::Respawned { .. } => {}
                GameEvent::EchoesGained { amount, .. } => self.toast(format!("+{amount} echoes")),
                GameEvent::EchoesLost { amount } => self.toast(format!("{amount} echoes are lost for good")),
                GameEvent::EchoesReclaimed { amount } => {
                    self.banner("ECHOES RECLAIMED", &format!("{amount} return to you"), "sickly_hi", 2.5)
                }
                GameEvent::ItemGained { name, qty, .. } => {
                    self.toast(if qty > 1 { format!("Found {name} x{qty}") } else { format!("Found {name}") })
                }
                GameEvent::LanternLit { name, .. } => self.banner("LANTERN LIT", &name, "amber_hi", 2.8),
                GameEvent::Rested { .. } => self.toast("You rest. Health, draughts and wick restored."),
                GameEvent::LevelUp { level } => self.toast(format!("You feel more yourself. Level {level}.")),
                GameEvent::BossPhase { .. } => self.banner("THE BELL CRACKS", "", "sickly_hi", 2.5),
                GameEvent::BossDefeated { name, .. } => {
                    self.banner(&format!("{} SILENCED", name.to_uppercase()), "", "gold", 4.5)
                }
                GameEvent::BossReset { .. } => self.toast("The belfry falls silent and waits."),
                GameEvent::Toll { x, y, radius } => {
                    self.fx(FxKind::Ring { radius: radius as f32 / 10.0, color: "bone" }, x, y, 900);
                }
                GameEvent::DoorOpened { .. } => self.toast("A way opens."),
                GameEvent::Notice { text } => self.toast(sanitize_display(&text)),
                GameEvent::ZoneEntered { name, .. } => self.banner(&name, "", "fog_hi", 2.0),
            }
        }
    }

    pub fn prune(&mut self, now: Instant) {
        self.effects.retain(|e| now.duration_since(e.born) < e.life);
        while self.toasts.front().is_some_and(|(_, t)| now.duration_since(*t) > Duration::from_secs(6)) {
            self.toasts.pop_front();
        }
        if self.banner.as_ref().is_some_and(|b| now > b.until) {
            self.banner = None;
        }
    }

    /// Telegraph cells with their remaining fraction (1 = just started).
    pub fn telegraph_cells(&self, now: Instant) -> HashMap<(i32, i32), (TelegraphKind, f32)> {
        let mut out: HashMap<(i32, i32), (TelegraphKind, f32)> = HashMap::new();
        let tick_secs = 1.0 / self.tick_hz as f32;
        for (t, at) in &self.tele {
            let elapsed_ticks = now.duration_since(*at).as_secs_f32() / tick_secs;
            let left = (t.left as f32 - elapsed_ticks).max(0.0);
            let frac = if t.total == 0 { 0.0 } else { (left / t.total as f32).clamp(0.0, 1.0) };
            for c in &t.cells {
                let key = (c[0] as i32, c[1] as i32);
                let rank = |k: TelegraphKind| match k {
                    TelegraphKind::Shockwave => 5,
                    TelegraphKind::Heavy => 4,
                    TelegraphKind::Area => 3,
                    TelegraphKind::Strike => 2,
                    TelegraphKind::Hazard => 1,
                    TelegraphKind::Weak => 0,
                };
                let replace = out.get(&key).is_none_or(|(k, _)| rank(t.kind) > rank(*k));
                if replace {
                    out.insert(key, (t.kind, frac));
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet() -> CharacterSheet {
        CharacterSheet {
            id: "c".into(),
            name: "T".into(),
            archetype: Archetype::AshWarden,
            look: Appearance::default(),
            level: 1,
            attrs: [1; 5],
            echoes: 0,
            next_level_cost: 1,
            inventory: vec![],
            equipment: Equipment::default(),
            open_doors: vec![],
            quests: vec![],
            checkpoint: String::new(),
            lanterns: vec![],
            stats: DerivedStats::default(),
        }
    }

    fn map() -> MapData {
        MapData {
            id: "m".into(),
            name: "m".into(),
            width: 7,
            height: 3,
            rows: vec!["#######".into(), "#..D..#".into(), "#######".into()],
            zones: vec![],
            lanterns: vec![],
            doors: vec![DoorInfo { id: "d".into(), x: 3, y: 1 }],
        }
    }

    #[test]
    fn closed_doors_block_sight_and_movement_until_opened() {
        let mut m = Model::new(1, map(), sheet(), 120, 300, 30);
        m.update_fov((1, 1));
        assert!(m.is_visible(3, 1));
        assert!(!m.is_visible(5, 1));
        assert!(!m.passable(3, 1));
        m.sheet.open_doors.push("d".into());
        m.update_fov((1, 1));
        assert!(m.is_visible(5, 1));
        assert!(m.passable(3, 1));
    }

    #[test]
    fn interpolation_is_bounded_by_samples() {
        let now = Instant::now();
        let mut t = Tracked {
            view: EntityView {
                id: 1,
                kind: EntityKind::Player,
                def: "x".into(),
                name: None,
                x: 0,
                y: 0,
                facing: Dir8::S,
                state: ActorState::Idle,
                action: None,
                progress: 0,
                rel: Relation::Me,
                hp: None,
                look: None,
                phase: None,
            },
            samples: VecDeque::new(),
            flash_until: None,
        };
        t.push(now, 0, 0);
        t.push(now + Duration::from_millis(100), 1000, 0);
        assert_eq!(t.pos_at(now + Duration::from_millis(50)).0, 500);
        assert_eq!(t.pos_at(now + Duration::from_secs(5)).0, 1000);
        assert_eq!(t.pos_at(now - Duration::from_secs(5)).0, 0);
    }

    #[test]
    fn display_text_drops_control_characters() {
        assert_eq!(sanitize_display("a\x1b[31mb\x07"), "a[31mb");
    }
}
