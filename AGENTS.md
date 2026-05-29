# AGENTS.md — niri-animation-rotate

Single-binary Rust daemon that subscribes to Niri compositor IPC events and cycles through KDL animation files.

## Build & test

```bash
cargo build
cargo build --release
cargo test          # all 15 tests are inline, no tests/ directory
cargo clippy        # no clippy config — uses defaults
cargo fmt           # no rustfmt config — uses defaults
```

No Makefile, no CI, no pre-commit hooks.

## Rust toolchain

`edition = "2024"` in Cargo.toml requires **Rust ≥ 1.85**. No `rust-toolchain.toml` exists to enforce this — the system default Rust is used.

## Structure

```
src/main.rs          ← entry point: tokio runtime, event loop
src/config.rs        ← CLI (clap) + KDL config file (knuffel) merging
src/animation.rs     ← AnimationRotator: scan, shuffle, rotate, atomic write
src/niri.rs          ← reload_niri(): spawns `niri msg action reload`
```

No lib.rs — this is a binary-only crate.

## Runtime requirements

- `NIRI_SOCKET` env var must be set (it's set by the Niri session).
- `niri` binary must be on `$PATH` for config reload.
- Logging via `tracing` + `tracing-subscriber`, controlled by `RUST_LOG` (default `info`).

## Config precedence (highest wins)

1. CLI args (`--config`, `--animation-dir`, `--animation-target`)
2. KDL config file (`~/.config/niri/niri-animation-rotate/config.kdl`)
3. Built-in defaults (paths under `~/.config/niri/niri-animation-rotate/`)

## Architecture quirks

- **Single-connection IPC**: Niri subscribes the connection where the `"EventStream"` command is sent. Must use one socket: write command, `shutdown()` write half, then read events from the same connection. Two separate connections (one for write, one for read) will not receive events.
- **`--no-reload` flag**: Skips `niri msg action reload` entirely for environments (NixOS, etc.) that auto-reload config on file change.
- **`--cooldown-ms` flag**: Minimum time between rotations to prevent swapping animations mid-play. Default 0 (no cooldown).
- **Manual mode (`--mode manual`)**: Instead of listening to Niri events, the daemon listens on a Unix control socket for `rotate` commands. Combine with a Niri keybind that sends `"rotate\n"` to the socket (e.g., via `socat` or `nc -U`). Control socket path configurable via `--control-socket`.
- **`rotate_and_reload()` helper**: Common rotation logic extracted to avoid duplication between auto and manual modes.
- **Event filtering**: The first 5 Niri events sent on connection are initial state and are skipped. Only `WindowOpenedOrChanged`, `WindowClosed`, and `WorkspaceActivated` trigger rotation.
- **reload_niri() swallows errors**: If `niri msg action reload` fails, it logs a warning and returns `Ok(())`. The daemon never crashes from a reload failure.
- **Atomic writes**: `AnimationRotator.apply_current()` writes to `{target}.tmp` then renames to `{target}`.
- **Empty startup**: If the animation directory has no `.kdl` files at startup, an empty rotator is created (not an error). The filesystem watcher populates it later.
- **Filesystem watcher**: Non-recursive, watches only the animation directory for `Create`/`Remove`/`Modify` events. Sends a signal to trigger `refresh()`.
- **Refresh preserves active file**: If the currently active animation still exists after a directory rescan, its index is preserved. Otherwise rotation restarts from index 0.

## No .gitignore

The repo has no `.gitignore`. Cargo's built-in global gitignore handles `target/`, but any other generated files are not excluded.
