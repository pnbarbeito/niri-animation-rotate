# Plan: Configurable Event Triggers

## Objective

Allow users to choose which Niri compositor events trigger animation rotation, instead of the current hardcoded set of all three events. This enables use cases like "rotate only when opening an application, not when closing or switching workspaces."

## Requirements Snapshot

- **R1:** Add `--no-window-opened` flag to disable rotation on `WindowOpenedOrChanged` events.
- **R2:** Add `--no-window-closed` flag to disable rotation on `WindowClosed` events.
- **R3:** Add `--no-workspace-activated` flag to disable rotation on `WorkspaceActivated` events.
- **R4:** Config file equivalents (`no-window-opened`, `no-window-closed`, `no-workspace-activated`) in the KDL config.
- **R5:** Precedence: CLI flag > config file > default `false` (all events enabled by default).
- **R6:** Backward compatibility — default behavior is identical (all three events trigger rotation).
- **R7:** Resolved `Config` stores plain `bool` fields with `false` as the default (meaning "do not suppress" = event triggers rotation).

## Scope

- Add `--no-window-opened`, `--no-window-closed`, `--no-workspace-activated` CLI flags
- Add corresponding KDL config file fields
- Add resolved boolean fields to `Config`
- Implement merge logic in `Config::load()`
- Replace hardcoded event match in `main.rs` with config-driven check
- Add tests for the new fields
- Update README documentation (English and Spanish)

## Assumptions and Constraints

- Follows the exact same pattern as the existing `--no-reload` flag.
- All new flags default to `false` (meaning "do rotate on this event").
- The three existing event types are the only ones users will filter — no new event types are added.
- The `KdlConfig` fields use `Option<bool>` with `#[knuffel(child, unwrap(argument))]` as with `no_reload`.

## Risks and Areas Requiring Care

- The event filter in `main.rs` currently uses a single `matches!()` — switching to a `match` with config checks must preserve the exact same logic.
- No changes to manual mode (it uses the control socket, not Niri events).
- Existing tests for `extract_event_type` should be unaffected.

## Core concepts

The current hardcoded filter (in `run_auto_event_loop`):

```rust
if matches!(
    event_type.as_str(),
    "WindowOpenedOrChanged" | "WindowClosed" | "WorkspaceActivated"
) {
    // rotate...
}
```

Will become:

```rust
let should_rotate = match event_type.as_str() {
    "WindowOpenedOrChanged" => !config.no_window_opened,
    "WindowClosed" => !config.no_window_closed,
    "WorkspaceActivated" => !config.no_workspace_activated,
    _ => false,
};
if should_rotate {
    // rotate...
}
```

The `Config` merge follows the existing `no_reload` pattern:

```rust
// In Config::load():
let no_window_opened = cli.no_window_opened || kdl_config.no_window_opened.unwrap_or(false);
let no_window_closed = cli.no_window_closed || kdl_config.no_window_closed.unwrap_or(false);
let no_workspace_activated = cli.no_workspace_activated || kdl_config.no_workspace_activated.unwrap_or(false);

Ok(Config {
    no_window_opened,
    no_window_closed,
    no_workspace_activated,
    // ... existing fields
})
```

## Sub-Tasks

### Sub-Task 1: Add fields to config types and resolution

- **Status:** Completed
- **Objective:** Add `no_window_opened`, `no_window_closed`, `no_workspace_activated` to `Cli`, `KdlConfig`, and resolved `Config` structs, with merge logic in `Config::load()`.
- **Related Requirements:** R1, R2, R3, R4, R5, R6, R7
- **Dependencies and Preconditions:** None
- **In Scope for This Sub-Task:**
  - Add three `#[arg(long)] pub no_window_opened: bool` (etc.) fields to `Cli`
  - Add three `#[knuffel(child, unwrap(argument))] pub no_window_opened: Option<bool>` (etc.) fields to `KdlConfig`
  - Add three `pub no_window_opened: bool` (etc.) fields to the resolved `Config` struct
  - Merge in `Config::load()` following the exact pattern of `no_reload`:
    ```rust
    cli.no_window_opened || kdl_config.no_window_opened.unwrap_or(false)
    ```
  - Update the doc comment examples on `Cli` (the `--config` KDL example) and `KdlConfig` to include the new fields
