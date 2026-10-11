use std::time::Instant;

use hollowrite_protocol::*;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Overlay, Screen};
use crate::model::{FxKind, Model};
use crate::theme::{cell_hash, Theme};

pub const MIN_W: u16 = 80;
pub const MIN_H: u16 = 24;

#[allow(clippy::too_many_arguments)]
fn put(buf: &mut Buffer, area: Rect, x: i32, y: i32, sym: &str, fg: Color, bg: Option<Color>, m: Modifier) {
    if x < area.x as i32 || y < area.y as i32 || x >= (area.x + area.width) as i32 || y >= (area.y + area.height) as i32
    {
        return;
    }
    if let Some(cell) = buf.cell_mut((x as u16, y as u16)) {
        cell.set_symbol(sym).set_fg(fg);
        if let Some(bg) = bg {
            cell.set_bg(bg);
        }
        cell.modifier = m;
    }
}

fn text(buf: &mut Buffer, area: Rect, x: i32, y: i32, s: &str, style: Style) {
    let mut cx = x;
    for ch in s.chars() {
        let w = UnicodeWidthStr::width(ch.to_string().as_str()) as i32;
        if w == 0 {
            continue;
        }
        if cx >= area.x as i32
            && cx < (area.x + area.width) as i32
            && y >= area.y as i32
            && y < (area.y + area.height) as i32
        {
            if let Some(cell) = buf.cell_mut((cx as u16, y as u16)) {
                cell.set_symbol(&ch.to_string()).set_style(style);
            }
        }
        cx += w;
    }
}

fn panel<'a>(t: &Theme, title: &'a str, focus: bool) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(if t.ascii { BorderType::Plain } else { BorderType::Rounded })
        .border_style(Style::default().fg(if focus { t.ui("border_focus") } else { t.ui("border") }))
        .title(Span::styled(format!(" {title} "), Style::default().fg(t.ui("title")).add_modifier(Modifier::BOLD)))
        .style(Style::default().bg(t.color("void")))
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect { x: area.x + (area.width - w) / 2, y: area.y + (area.height - h) / 2, width: w, height: h }
}

pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();
    let t = &app.theme;
    f.render_widget(Block::default().style(Style::default().bg(t.color("void"))), area);
    if area.width < MIN_W || area.height < MIN_H {
        let msg = format!(
            "HOLLOWRITE needs at least {MIN_W}x{MIN_H} cells (now {}x{}). Enlarge the window or reduce the font size.",
            area.width, area.height
        );
        f.render_widget(Paragraph::new(msg).wrap(Wrap { trim: true }).style(Style::default().fg(t.ui("text"))), area);
        return;
    }
    match &app.screen {
        Screen::Login(_) => draw_login(f, app, area),
        Screen::Connecting(msg) => {
            let r = centered(area, 60, 5);
            f.render_widget(
                Paragraph::new(msg.as_str())
                    .block(panel(t, "HOLLOWRITE", true))
                    .style(Style::default().fg(t.ui("text"))),
                r,
            );
        }
        Screen::Lobby(_) => draw_lobby(f, app, area),
        Screen::Game => draw_game(f, app, area),
    }
}

const TITLE: [&str; 3] =
    ["╻ ╻┏━┓╻  ╻  ┏━┓╻ ╻┏━┓╻╺┳╸┏━╸", "┣━┫┃ ┃┃  ┃  ┃ ┃┃╻┃┣┳┛┃ ┃ ┣╸ ", "╹ ╹┗━┛┗━╸┗━╸┗━┛┗┻┛╹┗╸╹ ╹ ┗━╸"];

fn draw_title(f: &mut Frame, t: &Theme, area: Rect, y: u16) {
    let buf = f.buffer_mut();
    if t.ascii {
        let s = "H O L L O W R I T E";
        text(
            buf,
            area,
            area.x as i32 + (area.width as i32 - s.len() as i32) / 2,
            y as i32 + 1,
            s,
            Style::default().fg(t.color("amber")).add_modifier(Modifier::BOLD),
        );
    } else {
        for (i, row) in TITLE.iter().enumerate() {
            let w = row.width() as i32;
            text(
                buf,
                area,
                area.x as i32 + (area.width as i32 - w) / 2,
                (y as usize + i) as i32,
                row,
                Style::default().fg(t.color(if i == 1 { "amber" } else { "gold" })),
            );
        }
    }
    let tag = "Death keeps what you forget.";
    text(
        buf,
        area,
        area.x as i32 + (area.width as i32 - tag.len() as i32) / 2,
        y as i32 + 4,
        tag,
        Style::default().fg(t.ui("muted")).add_modifier(Modifier::ITALIC),
    );
}

fn draw_login(f: &mut Frame, app: &App, area: Rect) {
    let Screen::Login(form) = &app.screen else { return };
    let t = &app.theme;
    draw_title(f, t, area, area.y + area.height / 2 - 10);
    let r = centered(area, 52, 11);
    let r = Rect { y: r.y + 2, ..r };
    let title = if form.register { "Create account" } else { "Sign in" };
    let field = |label: &str, value: String, active: bool| {
        let style = if active {
            Style::default().fg(t.color("amber_hi")).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(t.ui("text"))
        };
        Line::from(vec![
            Span::styled(format!("{label:>9} "), Style::default().fg(t.ui("muted"))),
            Span::styled(format!("{value}{}", if active { "▏" } else { "" }), style),
        ])
    };
    let mut lines = vec![
        Line::from(""),
        field("name", form.username.clone(), form.field == 0),
        field("password", "•".repeat(form.password.chars().count()), form.field == 1),
        Line::from(""),
    ];
    if form.busy {
        lines.push(Line::styled("  contacting the lantern...", Style::default().fg(t.ui("muted"))));
    } else if let Some(e) = &form.error {
        lines.push(Line::styled(format!("  {e}"), Style::default().fg(t.ui("danger"))));
    } else {
        lines.push(Line::from(""));
    }
    lines.push(Line::from(""));
    lines.push(Line::styled(
        format!(
            "  Enter: {}   F2: {}   Esc: quit",
            if form.register { "register" } else { "sign in" },
            if form.register { "have an account?" } else { "new account" }
        ),
        Style::default().fg(t.ui("muted")),
    ));
    lines.push(Line::styled(format!("  server {}", app.server), Style::default().fg(t.color("slate_hi"))));
    f.render_widget(Paragraph::new(lines).block(panel(t, title, true)), r);
}

fn archetype_blurb(a: Archetype) -> (&'static str, &'static str) {
    match a {
        Archetype::AshWarden => {
            ("Sword and buckler. Sturdy, patient, made for parries.", "VIT 12  END 12  MIG 11  FIN 8  OCC 6")
        }
        Archetype::VeilSeer => (
            "Fragile caster. Ember Psalm and a thorn knife for when it gets close.",
            "VIT 8  END 9  MIG 6  FIN 9  OCC 15",
        ),
        Archetype::GraveRunner => {
            ("Light blade, quicker dodge. Strike, roll, strike again.", "VIT 10  END 11  MIG 8  FIN 14  OCC 6")
        }
    }
}

