mod app;
mod model;
mod net;
mod theme;
mod view;

use std::io::{self, Stdout};
use std::time::{Duration, Instant};

use anyhow::Result;
use clap::Parser;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{
    self, Event, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use ratatui::crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::crossterm::{cursor, execute};
use ratatui::Terminal;
use tokio::sync::mpsc;

use app::{App, AppEvent, InputMode, Settings};
use theme::{ColorMode, Theme};

#[derive(Parser)]
#[command(name = "hollowrite", version, about = "HOLLOWRITE terminal client")]
struct Args {
    /// Server base URL (http:// for local play, https:// for anything public).
    #[arg(long, env = "HOLLOWRITE_SERVER", default_value = "http://127.0.0.1:7700")]
    server: String,
    /// Plain ASCII glyphs for limited terminals and fonts.
    #[arg(long)]
    ascii: bool,
    /// Colour depth: auto, 24bit, 256 or 16.
    #[arg(long, default_value = "auto")]
    colors: String,
    /// Brighter palette for low-contrast displays.
    #[arg(long)]
    high_contrast: bool,
    /// Disable flicker, drifting embers and other animation.
    #[arg(long)]
    reduced_motion: bool,
    /// Movement input: auto, hold (needs key release events) or repeat.
    #[arg(long, default_value = "auto")]
    input: String,
    /// Do not store the session token on disk.
    #[arg(long)]
    no_remember: bool,
}

type Term = Terminal<CrosstermBackend<Stdout>>;

fn restore_terminal(enhanced: bool) {
    let mut out = io::stdout();
    if enhanced {
        let _ = execute!(out, PopKeyboardEnhancementFlags);
    }
    let _ = execute!(out, LeaveAlternateScreen, cursor::Show);
    let _ = terminal::disable_raw_mode();
}

fn setup_terminal() -> Result<(Term, bool)> {
    terminal::enable_raw_mode()?;
    let mut out = io::stdout();
    execute!(out, EnterAlternateScreen, cursor::Hide)?;
    // Key release events are only trustworthy on terminals that implement the
    // kitty keyboard protocol; everywhere else movement falls back to
    // repeat-based holding.
    let enhanced = matches!(terminal::supports_keyboard_enhancement(), Ok(true));
    if enhanced {
        execute!(
            out,
            PushKeyboardEnhancementFlags(
                KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
            )
        )?;
    }
    let term = Terminal::new(CrosstermBackend::new(out))?;
    Ok((term, enhanced))
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let mode = match args.colors.as_str() {
        "24bit" | "true" | "truecolor" => ColorMode::True,
        "256" => ColorMode::Indexed,
        "16" => ColorMode::Ansi16,
        _ => ColorMode::detect(),
    };
    let input_mode = match args.input.as_str() {
        "hold" => InputMode::Hold,
        "repeat" => InputMode::Repeat,
        _ => InputMode::Auto,
    };
    let server = args.server.trim_end_matches('/').to_string();
    if !server.starts_with("https://") && !server.contains("127.0.0.1") && !server.contains("localhost") {
        eprintln!("warning: {server} is not using TLS; passwords and tokens would cross the network in clear text");
    }

    let (term, enhanced) = setup_terminal()?;
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal(enhanced);
        default_hook(info);
    }));
    let result = run(term, args, server, mode, input_mode, enhanced).await;
    restore_terminal(enhanced);
    result
}

async fn run(
    mut term: Term,
    args: Args,
    server: String,
    mode: ColorMode,
    input_mode: InputMode,
    enhanced: bool,
) -> Result<()> {
    let (tx, mut rx) = mpsc::unbounded_channel::<AppEvent>();
    let (net_tx, mut net_rx) = mpsc::unbounded_channel();
    let key_tx = tx.clone();
    std::thread::spawn(move || loop {
        match event::poll(Duration::from_millis(250)) {
            Ok(true) => match event::read() {
                Ok(Event::Key(k)) => {
                    if key_tx.send(AppEvent::Key(k)).is_err() {
                        return;
                    }
                }
                Ok(Event::Resize(_, _)) => {
                    let _ = key_tx.send(AppEvent::Resize);
                }
                Ok(_) => {}
                Err(_) => return,
            },
            Ok(false) => {
                if key_tx.is_closed() {
                    return;
                }
            }
            Err(_) => return,
        }
    });

    let theme = Theme::load(mode, args.ascii, args.high_contrast);
    let settings = Settings { reduced_motion: args.reduced_motion, input_mode, remember: !args.no_remember };
    let mut app = App::new(server, theme, settings, tx, net_tx, enhanced);
    app.resume();

    let frame = Duration::from_millis(33);
    let mut next = Instant::now();
    loop {
        tokio::select! {
            ev = rx.recv() => match ev {
                Some(AppEvent::Key(k)) => app.on_key(k),
                Some(AppEvent::Resize) => { let _ = term.autoresize(); }
                Some(AppEvent::Auth(r)) => app.on_auth(r),
                None => break,
            },
            n = net_rx.recv() => {
                if let Some(n) = n {
                    app.on_net(n);
                }
            }
            _ = tokio::time::sleep_until(next.into()) => {
                let now = Instant::now();
                next = now + frame;
                app.update(now);
                term.draw(|f| view::draw(f, &app))?;
            }
        }
        if app.quit {
            if app.net.is_some() {
                app.send(hollowrite_protocol::ClientMsg::Leave);
                tokio::time::sleep(Duration::from_millis(150)).await;
            }
            break;
        }
    }
    Ok(())
}
