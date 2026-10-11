use std::collections::HashSet;
use std::time::{Duration, Instant};

use hollowrite_protocol::*;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use tokio::sync::mpsc::UnboundedSender;

use crate::model::Model;
use crate::net::{self, NetEvent, Stored};
use crate::theme::{ColorMode, Theme};

/// Without key-release events a single press holds the direction long
/// enough to bridge the OS key-repeat delay; repeats then extend it briefly.
const FIRST_HOLD: Duration = Duration::from_millis(480);
const REPEAT_HOLD: Duration = Duration::from_millis(140);
const INPUT_HEARTBEAT: Duration = Duration::from_millis(100);

pub enum AppEvent {
    Key(KeyEvent),
    Resize,
    Auth(Result<String, String>),
}

#[derive(Default)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
    pub field: usize,
    pub register: bool,
    pub error: Option<String>,
    pub busy: bool,
}

pub struct CreateForm {
    pub name: String,
    pub archetype: usize,
    pub sigil: u8,
    pub hue: u8,
    pub field: usize,
}

#[derive(Default)]
pub struct Lobby {
    pub chars: Vec<CharacterSummary>,
    pub sel: usize,
    pub create: Option<CreateForm>,
    pub error: Option<String>,
    pub loaded: bool,
}

pub enum Screen {
    Login(LoginForm),
    Connecting(String),
    Lobby(Lobby),
    Game,
}

#[derive(Clone, PartialEq, Eq)]
pub enum Overlay {
    None,
    Character { sel: usize },
    Level { sel: usize },
    Dialogue { sel: usize },
    Map,
    Help,
    Chat { text: String },
    Menu { sel: usize },
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Auto,
    Hold,
    Repeat,
}

pub struct Settings {
    pub reduced_motion: bool,
    pub input_mode: InputMode,
    pub remember: bool,
}

pub struct Input {
    pub enhanced: bool,
    held_keys: HashSet<Dir8>,
    emulated: Option<(Dir8, Instant)>,
    seq: u32,
    last_sent: Instant,
    last_dir: Option<Dir8>,
    pub guard: bool,
}

pub struct App {
    pub server: String,
    pub theme: Theme,
    pub settings: Settings,
    pub screen: Screen,
    pub overlay: Overlay,
    pub model: Option<Model>,
    pub net: Option<UnboundedSender<ClientMsg>>,
    pub token: Option<String>,
    pub username: String,
    pub input: Input,
    pub welcome: (i32, i32, u32),
    pub status: Option<String>,
    pub quit: bool,
    pub started: Instant,
    events: UnboundedSender<AppEvent>,
    net_events: UnboundedSender<NetEvent>,
    last_frame: Instant,
}

fn dir_for(code: KeyCode) -> Option<Dir8> {
    Some(match code {
        KeyCode::Char('w' | 'W') | KeyCode::Up => Dir8::N,
        KeyCode::Char('s' | 'S') | KeyCode::Down => Dir8::S,
        KeyCode::Char('a' | 'A') | KeyCode::Left => Dir8::W,
        KeyCode::Char('d' | 'D') | KeyCode::Right => Dir8::E,
        KeyCode::Char('q' | 'Q') | KeyCode::Home => Dir8::Nw,
        KeyCode::Char('e' | 'E') | KeyCode::PageUp => Dir8::Ne,
        KeyCode::Char('z' | 'Z') | KeyCode::End => Dir8::Sw,
        KeyCode::Char('c' | 'C') | KeyCode::PageDown => Dir8::Se,
        _ => return None,
    })
}

fn combine(keys: &HashSet<Dir8>) -> Option<Dir8> {
    let (mut dx, mut dy) = (0, 0);
    for d in keys {
        let (x, y) = d.offset();
        dx += x;
        dy += y;
    }
    Dir8::from_offset(dx.clamp(-1, 1), dy.clamp(-1, 1))
}