fn player_glyph(t: &Theme, look: Appearance) -> (&str, &str) {
    let i = look.sigil as usize % t.entities.player.glyphs.len();
    let hue = t.entities.hues.get(look.hue as usize).map(String::as_str).unwrap_or("bone");
    (t.pick(&t.entities.player.glyphs[i], &t.entities.player.ascii[i]), hue)
}

fn draw_lobby(f: &mut Frame, app: &App, area: Rect) {
    let Screen::Lobby(lobby) = &app.screen else { return };
    let t = &app.theme;
    draw_title(f, t, area, area.y + 1);
    let r = centered(area, 70, 16);
    let r = Rect { y: (area.y + 7).max(r.y), ..r };
    if let Some(c) = &lobby.create {
        let a = Archetype::ALL[c.archetype];
        let (blurb, stats) = archetype_blurb(a);
        let (glyph, hue) = player_glyph(t, Appearance { sigil: c.sigil, hue: c.hue });
        let sel = |i: usize| {
            if c.field == i {
                Style::default().fg(t.color("amber_hi")).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(t.ui("text"))
            }
        };
        let lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("      Name  ", Style::default().fg(t.ui("muted"))),
                Span::styled(format!("{}{}", c.name, if c.field == 0 { "▏" } else { "" }), sel(0)),
            ]),
            Line::from(vec![
                Span::styled("  Origin    ", Style::default().fg(t.ui("muted"))),
                Span::styled(format!("◂ {} ▸", a.label()), sel(1)),
            ]),
            Line::styled(format!("            {blurb}"), Style::default().fg(t.ui("muted"))),
            Line::styled(format!("            {stats}"), Style::default().fg(t.color("stone"))),
            Line::from(vec![
                Span::styled("     Sigil  ", Style::default().fg(t.ui("muted"))),
                Span::styled(format!("◂ {} ▸", c.sigil + 1), sel(2)),
            ]),
            Line::from(vec![
                Span::styled("    Colour  ", Style::default().fg(t.ui("muted"))),
                Span::styled(format!("◂ {} ▸", c.hue + 1), sel(3)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("  You will appear as  ", Style::default().fg(t.ui("muted"))),
                Span::styled(
                    glyph.to_string(),
                    Style::default().fg(t.color(hue)).bg(t.color("slate")).add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(""),
            Line::styled(lobby.error.clone().unwrap_or_default(), Style::default().fg(t.ui("danger"))),
            Line::styled(
                "  Tab/arrows: field  ◂ ▸: choose  Enter: create  Esc: back",
                Style::default().fg(t.ui("muted")),
            ),
        ];
        f.render_widget(Paragraph::new(lines).block(panel(t, "A new Hollowbound", true)), r);
        return;
    }
    let mut lines = vec![Line::from("")];
    if !lobby.loaded {
        lines.push(Line::styled("  listening for names...", Style::default().fg(t.ui("muted"))));
    }
    for (i, c) in lobby.chars.iter().enumerate() {
        let (glyph, hue) = player_glyph(t, c.look);
        let active = i == lobby.sel;
        lines.push(Line::from(vec![
            Span::styled(if active { "  ▸ " } else { "    " }, Style::default().fg(t.color("amber"))),
            Span::styled(glyph.to_string(), Style::default().fg(t.color(hue)).add_modifier(Modifier::BOLD)),
            Span::styled(
                format!("  {:<16} {:<13} level {:<3}", c.name, c.archetype.label(), c.level),
                if active {
                    Style::default().fg(t.color("amber_hi")).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(t.ui("text"))
                },
            ),
            Span::styled(format!(" {}", c.location.replace('_', " ")), Style::default().fg(t.ui("muted"))),
        ]));
    }
    lines.push(Line::from(""));
    if let Some(e) = &lobby.error {
        lines.push(Line::styled(format!("  {e}"), Style::default().fg(t.ui("danger"))));
    }
    if let Some(s) = &app.status {
        lines.push(Line::styled(format!("  {s}"), Style::default().fg(t.ui("system"))));
    }
    lines.push(Line::styled(
        "  ↑↓ choose   Enter: wake   N: new   L: sign out   Esc: quit",
        Style::default().fg(t.ui("muted")),
    ));
    f.render_widget(Paragraph::new(lines).block(panel(t, &format!("{} — characters", app.username), true)), r);
}

fn bar(t: &Theme, label: &str, cur: u32, max: u32, width: usize, fg: &str, back: &str) -> Vec<Span<'static>> {
    let filled = if max == 0 { 0 } else { (cur as usize * width).div_ceil(max as usize).min(width) };
    let (full, empty) = if t.ascii { ("#", "-") } else { ("█", "░") };
    vec![
        Span::styled(format!("{label} "), Style::default().fg(t.color(fg)).add_modifier(Modifier::BOLD)),
        Span::styled(full.repeat(filled), Style::default().fg(t.color(fg))),
        Span::styled(empty.repeat(width - filled), Style::default().fg(t.color(back))),
        Span::styled(format!(" {cur}/{max}  "), Style::default().fg(t.ui("text"))),
    ]
}

fn draw_game(f: &mut Frame, app: &App, area: Rect) {
    let Some(m) = &app.model else { return };
    let t = &app.theme;
    let now = Instant::now();
    let wide = area.width >= 110 && area.height >= 30;
    let (main, side) = if wide {
        let c = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(60), Constraint::Length(34)])
            .split(area);
        (c[0], Some(c[1]))
    } else {
        (area, None)
    };
    let rows = if wide {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(2), Constraint::Min(10), Constraint::Length(8)])
            .split(main)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(2), Constraint::Min(10), Constraint::Length(2)])
            .split(main)
    };
    draw_hud(f, app, m, rows[0]);
    draw_world(f, app, m, rows[1], now);
    if wide {
        draw_chat(f, app, m, rows[2]);
    } else {
        draw_footer(f, app, m, rows[2]);
    }
    if let Some(s) = side {
        draw_sidebar(f, app, m, s);
    }
    match &app.overlay {
        Overlay::None => {}
        Overlay::Character { sel } => draw_character(f, app, m, area, *sel),
        Overlay::Level { sel } => draw_level(f, app, m, area, *sel),
        Overlay::Dialogue { sel } => draw_dialogue(f, app, m, area, *sel),
        Overlay::Map => draw_map(f, app, m, area, now),
        Overlay::Help => draw_help(f, app, area),
        Overlay::Chat { text } => {
            let r =
                Rect { x: rows[1].x, y: rows[1].y + rows[1].height.saturating_sub(3), width: rows[1].width, height: 3 };
            f.render_widget(Clear, r);
            f.render_widget(
                Paragraph::new(format!("{text}▏")).style(Style::default().fg(t.ui("text"))).block(panel(
                    t,
                    "say (/p party, /invite name, Esc cancel)",
                    true,
                )),
                r,
            );
        }
        Overlay::Menu { sel } => draw_menu(f, app, area, *sel),
    }
}

