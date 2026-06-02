# Plan: Manual Mode Extensions + Configurable Random

## Objective

Extend the daemon with three new capabilities:

1. **Configurable random shuffle** — `--random-order` flag, disabled by default. When enabled, animations are shuffled on load; when disabled, they follow alphabetical order.
2. **Dual-direction rotation in manual mode** — `next` and `prev` commands on the control socket (keep `rotate` as an alias for `next`).
3. **Bidirectional control socket protocol** — the socket now writes responses back. New commands: `current` (active animation name), `list` (all available animations), and `select <name>` (pick a specific animation by name).

Hot-switch between auto and manual mode (`3.4`) is deferred to a separate phase.

## Requirements Snapshot

- **R1:** `--random-order` flag (CLI + KDL config), default `false`. When `true`, shuffle on `new()` and `refresh()`; when `false`, use alphabetical order.
- **R2:** `rotate_prev()` method on `AnimationRotator` that decrements `current_index` with wrap-around and calls `apply_current()`.
- **R3:** Control socket accepts `next` (forward), `prev` (backward), and `rotate` (alias for `next`). Both respect the existing cooldown in manual mode.
- **R4:** Control socket writes responses. `current` returns `file_stem` of the active animation. `list` returns all `file_stem`s, one per line. `select <name>` searches case-insensitive by `file_stem`, sets `current_index`, calls `apply_current`, and writes `ok` or `error: not found`. Format: plain text, one line per response (multiple lines for `list`, each terminated by `\n`).
- **R5:** `select` bypasses cooldown (explicit user action overrides the timer).
- **R6:** `current` and `list` do not trigger rotation or cooldown — read-only queries.
- **R7:** Backward compatibility: `rotate` remains as valid forward command.

## Scope

- Modified: `src/config.rs` — `random_order: bool` field in `KdlConfig`, `Cli`, `Config`
- Modified: `src/animation.rs` — `random_order` field in `AnimationRotator`, conditional `shuffle()`, new `rotate_prev()`, new `select_by_name()`
- Modified: `src/main.rs` — bidirectional socket protocol in `run_manual_event_loop`, new command dispatch
- New tests: random order behavior, prev rotation, socket commands (current, list, select)
- Updated: README docs, CLI help strings

**Out of scope:** hot-switch mode (deferred), changes to auto event loop, `select` outside manual mode.

## Assumptions and Constraints

- The manual mode socket currently uses `last_rotation` + fixed `--cooldown-ms` (not the duration-aware `bloqueo_hasta` from auto mode). This is correct per R9 from the previous plan and remains unchanged in this phase.
- `select` searches by `file_stem()` (filename without `.kdl` extension), case-insensitive.
- `current` returns only the `file_stem()`, not the full path.
- `list` returns entries in the current internal order (alphabetical or shuffled).
- The control socket is `tokio::net::UnixListener`, connections are one-command-per-connection (no persistent session). This is preserved — each accepted connection reads one line, processes it, writes a response, and closes.
- Plain text protocol: no JSON framing, no length prefixes.

## Risks and Areas Requiring Care

- **Random order default is `false`:** This changes current behavior (always shuffled). Users upgrading will get alphabetical order unless they set `random-order true`. Document this clearly as a breaking change.
- **`select` bypassing cooldown:** `select` should reset the `last_rotation` timer (or `bloqueo_hasta`) just like a normal rotation would, so subsequent `next`/`prev` commands respect the new animation's cooldown.
- **`select` with no args:** Command is `select` alone (no name) → respond `error: missing name`.
- **`select` on deleted file:** If the selected animation file has been deleted, `apply_current` should handle it the same way `rotate` does (check existence, refresh if missing).
- **Socket response encoding:** Use `writeln!` for line-terminated responses. For `list`, each filename on its own line. The client reads until EOF or until a special terminator. Since the connection is one-command-per-connection, EOF after the last line is the natural terminator.
- **`rotate_prev` edge case:** When `files` is empty, same no-op as `rotate`.

## Core Concepts

### New `AnimationRotator` methods