impl App {
    pub fn new(
        server: String,
        theme: Theme,
        settings: Settings,
        events: UnboundedSender<AppEvent>,
        net_events: UnboundedSender<NetEvent>,
        enhanced: bool,
    ) -> App {
        let now = Instant::now();
        App {
            server,
            theme,
            settings,
            screen: Screen::Login(LoginForm::default()),
            overlay: Overlay::None,
            model: None,
            net: None,
            token: None,
            username: String::new(),
            input: Input {
                enhanced,
                held_keys: HashSet::new(),
                emulated: None,
                seq: 0,
                last_sent: now,
                last_dir: None,
                guard: false,
            },
            welcome: (120, 300, 30),
            status: None,
            quit: false,
            started: now,
            events,
            net_events,
            last_frame: now,
        }
    }

    pub fn resume(&mut self) {
        if let Some(stored) = net::load_session(&self.server) {
            self.username = stored.username.clone();
            self.connect(stored.token);
        }
    }

    fn connect(&mut self, token: String) {
        self.screen = Screen::Connecting(format!("Connecting to {}...", self.server));
        self.token = Some(token.clone());
        self.net = Some(net::spawn(&self.server, &token, self.net_events.clone()));
    }

    pub fn send(&self, msg: ClientMsg) {
        if let Some(n) = &self.net {
            let _ = n.send(msg);
        }
    }

    fn uses_release_events(&self) -> bool {
        match self.settings.input_mode {
            InputMode::Hold => true,
            InputMode::Repeat => false,
            InputMode::Auto => self.input.enhanced,
        }
    }

    pub fn held_dir(&self, now: Instant) -> Option<Dir8> {
        if self.uses_release_events() {
            combine(&self.input.held_keys)
        } else {
            self.input.emulated.filter(|(_, until)| *until > now).map(|(d, _)| d)
        }
    }

    fn send_input(&mut self, act: Option<Action>) {
        let now = Instant::now();
        let mv = self.held_dir(now);
        self.input.seq += 1;
        let seq = self.input.seq;
        self.input.last_sent = now;
        self.input.last_dir = mv;
        self.send(ClientMsg::Input { seq, mv, act, face: None });
    }

    // ---- network ---------------------------------------------------

    pub fn on_net(&mut self, ev: NetEvent) {
        match ev {
            NetEvent::Rtt(d) => {
                if let Some(m) = &mut self.model {
                    m.rtt = Some(match m.rtt {
                        Some(old) => (old * 3 + d) / 4,
                        None => d,
                    });
                }
            }
            NetEvent::Closed(reason) => {
                self.net = None;
                let lower = reason.to_lowercase();
                if lower.contains("401") || lower.contains("unauthorized") {
                    net::clear_session();
                    self.token = None;
                    self.screen = Screen::Login(LoginForm {
                        error: Some("Session expired. Please sign in again.".into()),
                        ..Default::default()
                    });
                } else {
                    let error = Some(format!("Disconnected: {reason}"));
                    self.screen =
                        Screen::Login(LoginForm { username: self.username.clone(), error, ..Default::default() });
                }
                self.model = None;
                self.overlay = Overlay::None;
            }
            NetEvent::Msg(msg) => self.on_server(*msg),
        }
    }