fn draw_hud(f: &mut Frame, app: &App, m: &Model, area: Rect) {
    let t = &app.theme;
    let Some(me) = &m.me_view else { return };
    let w = match area.width {
        100.. => 14,
        86..=99 => 10,
        _ => 7,
    };
    let mut l1 = Vec::new();
    l1.extend(bar(t, "HP", me.hp, me.hp_max, w, t.ui_name("hp"), t.ui_name("hp_back")));
    l1.extend(bar(t, "ST", me.stamina, me.stamina_max, w, t.ui_name("stamina"), t.ui_name("stamina_back")));
    if me.wick_max > 0 {
        l1.extend(bar(t, "WK", me.wick, me.wick_max, w / 2, t.ui_name("wick"), t.ui_name("wick_back")));
    }
    l1.push(Span::styled(
        format!("flask {}/{}  ", me.draughts, me.draughts_max),
        Style::default().fg(t.color("amber")),
    ));
    l1.push(Span::styled(
        format!("{} {}", t.pick("✧", "*"), me.echoes),
        Style::default().fg(t.ui("echoes")).add_modifier(Modifier::BOLD),
    ));
    let mut l2 = vec![Span::styled(format!("{}  ", m.zone_name), Style::default().fg(t.color("fog_hi")))];
    if let Some(hint) = &me.interact {
        l2.push(Span::styled(format!("[F] {hint}  "), Style::default().fg(t.color("amber_hi"))));
    }
    if let Some([rx, ry]) = me.remnant {
        if let Some((px, py)) = m.render_pos(m.me, Instant::now()) {
            let (dx, dy) = ((rx - px) / CELL, (ry - py) / CELL);
            let dist = ((dx * dx + dy * dy) as f32).sqrt() as i32;
            let dir = Dir8::from_offset(dx.signum(), dy.signum())
                .map(|d| format!("{d:?}").to_uppercase())
                .unwrap_or_default();
            l2.push(Span::styled(
                format!("{} remnant {dist} {dir}  ", t.pick("◊", "&")),
                Style::default().fg(t.color("sickly_hi")),
            ));
        }
    }
    let state = match me.state {
        ActorState::Guarding => "guarding",
        ActorState::Staggered => "staggered",
        ActorState::Healing => "drinking",
        ActorState::Rolling => "rolling",
        _ => "",
    };
    if !state.is_empty() {
        l2.push(Span::styled(format!("({state})  "), Style::default().fg(t.ui("muted"))));
    }
    if me.cooldowns.iter().any(|c| *c > 0) {
        l2.push(Span::styled(
            format!("rites {}s/{}s  ", me.cooldowns[0].div_ceil(30), me.cooldowns[1].div_ceil(30)),
            Style::default().fg(t.ui("muted")),
        ));
    }
    f.render_widget(
        Paragraph::new(vec![Line::from(l1), Line::from(l2)]).style(Style::default().bg(t.color("void"))),
        area,
    );
    let (on, off) = (t.pick("●", "*"), t.pick("○", "o"));
    let (net, color) = match m.rtt {
        Some(r) if r.as_millis() < 120 => (format!("{on} {}ms", r.as_millis()), "sickly_hi"),
        Some(r) if r.as_millis() < 250 => (format!("{on} {}ms", r.as_millis()), "amber"),
        Some(r) => (format!("{on} {}ms", r.as_millis()), "blood_hi"),
        None => (format!("{off} ..."), "ash"),
    };
    let stale = Instant::now().duration_since(m.last_snapshot).as_millis() > 1500;
    let net = if stale { format!("{off} waiting for server") } else { net };
    let x = area.x as i32 + area.width as i32 - net.width() as i32 - 1;
    text(
        f.buffer_mut(),
        area,
        x,
        area.y as i32 + 1,
        &net,
        Style::default().fg(t.color(if stale { "blood_hi" } else { color })),
    );
}

fn wall_glyph<'a>(t: &'a Theme, m: &Model, x: i32, y: i32) -> &'a str {
    let is_wall = |x: i32, y: i32| {
        matches!(m.tile(x, y), Tile::Wall | Tile::FalseWall)
            && !(m.tile(x, y) == Tile::FalseWall && !m.door_closed(x, y))
    };
    let edge = |x: i32, y: i32| {
        is_wall(x, y)
            && (-1..=1).any(|dy| {
                (-1..=1).any(|dx| {
                    let n = m.tile(x + dx, y + dy);
                    !matches!(n, Tile::Wall | Tile::Void | Tile::FalseWall)
                })
            })
    };
    if !edge(x, y) {
        return &t.walls.solid;
    }
    let key =
        format!("{}{}{}{}", edge(x, y - 1) as u8, edge(x + 1, y) as u8, edge(x, y + 1) as u8, edge(x - 1, y) as u8);
    t.walls.heavy.get(&key).map(String::as_str).unwrap_or("■")
}