```rust
/// Rotate to the previous animation (decrement with wrap-around).
pub async fn rotate_prev(&mut self) -> Result<()> {
    if self.files.is_empty() {
        return Ok(());
    }
    self.current_index = if self.current_index == 0 {
        self.files.len() - 1
    } else {
        self.current_index - 1
    };
    // Same existence check + apply_current as rotate()
    let path = self.files[self.current_index].path.clone();
    if !path.exists() {
        self.refresh().await;
        if self.files.is_empty() { return Ok(()); }
        self.current_index = 0;
    }
    self.apply_current().await
}

/// Select an animation by filename stem (case-insensitive).
/// Returns Ok(true) if found and applied, Ok(false) if not found.
pub async fn select_by_name(&mut self, name: &str) -> Result<bool> {
    let found = self.files.iter().position(|e| {
        e.path.file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.eq_ignore_ascii_case(name))
            .unwrap_or(false)
    });
    match found {
        Some(pos) => {
            self.current_index = pos;
            let path = self.files[self.current_index].path.clone();
            if !path.exists() {
                self.refresh().await;
                if self.files.is_empty() { return Ok(true); /* file gone */ }
                self.current_index = 0;
            }
            self.apply_current().await?;
            Ok(true)
        }
        None => Ok(false),
    }
}
```

### Socket protocol

| Command | Response | Side effects |
|---------|----------|-------------|
| `rotate` | _(no response, legacy compat)_ | rotate + reload, update cooldown |
| `next` | _(no response)_ | rotate + reload, update cooldown |
| `prev` | _(no response)_ | rotate_prev + reload, update cooldown |
| `current` | `prism_fold` | none |
| `list` | `bloom`\n`prism_fold`\n`tv_crt`\n | none |
| `select prism_fold` | `ok` | apply_current + reload, update cooldown |
| `select nonexistent` | `error: not found` | none |
| `select` | `error: missing name` | none |
| anything else | `error: unknown command` | none |

### Cooldown interaction with `select`

```rust
// In run_manual_event_loop, on select command:
let ahora = Instant::now();
let mut anim = animator.lock().await;
match anim.select_by_name(name).await {
    Ok(true) => {
        writeln!(stream, "ok")?;
        drop(anim);
        if !config.no_reload { niri::reload_niri().await; }
        // Update cooldown timer (select resets it)
        last_rotation = Some(ahora);
    }
    Ok(false) => {
        writeln!(stream, "error: not found")?;
    }
    Err(e) => {
        writeln!(stream, "error: {}", e)?;
    }
}
```

## Sub-Tasks

### Sub-Task 1: Configurable random order

- **Status:** Completed
- **Objective:** Add `random_order: bool` field to the config pipeline and `AnimationRotator`, making shuffle conditional.
- **Related Requirements:** R1
- **Dependencies and Preconditions:** None
- **In Scope for This Sub-Task:**
  - Add `random_order: Option<bool>` to `KdlConfig` (knuffel child)
  - Add `random_order: bool` to `Cli` (clap arg, `--random-order`, default `false`)
  - Add `random_order: bool` to `Config` (merged: `cli.random_order || kdl.random_order.unwrap_or(false)`)
  - Add `random_order: bool` field to `AnimationRotator`
  - Modify `AnimationRotator::new()` to conditionally call `shuffle()` based on `random_order`
  - Modify `refresh()` to conditionally call `shuffle()` based on `self.random_order`
  - Pass `config.random_order` from `main.rs` to `AnimationRotator::new()` and `AnimationRotator::empty()`
- **Out of Scope for This Sub-Task:**
  - Per-rotation random pick (only initial shuffle is controlled)
  - Changes to the socket protocol
- **Instructions:**
  1. Add field to `KdlConfig` (after `duration_fallback_ms`)
  2. Add field to `Cli` with doc comment and `#[arg(long)]` (after `duration_fallback_ms`)
  3. Add field to `Config` (after `duration_fallback_ms`)
  4. Add merge logic in `Config::load()`: `let random_order = cli.random_order || kdl_config.random_order.unwrap_or(false);`
  5. Add field to `AnimationRotator` struct
  6. Update `new()` signature to accept `random_order: bool`
  7. Add `if self.random_order { self.shuffle(); }` in `new()`
  8. Update `refresh()`: wrap the existing `rotator.shuffle()` call in `if self.random_order`
  9. Update `empty()` signature to accept `random_order: bool`
  10. Update `main.rs`: pass `config.random_order` to `AnimationRotator::new()` and `AnimationRotator::empty()`
  11. Update KDL doc comment example in `Cli` to show `random-order true`
  12. Add tests in `animation.rs`: `test_random_order_true_shuffles`, `test_random_order_false_preserves_order`
  13. Add tests in `config.rs`: `test_random_order_defaults_to_false`, `test_random_order_kdl_parsing`
