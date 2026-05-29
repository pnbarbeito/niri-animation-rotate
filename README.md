# niri-animation-rotate

A lightweight daemon that rotates Niri window animations on compositor events or via manual keybinds.

Connects to the Niri compositor's IPC event stream and cycles through animation KDL files. Supports both automatic mode (event-driven) and manual mode (triggered via a keybind).

## Features

- **Automatic mode** — rotates on `WindowOpenedOrChanged`, `WindowClosed`, and `WorkspaceActivated` events
- **Manual mode** — rotate on demand via a Unix control socket and a Niri keybind
- **Random shuffle** — animation order is shuffled on every startup
- **Auto-refresh** — watches the animation directory for new, removed, or modified files in real time
- **Cooldown** — optional minimum time between rotations to prevent mid-play swaps
- **No-reload mode** — skip `niri msg action reload` for environments that auto-reload on file change
- **KDL config** — uses the same format as Niri for configuration
- **CLI + config file** — flexible configuration with `--flags` or a persistent config file
- **Atomic writes** — writes animation files safely to avoid Niri reading partial content
- **Graceful shutdown** — clean exit on SIGINT/SIGTERM
- **Debug logging** — `--log-socket` to inspect raw Niri IPC messages

## Prerequisites

- A running [Niri](https://github.com/YaLTeR/niri) compositor session
- Rust toolchain (for building from source)

## Installation

### From source

```bash
git clone https://github.com/yourusername/niri-animation-rotate.git
cd niri-animation-rotate
cargo build --release
```

The binary will be at `target/release/niri-animation-rotate`.

### Cargo install

```bash
cargo install --path .
```

## Setup

### 1. Create animation files

Place your animation `.kdl` files in the animations directory:

```bash
mkdir -p ~/.config/niri/niri-animation-rotate/animations
```

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
| `--mode <MODE>` | Operation mode: `auto` (Niri events) or `manual` (control socket) | `auto` |
| `--control-socket <PATH>` | Unix socket path for manual mode | `~/.config/niri/niri-animation-rotate/control.sock` |
| `--cooldown-ms <MS>` | Minimum ms between rotations (0 = no cooldown) | `0` |
| `--no-reload` | Skip `niri msg action reload` after rotation | — |
| `--log-socket` | Print raw Niri IPC lines to stderr (debugging) | — |
| `-h`, `--help` | Print help | — |
| `-V`, `--version` | Print version | — |

### Environment

- `NIRI_SOCKET` — must be set (automatically set by Niri in your session). Not needed in manual mode.
- `RUST_LOG` — controls log verbosity (default: `info`)

### Modes

#### Auto mode (default)

The daemon connects to the Niri IPC event stream and rotates animations automatically on `WindowOpenedOrChanged`, `WindowClosed`, and `WorkspaceActivated`. This is the default behavior.

```bash
niri-animation-rotate
```

The first 5 events received are initial Niri state and are skipped.

#### Manual mode

Instead of listening to Niri events, the daemon listens on a Unix control socket for `rotate` commands. Use together with a Niri keybind.

First, start the daemon in manual mode:

```bash
niri-animation-rotate --mode manual
```

Then add a keybind to your Niri config (`~/.config/niri/config.kdl`):

```kdl
binds {
    Mod+Shift+A { spawn-sh "echo 'rotate' | nc -U $HOME/.config/niri/niri-animation-rotate/control.sock"; }
}
```

Or with `socat`:

```kdl
binds {
    Mod+Shift+A { spawn-sh "echo 'rotate' | socat - UNIX-CONNECT:$HOME/.config/niri/niri-animation-rotate/control.sock"; }
}
```

The socket file is cleaned up automatically on shutdown.

> **Note:** Niri's `spawn` does not use a shell and does not expand `~` or `$HOME`. Use `spawn-sh` (Niri ≥ 25.08) or pass the full absolute path with `spawn "sh" "-c" "..."`.

#### Cooldown

To prevent animation swaps mid-play, set a minimum time between rotations:

```bash
niri-animation-rotate --cooldown-ms 3000
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
log-socket true
no-reload true
cooldown-ms 2000
mode "manual"
control-socket "~/.config/niri/niri-animation-rotate/control.sock"
```

For boolean options (`log-socket`, `no-reload`), the config file can only enable them. To disable, omit the line or use the CLI flag.

### Merge precedence

| Setting type | CLI | Config file | Default |
|---|---|---|---|
| Paths (`animation-dir`, etc.) | `--path /x` wins | `path "/x"` | `~/.config/niri/...` |
| Bools (`log-socket`, `no-reload`) | `--flag` wins (always enables) | `flag true` enables | `false` |
| Values (`cooldown-ms`, `mode`) | `--value X` wins | `value X` applies | `0` / `auto` |

## How it works

1. On startup, scans the animation directory for all `.kdl` files
2. Shuffles the file list randomly (current selection is preserved across directory rescans)
3. **Preserves the existing output file** — no overwrite on startup
4. In auto mode: connects to Niri's event stream via Unix socket
5. In manual mode: listens on a control socket for `rotate` commands
6. On each rotation trigger, writes the next animation file atomically and reloads Niri's config
7. Watches the animation directory for filesystem changes and refreshes the cache automatically

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

Create `~/.config/systemd/user/niri-animation-rotate.service`:

```ini
[Unit]
Description=Niri Animation Rotate
After=niri-session.service

[Service]
Type=simple
ExecStart=%h/.cargo/bin/niri-animation-rotate
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
```

For manual mode, include the flags:

```
ExecStart=%h/.cargo/bin/niri-animation-rotate --mode manual
```

Enable and start:

```bash
systemctl --user daemon-reload
systemctl --user enable niri-animation-rotate
systemctl --user start niri-animation-rotate
```

## License

MIT