fn draw_world(f: &mut Frame, app: &App, m: &Model, area: Rect, now: Instant) {
    let t = &app.theme;
    let motion = !app.settings.reduced_motion;
    let secs = now.duration_since(app.started).as_secs_f32();
    let Some((px, py)) = m.render_pos(m.me, now) else { return };
    let (pcx, pcy) = (px.div_euclid(CELL), py.div_euclid(CELL));
    let vw = area.width as i32;
    let vh = area.height as i32;
    let ox = pcx - vw / 2;
    let oy = pcy - vh / 2;
    let to_screen = |cx: i32, cy: i32| (area.x as i32 + cx - ox, area.y as i32 + cy - oy);
    let tele = m.telegraph_cells(now);
    let buf = f.buffer_mut();
    let pr = t.light.player_radius as f32;
    let lr = t.light.lantern_radius as f32;
    let lanterns: Vec<(i32, i32)> = m.map.lanterns.iter().map(|l| (l.x as i32, l.y as i32)).collect();

    for sy in 0..vh {
        for sx in 0..vw {
            let (cx, cy) = (ox + sx, oy + sy);
            let (x, y) = (area.x as i32 + sx, area.y as i32 + sy);
            let tile = m.tile(cx, cy);
            if !m.is_explored(cx, cy) || tile == Tile::Void {
                put(buf, area, x, y, " ", t.color("void"), Some(t.color("void")), Modifier::empty());
                continue;
            }
            let visible = m.is_visible(cx, cy);
            let light = if visible {
                let d = (((cx - pcx).pow(2) + (cy - pcy).pow(2)) as f32).sqrt();
                let mut l = t.light.ambient + (1.0 - t.light.ambient) * (1.0 - d / pr).max(0.0).powf(0.8);
                for &(lx, ly) in &lanterns {
                    let d = (((cx - lx).pow(2) + (cy - ly).pow(2)) as f32).sqrt();
                    let flicker = if motion { 0.92 + 0.08 * (secs * 7.0 + lx as f32).sin() } else { 1.0 };
                    l = l.max((1.0 - d / lr).max(0.0) * flicker);
                }
                l.clamp(t.light.ambient, 1.0)
            } else {
                t.light.remembered
            };
            let style = &t.tiles[&tile.code()];
            let h = cell_hash(cx, cy);
            let mut fg_name = style.fg.as_str();
            let mut glyph: &str = match tile {
                Tile::Wall | Tile::FalseWall => {
                    if tile == Tile::FalseWall && !m.door_closed(cx, cy) {
                        t.pick("·", ".")
                    } else if t.ascii {
                        "#"
                    } else {
                        wall_glyph(t, m, cx, cy)
                    }
                }
                Tile::Door | Tile::Shortcut => {
                    if m.door_closed(cx, cy) {
                        t.pick(&style.glyphs[0], &style.ascii)
                    } else {
                        style.open.as_deref().unwrap_or("'")
                    }
                }
                _ => {
                    let g = &style.glyphs[h as usize % style.glyphs.len()];
                    t.pick(g, &style.ascii)
                }
            };
            let mut bg_name = style.bg.as_str();
            let mut tile_light = light;
            match tile {
                Tile::Lantern => {
                    fg_name = if motion && (secs * 3.0 + cx as f32).sin() > 0.3 { "amber_hi" } else { "amber" };
                    tile_light = 1.0;
                }
                Tile::EmberPit if motion => {
                    let phase = ((secs * 4.0 + h as f32 * 0.001).sin() + 1.0) / 2.0;
                    glyph = t.pick(if phase > 0.5 { "▒" } else { "░" }, "^");
                    fg_name = if phase > 0.7 { "amber_hi" } else { "amber" };
                }
                Tile::FogGate => {
                    let sealed = m.fog_sealed();
                    glyph = t.pick(if (secs * 2.0 + cy as f32).sin() > 0.0 || !motion { "▒" } else { "░" }, "&");
                    fg_name = if sealed { "bone" } else { "fog_hi" };
                    tile_light = tile_light.max(0.7);
                }
                Tile::Water if motion => {
                    glyph = t.pick(
                        if ((secs * 1.5 + cx as f32 * 0.7).sin() + (cy as f32)).sin() > 0.0 { "≈" } else { "~" },
                        "~",
                    );
                }
                _ => {}
            }
            let mut modifier = Modifier::empty();
            if visible {
                if let Some((kind, frac)) = tele.get(&(cx, cy)) {
                    let key = match kind {
                        TelegraphKind::Strike => "strike",
                        TelegraphKind::Heavy => "heavy",
                        TelegraphKind::Area => "area",
                        TelegraphKind::Shockwave => "shockwave",
                        TelegraphKind::Hazard => "hazard",
                        TelegraphKind::Weak => "weak",
                    };
                    let ts = &t.telegraphs[key];
                    let urgency = 1.0 - frac;
                    glyph = t.pick(&ts.glyph, &ts.ascii);
                    fg_name = &ts.fg;
                    bg_name = &ts.bg;
                    tile_light = 0.55 + 0.45 * urgency;
                    if urgency > 0.75 {
                        modifier = Modifier::BOLD;
                    }
                }
            }
            let fg = t.shade(fg_name, tile_light);
            let bg =
                t.shade(bg_name, if visible { 0.45 + 0.55 * tile_light.min(1.0) } else { t.light.remembered + 0.1 });
            put(buf, area, x, y, glyph, fg, Some(bg), modifier);
        }
    }

    if motion {
        for &(lx, ly) in &lanterns {
            if !m.is_visible(lx, ly) {
                continue;
            }
            for i in 0..3 {
                let phase = (secs * 0.6 + i as f32 * 0.33 + lx as f32 * 0.1).fract();
                let ex = lx + ((secs * 1.3 + i as f32 * 2.1).sin() * 1.5).round() as i32;
                let ey = ly - (phase * 3.0) as i32 - 1;
                let (sx, sy) = to_screen(ex, ey);
                if m.is_visible(ex, ey) && m.tile(ex, ey).walkable() {
                    put(
                        buf,
                        area,
                        sx,
                        sy,
                        t.pick("·", "."),
                        t.shade("amber_hi", 1.0 - phase * 0.6),
                        None,
                        Modifier::empty(),
                    );
                }
            }
        }
    }

    let mut ents: Vec<_> = m.entities.values().collect();
    ents.sort_by_key(|e| match e.view.kind {
        EntityKind::Loot | EntityKind::Remnant => 0,
        EntityKind::Npc => 1,
        EntityKind::Enemy | EntityKind::Boss => 2,
        EntityKind::Projectile => 3,
        EntityKind::Player => {
            if e.view.id == m.me {
                5
            } else {
                4
            }
        }
    });
    for e in ents {
        let v = &e.view;
        let Some((x, y)) = m.render_pos(v.id, now) else { continue };
        let (cx, cy) = (x.div_euclid(CELL), y.div_euclid(CELL));
        let always = v.id == m.me || v.rel == Relation::Party;
        if !always && !m.is_visible(cx, cy) {
            continue;
        }
        let (sx, sy) = to_screen(cx, cy);
        let flash = e.flash_until.is_some_and(|u| u > now);
        let lit_bg = |name: &str| t.shade(name, 0.55);
        match v.kind {
            EntityKind::Player => {
                let (glyph, hue) = player_glyph(t, v.look.unwrap_or_default());
                let mut md = Modifier::BOLD;
                let bg = if v.id == m.me { Some(lit_bg("slate_hi")) } else { None };
                if v.rel == Relation::Party {
                    md |= Modifier::UNDERLINED;
                }
                if flash {
                    md |= Modifier::REVERSED;
                }
                let glyph = if v.state == ActorState::Dead { t.pick("%", "%") } else { glyph };
                put(buf, area, sx, sy, glyph, t.color(hue), bg, md);
                if v.id != m.me {
                    if let Some(name) = &v.name {
                        let label = if v.rel == Relation::Party { format!("{name}+") } else { name.clone() };
                        let lx = sx - label.width() as i32 / 2;
                        text(
                            buf,
                            area,
                            lx,
                            sy - 1,
                            &label,
                            Style::default().fg(t.color(if v.rel == Relation::Party { "sickly_hi" } else { "ash" })),
                        );
                    }
                }
                if v.state == ActorState::Guarding {
                    let (dx, dy) = v.facing.offset();
                    put(
                        buf,
                        area,
                        sx + dx,
                        sy + dy,
                        t.pick(if dx == 0 { "─" } else { "│" }, "]"),
                        t.color("stone"),
                        None,
                        Modifier::empty(),
                    );
                }
            }
            EntityKind::Npc => {
                let g = t.entities.npc.get(&v.def);
                let (glyph, fg) = g.map(|g| (t.pick(&g.glyph, &g.ascii), g.fg.as_str())).unwrap_or(("&", "bone"));
                put(buf, area, sx, sy, glyph, t.color(fg), None, Modifier::BOLD);
                if let Some(name) = &v.name {
                    text(buf, area, sx - name.width() as i32 / 2, sy - 1, name, Style::default().fg(t.color("gold")));
                }
            }
            EntityKind::Enemy => {
                let g = t.entities.enemy.get(&v.def);
                let (glyph, fg) = g.map(|g| (t.pick(&g.glyph, &g.ascii), g.fg.as_str())).unwrap_or(("e", "blood_hi"));
                let mut md = Modifier::BOLD;
                if flash {
                    md |= Modifier::REVERSED;
                }
                let (glyph, fg) = match v.state {
                    ActorState::Dead => (t.pick("%", "%"), "stone"),
                    ActorState::Windup => (glyph, "amber_hi"),
                    ActorState::Staggered => (glyph, "fog_hi"),
                    _ => (glyph, fg),
                };
                put(buf, area, sx, sy, glyph, t.color(fg), None, md);
                match v.state {
                    ActorState::Windup => text(
                        buf,
                        area,
                        sx,
                        sy - 1,
                        "!",
                        Style::default().fg(t.color("amber_hi")).add_modifier(Modifier::BOLD),
                    ),
                    ActorState::Staggered => text(buf, area, sx, sy - 1, "?", Style::default().fg(t.color("fog_hi"))),
                    _ => {}
                }
                if let Some([hp, max]) = v.hp {
                    if hp < max && v.state != ActorState::Dead {
                        let pips = ((hp as f32 / max as f32) * 3.0).ceil() as usize;
                        let s = format!("{}{}", t.pick("▰", "=").repeat(pips), t.pick("▱", "-").repeat(3 - pips));
                        text(buf, area, sx - 1, sy + 1, &s, Style::default().fg(t.color("blood_hi")));
                    }
                }
            }
            EntityKind::Boss => {
                let Some(b) = t.entities.boss.get(&v.def) else { continue };
                let cracked = v.phase.unwrap_or(1) >= 2;
                let rows = if t.ascii {
                    &b.ascii
                } else if cracked {
                    &b.sprite_cracked
                } else {
                    &b.sprite
                };
                let fg = if cracked && (!motion || (secs * 5.0).sin() > -0.3) { &b.phase2 } else { &b.fg };
                let mut md = Modifier::BOLD;
                if flash {
                    md |= Modifier::REVERSED;
                }
                if v.state == ActorState::Windup {
                    md |= Modifier::UNDERLINED;
                }
                for (i, row) in rows.iter().enumerate() {
                    for (j, ch) in row.chars().enumerate() {
                        put(
                            buf,
                            area,
                            sx - 1 + j as i32,
                            sy - 1 + i as i32,
                            &ch.to_string(),
                            t.color(if v.state == ActorState::Windup { "amber_hi" } else { fg }),
                            None,
                            md,
                        );
                    }
                }
                if v.state == ActorState::Windup {
                    text(
                        buf,
                        area,
                        sx,
                        sy - 2,
                        "!",
                        Style::default().fg(t.color("amber_hi")).add_modifier(Modifier::BOLD),
                    );
                }
            }
            EntityKind::Projectile => {
                let g = t.entities.projectile.get(&v.def).unwrap_or(&t.entities.projectile["default"]);
                put(buf, area, sx, sy, t.pick(&g.glyph, &g.ascii), t.color(&g.fg), None, Modifier::BOLD);
            }
            EntityKind::Loot => {
                let key = v.def.ends_with("_key") || v.def == "tallow_cup";
                let g = if key { &t.entities.key_loot } else { &t.entities.loot };
                let pulse = if motion && (secs * 3.0).sin() > 0.0 { "amber_hi" } else { g.fg.as_str() };
                put(buf, area, sx, sy, t.pick(&g.glyph, &g.ascii), t.color(pulse), None, Modifier::BOLD);
            }
            EntityKind::Remnant => {
                let g = &t.entities.remnant;
                let pulse = if motion && (secs * 2.0).sin() > 0.0 { "sickly_hi" } else { "sickly" };
                put(buf, area, sx, sy, t.pick(&g.glyph, &g.ascii), t.color(pulse), None, Modifier::BOLD);
            }
        }
    }

    for (i, fx) in m.effects.iter().enumerate() {
        let age = fx.age(now);
        let spread = (i % 3) as i32 - 1;
        let (cx, cy) = (fx.x.div_euclid(CELL), fx.y.div_euclid(CELL));
        let (sx, sy) = to_screen(cx, cy);
        match &fx.kind {
            FxKind::Number { text: s, color } => {
                let rise = if motion { (age * 2.0) as i32 } else { 0 };
                text(
                    buf,
                    area,
                    sx - s.width() as i32 / 2 + spread * 2,
                    sy - 1 - rise,
                    s,
                    Style::default().fg(t.shade(color, 1.0 - age * 0.5)).add_modifier(Modifier::BOLD),
                );
            }
            FxKind::Swing { facing, heavy } => {
                let idx = Dir8::ALL.iter().position(|d| d == facing).unwrap_or(0) as i32;
                for k in -1..=1 {
                    let d = Dir8::ALL[((idx + k).rem_euclid(8)) as usize];
                    let (dx, dy) = d.offset();
                    let g = match (d, heavy) {
                        (Dir8::N | Dir8::S, false) => t.pick("│", "|"),
                        (Dir8::N | Dir8::S, true) => t.pick("┃", "|"),
                        (Dir8::E | Dir8::W, false) => t.pick("─", "-"),
                        (Dir8::E | Dir8::W, true) => t.pick("━", "="),
                        (Dir8::Ne | Dir8::Sw, _) => t.pick("╱", "/"),
                        _ => t.pick("╲", "\\"),
                    };
                    if m.tile(cx + dx, cy + dy).walkable() {
                        put(buf, area, sx + dx, sy + dy, g, t.shade("bone", 1.0 - age * 0.6), None, Modifier::BOLD);
                    }
                }
            }
            FxKind::Burst { color } => {
                for (i, d) in Dir8::ALL.iter().enumerate() {
                    if i % 2 == 1 && age > 0.5 {
                        continue;
                    }
                    let r = if motion { (age * 2.5) as i32 + 1 } else { 1 };
                    let (dx, dy) = d.offset();
                    put(
                        buf,
                        area,
                        sx + dx * r,
                        sy + dy * r,
                        t.pick("·", "."),
                        t.shade(color, 1.0 - age),
                        None,
                        Modifier::empty(),
                    );
                }
            }
            FxKind::Ring { radius, color } => {
                let r = radius * if motion { age } else { 1.0 };
                let steps = (r * 8.0).max(8.0) as i32;
                for i in 0..steps {
                    let a = i as f32 / steps as f32 * std::f32::consts::TAU;
                    let (rx, ry) = ((a.cos() * r).round() as i32, (a.sin() * r).round() as i32);
                    if m.is_visible(cx + rx, cy + ry) {
                        put(
                            buf,
                            area,
                            sx + rx,
                            sy + ry,
                            t.pick("◌", "o"),
                            t.shade(color, 1.0 - age * 0.7),
                            None,
                            Modifier::empty(),
                        );
                    }
                }
            }
        }
    }

    if let Some(me) = &m.me_view {
        if let Some(b) = &me.boss {
            let w = (area.width as usize).saturating_sub(20).min(60);
            let filled = (b.hp as usize * w).div_ceil(b.hp_max.max(1) as usize).min(w);
            let label = format!("{}{}", b.name, if b.phase >= 2 { " — cracked" } else { "" });
            let y = area.y as i32 + area.height as i32 - 2;
            let x = area.x as i32 + (area.width as i32 - w as i32) / 2;
            text(buf, area, x, y - 1, &label, Style::default().fg(t.ui("boss")).add_modifier(Modifier::BOLD));
            if b.vulnerable {
                text(
                    buf,
                    area,
                    x + label.width() as i32 + 2,
                    y - 1,
                    "OPEN — STRIKE",
                    Style::default().fg(t.color("amber_hi")).add_modifier(Modifier::BOLD),
                );
            }
            let full = t.pick("█", "#").repeat(filled);
            let empty = t.pick("░", "-").repeat(w - filled);
            text(buf, area, x, y, &full, Style::default().fg(t.color("gold")));
            text(buf, area, x + filled as i32, y, &empty, Style::default().fg(t.color("blood")));
        }
    }

    for (i, (msg, _)) in m.toasts.iter().rev().take(4).enumerate() {
        let s = format!(" {msg} ");
        let x = area.x as i32 + area.width as i32 - s.width() as i32 - 1;
        text(buf, area, x, area.y as i32 + i as i32, &s, Style::default().fg(t.ui("text")).bg(t.color("soot")));
    }

    if let Some(b) = &m.banner {
        let y = area.y as i32 + area.height as i32 / 3;
        let spaced: String = if t.ascii {
            b.text.clone()
        } else {
            b.text.chars().flat_map(|c| [c, ' ']).collect::<String>().trim_end().to_string()
        };
        let x = area.x as i32 + (area.width as i32 - spaced.width() as i32) / 2;
        let rule = t.pick("─", "-").repeat(spaced.width() + 6);
        let rx = area.x as i32 + (area.width as i32 - rule.width() as i32) / 2;
        text(buf, area, rx, y - 1, &rule, Style::default().fg(t.shade(b.color, 0.5)));
        text(buf, area, x, y, &spaced, Style::default().fg(t.color(b.color)).add_modifier(Modifier::BOLD));
        text(buf, area, rx, y + 1, &rule, Style::default().fg(t.shade(b.color, 0.5)));
        if !b.sub.is_empty() {
            text(
                buf,
                area,
                area.x as i32 + (area.width as i32 - b.sub.width() as i32) / 2,
                y + 2,
                &b.sub,
                Style::default().fg(t.ui("muted")),
            );
        }
    }
}

