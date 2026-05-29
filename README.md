# niri-animation-rotate

A lightweight daemon that rotates Niri window animations on compositor events.

Connects to the Niri compositor's IPC event stream and cycles through animation KDL files every time a window is opened, closed, or a workspace is activated. Each session starts with a randomized order, so your animations are never the same twice.

## Features

- **Event-driven rotation** — rotates on `WindowOpenedOrChanged`, `WindowClosed`, and `WorkspaceActivated` events
- **Random shuffle** — animation order is shuffled on every startup
- **Auto-refresh** — watches the animation directory for new, removed, or modified files in real time
- **KDL config** — uses the same format as Niri for configuration
- **CLI + config file** — flexible configuration with `--flags` or a persistent config file
- **Atomic writes** — writes animation files safely to avoid Niri reading partial content
- **Graceful shutdown** — clean exit on SIGINT/SIGTERM

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
mkdir -p ~/.config/niri/niri-animation-rotate/animation.kdl
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
include "niri-animation-rotate/animations"
```

### 3. Create a config file (optional)

The default config file location is `~/.config/niri/niri-animation-rotate/config.kdl`:

```kdl
animation-dir "~/.config/niri/niri-animation-rotate/animations"
animation-target "~/.config/niri/niri-animation-rotate/animation.kdl"
```

### 4. Run the daemon

```bash
niri-animation-rotate
```

Open and close windows, or switch workspaces, and the animations will rotate automatically.

## Usage

```
niri-animation-rotate [OPTIONS]
```

### Options

```
--config <CONFIG>
        Path to the configuration file.

        KDL format with optional "animation-dir" and "animation-target" properties.

        Example:
          animation-dir "/home/user/.config/niri/niri-animation-rotate/animation.kdl"
          animation-target "/home/user/.config/niri/niri-animation-rotate/animation.kdl"

        [default: ~/.config/niri/niri-animation-rotate/config.kdl]

--animation-dir <ANIMATION_DIR>
        Directory containing animation .kdl files.

        All .kdl files in this directory will be shuffled and rotated through.
        Overrides the value from the config file.

        [default: ~/.config/niri/niri-animation-rotate/animations]

--animation-target <ANIMATION_TARGET>
        Path to the animation output file to write.

        This is the file that Niri reads via `include` in its config.
        Overrides the value from the config file.

        [default: ~/.config/niri/niri-animation-rotate/animation.kdl]

-h, --help
        Print help

-V, --version
        Print version
```

### Environment

- `NIRI_SOCKET` — must be set (automatically set by Niri in your session)
- `RUST_LOG` — controls log verbosity (default: `info`)

## Configuration

The app uses a three-tier configuration system (highest priority wins):

1. **CLI arguments** (highest priority)
2. **Config file** (KDL format)
3. **Built-in defaults** (lowest priority)

### Config file format

```kdl
animation-dir "~/.config/niri/animations"
animation-target "~/.config/niri/niri-animation-rotate/animations"
```

## How it works

1. On startup, scans the animation directory for all `.kdl` files
2. Shuffles the file list randomly
3. Writes the first animation file to the target path
4. Connects to Niri's event stream via Unix socket
5. On each relevant event, writes the next animation file and reloads Niri's config
6. Watches the animation directory for filesystem changes and refreshes the cache automatically

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

Enable and start:

```bash
systemctl --user daemon-reload
systemctl --user enable niri-animation-rotate
systemctl --user start niri-animation-rotate
```

## License

MIT
