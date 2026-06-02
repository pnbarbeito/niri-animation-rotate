# Code Review Summary

**Scope**: ST2 — backward rotation (`rotate_prev`) and dual commands (`next`, `prev`, `rotate`) in manual mode control socket.  
**Files reviewed**: `src/animation.rs` (lines 88–148, 441–527), `src/main.rs` (lines 234–332)  
**Overall risk**: Low  
**Verdict**: Approve with comments

---

## Findings

### [P2] Medium — Lock handling asymmetry between forward and backward rotation paths

- **Location**: `src/main.rs:74-83` vs `src/main.rs:300-308`
- **Why it matters**: The forward path delegates to `rotate_and_reload()`, which holds the `tokio::sync::Mutex` lock across **both** `anim.rotate().await` and `niri::reload_niri().await`. The backward path inlines the logic and **drops the lock** (`drop(anim)`) before calling `reload_niri()`. This creates a structural inconsistency and a maintenance trap: any future change to lock semantics (e.g., changing the lock type, adding a held-state invariant) must be verified in two different code paths that have different lock scoping.
- **Evidence**:
  - Forward (lines 74–82): the lock is held through the entire function body, including the reload call.
  - Backward (lines 300–308): the lock is acquired, `rotate_prev()` is called, then `drop(anim)` is called explicitly before reload — but only on the `else if !no_reload` path. If `rotate_prev()` fails *or* `no_reload` is true, the lock is dropped implicitly at end-of-scope instead.
- **Fix**: Extract a `rotate_prev_and_reload()` helper (mirroring `rotate_and_reload()`) and call it from the `"prev"` branch:

  ```rust
  async fn rotate_prev_and_reload(animator: &Arc<Mutex<AnimationRotator>>, no_reload: bool) {
      let mut anim = animator.lock().await;
      if let Err(e) = anim.rotate_prev().await {
          tracing::warn!(error = %e, "Failed to rotate animation backward");
      } else if !no_reload {
          if let Err(e) = niri::reload_niri().await {
              tracing::warn!(error = %e, "Failed to reload Niri config");
          }
      }
  }
  ```

  Then replace the inline `"prev"` branch body with `rotate_prev_and_reload(&animator, config.no_reload).await;`. This eliminates the asymmetry and makes the two arms structurally identical, differing only in the direction function called.

### [P3] Low — Duplicated cooldown check across both arms

- **Location**: `src/main.rs:268-278` and `src/main.rs:286-297`
- **Why it matters**: Seven lines of identical cooldown logic are copy-pasted between the two rotation branches. A future change to the cooldown policy (e.g., exponential backoff on failure, per-direction cooldown, configurable max rate) must be applied in two places, creating a realistic risk of drift.
- **Fix**: Hoist the cooldown check before the `match` statement. Since the `_` (unknown command) branch also ends with `continue`, hoisting doesn't affect its behavior:

  ```rust
  // Cooldown check: applies to all rotation commands
  let command = buf.trim();
  if matches!(command, "next" | "rotate" | "prev") {
      if let Some(t) = last_rotation {
          let elapsed = t.elapsed().as_millis() as u64;
          if elapsed < config.cooldown_ms {
              tracing::debug!(
                  elapsed_ms = elapsed,
                  cooldown_ms = config.cooldown_ms,
                  "Rotation skipped: cooldown active"
              );
              continue;
          }
      }
  }
  match command {
      "next" | "rotate" => { ... }
      "prev" => { ... }
      _ => { ... }
  }
  ```

  Alternatively, extract a small helper:

  ```rust
  fn cooldown_active(last_rotation: Option<Instant>, cooldown_ms: u64) -> bool {
      last_rotation.is_some_and(|t| t.elapsed().as_millis() as u64 < cooldown_ms)
  }
  ```

### [P3] Low — Minor test coverage gaps for `rotate_prev`

- **Location**: `src/animation.rs:440-527`
- **Why it matters**: Three tests were added (backward advance, wrap from zero, deleted file), which cover the main paths. However, two low-risk edge cases lack coverage:
  1. **Single-file wrap**: `rotate_prev` on a rotator with exactly one file should stay at index 0 (identity). This path exercises `self.files.len() - 1` when `len() == 1`, which is `0` — a trivial but untested boundary.
  2. **Empty rotator**: `rotate_prev` on a rotator built via `AnimationRotator::empty()` should be a no-op (returns `Ok(())` immediately). The empty path (line 121–124) has no dedicated test. The forward `rotate()` has the same gap, so this is inherited.
- **Fix**: Add two tests (low priority, can defer):

  ```rust
  #[tokio::test]
  async fn test_rotate_prev_single_file_stays_at_zero() {
      let dir = setup_test_dir("prev-single");
      create_test_file(&dir, "only.kdl", "content");
      let target = dir.join("animation.kdl");
      let mut rot = AnimationRotator::new(dir.clone(), target, 500, false).unwrap();
      assert_eq!(rot.current_index, 0);
      rot.rotate_prev().await.unwrap();
      assert_eq!(rot.current_index, 0);
      let _ = fs::remove_dir_all(&dir);
  }

  #[tokio::test]
  async fn test_rotate_prev_empty_rotator_noop() {
      let dir = setup_test_dir("prev-empty");
      let target = dir.join("animation.kdl");
      let mut rot = AnimationRotator::empty(dir.clone(), target, 500, false);
      assert!(rot.is_empty());
      let result = rot.rotate_prev().await;
      assert!(result.is_ok());
      assert!(rot.is_empty());
      let _ = fs::remove_dir_all(&dir);
  }
  ```

---

## Items Checked (with no issues found)

| Check | Result |
|---|---|
| **Correctness**: `rotate_prev` wrap-around logic (`if current == 0 { len - 1 } else { current - 1 }`) | Correct — handles 1-file identity, 2+ file wrap. |
| **Correctness**: Deleted-file handling in `rotate_prev` (refresh + reset to index 0) | Correct — mirrors `rotate()` exactly. |
| **Correctness**: `"rotate"` alias still works as forward | Confirmed: `"next" \| "rotate"` in the match pattern. |
| **Correctness**: Cooldown is respected for both forward and backward | Both arms check `last_rotation` against `config.cooldown_ms`. |
| **Correctness**: `last_rotation` is updated after both successful and failed rotations | Both arms update the timestamp unconditionally. |
| **Correctness**: Atomic write path | `rotate_prev` delegates to `apply_current()`, same as `rotate()`. |
| **Correctness**: `refresh()` index preservation when the target file is deleted | After `refresh()`, the deleted file won't be found, so index resets to 0 — consistent with forward behavior. |
| **Security**: New control socket commands | No additional exposure — unknown commands are still ignored, cooldown limits rate. |
| **Robustness**: Race condition between `refresh()` and `apply_current()` | Same TOCTOU window as `rotate()` — acceptable for this use case. |
| **Logging**: Updated message from "waiting for 'rotate' commands" to "waiting for commands" | Done at `src/main.rs:245`. |
| **Style consistency**: `rotate_prev` mirrors `rotate` structure | Structurally identical (empty check, index update, deleted check, apply). |

---

## Suggested Next Steps

- [ ] Consider adding `rotate_prev_and_reload()` helper to unify lock handling (P2).
- [ ] Consider hoisting deduplicated cooldown check above the `match` (P3).
- [ ] Consider adding the two low-priority tests for single-file and empty-rotator edge cases (P3).
- [ ] No blockers — the implementation is correct and safe to merge as-is.