fn chat_line<'a>(t: &Theme, c: &crate::model::ChatLine) -> Line<'a> {
    let (tag, color) = match c.channel {
        ChatChannel::Local => ("", t.ui_name("local")),
        ChatChannel::Party => ("[party] ", t.ui_name("party_chat")),
        ChatChannel::System => ("", t.ui_name("system")),
    };
    let mut spans = vec![Span::styled(tag.to_string(), Style::default().fg(t.color(color)))];
    if !c.from.is_empty() {
        spans.push(Span::styled(format!("{}: ", c.from), Style::default().fg(t.color("gold"))));
    }
    spans.push(Span::styled(c.text.clone(), Style::default().fg(t.color(color))));
    Line::from(spans)
}

fn draw_chat(f: &mut Frame, app: &App, m: &Model, area: Rect) {
    let t = &app.theme;
    let inner = area.height.saturating_sub(2) as usize;
    let lines: Vec<Line> = m.chat.iter().rev().take(inner).rev().map(|c| chat_line(t, c)).collect();
    f.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }).block(panel(t, "voices  (Enter to speak)", false)),
        area,
    );
}

fn draw_footer(f: &mut Frame, app: &App, m: &Model, area: Rect) {
    let t = &app.theme;
    let first = m.chat.back().map(|c| chat_line(t, c)).unwrap_or_else(|| Line::from(""));
    let hints = Line::styled(
        "WASD move  J/K hit  Space roll  L guard  I parry  H drink  F use  Tab gear  ? help",
        Style::default().fg(t.ui("muted")),
    );
    f.render_widget(Paragraph::new(vec![first, hints]), area);
}