- **Acceptance Criteria:**
  - `cargo build` compiles
  - 45 existing tests + new tests pass
  - `cargo run -- --help` shows `--random-order`
  - `cargo clippy` clean (only pre-existing warnings)
- **Cautionary Points (Risks & Edge Cases):**
  - `refresh()` currently rebuilds a new `AnimationRotator` struct. Ensure `random_order` is carried through (using `self.random_order` from the existing instance).
  - When `random_order` is `false` and `refresh()` preserves the active file's index, the preserve logic still works correctly (index matches after alphabetical re-scan since the order is stable).
- **Implementation Suggestions:** The `random_order` field should be placed after `fallback_duration_ms` in all structs for consistency.
- **Testing Suggestions:** For shuffle test, check that the order differs from alphabetical (since true randomness might coincidentally produce alphabetical, accept that it's probabilistic — or use a deterministic test with a seeded RNG, but that's harder with `thread_rng`. Simpler: test that with `random_order = false`, the order is alphabetical as returned by `scan_directory`).
- **Done When:** All acceptance criteria met. Tests pass. Clippy clean.

### Sub-Task 2: `rotate_prev()` and dual commands on socket

- **Status:** In Progress
- **Objective:** Add `rotate_prev()` to `AnimationRotator` and modify the manual socket to accept `next`, `prev`, and `rotate` commands.
- **Related Requirements:** R2, R3, R7
- **Dependencies and Preconditions:** Sub-Task 1 completed (`AnimationRotator` struct may have new field).
- **In Scope for This Sub-Task:**
  - Add `pub async fn rotate_prev(&mut self) -> Result<()>` to `AnimationRotator`
  - Modify `run_manual_event_loop` command dispatch: match `"next" | "rotate"` → forward, `"prev"` → backward
  - Both commands respect the existing `last_rotation` cooldown check
  - After rotation, update `last_rotation = Some(Instant::now())`
- **Out of Scope for This Sub-Task:**
  - Bidirectional responses (Sub-Task 3)
  - `current`, `list`, `select` commands (Sub-Task 3)
  - Changes to `rotate_and_reload` helper (it only does forward — prev needs separate handling or a param)
- **Instructions:**
  1. Add `rotate_prev()` to `AnimationRotator` (mirror `rotate()` but decrement with wrap-around)
  2. In `run_manual_event_loop`, extract the command dispatch into a match:
     ```rust
     match command {
         "next" | "rotate" => {
             // existing cooldown check + rotate_and_reload + update last_rotation
         }
         "prev" => {
             // same cooldown check + call rotate_prev via lock + reload
         }
         _ => {
             tracing::debug!(command = %command, "Unknown command on control socket");
         }
     }
     ```
  3. For `prev`: lock the animator, call `anim.rotate_prev().await`, if Ok + !no_reload → reload
  4. Both `next` and `prev` share the same cooldown check (use `last_rotation` as before)
  5. Add test: `test_rotate_prev_advances_backward` in `animation.rs`
  6. Add test: `test_rotate_prev_wraps_from_zero` in `animation.rs`
- **Acceptance Criteria:**
  - `rotate_prev()` works: current_index decrements, wraps from 0 to len-1
  - Sending `prev\n` to the socket triggers backward rotation
  - Sending `rotate\n` still works (forward, backward compat)
  - Cooldown respected for both directions
  - Tests pass
- **Cautionary Points (Risks & Edge Cases):**
  - `rotate_prev()` must handle the deleted-file edge case same as `rotate()` (check existence, refresh if missing)
  - The `rotate_and_reload` helper only does forward rotation. For `prev`, either: (a) add a parameter, (b) inline the logic in the event loop, or (c) create `rotate_prev_and_reload`. Option (b) is simplest and avoids changing the helper's signature (which is also used by auto mode where prev doesn't apply).