- **Out of Scope for This Sub-Task:**
  - Changes to `main.rs` (Sub-Task 2)
  - Adding tests (Sub-Task 3)
  - Documentation beyond doc comments (Sub-Task 4)
- **Instructions:**
  1. In `src/config.rs`:
     - Add `no_window_opened`, `no_window_closed`, `no_workspace_activated` to `KdlConfig` struct as `Option<bool>`
     - Add the same three fields to `Cli` struct as `bool`
     - Add the same three fields to `Config` struct as `bool`
     - In `Config::load()`, add merge lines following `no_reload` pattern
     - Include them in `Ok(Config { ... })`
     - Add doc comments with `///` explaining each field
  2. Update the KDL example in the `--config` doc comment to include the new fields
- **Acceptance Criteria:**
  - `cargo build` succeeds with no warnings
  - `cargo test` passes (all existing tests)
  - `cargo clippy` shows no new warnings
- **Cautionary Points (Risks & Edge Cases):**
  - The knuffel attribute is `#[knuffel(child, unwrap(argument))]` — verify `Option<bool>` works with this (same as `log_socket`, `no_reload`)
  - clap generates `--no-window-opened` for `#[arg(long)] pub no_window_opened: bool` — verify this works as a simple flag (no value needed)
- **Implementation Suggestions:**
  - For `Cli`, these should be plain `bool` (not `Option<bool>`), just like `no_reload` and `log_socket`
  - For `KdlConfig`, these should be `Option<bool>` (not present = not set)
  - Merge: `cli.no_window_opened || kdl_config.no_window_opened.unwrap_or(false)`
- **Testing Suggestions:** `cargo build && cargo test && cargo clippy`
- **Done When:** Build succeeds, all tests pass, no new clippy warnings.

### Sub-Task 2: Wire event filtering in main.rs

- **Status:** Completed
- **Objective:** Replace the hardcoded `matches!()` event filter in `run_auto_event_loop` with a config-driven match that checks `config.no_window_opened`, etc.
- **Related Requirements:** R6 (must preserve default behavior)
- **Dependencies and Preconditions:** Sub-Task 1 (the `Config` must have the new fields)
- **In Scope for This Sub-Task:**
  - In `src/main.rs`, replace lines 141-145:
    ```rust
    if matches!(
        event_type.as_str(),
        "WindowOpenedOrChanged" | "WindowClosed" | "WorkspaceActivated"
    ) {
    ```
    with:
    ```rust
    let should_rotate = match event_type.as_str() {
        "WindowOpenedOrChanged" => !config.no_window_opened,
        "WindowClosed" => !config.no_window_closed,
        "WorkspaceActivated" => !config.no_workspace_activated,
        _ => false,
    };
    if should_rotate {
    ```
  - Adjust the indentation and braces so the cooldown check and `rotate_and_reload` call remain inside the `if should_rotate { ... }` block.
- **Out of Scope for This Sub-Task:**
  - Changes to manual mode or `run_manual_event_loop`
  - Changes to `extract_event_type` function
- **Instructions:**
  1. Read the exact lines around the match in `main.rs`
  2. Replace the `if matches!(...)` block with the `let should_rotate = match ...` approach
  3. Ensure the cooldown check, tracing, and `rotate_and_reload` call remain inside the conditional
  4. Ensure the `else` branch (logging ignored events) remains unchanged
- **Acceptance Criteria:**
  - `cargo build` succeeds
  - `cargo test` passes (all 17+)
  - The logic is identical when all flags are `false` (default)
- **Cautionary Points (Risks & Edge Cases):**
  - Pay careful attention to brace matching — the existing code has a cooldown check inside the `if matches!` block that must remain inside `if should_rotate`
  - The `_ => false` branch replaces the implicit behavior of the existing `matches!()` macro
- **Testing Suggestions:** `cargo build && cargo test`
- **Done When:** Build and tests pass, event filtering logic is extracted from `matches!()` macro into config-driven match.

