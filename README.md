# niri-animation-rotate

A lightweight daemon that rotates Niri window animations on compositor events or via manual keybinds.

Connects to the Niri compositor's IPC event stream and cycles through animation KDL files. Supports both automatic mode (event-driven) and manual mode (triggered via a keybind).

## Quick Start

Get from zero to animated in one minute:

```bash
# Option A — use the install script (recommended for new users)
git clone https://github.com/pnbarbeito/niri-animation-rotate.git
cd niri-animation-rotate
./install.sh

# Option B — manual installation
# cargo build --release
# cp target/release/niri-animation-rotate ~/.local/bin/
# mkdir -p ~/.config/niri/niri-animation-rotate/animations
# cp animations/*.kdl ~/.config/niri/niri-animation-rotate/animations/

# 1. Add to ~/.config/niri/config.kdl:
#    include "niri-animation-rotate/animation.kdl"

# 2. Run the daemon (inside your Niri session):
niri-animation-rotate
```

That's it! 49 bundled animations will cycle every time you open or close a window.

## Features

- **Automatic mode** — rotates on `WindowOpenedOrChanged` and `WindowClosed` events
- **Configurable event filters** — exclude specific events (`--no-window-opened`, `--no-window-closed`)
- **Duration-aware cooldown** — smart cooldown that parses each animation's actual duration to avoid mid-play swaps
- **Manual mode** — rotate on demand via a Unix control socket with bidirectional responses
- **Full command set** — `next`, `prev`, `current`, `list`, and `select <name>` for manual control
- **Configurable shuffle** — randomly shuffle animation order on startup (`--random-order`, disabled by default)
- **Auto-refresh** — watches the animation directory for new, removed, or modified files in real time
- **49 bundled animations** — ready-to-use presets included in the repository
- **No-reload mode** — skip `niri msg action reload` for environments that auto-reload on file change
- **KDL config** — uses the same format as Niri for configuration
- **CLI + config file** — flexible configuration with `--flags` or a persistent config file
- **Atomic writes** — writes animation files safely to avoid Niri reading partial content
- **Graceful shutdown** — clean exit on SIGINT/SIGTERM
- **Debug logging** — `--log-socket` to inspect raw Niri IPC messages
- **nrctl CLI** — quick terminal control for manual mode (`nrctl next`, `nrctl select <name>`, etc.)

## Prerequisites