- **Implementation Suggestions:** Inline the `prev` handling in `run_manual_event_loop` rather than creating a new helper, since it's only used in manual mode.
- **Testing Suggestions:** `cargo test` — target the new `rotate_prev` tests in `animation.rs`.
- **Done When:** `cargo build` compiles, all tests pass, both `next` and `prev` commands work via socket.

### Sub-Task 3: Bidirectional socket with `current`, `list`, `select`

- **Status:** Pending
- **Objective:** Transform the control socket from read-only to read+write. Implement `current`, `list`, and `select` commands with plain text responses.
- **Related Requirements:** R4, R5, R6
- **Dependencies and Preconditions:** Sub-Task 2 completed (socket command dispatch already refactored).
- **In Scope for This Sub-Task:**
  - After reading and dispatching a command, write a response back to `stream`
  - `current` command: lock animator, get `file_stem()` of active file, write to stream
  - `list` command: lock animator, iterate `files`, write each `file_stem()` on its own line
  - `select <name>` command: lock animator, call `select_by_name(name)`, write `ok` or `error: not found`
  - `select` with no name: write `error: missing name`
  - Unknown commands: write `error: unknown command`
  - `rotate`/`next`/`prev`: no response (backward compat — clients that don't read responses aren't affected)
  - Add `select_by_name()` to `AnimationRotator`
  - `select` updates `last_rotation` (resets cooldown)
  - `select` triggers `reload_niri()` on success (unless `--no-reload`)
  - All responses use `writeln!` for line termination
- **Out of Scope for This Sub-Task:**
  - Persistent connections (commands are still one-per-connection)
  - JSON responses
  - Rate limiting or authentication
- **Instructions:**
  1. Add `select_by_name(&mut self, name: &str) -> Result<bool>` to `AnimationRotator`
  2. Add `current_file_stem(&self) -> Option<String>` helper to `AnimationRotator` (returns `None` if empty)
  3. Add `file_stems(&self) -> Vec<String>` helper to `AnimationRotator`
  4. Refactor the command dispatch block in `run_manual_event_loop` into a `match` that handles all commands:
     ```rust
     let response = match command {
         "current" => {
             let anim = animator.lock().await;
             match anim.current_file_stem() {
                 Some(name) => name,
                 None => "error: no animations available".to_string(),
             }
         }
         "list" => {
             let anim = animator.lock().await;
             anim.file_stems().join("\n")
         }
         cmd if cmd.starts_with("select ") => {
             let name = cmd["select ".len()..].trim();
             if name.is_empty() {
                 "error: missing name".to_string()
             } else {
                 let ahora = Instant::now();
                 let mut anim = animator.lock().await;
                 match anim.select_by_name(name).await {
                     Ok(true) => {
                         drop(anim);
                         if !config.no_reload {
                             let _ = niri::reload_niri().await;
                         }
                         last_rotation = Some(ahora);
                         "ok".to_string()
                     }
                     Ok(false) => "error: not found".to_string(),
                     Err(e) => format!("error: {}", e),
                 }
             }
         }
         cmd if cmd == "select" => "error: missing name".to_string(),
         "next" | "rotate" => {
             // existing cooldown + rotate logic (no response)
             continue; // or write nothing and close
         }
         "prev" => {
             // existing cooldown + rotate_prev logic (no response)
             continue;
         }
         _ => format!("error: unknown command: {}", command),
     };
     writeln!(stream, "{}", response)?;
     ```
  5. For `rotate`/`next`/`prev`: these historically had no response. Keep it that way for backward compat. Use `continue` after rotation (skip the `writeln!` at the bottom).
  6. Add tests: `test_select_by_name_found`, `test_select_by_name_not_found`, `test_select_by_name_case_insensitive`, `test_current_file_stem`, `test_file_stems` in `animation.rs`
- **Acceptance Criteria:**
  - `current` returns the active animation's stem (e.g., `prism_fold`)
  - `list` returns all stems, one per line
  - `select prism_fold` returns `ok` and applies the animation
  - `select nonexistent` returns `error: not found`
  - `select` (no args) returns `error: missing name`
  - Unknown command returns `error: unknown command: <cmd>`
  - `rotate`/`next`/`prev` still work and don't break clients that don't read responses
  - `select` bypasses cooldown and resets it
  - Tests pass
