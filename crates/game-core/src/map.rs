use std::collections::{BTreeMap, HashMap, VecDeque};

use hollowrite_protocol::{Dir8, DoorInfo, LanternInfo, MapData, Tile, ZoneInfo};
use serde::Deserialize;

use crate::content::ContentError;

#[derive(Debug, Clone, Deserialize)]
pub struct MapMeta {
    pub id: String,
    pub name: String,
    pub grid: String,
    pub start_lantern: String,
    pub legend: BTreeMap<String, LegendEntry>,
    pub zones: Vec<Zone>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LegendEntry {
    pub tile: char,
    #[serde(default)]
    pub enemy: Option<String>,
    #[serde(default)]
    pub npc: Option<String>,
    #[serde(default)]
    pub item: Option<String>,
    #[serde(default)]
    pub qty: Option<u32>,
    #[serde(default)]
    pub lantern: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub door: Option<String>,
    #[serde(default)]
    pub rule: Option<DoorRule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DoorRule {
    Key { key: String },
    Oneway { from: Dir8 },
    Secret,
    Fog,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Zone {
    pub id: String,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    #[serde(default)]
    pub safe: bool,
    #[serde(default)]
    pub arena: bool,
}

impl Zone {
    pub fn contains(&self, cx: i32, cy: i32) -> bool {
        cx >= self.x && cy >= self.y && cx < self.x + self.w && cy < self.y + self.h
    }
}

#[derive(Debug, Clone)]
pub struct Lantern {
    pub id: String,
    pub name: String,
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone)]
pub struct Door {
    pub id: String,
    pub x: i32,
    pub y: i32,
    pub rule: DoorRule,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpawnKind {
    Enemy(String),
    Npc(String),
    Item { item: String, qty: u32, pickup_id: String },
}

#[derive(Debug, Clone)]
pub struct Spawn {
    pub x: i32,
    pub y: i32,
    pub what: SpawnKind,
}

#[derive(Debug, Clone)]
pub struct MapDef {
    pub id: String,
    pub name: String,
    pub width: i32,
    pub height: i32,
    tiles: Vec<Tile>,
    pub zones: Vec<Zone>,
    pub lanterns: Vec<Lantern>,
    pub doors: Vec<Door>,
    door_index: HashMap<(i32, i32), usize>,
    pub spawns: Vec<Spawn>,
    pub start_lantern: String,
}

impl MapDef {
    pub fn build(meta: MapMeta, grid: &str) -> Result<MapDef, ContentError> {
        let bad = |m: String| ContentError::Invalid(format!("map {}: {m}", meta.id));
        let lines: Vec<&str> = grid.lines().filter(|l| !l.starts_with(';')).collect();
        if lines.is_empty() {
            return Err(bad("empty grid".into()));
        }
        let width = lines[0].chars().count();
        let height = lines.len();
        let mut tiles = Vec::with_capacity(width * height);
        let mut lanterns = Vec::new();
        let mut doors = Vec::new();
        let mut spawns = Vec::new();
        for (y, line) in lines.iter().enumerate() {
            if line.chars().count() != width {
                return Err(bad(format!("row {y} has {} columns, expected {width}", line.chars().count())));
            }
            for (x, c) in line.chars().enumerate() {
                let (x, y) = (x as i32, y as i32);
                if let Some(t) = Tile::from_code(c) {
                    if matches!(t, Tile::Lantern | Tile::Shortcut | Tile::FogGate | Tile::FalseWall) {
                        return Err(bad(format!("special tile '{c}' at {x},{y} must be placed via a legend marker")));
                    }
                    tiles.push(t);
                    continue;
                }
                let entry =
                    meta.legend.get(&c.to_string()).ok_or_else(|| bad(format!("unknown glyph '{c}' at {x},{y}")))?;
                let tile = Tile::from_code(entry.tile)
                    .ok_or_else(|| bad(format!("legend '{c}' maps to unknown tile '{}'", entry.tile)))?;
                tiles.push(tile);
                if let Some(id) = &entry.enemy {
                    spawns.push(Spawn { x, y, what: SpawnKind::Enemy(id.clone()) });
                }
                if let Some(id) = &entry.npc {
                    spawns.push(Spawn { x, y, what: SpawnKind::Npc(id.clone()) });
                }
                if let Some(item) = &entry.item {
                    spawns.push(Spawn {
                        x,
                        y,
                        what: SpawnKind::Item {
                            item: item.clone(),
                            qty: entry.qty.unwrap_or(1),
                            pickup_id: format!("{item}@{x},{y}"),
                        },
                    });
                }
                if let Some(id) = &entry.lantern {
                    if tile != Tile::Lantern {
                        return Err(bad(format!("lantern {id} must use the lantern tile")));
                    }
                    let name = entry.name.clone().unwrap_or_else(|| id.clone());
                    lanterns.push(Lantern { id: id.clone(), name, x, y });
                }
                if let Some(id) = &entry.door {
                    let rule = entry.rule.clone().ok_or_else(|| bad(format!("door {id} needs a rule")))?;
                    doors.push(Door { id: id.clone(), x, y, rule });
                }
            }
        }
        let mut ids = std::collections::HashSet::new();
        for l in &lanterns {
            if !ids.insert(l.id.clone()) {
                return Err(bad(format!("lantern {} placed twice", l.id)));
            }
        }
        if !lanterns.iter().any(|l| l.id == meta.start_lantern) {
            return Err(bad(format!("start lantern {} is not on the map", meta.start_lantern)));
        }
        let door_index = doors.iter().enumerate().map(|(i, d)| ((d.x, d.y), i)).collect();
        let map = MapDef {
            id: meta.id.clone(),
            name: meta.name.clone(),
            width: width as i32,
            height: height as i32,
            tiles,
            zones: meta.zones,
            lanterns,
            doors,
            door_index,
            spawns,
            start_lantern: meta.start_lantern,
        };
        for z in &map.zones {
            if z.x < 0 || z.y < 0 || z.x + z.w > map.width || z.y + z.h > map.height {
                return Err(bad(format!("zone {} exceeds the map", z.id)));
            }
        }
        Ok(map)
    }

    pub fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.width && y < self.height
    }

    pub fn tile(&self, x: i32, y: i32) -> Tile {
        if self.in_bounds(x, y) {
            self.tiles[(y * self.width + x) as usize]
        } else {
            Tile::Void
        }
    }

    pub fn door_at(&self, x: i32, y: i32) -> Option<&Door> {
        self.door_index.get(&(x, y)).map(|&i| &self.doors[i])
    }

    pub fn lantern(&self, id: &str) -> Option<&Lantern> {
        self.lanterns.iter().find(|l| l.id == id)
    }

    pub fn zone(&self, id: &str) -> Option<&Zone> {
        self.zones.iter().find(|z| z.id == id)
    }

    /// The most specific zone containing the cell: later zones in the file
    /// are nested inside earlier ones and win.
    pub fn zone_at(&self, x: i32, y: i32) -> Option<&Zone> {
        self.zones.iter().rev().find(|z| z.contains(x, y))
    }

    pub fn arena(&self) -> Option<&Zone> {
        self.zones.iter().find(|z| z.arena)
    }

    /// Bresenham walk from cell centre to cell centre. Endpoints are not
    /// tested so a wall-hugging actor can still see and be seen.
    pub fn line_clear(&self, a: (i32, i32), b: (i32, i32), opaque: impl Fn(i32, i32) -> bool) -> bool {
        let (mut x0, mut y0) = a;
        let (x1, y1) = b;
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            if (x0, y0) == (x1, y1) {
                return true;
            }
            if (x0, y0) != a && opaque(x0, y0) {
                return false;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x0 += sx;
            }
            if e2 <= dx {
                err += dx;
                y0 += sy;
            }
        }
    }

    /// Breadth-first search over walkable cells using `passable`. Returns
    /// the first step from `from` towards `to`, or `None` when unreachable
    /// within `max_nodes` expansions.
    pub fn next_step(
        &self,
        from: (i32, i32),
        to: (i32, i32),
        max_nodes: usize,
        passable: impl Fn(i32, i32) -> bool,
    ) -> Option<(i32, i32)> {
        if from == to {
            return None;
        }
        let mut prev: HashMap<(i32, i32), (i32, i32)> = HashMap::new();
        let mut queue = VecDeque::new();
        queue.push_back(from);
        prev.insert(from, from);
        let mut expanded = 0;
        while let Some(cur) = queue.pop_front() {
            if cur == to {
                let mut step = cur;
                while prev[&step] != from {
                    step = prev[&step];
                }
                return Some(step);
            }
            expanded += 1;
            if expanded > max_nodes {
                break;
            }
            for d in Dir8::ALL {
                let (dx, dy) = d.offset();
                let n = (cur.0 + dx, cur.1 + dy);
                if prev.contains_key(&n) || !passable(n.0, n.1) {
                    continue;
                }
                // No corner cutting: both orthogonal neighbours must be open.
                if d.is_diagonal() && (!passable(cur.0 + dx, cur.1) || !passable(cur.0, cur.1 + dy)) {
                    continue;
                }
                prev.insert(n, cur);
                queue.push_back(n);
            }
        }
        None
    }

    pub fn to_protocol(&self) -> MapData {
        let rows = (0..self.height).map(|y| (0..self.width).map(|x| self.tile(x, y).code()).collect()).collect();
        MapData {
            id: self.id.clone(),
            name: self.name.clone(),
            width: self.width as u16,
            height: self.height as u16,
            rows,
            zones: self
                .zones
                .iter()
                .map(|z| ZoneInfo {
                    id: z.id.clone(),
                    name: z.name.clone(),
                    x: z.x as u16,
                    y: z.y as u16,
                    w: z.w as u16,
                    h: z.h as u16,
                    safe: z.safe,
                })
                .collect(),
            lanterns: self
                .lanterns
                .iter()
                .map(|l| LanternInfo { id: l.id.clone(), name: l.name.clone(), x: l.x as u16, y: l.y as u16 })
                .collect(),
            doors: self.doors.iter().map(|d| DoorInfo { id: d.id.clone(), x: d.x as u16, y: d.y as u16 }).collect(),
        }
    }
}