- A running [Niri](https://github.com/YaLTeR/niri) compositor session
- Rust toolchain (for building from source)

## Installation

### Automatic (install script)

The easiest way to get started:

```bash
git clone https://github.com/pnbarbeito/niri-animation-rotate.git
cd niri-animation-rotate
./install.sh
```

This script will:

1. Check for Rust/Cargo and Git
2. Build the binary
3. Copy it to `~/.local/bin/`
4. Create the config directory structure
5. Copy all 49 bundled animations to `~/.config/niri/niri-animation-rotate/animations/`
6. Install the `nrctl` control script to `~/.local/bin/`
7. Print the next steps for Niri configuration

To also install a systemd user service, add `--systemd`:

```bash
./install.sh --systemd
```

For more details, see `./install.sh --help`.

### From source

```bash
git clone https://github.com/pnbarbeito/niri-animation-rotate.git
cd niri-animation-rotate
cargo build --release
```

The binary will be at `target/release/niri-animation-rotate`.

### Copy to ~/.local/bin (recommended)

Copy the binary to a directory in your `PATH` so you can run it from anywhere:

```bash
cp target/release/niri-animation-rotate ~/.local/bin/
```

This is especially useful if you plan to run it as a systemd service (see below).

### Via cargo (alternative)

If you prefer, you can install it directly from the repository:

```bash
cargo install --git https://github.com/pnbarbeito/niri-animation-rotate.git
```

The binary will be at `~/.cargo/bin/niri-animation-rotate`.

## Setup

### 1. Animation files

The repository comes with **49 ready-to-use animation presets** in the `animations/` folder. After building, copy them to the config directory:

```bash
mkdir -p ~/.config/niri/niri-animation-rotate/animations
cp animations/*.kdl ~/.config/niri/niri-animation-rotate/animations/
```

You can also add your own `.kdl` files here — the daemon picks them up automatically in real time.

Each `.kdl` file should contain a complete Niri `animations { ... }` block. For example:

```kdl
// ~/.config/niri/niri-animation-rotate/animations/spring-bouncy.kdl
animations {
    workspace-switch {
        spring damping-ratio=0.8 stiffness=1000 epsilon=0.0001
    }
    window-open {
        duration-ms 200
        curve "ease-out-expo"
    }
    window-close {
        duration-ms 150
        curve "ease-out-quad"
    }
}
```

### 2. Configure Niri to include the animation file

Add this line to your main Niri config (`~/.config/niri/config.kdl`):

```kdl
include "niri-animation-rotate/animation.kdl"
```

### 3. Create a config file (optional)

See the [configuration section](#configuration) below for all available options.

### 4. Run the daemon

```bash
niri-animation-rotate
```

---

## Usage

```
niri-animation-rotate [OPTIONS]
```

### Options

| Flag | Description | Default |
|---|---|---|
| `--config <PATH>` | Path to the configuration file (KDL format) | `~/.config/niri/niri-animation-rotate/config.kdl` |
| `--animation-dir <DIR>` | Directory containing `.kdl` animation files | `~/.config/niri/niri-animation-rotate/animations` |
| `--animation-target <PATH>` | Output file that Niri reads via `include` | `~/.config/niri/niri-animation-rotate/animation.kdl` |
| `--mode <MODE>` | Operation mode: `auto` (Niri events) or `manual` (control socket with `next`/`prev`/`select`/`current`/`list`) | `auto` |
| `--control-socket <PATH>` | Unix socket path for manual mode | `~/.config/niri/niri-animation-rotate/control.sock` |
| `--random-order` | Randomly shuffle animation order on startup and directory refresh (disabled by default) | — |
| `--niri-socket <PATH>` | Niri IPC socket path (overrides `$NIRI_SOCKET`) | `$NIRI_SOCKET` env var |
| `--cooldown-ms <MS>` | Extra buffer added on top of the parsed animation duration (ms) | `0` |
| `--duration-fallback-ms <MS>` | Fallback duration for animation files without `duration-ms` | `500` |
| `--no-reload` | Skip `niri msg action reload` after rotation | — |
| `--no-window-opened` | Do not rotate on window open/change events | — |
| `--no-window-closed` | Do not rotate on window close events | — |
| `--log-socket` | Print raw Niri IPC lines to stderr (debugging) | — |
| `-h`, `--help` | Print help | — |
| `-V`, `--version` | Print version | — |

### Environment

- `NIRI_SOCKET` — Niri IPC socket path (automatically set by Niri in your session). Can be overridden with `--niri-socket` or the config file. Not needed in manual mode.
- `RUST_LOG` — controls log verbosity (default: `info`)

### Modes

#### Auto mode (default)

The daemon connects to the Niri IPC event stream and rotates animations automatically on `WindowOpenedOrChanged` and `WindowClosed` events. This is the default behavior.

```bash
niri-animation-rotate
```

The first 5 events received are initial Niri state and are skipped.

#### Manual mode

Instead of listening to Niri events, the daemon listens on a Unix control socket for rotation commands. Use together with Niri keybinds.

First, start the daemon in manual mode:

```bash
niri-animation-rotate --mode manual
```

Then add keybinds to your Niri config (`~/.config/niri/config.kdl`):

```kdl
binds {
    Mod+Shift+A { spawn-sh "echo 'next' | nc -U $HOME/.config/niri/niri-animation-rotate/control.sock"; }
    Mod+Shift+D { spawn-sh "echo 'prev' | nc -U $HOME/.config/niri/niri-animation-rotate/control.sock"; }
}
```

**Available commands:**

| Command | Response | Description |
|---------|----------|-------------|
| `next` | _(none)_ | Advance to the next animation |
| `prev` | _(none)_ | Go back to the previous animation |
| `rotate` | _(none)_ | Alias for `next` (backward compatible) |
| `current` | `prism_fold` | Return the filename (without `.kdl`) of the active animation |
| `list` | `bloom`\n`prism_fold`\n`tv_crt`\n | Return all available animation filenames, one per line |
| `select <name>` | `ok` or `error: not found` | Select a specific animation by name (without `.kdl`, case-insensitive) |
| `mode auto` | `ok` | Hot-switch to auto mode (Niri events). Re-initializes debounce from current animation |
| `mode manual` | `ok` | Hot-switch to manual mode (socket control). Manual cooldown starts fresh |

All responses are plain text, one line each (multiple lines for `list`). Unknown commands return `error: unknown command: <cmd>`.

> **Hot-switch:** You can switch modes at any time via the control socket. Start in auto mode, send `mode manual` to take manual control, and `mode auto` to resume automatic rotation. The control socket (`current`/`list` queries) works in both modes.

**Examples using `nc` (netcat):**

> **Tip:** For day-to-day control, use `nrctl` instead of raw `nc` — it's simpler and shows the result after each rotation. See the [nrctl section](#nrctl--terminal-control-tool) below.

```bash
# Get the current animation name
echo "current" | nc -U ~/.config/niri/niri-animation-rotate/control.sock
# → prism_fold

# List all available animations
echo "list" | nc -U ~/.config/niri/niri-animation-rotate/control.sock
# → bloom
# → prism_fold
# → tv_crt

# Select a specific animation
echo "select tv_crt" | nc -U ~/.config/niri/niri-animation-rotate/control.sock
# → ok

echo "select nonexistent" | nc -U ~/.config/niri/niri-animation-rotate/control.sock
# → error: not found
```

The socket file is cleaned up automatically on shutdown.

> **Note:** Niri's `spawn` does not use a shell and does not expand `~` or `$HOME`. Use `spawn-sh` (Niri ≥ 25.08) or pass the full absolute path with `spawn "sh" "-c" "..."`.

#### Cooldown (duration-aware)

The daemon reads each animation's `duration-ms` value from the KDL file and uses it to calculate when the animation will finish playing. Rotations are blocked while the current animation is still active.

```bash
# No extra buffer — rotation only when the current animation finishes
niri-animation-rotate --cooldown-ms 0

# Add a 1000ms buffer after the animation finishes
niri-animation-rotate --cooldown-ms 1000
```

**How it works:**

- On startup, the daemon parses the currently-active animation to determine how much longer it will play
- Each incoming event resets the block timer using the current animation's duration — no rotation occurs until the timer expires
- After a rotation, the block timer uses the **new** animation's parsed duration
- If an animation file has no `duration-ms`, the `--duration-fallback-ms` value is used (default: 500ms)
- If the animation file has a `slowdown <float>` multiplier (e.g., `slowdown 1.5`), it's automatically applied
- In **manual mode**, the cooldown is fixed (duration parsing is not used)

This eliminates mid-animation replacement and rapid-fire rotations by design.

### nrctl — terminal control tool

`nrctl` is a convenience script installed alongside the daemon that lets you control it from the terminal without typing raw `nc -U` commands.

If the daemon is running:

```bash
nrctl next                  # → rotate to next animation, prints result
nrctl prev                  # ← go back to previous animation
nrctl current               # show current animation name
nrctl list                  # list all available animations
nrctl list --json           # list as JSON array
nrctl select <name>         # select a specific animation (case-insensitive)
nrctl status                # show runtime status (mode, cooldown, etc.)
nrctl status --json         # status as JSON
nrctl mode auto             # switch to auto mode
nrctl mode manual           # switch to manual mode

nrctl --help                # full command reference
```

#### `nrctl set` — runtime settings

All `set` changes persist to the config file (`~/.config/niri/niri-animation-rotate/config.kdl`).

| Key | Values | Description |
|-----|--------|-------------|
| `cooldown-ms` | milliseconds (e.g. `500`, `2000`, `0`) | Minimum delay between rotations |
| `random-order` | `true`, `false`, `1`, `0`, `yes`, `no` | Shuffle animation order on startup/refresh |
| `no-window-opened` | `true`, `false`, `1`, `0`, `yes`, `no` | Ignore window open/change events (auto mode) |
| `no-window-closed` | `true`, `false`, `1`, `0`, `yes`, `no` | Ignore window close events (auto mode) |

Examples:

```bash
nrctl set cooldown-ms 2000       # 2-second cooldown
nrctl set cooldown-ms 0          # disable cooldown
nrctl set random-order true      # enable shuffle
nrctl set random-order yes       # same as above (boolean aliases)
nrctl set no-window-opened 1     # ignore window events
nrctl set no-window-closed no    # re-enable close events
```

Set `NRCTL_SOCKET` to override the default control socket path:

```bash
NRCTL_SOCKET=/tmp/custom.sock nrctl current
```

## Configuration

The app uses a three-tier configuration system (highest priority wins):

1. **CLI arguments** (highest priority)
2. **Config file** (KDL format)
3. **Built-in defaults** (lowest priority)

### Config file format

The config file (`~/.config/niri/niri-animation-rotate/config.kdl`) supports all CLI options:

```kdl
animation-dir "~/.config/niri/niri-animation-rotate/animations"
animation-target "~/.config/niri/niri-animation-rotate/animation.kdl"
niri-socket "/run/user/1000/niri.sock"
log-socket true
no-reload true
no-window-opened false
no-window-closed false
cooldown-ms 2000
duration-fallback-ms 500
random-order true
mode "manual"
control-socket "~/.config/niri/niri-animation-rotate/control.sock"
```

For boolean options (`log-socket`, `no-reload`, `no-window-opened`, `no-window-closed`), the config file can only enable them. To disable, omit the line or use the CLI flag.

### Merge precedence

| Setting type | CLI | Config file | Default |
|---|---|---|---|---|
| Paths (`animation-dir`, `animation-target`, `control-socket`) | `--path /x` wins | `path "/x"` | `~/.config/niri/...` |
| Niri socket (`--niri-socket`) | `--niri-socket /x` wins | `niri-socket "/x"` | `$NIRI_SOCKET` env var |
| Bools (`log-socket`, `no-reload`, `no-window-opened`, `no-window-closed`, `random-order`) | `--flag` wins (always enables) | `flag true` enables | `false` |
| Values (`cooldown-ms`, `duration-fallback-ms`, `mode`) | `--value X` wins | `value X` applies | `0` / `500` / `auto` |

## How it works

1. On startup, scans the animation directory for all `.kdl` files
2. Parses each file's animation duration from `duration-ms` values and `slowdown` multiplier
3. Reads the current animation target to set the initial cooldown timer
4. Optionally shuffles the file list (if `--random-order` is enabled); current selection preserved across rescans
5. **Preserves the existing output file** — no overwrite on startup
6. In auto mode: connects to Niri's event stream via Unix socket
7. On each `WindowOpenedOrChanged` or `WindowClosed` event, checks the debounce timer:
    - If the current animation has finished playing → rotates
    - If the animation is still playing → extends the block timer (no rotation)
8. In manual mode: listens on a control socket for `next`/`prev`/`select`/`current`/`list` commands (fixed cooldown)
9. On each rotation trigger, writes the next animation file atomically and reloads Niri's config
10. Watches the animation directory for filesystem changes and refreshes the cache automatically

## Logging

Logs are written to stderr. Control verbosity with `RUST_LOG`:

```bash
# Default (info level)
niri-animation-rotate

# Debug output
RUST_LOG=debug niri-animation-rotate

# Trace (very verbose)
RUST_LOG=trace niri-animation-rotate
```

## Running as a systemd service (optional)

Create `~/.config/systemd/user/niri-animation-rotate.service`.

Adjust `ExecStart` to match where you placed the binary:
- If you copied it to `~/.local/bin/` (recommended), use the path below
- If you used `cargo install --git`, the binary is at `~/.cargo/bin/`
- If you built from source without copying, point it to `target/release/niri-animation-rotate`

```ini
[Unit]
Description=Niri Animation Rotate
After=niri-session.service

[Service]
Type=simple
ExecStart=%h/.local/bin/niri-animation-rotate
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
```

For manual mode, add the `--mode manual` flag:

```
ExecStart=%h/.local/bin/niri-animation-rotate --mode manual
```

Enable and start:

```bash
systemctl --user daemon-reload
systemctl --user enable niri-animation-rotate
systemctl --user start niri-animation-rotate
```

## Acknowledgments

The 49 animation presets included in this repository are **not my own work**. They were created by talented authors and collected from the community. I'm deeply grateful for their excellent work and the quality of the animations they've created.

### Authors and sources

- **[chaoscatsofficial@gmail.com](mailto:chaoscatsofficial@gmail.com)** — 16 animations (bloom, burn-ashes, burn, burn-multicolor, explode, fold-window, glitch_00, glitch-cyberpunk, glitch, halftone, pixelate, pop-drop, ribbons, roll-drop, swipe-window, unravel)  
  Source: [XansiVA/nirimation](https://github.com/XansiVA/nirimation)

- **[Justin Garza](mailto:JGarza9788@gmail.com)** — 7 animations (blur, chromatic_edge, energize_b_niri, glide, incinerate, prism_fold, tv_crt)  
  Source: [jgarza9788/niri-animation-collection](https://github.com/jgarza9788/niri-animation-collection)

- **[Joe Hsu](mailto:jhsu.x1@gmail.com)** — 2 animations (dither-glitch, pixel-sort)  
  Source: [jgarza9788/niri-animation-collection](https://github.com/jgarza9788/niri-animation-collection)

- **liixini** ([github.com/liixini](https://github.com/liixini)) — 24 animations (circle, crosshatch, crosswarp, directiona-wipe, directional, disolve, fade, flyeye, glass-warp, glitch_01, heat-melt, ink-splash, inkwell-drop, morph, perlin, pixelfade-wave, plasma-flow, polar-function, polka-dots-curtain, randomsquares, smoke, snap, voronoi-shatter, wave-warp)  
  Sources: [liixini/shaders](https://github.com/liixini/shaders)

Thank you all for sharing your incredible work with the Niri community!

## AI Assistance

AI tools were used during the development of this project to assist with code generation, debugging, and documentation. However, the vast majority of the code and **all design decisions** are my own.

## License

MIT