- **Cautionary Points (Risks & Edge Cases):**
  - The `list` response uses `\n` as separator. If a filename contains `\n` (unlikely on Linux but possible), this breaks. Not a realistic concern.
  - `select` resets `last_rotation` so that subsequent `next`/`prev` don't fire immediately (the new animation needs time to play).
  - Cooldown for `rotate`/`next`/`prev` uses `last_rotation` + `config.cooldown_ms` — the `config.cooldown_ms` is a fixed value, not the duration-aware debounce. This is intentional (manual mode always used fixed cooldown). `select` bypasses this check entirely but resets the timer.
  - The `select` command's async lock pattern: lock, find+apply, unlock, then reload (avoids holding lock across reload).
- **Implementation Suggestions:** Keep `current_file_stem()` and `file_stems()` as simple `pub fn` methods (not async, just reads fields).
- **Testing Suggestions:** `cargo test` — target the new animation.rs tests.
- **Done When:** `cargo build` compiles, all tests pass, `echo "current" | nc -U control.sock` returns the active filename.

### Sub-Task 4: Tests and documentation

- **Status:** Pending
- **Objective:** Add integration-level tests for the socket protocol and update README documentation.
- **Related Requirements:** R1, R2, R3, R4, R5, R6, R7
- **Dependencies and Preconditions:** Sub-Tasks 1-3 completed.
- **In Scope for This Sub-Task:**
  - Update README.md and README_ES.md: document `--random-order`, new socket commands (`next`, `prev`, `current`, `list`, `select`)
  - Update CLI doc comments in `config.rs` for `--random-order`
  - Update mode description in `--mode` doc comment (manual mode now supports multiple commands)
  - Add any missing edge-case tests discovered during ST1-ST3
  - Review all existing tests still pass
- **Out of Scope for This Sub-Task:**
  - Socket-level integration tests (requiring actual Unix socket setup) — unit tests for the logic are sufficient
- **Instructions:**
  1. Update README.md:
     - Add `--random-order` to Options table
     - Expand Manual Mode section: document all 6 socket commands with examples
     - Add `nc -U` usage examples for `current`, `list`, `select`
     - Update config file KDL example to include `random-order true`
  2. Update README_ES.md with equivalent Spanish text
  3. Update `Cli` doc comments for `--random-order` and `--mode`
  4. `cargo test` — confirm all tests pass
  5. `cargo clippy` — confirm clean
- **Acceptance Criteria:**
  - Both READMEs accurately describe new behavior
  - CLI help shows all new options
  - `cargo test` passes
  - `cargo clippy` clean (only pre-existing warnings)
- **Done When:** Docs updated, all tests pass, ready for manual testing.

## Final Integration & Verification

- **System-Wide Test:**
  1. `cargo build --release` — no errors, no warnings
  2. `cargo test` — all tests pass
  3. `cargo clippy` — only pre-existing warnings
  4. `cargo fmt -- --check` — no formatting changes
  5. `cargo run -- --help` — shows `--random-order`
  6. Manual test in Niri session:
     - Start daemon with `--mode manual --random-order true`
     - `echo "current" | nc -U control.sock` → returns active animation stem
     - `echo "list" | nc -U control.sock` → returns all animations
     - `echo "next" | nc -U control.sock` → rotates forward
     - `echo "prev" | nc -U control.sock` → rotates backward
     - `echo "select bloom" | nc -U control.sock` → returns `ok`
     - `echo "select nonexistent" | nc -U control.sock` → returns `error: not found`
     - Verify `rotate` still works as forward alias
     - Verify `random-order false` (default) gives alphabetical order

- **Completion Checklist:**
  - [x] ST1: `--random-order` flag works; shuffle conditional
  - [ ] ST2: `next` and `prev` commands on socket; `rotate` alias preserved
  - [ ] ST3: `current`, `list`, `select` commands with responses
  - [ ] ST4: Docs updated, all tests pass
  - [ ] `cargo build --release` compiles
  - [ ] `cargo test` passes all tests
  - [ ] `cargo clippy` clean (only pre-existing warnings)
  - [ ] `cargo fmt` clean

## Open Questions

- None — all design decisions confirmed with user (plain text protocol, `select` without extension, `select` bypasses cooldown, hot-switch deferred).
