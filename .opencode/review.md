# Review: Sub-Task 4 — Documentation Update

**Reviewer:** OpenCode agent  
**Plan scope:** Sub-Task 4 (README.md update + README_ES.md creation)  
**Branch:** main (working tree, no commits yet)  
**Date:** 2026-06-01  

---

## 1. Build & Test Results

| Check | Status | Details |
|---|---|---|
| `cargo build` | ✅ PASS | No errors, no warnings |
| `cargo test` | ✅ PASS | 21/21 tests passed |
| `cargo fmt --check` | ✅ PASS | No formatting changes needed |
| `cargo clippy` | ⚠️ 3 pre-existing warnings | See notes below |

**Clippy warnings** (all pre-existing, not introduced by Sub-Task 4):
- `src/animation.rs:155` — `collapsible_if` (unrelated to docs)
- `src/config.rs:83` — `doc_overindented_list_items` (unrelated to docs)
- `src/main.rs:73` — `collapsible_if` (unrelated to docs)

These existed before Sub-Task 4 and are not caused by any change in this review's scope.

---

## 2. Verification Checklist

### 2.1 All 3 new flags documented in `README.md` ✅

| Flag | Location(s) in README.md | Found? |
|---|---|---|
| `--no-window-opened` | Features (L10), Options table (L112), KDL example (L192), bool note (L200), precedence table (L208) | ✅ |
| `--no-window-closed` | Features (L10), Options table (L113), KDL example (L193), bool note (L200), precedence table (L208) | ✅ |
| `--no-workspace-activated` | Features (L10), Options table (L114), KDL example (L194), bool note (L200), precedence table (L208) | ✅ |

### 2.2 KDL config example in `README.md` includes new fields ✅

Lines 186–198 contain a complete KDL block including:
```
niri-socket "/run/user/1000/niri.sock"
no-window-opened false
no-window-closed false
no-workspace-activated false
```

### 2.3 `README_ES.md` contains equivalent documentation in Spanish ✅

- Same structure (271 lines, matching section-for-section)
- All features, including "Filtros de eventos configurables", translated
- Options table includes all 3 `--no-window-*` flags with Spanish descriptions
- KDL config example mirroring the English one with all new fields
- Merge precedence table includes boolean options with `no-window-*` fields
- All sections covered: Prerrequisitos, Instalación, Uso, Modos, Configuración, Cómo funciona, Registros, systemd, Licencia

### 2.4 Merge precedence table updated ✅

- New row for "Niri socket" (`--niri-socket`)
- Boolean row expanded to list all 5 boolean flags including the 3 new ones
- Present in both `README.md` (L204–209) and `README_ES.md` (L204–209)

### 2.5 Boolean options note updated ✅

Both files now say:
> "For boolean options (`log-socket`, `no-reload`, `no-window-opened`, `no-window-closed`, `no-workspace-activated`), the config file can only enable them."

---

## 3. Scope Check

### Files modified/created by Sub-Task 4

| File | Change | Within scope? |
|---|---|---|
| `README.md` | Modified | ✅ Sub-Task 4 scope |
| `README_ES.md` | Created (untracked) | ✅ Sub-Task 4 scope |

### Other changes in working tree (from earlier sub-tasks)

| File | Change | Notes |
|---|---|---|
| `src/config.rs` | Modified | Sub-Task 1 + Sub-Task 3 (config types + tests) |
| `src/main.rs` | Modified | Sub-Task 2 (event filter wiring) |
| `src/animation.rs` | Modified | Formatting-only changes (rustfmt) — appears incidental |

**Conclusion:** No files outside the plan's total scope were changed. The `src/animation.rs` formatting diff appears to be a side effect of `cargo fmt` being run on the full codebase (likely from the `cargo fmt --check` in the plan's final checklist). These are whitespace/line-break only — no logic changes.

---

## 4. Findings Summary

### ✅ Passed
1. `cargo build` — clean, no warnings
2. `cargo test` — all 21 tests pass
3. All 3 new flags documented in README.md (features, options table, KDL example, boolean note, precedence table)
4. KDL config example includes `niri-socket`, `no-window-opened`, `no-window-closed`, `no-workspace-activated`
5. README_ES.md is a complete Spanish translation covering all features, flags, config, and usage
6. Merge precedence table updated with new rows and expanded boolean column
7. No scope leakage — only documentation files were touched by this sub-task

### ⚠️ Observations (non-blocking)
- The `src/animation.rs` changes are formatting-only (likely from `cargo fmt`). They contain no logic changes and don't affect the feature.
- The 3 clippy warnings are pre-existing and unrelated to this sub-task.

---

## 5. Verdict

**Sub-Task 4 is complete and correct.** Both documentation files accurately describe the new event filter feature. All acceptance criteria are met. The sub-task can be marked as **Completed** in the plan.