    fn on_server(&mut self, msg: ServerMsg) {
        match msg {
            ServerMsg::Welcome { account, walk_speed, body_radius, tick_hz, .. } => {
                self.username = account;
                self.welcome = (walk_speed, body_radius, tick_hz);
                self.screen = Screen::Lobby(Lobby::default());
                self.send(ClientMsg::ListCharacters);
            }
            ServerMsg::Characters { list } => {
                let creating = list.is_empty();
                let mut lobby = Lobby { chars: list, loaded: true, ..Default::default() };
                if creating {
                    lobby.create = Some(CreateForm { name: String::new(), archetype: 0, sigil: 0, hue: 0, field: 0 });
                }
                self.screen = Screen::Lobby(lobby);
                self.model = None;
                self.overlay = Overlay::None;
            }
            ServerMsg::Error { code, message } => {
                let message = crate::model::sanitize_display(&message);
                match &mut self.screen {
                    Screen::Lobby(l) => l.error = Some(message),
                    Screen::Game => {
                        if let Some(m) = &mut self.model {
                            m.toast(message);
                        }
                    }
                    Screen::Login(f) => f.error = Some(message),
                    Screen::Connecting(_) => {
                        self.screen = Screen::Login(LoginForm { error: Some(message), ..Default::default() });
                    }
                }
                if code == ErrorCode::ProtocolMismatch {
                    self.net = None;
                }
            }
            ServerMsg::EnterWorld { you, map, sheet, .. } => {
                let (w, r, hz) = self.welcome;
                self.model = Some(Model::new(you, map, sheet, w, r, hz));
                self.screen = Screen::Game;
                self.overlay = Overlay::None;
                self.input.held_keys.clear();
                self.input.emulated = None;
            }
            ServerMsg::Snapshot(s) => {
                if let Some(m) = &mut self.model {
                    m.apply_snapshot(s);
                }
            }
            ServerMsg::Sheet { sheet } => {
                if let Some(m) = &mut self.model {
                    m.sheet = sheet;
                }
            }
            ServerMsg::Events { events, .. } => {
                if let Some(m) = &mut self.model {
                    m.apply_events(events);
                }
            }
            ServerMsg::Chat { channel, from, text } => {
                if let Some(m) = &mut self.model {
                    m.push_chat(channel, from, text);
                }
            }
            ServerMsg::Party { members, invites } => {
                if let Some(m) = &mut self.model {
                    m.party = members;
                    m.invites = invites;
                }
            }
            ServerMsg::Dialogue { npc, speaker, lines, choices } => {
                if let Some(m) = &mut self.model {
                    m.dialogue = Some(crate::model::Dialogue {
                        npc,
                        speaker: crate::model::sanitize_display(&speaker),
                        lines,
                        choices,
                    });
                    self.overlay = Overlay::Dialogue { sel: 0 };
                }
            }
            ServerMsg::Kicked { reason } => {
                self.status = Some(crate::model::sanitize_display(&reason));
            }
            ServerMsg::Pong { .. } => {}
        }
    }

    pub fn on_auth(&mut self, result: Result<String, String>) {
        match result {
            Ok(token) => {
                if self.settings.remember {
                    let _ = net::save_session(&Stored {
                        server: self.server.clone(),
                        username: self.username.clone(),
                        token: token.clone(),
                    });
                }
                self.connect(token);
            }
            Err(e) => {
                if let Screen::Login(f) = &mut self.screen {
                    f.busy = false;
                    f.error = Some(e);
                }
            }
        }
    }

    // ---- per frame ---------------------------------------------------

    pub fn update(&mut self, now: Instant) {
        let dt = now.duration_since(self.last_frame);
        self.last_frame = now;
        if !matches!(self.screen, Screen::Game) {
            return;
        }
        let typing = matches!(self.overlay, Overlay::Chat { .. } | Overlay::Dialogue { .. } | Overlay::Menu { .. });
        if typing {
            self.input.held_keys.clear();
            self.input.emulated = None;
        }
        let dir = self.held_dir(now);
        let changed = dir != self.input.last_dir;
        let heartbeat = dir.is_some() && now.duration_since(self.input.last_sent) >= INPUT_HEARTBEAT;
        if changed || heartbeat {
            self.send_input(None);
        }
        if let Some(m) = &mut self.model {
            m.predict(now, dt, dir);
            if let Some(cell) = m.my_cell(now) {
                m.update_fov(cell);
            }
            m.prune(now);
        }
    }

    // ---- keys --------------------------------------------------------