fn equip_name(m: &Model, id: &Option<String>) -> String {
    match id {
        Some(i) => m.sheet.inventory.iter().find(|s| &s.item == i).map(|s| s.name.clone()).unwrap_or_else(|| i.clone()),
        None => "—".into(),
    }
}

fn draw_sidebar(f: &mut Frame, app: &App, m: &Model, area: Rect) {
    let t = &app.theme;
    let s = &m.sheet;
    let muted = Style::default().fg(t.ui("muted"));
    let text_s = Style::default().fg(t.ui("text"));
    let mut lines = vec![
        Line::from(vec![
            Span::styled(s.name.clone(), Style::default().fg(t.color("amber_hi")).add_modifier(Modifier::BOLD)),
            Span::styled(format!("  {}", s.archetype.label()), muted),
        ]),
        Line::from(vec![
            Span::styled(format!("level {}  ", s.level), text_s),
            Span::styled(format!("next {} {}", s.next_level_cost, t.pick("✧", "*")), muted),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled("hand  ", muted), Span::styled(equip_name(m, &s.equipment.main), text_s)]),
        Line::from(vec![Span::styled("off   ", muted), Span::styled(equip_name(m, &s.equipment.off), text_s)]),
        Line::from(vec![Span::styled("body  ", muted), Span::styled(equip_name(m, &s.equipment.armor), text_s)]),
        Line::from(vec![Span::styled("rite1 ", muted), Span::styled(equip_name(m, &s.equipment.spell1), text_s)]),
        Line::from(vec![Span::styled("rite2 ", muted), Span::styled(equip_name(m, &s.equipment.spell2), text_s)]),
        Line::from(""),
        Line::styled("PARTY", Style::default().fg(t.ui("title"))),
    ];
    if m.party.is_empty() {
        lines.push(Line::styled("alone. /invite name", muted));
    }
    for p in &m.party {
        let hp = p.hp.map(|[h, mx]| format!(" {}%", h * 100 / mx.max(1))).unwrap_or_default();
        lines.push(Line::styled(
            format!("{}{}{}", if p.leader { "★ " } else { "  " }, p.name, hp),
            Style::default().fg(t.ui("party")),
        ));
    }
    for i in &m.invites {
        lines.push(Line::styled(format!("invite from {i}: /accept {i}"), Style::default().fg(t.color("amber"))));
    }
    lines.push(Line::from(""));
    lines.push(Line::styled("PATH", Style::default().fg(t.ui("title"))));
    let active: Vec<_> = s.quests.iter().filter(|q| !q.done).collect();
    if active.is_empty() {
        lines.push(Line::styled("Speak with those who remember.", muted));
    }
    for q in active {
        lines.push(Line::styled(q.title.clone(), Style::default().fg(t.color("gold"))));
        lines.push(Line::styled(q.note.clone(), text_s));
    }
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }).block(panel(t, "hollowbound", false)), area);
}

