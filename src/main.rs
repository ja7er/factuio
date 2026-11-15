//! factuio: a terminal factory-automation game.

mod app;
mod game;
mod inventory;
mod items;
#[cfg(test)]
mod playtest;
mod recipes;
mod records;
mod research;
mod scenario;
mod settings;
mod tools;
mod ui;
mod world;

use app::App;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

const USAGE: &str = "factuio - a terminal factory-automation game

USAGE:
    factuio [OPTIONS]

OPTIONS:
    --new             start a new game immediately
    --continue        load the save file immediately
    --seed <N>        world seed for a new game
    --scenario <ID>   start a scenario: standard, tutorial, far-fields, lean, sprint
    --save <PATH>     save file location (default: ~/.factuio/save.json)
    --ascii           ASCII-only glyphs (for terminals without Unicode arrows)
    --256             force 256-colour mode
    --truecolor       force 24-bit colour
    -h, --help        show this help
";

fn default_save_path() -> PathBuf {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".factuio").join("save.json")
}

struct Args {
    new: bool,
    cont: bool,
    seed: Option<u64>,
    scenario: Option<&'static scenario::Scenario>,
    save: PathBuf,
    ascii: bool,
    color: Option<bool>,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args {
        new: false,
        cont: false,
        seed: None,
        scenario: None,
        save: default_save_path(),
        ascii: false,
        color: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--new" => a.new = true,
            "--continue" => a.cont = true,
            "--seed" => {
                let v = it.next().ok_or("--seed needs a value")?;
                a.seed = Some(v.parse().map_err(|_| format!("bad seed {v}"))?);
            }
            "--scenario" => {
                let v = it.next().ok_or("--scenario needs a name")?;
                let names: Vec<&str> = scenario::SCENARIOS.iter().map(|s| s.id).collect();
                a.scenario =
                    Some(scenario::Scenario::by_id(&v).ok_or_else(|| {
                        format!("unknown scenario {v} (try {})", names.join(", "))
                    })?);
            }
            "--save" => a.save = PathBuf::from(it.next().ok_or("--save needs a path")?),
            "--ascii" => a.ascii = true,
            "--256" => a.color = Some(false),
            "--truecolor" => a.color = Some(true),
            "-h" | "--help" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            other => return Err(format!("unknown option {other}\n\n{USAGE}")),
        }
    }
    Ok(a)
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    ui::detect_color_support();
    if let Some(c) = args.color {
        ui::set_truecolor(c);
    }
    let mut app = App::new(args.save, args.seed, args.ascii);
    app.scenario = args.scenario;
    let (settings, warning) = settings::Settings::load_or_create(&app.settings_path());
    let mouse = settings.mouse;
    app.apply_settings(settings);
    if warning.is_some() {
        app.title_msg = warning;
    }
    if args.cont {
        if let Err(e) = app.continue_game() {
            eprintln!("{e}");
            std::process::exit(1);
        }
    } else if args.new || args.seed.is_some() || args.scenario.is_some() {
        app.new_game();
    }

    // Closing the terminal window or `kill` should save, like Ctrl+C does.
    let stop = Arc::new(AtomicBool::new(false));
    register_stop_signals(&stop);

    let mut terminal = ratatui::init();
    if mouse {
        let _ = ratatui::crossterm::execute!(
            std::io::stdout(),
            ratatui::crossterm::event::EnableMouseCapture
        );
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run(&mut terminal, &mut app, &stop)
    }));
    if mouse {
        let _ = ratatui::crossterm::execute!(
            std::io::stdout(),
            ratatui::crossterm::event::DisableMouseCapture
        );
    }
    ratatui::restore();
    match result {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            // The terminal went away or broke: keep the player's progress.
            app.autosave();
            eprintln!("error: {e}");
            std::process::exit(1);
        }
        Err(_) => {
            // Don't overwrite a good save with state from a crash; write a separate file.
            if let Some(g) = app.game.as_ref() {
                let path = app.save_path.with_extension("crash.json");
                if g.save(&path).is_ok() {
                    eprintln!("factuio crashed. Your game was saved to {}", path.display());
                }
            }
            std::process::exit(101);
        }
    }
}

#[cfg(unix)]
fn register_stop_signals(stop: &Arc<AtomicBool>) {
    use signal_hook::consts::{SIGHUP, SIGTERM};
    for sig in [SIGHUP, SIGTERM] {
        let _ = signal_hook::flag::register(sig, Arc::clone(stop));
    }
}

#[cfg(not(unix))]
fn register_stop_signals(_stop: &Arc<AtomicBool>) {}

/// Reads terminal events on their own thread. crossterm can spin forever inside
/// `poll`/`read` once the terminal is gone, so the game loop must never block on it.
fn spawn_input_thread() -> mpsc::Receiver<std::io::Result<Event>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        loop {
            let ev = event::read();
            let failed = ev.is_err();
            if tx.send(ev).is_err() || failed {
                break;
            }
        }
    });
    rx
}

fn run(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
    stop: &AtomicBool,
) -> std::io::Result<()> {
    let tick = Duration::from_millis(1000 / world::TICKS_PER_SEC as u64);
    let mut next_tick = Instant::now() + tick;
    let mut redraw = true;
    let input = spawn_input_thread();
    while !app.quit {
        if stop.load(Ordering::Relaxed) {
            app.autosave();
            break;
        }
        if redraw {
            // Tunnel links and power networks shown on screen must be current even while paused.
            if let Some(g) = app.game.as_mut() {
                g.world.ensure_caches();
            }
            terminal.draw(|f| ui::draw(f, app))?;
            redraw = false;
        }
        let now = Instant::now();
        let timeout = next_tick.saturating_duration_since(now);
        match input.recv_timeout(timeout) {
            Ok(ev) => match ev? {
                Event::Key(k) if k.kind != KeyEventKind::Release => {
                    app.handle_key(k);
                    redraw = true;
                }
                Event::Mouse(m) => {
                    app.handle_mouse(m);
                    redraw = true;
                }
                Event::Resize(..) => redraw = true,
                _ => {}
            },
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(std::io::Error::other("terminal input closed"));
            }
        }
        let now = Instant::now();
        if now >= next_tick {
            let mut steps = 0;
            while now >= next_tick && steps < 5 {
                if app.running() {
                    app.step();
                }
                next_tick += tick;
                steps += 1;
            }
            if now >= next_tick {
                // Too far behind: drop the backlog rather than spiralling.
                next_tick = now + tick;
            }
            redraw = true;
        }
    }
    Ok(())
}