    pub fn on_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.quit = true;
            return;
        }
        match &self.screen {
            Screen::Login(_) => self.login_key(key),
            Screen::Connecting(_) => {
                if key.kind == KeyEventKind::Press && key.code == KeyCode::Esc {
                    self.quit = true;
                }
            }
            Screen::Lobby(_) => self.lobby_key(key),
            Screen::Game => self.game_key(key),
        }
    }

    fn login_key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release {
            return;
        }
        let Screen::Login(f) = &mut self.screen else { return };
        if f.busy {
            return;
        }
        match key.code {
            KeyCode::Esc => self.quit = true,
            KeyCode::Tab | KeyCode::Down | KeyCode::Up | KeyCode::BackTab => f.field = (f.field + 1) % 2,
            KeyCode::F(2) => f.register = !f.register,
            KeyCode::Backspace => {
                if f.field == 0 {
                    f.username.pop();
                } else {
                    f.password.pop();
                }
            }
            KeyCode::Enter => {
                if f.field == 0 {
                    f.field = 1;
                    return;
                }
                if let Err(e) = validate_name(&f.username) {
                    f.error = Some(e.to_string());
                    return;
                }
                if f.password.chars().count() < 10 && f.register {
                    f.error = Some("password must be at least 10 characters".into());
                    return;
                }
                f.busy = true;
                f.error = None;
                self.username = f.username.clone();
                let (server, user, pass, register) =
                    (self.server.clone(), f.username.clone(), std::mem::take(&mut f.password), f.register);
                let tx = self.events.clone();
                tokio::spawn(async move {
                    let r = net::authenticate(&server, &user, &pass, register).await.map_err(|e| e.to_string());
                    let _ = tx.send(AppEvent::Auth(r));
                });
            }
            KeyCode::Char(c) if !c.is_control() => {
                if f.field == 0 {
                    if f.username.len() < MAX_NAME_CHARS && (c.is_ascii_alphanumeric() || c == '-' || c == '_') {
                        f.username.push(c);
                    }
                } else if f.password.chars().count() < 128 {
                    f.password.push(c);
                }
            }
            _ => {}
        }
    }

    fn lobby_key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release {
            return;
        }
        let Screen::Lobby(l) = &mut self.screen else { return };
        if let Some(c) = &mut l.create {
            match key.code {
                KeyCode::Esc => {
                    if !l.chars.is_empty() {
                        l.create = None;
                    }
                }
                KeyCode::Tab | KeyCode::Down => c.field = (c.field + 1) % 4,
                KeyCode::BackTab | KeyCode::Up => c.field = (c.field + 3) % 4,
                KeyCode::Left | KeyCode::Right => {
                    let delta = if key.code == KeyCode::Right { 1 } else { -1 };
                    match c.field {
                        1 => c.archetype = (c.archetype as i32 + delta).rem_euclid(3) as usize,
                        2 => c.sigil = (c.sigil as i32 + delta).rem_euclid(APPEARANCE_SIGILS as i32) as u8,
                        3 => c.hue = (c.hue as i32 + delta).rem_euclid(APPEARANCE_HUES as i32) as u8,
                        _ => {}
                    }
                }
                KeyCode::Backspace if c.field == 0 => {
                    c.name.pop();
                }
                KeyCode::Char(ch) if c.field == 0 && (ch.is_ascii_alphanumeric() || ch == '-' || ch == '_') => {
                    if c.name.len() < MAX_NAME_CHARS {
                        c.name.push(ch);
                    }
                }
                KeyCode::Enter => {
                    if let Err(e) = validate_name(&c.name) {
                        l.error = Some(e.to_string());
                        return;
                    }
                    let msg = ClientMsg::CreateCharacter {
                        name: c.name.clone(),
                        archetype: Archetype::ALL[c.archetype],
                        look: Appearance { sigil: c.sigil, hue: c.hue },
                    };
                    l.error = None;
                    self.send(msg);
                }
                _ => {}
            }
            return;
        }
        match key.code {
            KeyCode::Up => l.sel = l.sel.saturating_sub(1),
            KeyCode::Down => l.sel = (l.sel + 1).min(l.chars.len().saturating_sub(1)),
            KeyCode::Char('n') | KeyCode::Char('N') => {
                l.create = Some(CreateForm { name: String::new(), archetype: 0, sigil: 0, hue: 0, field: 0 });
            }
            KeyCode::Enter => {
                if let Some(c) = l.chars.get(l.sel) {
                    let id = c.id.clone();
                    self.send(ClientMsg::SelectCharacter { id });
                }
            }
            KeyCode::Char('l') | KeyCode::Char('L') => self.logout(),
            KeyCode::Esc => self.quit = true,
            _ => {}
        }
    }

    fn logout(&mut self) {
        if let Some(token) = self.token.take() {
            let server = self.server.clone();
            tokio::spawn(async move { net::logout(&server, &token).await });
        }
        net::clear_session();
        self.net = None;
        self.model = None;
        self.screen = Screen::Login(LoginForm::default());
    }

    fn game_key(&mut self, key: KeyEvent) {
        let now = Instant::now();
        if let Some(d) = dir_for(key.code) {
            if matches!(self.overlay, Overlay::None | Overlay::Map) {
                match key.kind {
                    KeyEventKind::Release => {
                        self.input.held_keys.remove(&d);
                    }
                    KeyEventKind::Press | KeyEventKind::Repeat => {
                        if self.uses_release_events() {
                            self.input.held_keys.insert(d);
                        } else {
                            let extend = matches!(self.input.emulated, Some((old, until)) if old == d && until > now);
                            self.input.emulated = Some((d, now + if extend { REPEAT_HOLD } else { FIRST_HOLD }));
                        }
                    }
                }
                if self.overlay == Overlay::None {
                    return;
                }
            }
        }
        let guard_key = matches!(key.code, KeyCode::Char('l') | KeyCode::Char('L'));
        if guard_key && self.overlay == Overlay::None {
            if self.uses_release_events() {
                match key.kind {
                    KeyEventKind::Press if !self.input.guard => {
                        self.input.guard = true;
                        self.send_input(Some(Action::GuardOn));
                    }
                    KeyEventKind::Release => {
                        self.input.guard = false;
                        self.send_input(Some(Action::GuardOff));
                    }
                    _ => {}
                }
            } else if key.kind == KeyEventKind::Press {
                self.input.guard = !self.input.guard;
                self.send_input(Some(if self.input.guard { Action::GuardOn } else { Action::GuardOff }));
            }
            return;
        }
        if key.kind == KeyEventKind::Release {
            return;
        }
        match self.overlay.clone() {
            Overlay::None => self.world_key(key),
            Overlay::Chat { mut text } => match key.code {
                KeyCode::Esc => self.overlay = Overlay::None,
                KeyCode::Enter => {
                    self.submit_chat(&text);
                    self.overlay = Overlay::None;
                }
                KeyCode::Backspace => {
                    text.pop();
                    self.overlay = Overlay::Chat { text };
                }
                KeyCode::Char(c) if !c.is_control() && text.chars().count() < MAX_CHAT_CHARS => {
                    text.push(c);
                    self.overlay = Overlay::Chat { text };
                }
                _ => {}
            },
            Overlay::Dialogue { sel } => self.dialogue_key(key, sel),
            Overlay::Character { sel } => self.character_key(key, sel),
            Overlay::Level { sel } => match key.code {
                KeyCode::Esc | KeyCode::Char('v') | KeyCode::Char('V') => self.overlay = Overlay::None,
                KeyCode::Up => self.overlay = Overlay::Level { sel: sel.saturating_sub(1) },
                KeyCode::Down => self.overlay = Overlay::Level { sel: (sel + 1).min(4) },
                KeyCode::Enter => self.send(ClientMsg::LevelUp { attr: Attribute::ALL[sel] }),
                _ => {}
            },
            Overlay::Map => {
                if matches!(key.code, KeyCode::Esc | KeyCode::Char('m') | KeyCode::Char('M')) {
                    self.overlay = Overlay::None;
                }
            }
            Overlay::Help => self.overlay = Overlay::None,
            Overlay::Menu { sel } => self.menu_key(key, sel),
        }
    }

    fn world_key(&mut self, key: KeyEvent) {
        let act = match key.code {
            KeyCode::Char(' ') => Some(Action::Roll),
            KeyCode::Char('j') | KeyCode::Char('J') => Some(Action::Light),
            KeyCode::Char('k') | KeyCode::Char('K') => Some(Action::Heavy),
            KeyCode::Char('i') | KeyCode::Char('I') => Some(Action::Parry),
            KeyCode::Char('h') | KeyCode::Char('H') => Some(Action::Heal),
            KeyCode::Char('1') => Some(Action::Cast1),
            KeyCode::Char('2') => Some(Action::Cast2),
            KeyCode::Char('f') | KeyCode::Char('F') => Some(Action::Interact),
            _ => None,
        };
        if let Some(a) = act {
            if key.kind == KeyEventKind::Press {
                self.send_input(Some(a));
            }
            return;
        }
        match key.code {
            KeyCode::Tab => self.overlay = Overlay::Character { sel: 0 },
            KeyCode::Char('m') | KeyCode::Char('M') => self.overlay = Overlay::Map,
            KeyCode::Char('v') | KeyCode::Char('V') => self.overlay = Overlay::Level { sel: 0 },
            KeyCode::Char('?') | KeyCode::F(1) => self.overlay = Overlay::Help,
            KeyCode::Enter | KeyCode::Char('t') | KeyCode::Char('T') => {
                self.overlay = Overlay::Chat { text: String::new() }
            }
            KeyCode::Char('/') => self.overlay = Overlay::Chat { text: "/".into() },
            KeyCode::Esc => self.overlay = Overlay::Menu { sel: 0 },
            _ => {}
        }
    }

    fn submit_chat(&mut self, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        let (cmd, rest) = text.split_once(' ').unwrap_or((text, ""));
        let rest = rest.trim().to_string();
        let msg = match cmd {
            "/p" | "/party" => ClientMsg::Chat { channel: ChatChannel::Party, text: rest },
            "/invite" => ClientMsg::PartyInvite { name: rest },
            "/accept" => ClientMsg::PartyAccept { from: rest },
            "/leave" => ClientMsg::PartyLeave,
            "/mute" => ClientMsg::Mute { name: rest, on: true },
            "/unmute" => ClientMsg::Mute { name: rest, on: false },
            "/report" => {
                let (name, reason) = rest.split_once(' ').unwrap_or((&rest, ""));
                ClientMsg::Report { name: name.to_string(), reason: reason.to_string() }
            }
            c if c.starts_with('/') => {
                if let Some(m) = &mut self.model {
                    m.toast("Commands: /p /invite /accept /leave /mute /unmute /report");
                }
                return;
            }
            _ => ClientMsg::Chat { channel: ChatChannel::Local, text: text.to_string() },
        };
        self.send(msg);
    }

    fn dialogue_key(&mut self, key: KeyEvent, sel: usize) {
        let Some(m) = &self.model else { return };
        let Some(d) = &m.dialogue else {
            self.overlay = Overlay::None;
            return;
        };
        let n = d.choices.len();
        let npc = d.npc;
        let pick = |i: usize| ClientMsg::Talk { npc, choice: Some(i as u8) };
        match key.code {
            KeyCode::Esc => {
                self.overlay = Overlay::None;
                if let Some(m) = &mut self.model {
                    m.dialogue = None;
                }
            }
            KeyCode::Up => self.overlay = Overlay::Dialogue { sel: sel.saturating_sub(1) },
            KeyCode::Down => self.overlay = Overlay::Dialogue { sel: (sel + 1).min(n.saturating_sub(1)) },
            KeyCode::Char(c @ '1'..='9') if (c as usize - '1' as usize) < n => {
                self.send(pick(c as usize - '1' as usize))
            }
            KeyCode::Enter | KeyCode::Char('f') | KeyCode::Char('F') => {
                if n == 0 {
                    self.overlay = Overlay::None;
                    if let Some(m) = &mut self.model {
                        m.dialogue = None;
                    }
                } else {
                    self.send(pick(sel));
                }
            }
            _ => {}
        }
    }

    fn character_key(&mut self, key: KeyEvent, sel: usize) {
        let Some(m) = &self.model else { return };
        let items = &m.sheet.inventory;
        let n = items.len();
        match key.code {
            KeyCode::Esc | KeyCode::Tab => self.overlay = Overlay::None,
            KeyCode::Up => self.overlay = Overlay::Character { sel: sel.saturating_sub(1) },
            KeyCode::Down => self.overlay = Overlay::Character { sel: (sel + 1).min(n.saturating_sub(1)) },
            KeyCode::Enter | KeyCode::Char('e') | KeyCode::Char('E') | KeyCode::Char('2') => {
                let Some(item) = items.get(sel) else { return };
                let second = key.code == KeyCode::Char('2');
                let eq = &m.sheet.equipment;
                let msg = match item.kind.as_str() {
                    "weapon" => Some((EquipSlot::Main, eq.main.as_deref())),
                    "shield" => Some((EquipSlot::Off, eq.off.as_deref())),
                    "armor" => Some((EquipSlot::Armor, eq.armor.as_deref())),
                    "rite" if second => Some((EquipSlot::Spell2, eq.spell2.as_deref())),
                    "rite" => Some((EquipSlot::Spell1, eq.spell1.as_deref())),
                    _ => None,
                };
                if let Some((slot, current)) = msg {
                    let new = if current == Some(item.item.as_str()) { None } else { Some(item.item.clone()) };
                    self.send(ClientMsg::Equip { slot, item: new });
                }
            }
            KeyCode::Char('u') | KeyCode::Char('U') => {
                if let Some(item) = items.get(sel).filter(|i| i.kind == "consumable") {
                    self.send(ClientMsg::UseItem { item: item.item.clone() });
                }
            }
            _ => {}
        }
    }

    pub const MENU: [&'static str; 8] = [
        "Resume",
        "Glyphs: Unicode / ASCII",
        "Colours",
        "High contrast",
        "Reduced motion",
        "Movement input",
        "Leave to character select",
        "Quit",
    ];

    fn menu_key(&mut self, key: KeyEvent, sel: usize) {
        match key.code {
            KeyCode::Esc => self.overlay = Overlay::None,
            KeyCode::Up => self.overlay = Overlay::Menu { sel: sel.saturating_sub(1) },
            KeyCode::Down => self.overlay = Overlay::Menu { sel: (sel + 1).min(Self::MENU.len() - 1) },
            KeyCode::Enter | KeyCode::Char(' ') => match sel {
                0 => self.overlay = Overlay::None,
                1 => self.theme.ascii = !self.theme.ascii,
                2 => {
                    self.theme.mode = match self.theme.mode {
                        ColorMode::True => ColorMode::Indexed,
                        ColorMode::Indexed => ColorMode::Ansi16,
                        ColorMode::Ansi16 => ColorMode::True,
                    }
                }
                3 => self.theme.high_contrast = !self.theme.high_contrast,
                4 => self.settings.reduced_motion = !self.settings.reduced_motion,
                5 => {
                    self.settings.input_mode = match self.settings.input_mode {
                        InputMode::Auto => InputMode::Hold,
                        InputMode::Hold => InputMode::Repeat,
                        InputMode::Repeat => InputMode::Auto,
                    };
                    self.input.held_keys.clear();
                    self.input.emulated = None;
                }
                6 => {
                    self.send(ClientMsg::Leave);
                    self.overlay = Overlay::None;
                }
                _ => self.quit = true,
            },
            _ => {}
        }
    }

    pub fn input_mode_label(&self) -> &'static str {
        match (self.settings.input_mode, self.input.enhanced) {
            (InputMode::Auto, true) => "auto (key release)",
            (InputMode::Auto, false) => "auto (key repeat)",
            (InputMode::Hold, _) => "hold (needs key release events)",
            (InputMode::Repeat, _) => "repeat",
        }
    }

    pub fn color_label(&self) -> &'static str {
        match self.theme.mode {
            ColorMode::True => "24-bit",
            ColorMode::Indexed => "256",
            ColorMode::Ansi16 => "16",
        }
    }
}
