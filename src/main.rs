//! factuio: a terminal factory-automation game.

mod app;
mod game;
mod inventory;
mod items;
#[cfg(test)]
mod playtest;
mod recipes;
mod research;
mod ui;
mod world;

use app::App;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const USAGE: &str = "factuio - a terminal factory-automation game

USAGE:
    factuio [OPTIONS]

OPTIONS:
    --new             start a new game immediately
    --continue        load the save file immediately
    --seed <N>        world seed for a new game
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
    save: PathBuf,
    ascii: bool,
    color: Option<bool>,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args {
        new: false,
        cont: false,
        seed: None,
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
    if args.cont {
        if let Err(e) = app.load_game() {
            eprintln!("{e}");
            std::process::exit(1);
        }
    } else if args.new || args.seed.is_some() {
        app.new_game();
    }

    let mut terminal = ratatui::init();
    let result = run(&mut terminal, &mut app);
    ratatui::restore();
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> std::io::Result<()> {
    let tick = Duration::from_millis(1000 / world::TICKS_PER_SEC as u64);
    let mut next_tick = Instant::now() + tick;
    let mut redraw = true;
    while !app.quit {
        if redraw {
            terminal.draw(|f| ui::draw(f, app))?;
            redraw = false;
        }
        let now = Instant::now();
        let timeout = next_tick.saturating_duration_since(now);
        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(k) if k.kind != KeyEventKind::Release => {
                    app.handle_key(k);
                    redraw = true;
                }
                Event::Resize(..) => redraw = true,
                _ => {}
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