fn draw_character(f: &mut Frame, app: &App, m: &Model, area: Rect, sel: usize) {
    let t = &app.theme;
    let r = centered(area, 78, 22);
    f.render_widget(Clear, r);
    let block = panel(t, "gear & echoes   ↑↓ select  Enter equip  2 rite slot 2  U use  Tab close", true);
    let inner = block.inner(r);
    f.render_widget(block, r);
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(34), Constraint::Min(20)])
        .split(inner);
    let s = &m.sheet;
    let eq = |id: &str| {
        [&s.equipment.main, &s.equipment.off, &s.equipment.armor, &s.equipment.spell1, &s.equipment.spell2]
            .iter()
            .any(|e| e.as_deref() == Some(id))
    };
    let items: Vec<Line> = s
        .inventory
        .iter()
        .enumerate()
        .map(|(i, it)| {
            let mark = if eq(&it.item) { "E" } else { " " };
            let qty = if it.qty > 1 { format!(" x{}", it.qty) } else { String::new() };
            let style = if i == sel {
                Style::default().fg(t.color("amber_hi")).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(t.ui("text"))
            };
            Line::styled(format!("{}{mark} {}{qty}", if i == sel { "▸" } else { " " }, it.name), style)
        })
        .collect();
    f.render_widget(Paragraph::new(items), cols[0]);
    let d = &s.stats;
    let mut right = vec![];
    if let Some(it) = s.inventory.get(sel) {
        right.push(Line::styled(
            format!("{} ({})", it.name, it.kind),
            Style::default().fg(t.color("gold")).add_modifier(Modifier::BOLD),
        ));
        right.push(Line::styled(it.desc.clone(), Style::default().fg(t.ui("text"))));
        right.push(Line::from(""));
    }
    let names = ["Vitality", "Endurance", "Might", "Finesse", "Occult"];
    right.push(Line::styled(
        format!("level {}   echoes {}   next {}", s.level, s.echoes, s.next_level_cost),
        Style::default().fg(t.ui("echoes")),
    ));
    for (i, n) in names.iter().enumerate() {
        right.push(Line::styled(format!("{n:<10} {:>3}", s.attrs[i]), Style::default().fg(t.ui("text"))));
    }
    right.push(Line::styled(
        format!("health {}  stamina {}  wick {}", d.hp_max, d.stamina_max, d.wick_max),
        Style::default().fg(t.ui("muted")),
    ));
    right.push(Line::styled(
        format!("defense {}  light {}  heavy {}  rite {}", d.defense, d.light_damage, d.heavy_damage, d.spell_power),
        Style::default().fg(t.ui("muted")),
    ));
    right.push(Line::from(""));
    for q in &s.quests {
        right.push(Line::styled(
            format!("{} {}", if q.done { "✓" } else { "·" }, q.title),
            Style::default().fg(t.color(if q.done { "stone" } else { "gold" })),
        ));
        if !q.done {
            right.push(Line::styled(format!("  {}", q.note), Style::default().fg(t.ui("text"))));
        }
    }
    f.render_widget(Paragraph::new(right).wrap(Wrap { trim: true }), cols[1]);
}

fn draw_level(f: &mut Frame, app: &App, m: &Model, area: Rect, sel: usize) {
    let t = &app.theme;
    let r = centered(area, 56, 13);
    f.render_widget(Clear, r);
    let s = &m.sheet;
    let names = [
        ("Vitality", "more health"),
        ("Endurance", "more stamina"),
        ("Might", "heavy weapon damage"),
        ("Finesse", "light weapon damage"),
        ("Occult", "rites and wick"),
    ];
    let mut lines = vec![Line::styled(
        format!("Echoes {}   cost of the next level {}", s.echoes, s.next_level_cost),
        Style::default().fg(t.ui("echoes")),
    )];
    lines.push(Line::from(""));
    for (i, (n, why)) in names.iter().enumerate() {
        let style = if i == sel {
            Style::default().fg(t.color("amber_hi")).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(t.ui("text"))
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{}{n:<10} {:>3}  ", if i == sel { "▸ " } else { "  " }, s.attrs[i]), style),
            Span::styled(why.to_string(), Style::default().fg(t.ui("muted"))),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::styled(
        "Only at a lit lantern you have rested at. Enter: invest  Esc: close",
        Style::default().fg(t.ui("muted")),
    ));
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }).block(panel(t, "invest echoes", true)), r);
}

fn draw_dialogue(f: &mut Frame, app: &App, m: &Model, area: Rect, sel: usize) {
    let t = &app.theme;
    let Some(d) = &m.dialogue else { return };
    let r = Rect {
        x: area.x + 2,
        y: area.y + area.height.saturating_sub(14),
        width: area.width.saturating_sub(4).min(100),
        height: 13.min(area.height),
    };
    f.render_widget(Clear, r);
    let mut lines: Vec<Line> = d
        .lines
        .iter()
        .map(|l| Line::styled(crate::model::sanitize_display(l), Style::default().fg(t.ui("text"))))
        .collect();
    lines.push(Line::from(""));
    for (i, c) in d.choices.iter().enumerate() {
        let style = if i == sel {
            Style::default().fg(t.color("amber_hi")).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(t.color("gold"))
        };
        lines.push(Line::styled(
            format!("{} {}. {}", if i == sel { "▸" } else { " " }, i + 1, crate::model::sanitize_display(c)),
            style,
        ));
    }
    lines.push(Line::styled(
        if d.choices.is_empty() { "Enter or Esc to close" } else { "↑↓ or 1-9 to answer, Esc to leave" },
        Style::default().fg(t.ui("muted")),
    ));
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }).block(panel(t, &d.speaker, true)), r);
}

