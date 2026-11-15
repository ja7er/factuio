# factuio

A factory-automation game for your terminal, written in Rust with [ratatui](https://ratatui.rs).

Start with a handful of buildings on a procedurally generated map. Mine ore, smelt it,
move it around on conveyors and arms, craft parts, research new technology, and finally
build the **Uplink array** and feed it 20 relay modules to win.

![The map](docs/screenshot-map.png)

## Install and run

Download a prebuilt binary for Linux, macOS or Windows from the
[Releases page](https://github.com/ja7er/factuio/releases), unpack it and run `factuio`.
On macOS, if Gatekeeper blocks the unsigned binary, run `xattr -d com.apple.quarantine factuio`.

Or build from source with a recent Rust toolchain (edition 2024, Rust 1.85+):

```sh
cargo run --release
```

Options:

```
--new             start a new game immediately
--continue        load the save file immediately
--seed <N>        world seed for a new game
--save <PATH>     save file location (default: ~/.factuio/save.json)
--ascii           ASCII-only glyphs (for terminals without Unicode arrows)
--256             force 256-colour mode (macOS Terminal.app, older terminals)
--truecolor       force 24-bit colour
```

A terminal of at least 100×30 is recommended. Colours are 24-bit when your terminal
advertises it through `COLORTERM`; otherwise they fall back to the 256-colour palette.

## Controls

| Key | Action |
| --- | --- |
| Arrows / WASD | Move the cursor (Shift or capital WASD: 8 tiles) |
| 1–9, 0 | Select a building from the hotbar |
| b | Building list (press 1–0 there to assign hotbar slots) |
| Space / Enter | Place the selected building, or inspect what's under the cursor |
| r | Rotate the building you're placing (or the one under the cursor) |
| v | Paint mode: moving the cursor keeps building; conveyors follow your path |
| q | Clear selection / pick the building under the cursor |
| x / Delete | Pick up a building and everything inside it |
| e | Inspect: `t` take, `i` insert, `f` add fuel, `r` recipe or rotate |
| m | Mine the ore under the cursor by hand |
| c / i / Tab | Crafting and inventory (`Enter` 1, `f` 5, `F` max, `x` cancel) |
| t | Research tree |
| g | Production statistics |
| o | Power coverage overlay |
| p / f | Pause / 4× fast-forward |
| F5 / F9 | Quick save / quick load |
| Esc | Menu (save, load, quit) |
| ? | Help |

The game autosaves every five minutes and on Ctrl+C.

## How it plays

**Resources.** Four ores spawn near the start, with richer patches further out:
ferrite, malachite, lignite (fuel) and quartz.

**Production chain.**

```
ferrite ──kiln──▶ iron ingot ──▶ cog, conveyor, crate ...
malachite ─kiln─▶ copper ingot ─▶ copper coil ─▶ logic chip
quartz ──kiln──▶ glass ─────────▶ lens (Optics)
iron ingot ×5 ─kiln─▶ steel beam (Metallurgy) ─▶ motor (Motors)
chip + lens + coil ─▶ processor (Computing)
processor + motor + steel ─▶ relay module ─▶ Uplink array ★
```

**Buildings.**

| Map | Building | Notes |
| --- | --- | --- |
| `D→` | Digger | Burns lignite, mines the 3×3 area under it, pushes ore ahead |
| `B→` | Electric bore | Electric, twice as fast (Electric mining) |
| `K` | Kiln | Burns lignite to smelt |
| `E` | Arc furnace | Electric, twice as fast (Arc smelting) |
| `→→` | Conveyor / express conveyor | Items ride on it, shown by their letter; side-loading works |
| `ɑ→` | Arm / swift arm | Grabs from behind, drops ahead; only picks what the target can use |
| `C` | Crate | Stores 800 items |
| `G` | Generator | Burns lignite for 900 kW |
| `P` | Pylon | Powers a 5×5 area and links to pylons up to 7 tiles away |
| `S` | Sun panel | Free 60 kW (Solar power) |
| `M` | Fabricator | Electric assembler for any crafting recipe (Fabrication) |
| `L` | Lab | Consumes datapacks for the current research |
| `U` | Uplink array | Win condition (Uplink theory) |

**Power.** Each pylon network sums its generators and panels. When demand exceeds
supply every machine on that network slows down proportionally.

**Research.** Twelve technologies use Alpha, Beta and Gamma datapacks. Pick one
with `t`, then keep your labs supplied.

The sidebar always shows a **Goal** line telling you what to do next.

## Development

```sh
cargo test            # simulation, crafting, save/load, UI rendering, scripted playthroughs
cargo clippy --all-targets
```

Code layout:

- `src/items.rs`: items, buildings, directions
- `src/recipes.rs`: recipes and the tech tree
- `src/world.rs`: map generation and the tick simulation (belts, arms, miners, furnaces, fabricators, labs, power)
- `src/game.rs`: player inventory, hand crafting, building actions, save/load
- `src/app.rs`: screens and keyboard handling
- `src/ui.rs`: ratatui rendering
- `src/playtest.rs`: scripted playthroughs used by the tests

The simulation runs at 20 ticks per second and is deterministic for a given seed.