### Sub-Task 3: Add tests

- **Status:** Completed
- **Objective:** Add unit tests for the new event filter fields.
- **Related Requirements:** R1, R2, R3, R5, R6
- **Dependencies and Preconditions:** Sub-Task 1 (fields must exist)
- **In Scope for This Sub-Task:**
  - Add tests for merge logic: verify that `cli` value overrides `kdl_config` value for each flag
  - Test that default is `false` (all events enabled)
  - Test the `cli.x || kdl_config.x.unwrap_or(false)` pattern
  - Tests go in the existing `#[cfg(test)] mod tests` block in `src/config.rs`
  - Unlike the socket path tests, these tests don't need env var manipulation — they can construct scenarios directly
- **Out of Scope for This Sub-Task:**
  - Integration tests with actual Niri events
  - Testing the main.rs event loop logic (hard to mock)
- **Instructions:**
  1. Add tests that verify the merge pattern:
     - Test that `Config` defaults have all flags `false`
     - Construct scenarios with `KdlConfig` values and verify the resolved `Config`
     - Test that CLI values take precedence (can test through `Config::load()` indirectly or test the merge pattern directly)
  2. Since `Config::load()` calls `Cli::parse()` (reads real args), consider adding a `#[cfg(test)]` helper that constructs `Config` from parts, or test the merge logic in isolation by calling the helper functions
- **Acceptance Criteria:**
  - `cargo test` passes
  - New tests cover the three boolean event flags
- **Cautionary Points (Risks & Edge Cases):**
  - No env var manipulation needed (unlike socket path tests)
  - Tests are simple boolean logic — no race conditions
- **Implementation Suggestions:**
  ```rust
  #[test]
  fn test_event_filter_defaults_are_false() {
      // If neither CLI nor config sets them, they should be false
      // (meaning all events trigger rotation by default)
  }

  #[test]
  fn test_event_filter_config_file_sets_true() {
      // If KdlConfig sets no_window_opened to Some(true),
      // the resolved Config should have no_window_opened = true
  }
  ```
- **Testing Suggestions:** `cargo test`
- **Done When:** All tests pass with `cargo test`.

### Sub-Task 4: Update documentation

- **Status:** Completed
- **Objective:** Update README.md and create README_ES.md documenting the new flags.
- **Related Requirements:** R1, R2, R3
- **Dependencies and Preconditions:** Sub-Task 1 (flags must exist to document)
- **In Scope for This Sub-Task:**
  - Update the **Options** table in `README.md` with the three new flags
  - Update the **Config file format** example in `README.md`
  - Update the **Merge precedence** table in `README.md`
  - Create `README_ES.md` with a full Spanish translation of the README
  - The `--help` documentation is auto-generated from doc comments (already covered in Sub-Task 1)
- **Out of Scope for This Sub-Task:**
  - Changing the app's doc comments (covered in Sub-Task 1)
- **Instructions:**
  1. Edit `README.md`:
     - Add three rows to the Options table:
       `--no-window-opened`, `--no-window-closed`, `--no-workspace-activated`
     - Add them to the config file KDL example
     - Add a row to the Merge precedence table
  2. Create `README_ES.md` as a complete Spanish translation of `README.md`
- **Acceptance Criteria:**
  - `README.md` documents all three new flags
  - `README_ES.md` is a complete Spanish translation
- **Testing Suggestions:** Visual review of the markdown files
- **Done When:** Both documentation files accurately describe the new feature.

## Final Integration & Verification

- **System-Wide Test:** `cargo build --release && cargo test`
- **Completion Checklist:**
  - [ ] Sub-Task 1: Config types and resolution compile and pass tests.
  - [ ] Sub-Task 2: Event filtering is config-driven in `main.rs`.
  - [ ] Sub-Task 3: Tests cover the new fields.
  - [ ] Sub-Task 4: `README.md` updated, `README_ES.md` created.
  - [ ] `cargo clippy` produces no new warnings.
  - [ ] `cargo fmt` produces no changes.

## Open Questions

- None — design is settled (negative flags following `--no-reload` pattern).