fn draw_map(f: &mut Frame, app: &App, m: &Model, area: Rect, now: Instant) {
    let t = &app.theme;
    let r = centered(area, area.width.saturating_sub(2), area.height.saturating_sub(2));
    f.render_widget(Clear, r);
    let block = panel(t, "map of Veyr   (M or Esc to close)", true);
    let inner = block.inner(r);
    f.render_widget(block, r);
    let buf = f.buffer_mut();
    let half = !t.ascii;
    let rows_per = if half { 2 } else { 1 };
    let me = m.my_cell(now).unwrap_or((0, 0));
    let vw = inner.width as i32;
    let vh = inner.height as i32 * rows_per;
    let ox = if m.width <= vw { -(vw - m.width) / 2 } else { (me.0 - vw / 2).clamp(0, m.width - vw) };
    let oy = if m.height <= vh { -(vh - m.height) / 2 } else { (me.1 - vh / 2).clamp(0, m.height - vh) };
    let color_of = |x: i32, y: i32| -> Color {
        if !m.is_explored(x, y) {
            return t.color("void");
        }
        match m.tile(x, y) {
            Tile::Void => t.color("void"),
            Tile::Wall | Tile::FalseWall => t.color("slate"),
            Tile::Tree => t.color("moss"),
            Tile::Water => t.color("water"),
            Tile::Lantern => t.color("amber_hi"),
            Tile::EmberPit => t.color("rust"),
            Tile::FogGate => t.color("fog_hi"),
            Tile::Door | Tile::Shortcut => t.color("rust"),
            _ => t.shade("stone", 0.6),
        }
    };
    for sy in 0..inner.height as i32 {
        for sx in 0..vw {
            let x = ox + sx;
            let y0 = oy + sy * rows_per;
            let (px, py) = (inner.x as i32 + sx, inner.y as i32 + sy);
            if half {
                put(buf, inner, px, py, "▀", color_of(x, y0), Some(color_of(x, y0 + 1)), Modifier::empty());
            } else {
                let g = if m.is_explored(x, y0) { t.tiles[&m.tile(x, y0).code()].ascii.as_str() } else { " " };
                put(buf, inner, px, py, g, color_of(x, y0), None, Modifier::empty());
            }
        }
    }
    let at = |x: i32, y: i32| (inner.x as i32 + x - ox, inner.y as i32 + (y - oy) / rows_per);
    for l in &m.map.lanterns {
        let (x, y) = (l.x as i32, l.y as i32);
        if m.is_explored(x, y) {
            let (sx, sy) = at(x, y);
            let known = m.sheet.lanterns.contains(&l.id);
            put(
                buf,
                inner,
                sx,
                sy,
                t.pick("☼", "*"),
                t.color(if known { "amber_hi" } else { "stone" }),
                None,
                Modifier::BOLD,
            );
            let label = l.name.clone();
            text(buf, inner, sx + 2, sy, &label, Style::default().fg(t.color("gold")));
        }
    }
    if let Some(me_v) = &m.me_view {
        if let Some([rx, ry]) = me_v.remnant {
            let (sx, sy) = at(rx / CELL, ry / CELL);
            put(buf, inner, sx, sy, t.pick("◊", "&"), t.color("sickly_hi"), None, Modifier::BOLD);
        }
    }
    let (sx, sy) = at(me.0, me.1);
    let blink = app.settings.reduced_motion || (now.duration_since(app.started).as_millis() / 400).is_multiple_of(2);
    put(buf, inner, sx, sy, "@", t.color(if blink { "bone" } else { "amber_hi" }), None, Modifier::BOLD);
}

fn draw_help(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let r = centered(area, 78, 22);
    f.render_widget(Clear, r);
    let k = |a: &str, b: &str| {
        Line::from(vec![
            Span::styled(format!("{a:>18}  "), Style::default().fg(t.color("amber_hi"))),
            Span::styled(b.to_string(), Style::default().fg(t.ui("text"))),
        ])
    };
    let mode = app.input_mode_label();
    let lines = vec![
        k("WASD / arrows", "move (Q E Z C diagonals)"),
        k("J / K", "light / heavy attack (turns toward a foe in reach)"),
        k("Space", "dodge roll: brief invulnerability, costs stamina"),
        k("L", "guard (hold, or toggle without key release support)"),
        k("I", "parry with a shield: tight window, staggers the attacker"),
        k("H", "drink a tallow draught (slow, can be interrupted)"),
        k("1 / 2", "cast prepared rites"),
        k("F", "talk, rest at a lantern, open doors, touch walls"),
        k("V", "invest echoes (at a rested lantern)"),
        k("Tab / M", "gear and quests / map"),
        k("Enter", "chat: /p party  /invite /accept /leave /mute /report"),
        k("Esc", "menu: glyphs, colours, contrast, motion, input, leave"),
        Line::from(""),
        Line::styled(
            "Reading danger: '!' over a foe means it is winding up. Tinted cells show where it will land:",
            Style::default().fg(t.ui("muted")),
        ),
        Line::styled(
            "× strike   ▓ heavy   ░ area   ◌ shockwave (roll through it)   ≈ lingering   + weak point",
            Style::default().fg(t.ui("text")),
        ),
        Line::styled(
            "Brighter, bolder cells hit sooner. Dying leaves your echoes as a ◊ where you fell.",
            Style::default().fg(t.ui("muted")),
        ),
        Line::from(""),
        Line::styled(format!("movement input: {mode}"), Style::default().fg(t.ui("muted"))),
    ];
    f.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: true }).block(panel(t, "how to stay alive (any key closes)", true)),
        r,
    );
}

fn draw_menu(f: &mut Frame, app: &App, area: Rect, sel: usize) {
    let t = &app.theme;
    let r = centered(area, 52, 13);
    f.render_widget(Clear, r);
    let values = [
        String::new(),
        if t.ascii { "ASCII".into() } else { "Unicode".into() },
        app.color_label().to_string(),
        if t.high_contrast { "on".into() } else { "off".into() },
        if app.settings.reduced_motion { "on".into() } else { "off".into() },
        app.input_mode_label().to_string(),
        String::new(),
        String::new(),
    ];
    let lines: Vec<Line> = App::MENU
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let style = if i == sel {
                Style::default().fg(t.color("amber_hi")).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(t.ui("text"))
            };
            Line::from(vec![
                Span::styled(format!("{}{label}", if i == sel { "▸ " } else { "  " }), style),
                Span::styled(
                    if values[i].is_empty() { String::new() } else { format!("  [{}]", values[i]) },
                    Style::default().fg(t.ui("muted")),
                ),
            ])
        })
        .collect();
    f.render_widget(Paragraph::new(lines).block(panel(t, "menu", true)), r);
}
