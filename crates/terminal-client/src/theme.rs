use std::collections::HashMap;

use ratatui::style::Color;
use serde::Deserialize;

const THEME_JSON: &str = include_str!("../../../content/palettes/theme.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    True,
    Indexed,
    Ansi16,
}

impl ColorMode {
    pub fn detect() -> ColorMode {
        let colorterm = std::env::var("COLORTERM").unwrap_or_default().to_lowercase();
        if colorterm.contains("truecolor") || colorterm.contains("24bit") {
            return ColorMode::True;
        }
        let term = std::env::var("TERM").unwrap_or_default();
        if term.contains("256") {
            ColorMode::Indexed
        } else if cfg!(windows) {
            ColorMode::True
        } else {
            ColorMode::Ansi16
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct TileStyle {
    pub glyphs: Vec<String>,
    pub ascii: String,
    #[serde(default)]
    pub open: Option<String>,
    pub fg: String,
    pub bg: String,
}

#[derive(Debug, Deserialize)]
pub struct Walls {
    pub heavy: HashMap<String, String>,
    pub solid: String,
}

#[derive(Debug, Deserialize)]
pub struct Glyph {
    pub glyph: String,
    pub ascii: String,
    pub fg: String,
}

#[derive(Debug, Deserialize)]
pub struct PlayerGlyphs {
    pub glyphs: Vec<String>,
    pub ascii: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct BossStyle {
    pub fg: String,
    pub phase2: String,
    pub sprite: Vec<String>,
    pub sprite_cracked: Vec<String>,
    pub ascii: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct Entities {
    pub player: PlayerGlyphs,
    pub hues: Vec<String>,
    pub npc: HashMap<String, Glyph>,
    pub enemy: HashMap<String, Glyph>,
    pub boss: HashMap<String, BossStyle>,
    pub projectile: HashMap<String, Glyph>,
    pub loot: Glyph,
    pub key_loot: Glyph,
    pub remnant: Glyph,
}

#[derive(Debug, Deserialize)]
pub struct TeleStyle {
    pub glyph: String,
    pub ascii: String,
    pub fg: String,
    pub bg: String,
}

#[derive(Debug, Deserialize)]
pub struct Light {
    pub player_radius: i32,
    pub lantern_radius: i32,
    pub ambient: f32,
    pub remembered: f32,
}

#[derive(Debug, Deserialize)]
struct RawTheme {
    palette: HashMap<String, String>,
    ansi16: HashMap<String, String>,
    tiles: HashMap<String, TileStyle>,
    walls: Walls,
    entities: Entities,
    telegraphs: HashMap<String, TeleStyle>,
    ui: HashMap<String, String>,
    light: Light,
}

pub struct Theme {
    rgb: HashMap<String, (u8, u8, u8)>,
    ansi: HashMap<String, Color>,
    pub tiles: HashMap<char, TileStyle>,
    pub walls: Walls,
    pub entities: Entities,
    pub telegraphs: HashMap<String, TeleStyle>,
    ui: HashMap<String, String>,
    pub light: Light,
    pub mode: ColorMode,
    pub ascii: bool,
    pub high_contrast: bool,
}

fn parse_hex(s: &str) -> (u8, u8, u8) {
    let h = s.trim_start_matches('#');
    let v = u32::from_str_radix(h, 16).unwrap_or(0);
    ((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

fn ansi_named(name: &str) -> Color {
    match name {
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "cyan" => Color::Cyan,
        "gray" => Color::Gray,
        "dark_gray" => Color::DarkGray,
        "light_red" => Color::LightRed,
        "light_green" => Color::LightGreen,
        "light_yellow" => Color::LightYellow,
        "light_blue" => Color::LightBlue,
        "light_magenta" => Color::LightMagenta,
        "light_cyan" => Color::LightCyan,
        _ => Color::White,
    }
}

/// Nearest xterm-256 colour for an RGB triple (6x6x6 cube or grey ramp).
pub fn to_256(r: u8, g: u8, b: u8) -> u8 {
    let cube = |v: u8| -> u8 {
        if v < 48 {
            0
        } else if v < 115 {
            1
        } else {
            (v - 35) / 40
        }
    };
    let (cr, cg, cb) = (cube(r), cube(g), cube(b));
    let level = |c: u8| if c == 0 { 0 } else { 55 + c as i32 * 40 };
    let cube_err = (level(cr) - r as i32).pow(2) + (level(cg) - g as i32).pow(2) + (level(cb) - b as i32).pow(2);
    let avg = (r as i32 + g as i32 + b as i32) / 3;
    let grey_idx = ((avg - 8).max(0) / 10).min(23);
    let gv = 8 + grey_idx * 10;
    let grey_err = (gv - r as i32).pow(2) + (gv - g as i32).pow(2) + (gv - b as i32).pow(2);
    if grey_err < cube_err {
        232 + grey_idx as u8
    } else {
        16 + 36 * cr + 6 * cg + cb
    }
}

impl Theme {
    pub fn load(mode: ColorMode, ascii: bool, high_contrast: bool) -> Theme {
        let raw: RawTheme = serde_json::from_str(THEME_JSON).expect("bundled theme is valid");
        Theme {
            rgb: raw.palette.iter().map(|(k, v)| (k.clone(), parse_hex(v))).collect(),
            ansi: raw.ansi16.iter().map(|(k, v)| (k.clone(), ansi_named(v))).collect(),
            tiles: raw.tiles.into_iter().filter_map(|(k, v)| Some((k.chars().next()?, v))).collect(),
            walls: raw.walls,
            entities: raw.entities,
            telegraphs: raw.telegraphs,
            ui: raw.ui,
            light: raw.light,
            mode,
            ascii,
            high_contrast,
        }
    }

    pub fn rgb(&self, name: &str) -> (u8, u8, u8) {
        self.rgb.get(name).copied().unwrap_or((200, 200, 200))
    }

    /// Palette colour scaled by `light` (0..=1), rendered for the active mode.
    pub fn shade(&self, name: &str, light: f32) -> Color {
        let light = if self.high_contrast { (light * 1.35).min(1.0) } else { light };
        match self.mode {
            ColorMode::Ansi16 => {
                if light < 0.3 {
                    if name == "void" || name == "charcoal" || name == "soot" {
                        Color::Black
                    } else {
                        Color::DarkGray
                    }
                } else {
                    self.ansi.get(name).copied().unwrap_or(Color::White)
                }
            }
            _ => {
                let (r, g, b) = self.rgb(name);
                let f = |c: u8| (c as f32 * light).round().clamp(0.0, 255.0) as u8;
                let (r, g, b) = (f(r), f(g), f(b));
                if self.mode == ColorMode::True {
                    Color::Rgb(r, g, b)
                } else {
                    Color::Indexed(to_256(r, g, b))
                }
            }
        }
    }

    pub fn color(&self, name: &str) -> Color {
        self.shade(name, 1.0)
    }

    pub fn ui(&self, key: &str) -> Color {
        let name = self.ui.get(key).map(String::as_str).unwrap_or("bone");
        self.color(name)
    }

    pub fn ui_name(&self, key: &str) -> &str {
        self.ui.get(key).map(String::as_str).unwrap_or("bone")
    }

    pub fn pick<'a>(&self, unicode: &'a str, ascii: &'a str) -> &'a str {
        if self.ascii {
            ascii
        } else {
            unicode
        }
    }
}

/// Cheap stable per-cell hash for glyph variation, identical to the web client.
pub fn cell_hash(x: i32, y: i32) -> u32 {
    let mut h = (x as u32).wrapping_mul(374_761_393) ^ (y as u32).wrapping_mul(668_265_263);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    h ^ (h >> 16)
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn theme_parses_and_covers_every_tile() {
        let t = Theme::load(ColorMode::True, false, false);
        for tile in hollowrite_protocol::Tile::ALL {
            assert!(t.tiles.contains_key(&tile.code()), "no style for {tile:?}");
        }
    }

    #[test]
    fn every_glyph_is_single_width() {
        let t = Theme::load(ColorMode::True, false, false);
        let mut all: Vec<&str> = Vec::new();
        for s in t.tiles.values() {
            all.extend(s.glyphs.iter().map(String::as_str).filter(|g| *g != "box"));
            all.push(&s.ascii);
        }
        all.extend(t.walls.heavy.values().map(String::as_str));
        all.extend(t.entities.player.glyphs.iter().map(String::as_str));
        all.extend(t.entities.npc.values().map(|g| g.glyph.as_str()));
        all.extend(t.entities.enemy.values().map(|g| g.glyph.as_str()));
        all.extend(t.telegraphs.values().map(|g| g.glyph.as_str()));
        for b in t.entities.boss.values() {
            for row in b.sprite.iter().chain(&b.sprite_cracked).chain(&b.ascii) {
                assert_eq!(row.width(), 3, "boss sprite row {row:?}");
            }
        }
        for g in all {
            assert_eq!(g.width(), 1, "glyph {g:?} is not single-width");
        }
    }

    #[test]
    fn ascii_mode_is_pure_ascii() {
        let t = Theme::load(ColorMode::Ansi16, true, false);
        for s in t.tiles.values() {
            assert!(s.ascii.is_ascii());
        }
        for g in t.entities.enemy.values().chain(t.entities.npc.values()) {
            assert!(g.ascii.is_ascii());
        }
        for g in &t.entities.player.ascii {
            assert!(g.is_ascii());
        }
    }

    #[test]
    fn indexed_colours_are_close() {
        assert_eq!(to_256(0, 0, 0), 16);
        assert_eq!(to_256(255, 255, 255), 231);
        assert!((232..=255).contains(&to_256(128, 128, 128)));
    }
}
