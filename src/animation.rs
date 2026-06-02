use crate::duration;
use anyhow::{Context, Result};
use rand::seq::SliceRandom;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// An animation file with its parsed effective duration.
pub struct AnimationEntry {
    pub path: PathBuf,
    pub duration_ms: u64,
}

/// Manages the set of animation files and rotates through them.
pub struct AnimationRotator {
    files: Vec<AnimationEntry>,
    current_index: usize,
    animation_dir: PathBuf,
    animation_target: PathBuf,
    fallback_duration_ms: u64,
    random_order: bool,
}

impl AnimationRotator {
    /// Create a new rotator by scanning the animation directory.
    ///
    /// If `random_order` is true, files are shuffled for a different order each session.
    /// Otherwise, they remain in alphabetical order.
    /// Returns an error if no `.kdl` files are found.
    pub fn new(
        animation_dir: PathBuf,
        animation_target: PathBuf,
        fallback_duration_ms: u64,
        random_order: bool,
    ) -> Result<Self> {
        let paths = Self::scan_directory(&animation_dir)?;
        let files = Self::with_durations(paths, fallback_duration_ms);

        if files.is_empty() {
            anyhow::bail!("No .kdl files found in {}", animation_dir.display());
        }

        let mut rotator = AnimationRotator {
            files,
            current_index: 0,
            animation_dir,
            animation_target,
            fallback_duration_ms,
            random_order,
        };

        if rotator.random_order {
            rotator.shuffle();
        }

        rotator.try_resume_from_target();

        info!(
            count = rotator.files.len(),
            animation_dir = %rotator.animation_dir.display(),
            animation_target = %rotator.animation_target.display(),
            random_order = rotator.random_order,
            "Animation rotator initialized"
        );

        Ok(rotator)
    }

    /// Create a rotator with an empty file list (used when the directory is empty at startup
    /// and we expect the watcher to populate it later).
    pub fn empty(
        animation_dir: PathBuf,
        animation_target: PathBuf,
        fallback_duration_ms: u64,
        random_order: bool,
    ) -> Self {
        info!(
            animation_target = %animation_target.display(),
            "Animation rotator initialized with empty file list (waiting for files)"
        );
        AnimationRotator {
            files: Vec::new(),
            current_index: 0,
            animation_dir,
            animation_target,
            fallback_duration_ms,
            random_order,
        }
    }

    /// Rotate to the next animation file and write it to the target.
    ///
    /// If the file list is empty, this is a no-op.
    pub async fn rotate(&mut self) -> Result<()> {
        if self.files.is_empty() {
            debug!("No animation files to rotate, skipping");
            return Ok(());
        }

        self.current_index = (self.current_index + 1) % self.files.len();

        // The file at current_index might have been deleted since last refresh.
        let path = self.files[self.current_index].path.clone();
        if !path.exists() {
            warn!(
                file = %path.display(),
                "Current animation file no longer exists, refreshing"
            );
            self.refresh().await;
            if self.files.is_empty() {
                debug!("No animation files remain after refresh");
                return Ok(());
            }
            self.current_index = 0;
        }

        self.apply_current().await
    }

    /// Rotate to the previous animation file and write it to the target.
    ///
    /// If the file list is empty, this is a no-op.
    pub async fn rotate_prev(&mut self) -> Result<()> {
        if self.files.is_empty() {
            debug!("No animation files to rotate, skipping");
            return Ok(());
        }

        self.current_index = if self.current_index == 0 {
            self.files.len() - 1
        } else {
            self.current_index - 1
        };

        // The file at current_index might have been deleted since last refresh.
        let path = self.files[self.current_index].path.clone();
        if !path.exists() {
            warn!(
                file = %path.display(),
                "Current animation file no longer exists, refreshing"
            );
            self.refresh().await;
            if self.files.is_empty() {
                debug!("No animation files remain after refresh");
                return Ok(());
            }
            self.current_index = 0;
        }

        self.apply_current().await
    }

    /// Write the current animation file's content to the target file.
    pub async fn apply_current(&self) -> Result<()> {
        if self.files.is_empty() {
            return Ok(());
        }

        let path = &self.files[self.current_index].path;
        let content = tokio::fs::read(path)
            .await
            .with_context(|| format!("Failed to read animation file: {}", path.display()))?;

        // Atomic write: write to temp file, then rename
        let tmp = self.animation_target.with_extension("tmp");
        tokio::fs::write(&tmp, &content)
            .await
            .with_context(|| format!("Failed to write temp file: {}", tmp.display()))?;

        tokio::fs::rename(&tmp, &self.animation_target)
            .await
            .with_context(|| {
                format!(
                    "Failed to rename temp file to target: {}",
                    self.animation_target.display()
                )
            })?;

        info!(
            file = %path.display(),
            index = self.current_index,
            total = self.files.len(),
            "Applied animation"
        );

        Ok(())
    }

    /// Get the duration of the currently active animation in milliseconds.
    ///
    /// Returns the fallback duration if the file list is empty.
    pub fn current_duration_ms(&self) -> u64 {
        if self.files.is_empty() {
            self.fallback_duration_ms
        } else {
            self.files[self.current_index].duration_ms
        }
    }

    /// Return the filename stem (without extension) of the currently active animation.
    ///
    /// Returns `None` if the file list is empty.
    pub fn current_file_stem(&self) -> Option<String> {
        if self.files.is_empty() {
            None
        } else {
            self.files[self.current_index]
                .path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
        }
    }

    /// Return all animation filename stems (without extensions).
    ///
    /// The order matches the current internal ordering (alphabetical or shuffled).
    pub fn file_stems(&self) -> Vec<String> {
        self.files
            .iter()
            .filter_map(|e| {
                e.path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string())
            })
            .collect()
    }

    /// Select an animation by filename stem (case-insensitive).
    ///
    /// Writes the selected animation to the target file if found.
    /// Returns `Ok(true)` if the animation was found and applied,
    /// `Ok(false)` if no animation matches the given name.
    pub async fn select_by_name(&mut self, name: &str) -> Result<bool> {
        let found = self.files.iter().position(|e| {
            e.path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.eq_ignore_ascii_case(name))
                .unwrap_or(false)
        });

        match found {
            Some(pos) => {
                self.current_index = pos;

                // The file might have been deleted since last refresh.
                let path = self.files[self.current_index].path.clone();
                if !path.exists() {
                    warn!(
                        file = %path.display(),
                        "Selected animation file no longer exists, refreshing"
                    );
                    self.refresh().await;
                    if self.files.is_empty() {
                        debug!("No animation files remain after refresh");
                        return Ok(true);
                    }
                    // Refresh resets to index 0; reselect by name after refresh
                    if let Some(new_pos) = self.files.iter().position(|e| {
                        e.path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .map(|s| s.eq_ignore_ascii_case(name))
                            .unwrap_or(false)
                    }) {
                        self.current_index = new_pos;
                    } else {
                        return Ok(false);
                    }
                }

                self.apply_current().await?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Rescan the animation directory and rebuild the shuffled file list.
    ///
    /// If the previously active file still exists, it remains at the current index.
    /// Otherwise, the list starts fresh from index 0.
    pub async fn refresh(&mut self) {
        let old_active = if self.files.is_empty() {
            None
        } else {
            Some(self.files[self.current_index].path.clone())
        };

        match Self::scan_directory(&self.animation_dir) {
            Ok(paths) if paths.is_empty() => {
                warn!(
                    animation_dir = %self.animation_dir.display(),
                    "No .kdl files found after refresh"
                );
                self.files.clear();
                self.current_index = 0;
            }
            Ok(paths) => {
                let file_count = paths.len();
                let new_files = Self::with_durations(paths, self.fallback_duration_ms);
                debug!(
                    count = file_count,
                    "Scanned animation directory, parsing durations"
                );
                let mut rotator = AnimationRotator {
                    files: new_files,
                    current_index: 0,
                    animation_dir: self.animation_dir.clone(),
                    animation_target: self.animation_target.clone(),
                    fallback_duration_ms: self.fallback_duration_ms,
                    random_order: self.random_order,
                };
                if rotator.random_order {
                    rotator.shuffle();
                }

                // Try to preserve the previously active animation
                if let Some(ref old_path) = old_active {
                    if let Some(pos) = rotator.files.iter().position(|e| e.path == *old_path) {
                        rotator.current_index = pos;
                        debug!(
                            file = %old_path.display(),
                            "Preserved current animation after refresh"
                        );
                    }
                }

                info!(count = rotator.files.len(), "Refreshed animation file list");

                *self = rotator;
                self.try_resume_from_target();
            }
            Err(e) => {
                warn!(
                    animation_dir = %self.animation_dir.display(),
                    error = %e,
                    "Failed to scan animation directory"
                );
            }
        }
    }

    /// Check if the rotator has any animation files loaded.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Get the number of animation files loaded.
    #[allow(dead_code)]
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Get a reference to the animation directory.
    #[allow(dead_code)]
    pub fn animation_dir(&self) -> &Path {
        &self.animation_dir
    }

    // --- Private helpers ---

    /// Scan a directory for .kdl files, sorted by filename for deterministic ordering
    /// before shuffling.
    fn scan_directory(dir: &Path) -> Result<Vec<PathBuf>> {
        let entries = std::fs::read_dir(dir)
            .with_context(|| format!("Failed to read animation directory: {}", dir.display()))?;

        let mut files: Vec<PathBuf> = entries
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let path = entry.path();
                if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("kdl") {
                    Some(path)
                } else {
                    None
                }
            })
            .collect();

        files.sort();
        Ok(files)
    }

    /// Convert a list of file paths to `AnimationEntry` objects by parsing each
    /// file's effective animation duration. Falls back to `fallback_duration_ms`
    /// for files that have no `duration-ms`.
    fn with_durations(paths: Vec<PathBuf>, fallback_duration_ms: u64) -> Vec<AnimationEntry> {
        paths
            .into_iter()
            .map(|path| {
                let duration_ms =
                    duration::parse_animation_duration(&path).unwrap_or(fallback_duration_ms);
                AnimationEntry { path, duration_ms }
            })
            .collect()
    }

    /// Shuffle the file list randomly.
    fn shuffle(&mut self) {
        self.files.shuffle(&mut rand::thread_rng());
        self.current_index = 0;
    }

    /// Try to resume from a previously-active animation by comparing
    /// the content of `animation_target` against all known files.
    ///
    /// If the target file's content matches one of our animation files,
    /// sets `current_index` to that file's position.
    fn try_resume_from_target(&mut self) {
        let target_content = match std::fs::read(&self.animation_target) {
            Ok(c) => c,
            Err(_) => return,
        };

        for (i, entry) in self.files.iter().enumerate() {
            if let Ok(file_content) = std::fs::read(&entry.path)
                && file_content == target_content
            {
                self.current_index = i;
                debug!(
                    file = %entry.path.display(),
                    index = i,
                    "Resumed from target: matched active animation"
                );
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Create a unique temp directory for each test to avoid interference.
    fn setup_test_dir(test_name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "niri-animation-rotate-test-{}-{}",
            test_name,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn create_test_file(dir: &Path, name: &str, content: &str) {
        fs::write(dir.join(name), content).unwrap();
    }

    #[test]
    fn test_new_succeeds_with_valid_dir() {
        let dir = setup_test_dir("new-valid");
        create_test_file(&dir, "a.kdl", "content a");
        create_test_file(&dir, "b.kdl", "content b");
        create_test_file(&dir, "c.kdl", "content c");

        let target = dir.join("animation.kdl");
        let rotator = AnimationRotator::new(dir.clone(), target, 500, false).unwrap();
        assert_eq!(rotator.file_count(), 3);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_new_fails_with_empty_dir() {
        let dir = setup_test_dir("new-empty");
        let target = dir.join("animation.kdl");
        let result = AnimationRotator::new(dir.clone(), target, 500, false);
        assert!(result.is_err(), "Should fail with empty directory");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_empty_rotator_is_empty() {
        let dir = setup_test_dir("empty");
        let target = dir.join("animation.kdl");
        let rotator = AnimationRotator::empty(dir.clone(), target, 500, false);
        assert!(rotator.is_empty());
        assert_eq!(rotator.file_count(), 0);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_scan_directory_finds_kdl_files() {
        let dir = setup_test_dir("scan");
        create_test_file(&dir, "animation1.kdl", "// animation 1");
        create_test_file(&dir, "animation2.kdl", "// animation 2");
        create_test_file(&dir, "readme.txt", "not a kdl file");

        let files = AnimationRotator::scan_directory(&dir).unwrap();
        assert_eq!(files.len(), 2);
        assert!(files.iter().all(|f| f.extension().unwrap() == "kdl"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_rotate_advances_index() {
        let dir = setup_test_dir("rotate-advance");
        create_test_file(&dir, "a.kdl", "content a");
        create_test_file(&dir, "b.kdl", "content b");
        create_test_file(&dir, "c.kdl", "content c");

        let target = dir.join("animation.kdl");
        let mut rotator = AnimationRotator::new(dir.clone(), target.clone(), 500, false).unwrap();

        let initial_index = rotator.current_index;

        rotator.rotate().await.unwrap();

        let new_index = rotator.current_index;
        assert_ne!(
            initial_index, new_index,
            "Index should have changed after rotation"
        );

        assert!(target.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_rotate_wraps_around() {
        let dir = setup_test_dir("rotate-wrap");
        create_test_file(&dir, "a.kdl", "content a");
        create_test_file(&dir, "b.kdl", "content b");

        let target = dir.join("animation.kdl");
        let mut rotator = AnimationRotator::new(dir.clone(), target.clone(), 500, false).unwrap();

        let mut seen_indices = Vec::new();
        for _ in 0..rotator.file_count() {
            seen_indices.push(rotator.current_index);
            rotator.rotate().await.unwrap();
        }
        seen_indices.push(rotator.current_index);

        let unique: std::collections::HashSet<_> = seen_indices.iter().collect();
        assert_eq!(unique.len(), 2);

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_rotate_prev_advances_backward() {
        let dir = setup_test_dir("rotate-prev");
        create_test_file(&dir, "a.kdl", "content a");
        create_test_file(&dir, "b.kdl", "content b");
        create_test_file(&dir, "c.kdl", "content c");

        let target = dir.join("animation.kdl");
        let mut rotator = AnimationRotator::new(dir.clone(), target.clone(), 500, false).unwrap();

        // With random_order=false, alphabetical: a=0, b=1, c=2. Start at 0.
        let stem = |r: &AnimationRotator| {
            r.files[r.current_index]
                .path
                .file_stem()
                .unwrap()
                .to_str()
                .unwrap()
                .to_string()
        };
        assert_eq!(stem(&rotator), "a");

        rotator.rotate_prev().await.unwrap();
        // prev from 0 → wraps to 2 (c)
        assert_eq!(stem(&rotator), "c");

        rotator.rotate_prev().await.unwrap();
        // prev from 2 → 1 (b)
        assert_eq!(stem(&rotator), "b");

        assert!(target.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_rotate_prev_wraps_from_zero() {
        let dir = setup_test_dir("rotate-prev-wrap");
        create_test_file(&dir, "a.kdl", "content a");
        create_test_file(&dir, "b.kdl", "content b");

        let target = dir.join("animation.kdl");
        let mut rotator = AnimationRotator::new(dir.clone(), target.clone(), 500, false).unwrap();

        let stem = |r: &AnimationRotator| {
            r.files[r.current_index]
                .path
                .file_stem()
                .unwrap()
                .to_str()
                .unwrap()
                .to_string()
        };

        // Start at index 0 (a)
        assert_eq!(stem(&rotator), "a");

        // prev from 0 should wrap to len-1 (b)
        rotator.rotate_prev().await.unwrap();
        assert_eq!(stem(&rotator), "b");

        // prev from len-1 should go to len-2 (a)
        rotator.rotate_prev().await.unwrap();
        assert_eq!(stem(&rotator), "a");

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_rotate_prev_handles_deleted_file() {
        let dir = setup_test_dir("rotate-prev-deleted");
        create_test_file(&dir, "a.kdl", "content a");
        create_test_file(&dir, "b.kdl", "content b");

        let target = dir.join("animation.kdl");
        let mut rotator = AnimationRotator::new(dir.clone(), target.clone(), 500, false).unwrap();

        // Move to index 1 (b), then delete b
        rotator.rotate().await.unwrap();
        let current_file = rotator.files[rotator.current_index].path.clone();
        fs::remove_file(&current_file).unwrap();

        // rotate_prev should handle the deleted file gracefully
        let result = rotator.rotate_prev().await;
        assert!(result.is_ok());
        assert!(!rotator.is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_rotate_prev_single_file_noop() {
        let dir = setup_test_dir("rotate-prev-single");
        create_test_file(&dir, "a.kdl", "content a");

        let target = dir.join("animation.kdl");
        let mut rotator = AnimationRotator::new(dir.clone(), target.clone(), 500, false).unwrap();

        let before = rotator.current_index;
        rotator.rotate_prev().await.unwrap();
        // Single file: prev should wrap to itself
        assert_eq!(rotator.current_index, before);
        assert_eq!(rotator.file_count(), 1);

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_rotate_prev_empty_rotator_noop() {
        let dir = setup_test_dir("rotate-prev-empty");
        let target = dir.join("animation.kdl");
        let mut rotator = AnimationRotator::empty(dir.clone(), target, 500, false);

        // Should not panic, just be a no-op
        let result = rotator.rotate_prev().await;
        assert!(result.is_ok());
        assert!(rotator.is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_refresh_picks_up_new_file() {
        let dir = setup_test_dir("refresh-add");
        create_test_file(&dir, "a.kdl", "content a");

        let target = dir.join("animation.kdl");
        let mut rotator = AnimationRotator::new(dir.clone(), target, 500, false).unwrap();
        assert_eq!(rotator.file_count(), 1);

        create_test_file(&dir, "b.kdl", "content b");

        rotator.refresh().await;
        assert_eq!(rotator.file_count(), 2);

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_refresh_removes_deleted_file() {
        let dir = setup_test_dir("refresh-remove");
        create_test_file(&dir, "a.kdl", "content a");
        create_test_file(&dir, "b.kdl", "content b");

        let target = dir.join("animation.kdl");
        let mut rotator = AnimationRotator::new(dir.clone(), target, 500, false).unwrap();
        assert_eq!(rotator.file_count(), 2);

        fs::remove_file(dir.join("b.kdl")).unwrap();

        rotator.refresh().await;
        assert_eq!(rotator.file_count(), 1);

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_rotate_handles_deleted_file() {
        let dir = setup_test_dir("rotate-deleted");
        create_test_file(&dir, "a.kdl", "content a");
        create_test_file(&dir, "b.kdl", "content b");

        let target = dir.join("animation.kdl");
        let mut rotator = AnimationRotator::new(dir.clone(), target.clone(), 500, false).unwrap();

        let _ = rotator.rotate().await.unwrap();

        let current_file = rotator.files[rotator.current_index].path.clone();
        fs::remove_file(&current_file).unwrap();

        let result = rotator.rotate().await;
        assert!(result.is_ok());
        assert!(!rotator.is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_current_duration_uses_fallback() {
        let dir = setup_test_dir("dur-fallback");
        create_test_file(&dir, "a.kdl", "// no duration-ms here");

        let target = dir.join("animation.kdl");
        let rotator = AnimationRotator::new(dir.clone(), target, 300, false).unwrap();
        // File has no duration-ms → should use fallback
        assert_eq!(rotator.current_duration_ms(), 300);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_current_duration_parsed() {
        let dir = setup_test_dir("dur-parsed");
        create_test_file(
            &dir,
            "a.kdl",
            "animations {\n    window-open { duration-ms 400 }\n}",
        );

        let target = dir.join("animation.kdl");
        let rotator = AnimationRotator::new(dir.clone(), target, 500, false).unwrap();
        assert_eq!(rotator.current_duration_ms(), 400);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_current_duration_empty_uses_fallback() {
        let dir = setup_test_dir("dur-empty");
        let target = dir.join("animation.kdl");
        let rotator = AnimationRotator::empty(dir.clone(), target, 350, false);
        assert_eq!(rotator.current_duration_ms(), 350);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_random_order_false_preserves_alphabetical() {
        let dir = setup_test_dir("rand-off");
        create_test_file(&dir, "c.kdl", "content c");
        create_test_file(&dir, "a.kdl", "content a");
        create_test_file(&dir, "b.kdl", "content b");

        let target = dir.join("animation.kdl");
        let rotator = AnimationRotator::new(dir.clone(), target, 500, false).unwrap();

        // With random_order=false, files should stay in alphabetical order
        let stems: Vec<&str> = rotator
            .files
            .iter()
            .map(|e| e.path.file_stem().unwrap().to_str().unwrap())
            .collect();
        assert_eq!(stems, vec!["a", "b", "c"]);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_random_order_true_stores_flag() {
        let dir = setup_test_dir("rand-on");
        create_test_file(&dir, "a.kdl", "content a");
        create_test_file(&dir, "b.kdl", "content b");

        let target = dir.join("animation.kdl");
        let rotator = AnimationRotator::new(dir.clone(), target, 500, true).unwrap();

        // Verify the flag is stored and new() doesn't panic
        assert!(rotator.random_order);
        assert_eq!(rotator.file_count(), 2);

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_random_order_refresh_carries_flag() {
        let dir = setup_test_dir("rand-refresh");
        create_test_file(&dir, "a.kdl", "content a");

        let target = dir.join("animation.kdl");
        let mut rotator = AnimationRotator::new(dir.clone(), target, 500, true).unwrap();
        assert!(rotator.random_order);

        // refresh should preserve the random_order flag
        rotator.refresh().await;
        assert!(rotator.random_order);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_scan_directory_fails_on_nonexistent() {
        let dir = PathBuf::from("/nonexistent/path");
        let result = AnimationRotator::scan_directory(&dir);
        assert!(result.is_err());
    }

    #[test]
    fn test_current_file_stem_returns_active() {
        let dir = setup_test_dir("stem-active");
        create_test_file(&dir, "alpha.kdl", "content a");
        create_test_file(&dir, "beta.kdl", "content b");

        let target = dir.join("animation.kdl");
        let rotator = AnimationRotator::new(dir.clone(), target, 500, false).unwrap();
        // Without shuffle, files are alphabetical: alpha, beta → current is alpha
        assert_eq!(rotator.current_file_stem(), Some("alpha".to_string()));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_current_file_stem_empty_returns_none() {
        let dir = setup_test_dir("stem-empty");
        let target = dir.join("animation.kdl");
        let rotator = AnimationRotator::empty(dir.clone(), target, 500, false);
        assert_eq!(rotator.current_file_stem(), None);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_file_stems_returns_all() {
        let dir = setup_test_dir("stems-all");
        create_test_file(&dir, "zebra.kdl", "content z");
        create_test_file(&dir, "apple.kdl", "content a");
        create_test_file(&dir, "mango.kdl", "content m");

        let target = dir.join("animation.kdl");
        let rotator = AnimationRotator::new(dir.clone(), target, 500, false).unwrap();
        // Alphabetical order (no shuffle): apple, mango, zebra
        let stems = rotator.file_stems();
        assert_eq!(stems, vec!["apple", "mango", "zebra"]);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_file_stems_empty_returns_empty_vec() {
        let dir = setup_test_dir("stems-empty");
        let target = dir.join("animation.kdl");
        let rotator = AnimationRotator::empty(dir.clone(), target, 500, false);
        let stems = rotator.file_stems();
        assert!(stems.is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_select_by_name_found() {
        let dir = setup_test_dir("select-found");
        create_test_file(&dir, "bloom.kdl", "content bloom");
        create_test_file(&dir, "prism.kdl", "content prism");

        let target = dir.join("animation.kdl");
        let mut rotator = AnimationRotator::new(dir.clone(), target.clone(), 500, false).unwrap();

        let result = rotator.select_by_name("prism").await;
        assert!(result.is_ok());
        assert!(result.unwrap()); // true = found
        assert_eq!(rotator.current_file_stem(), Some("prism".to_string()));

        // Verify the file was actually written
        let content = std::fs::read_to_string(&target).unwrap();
        assert!(content.contains("content prism"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_select_by_name_not_found() {
        let dir = setup_test_dir("select-notfound");
        create_test_file(&dir, "alpha.kdl", "content a");

        let target = dir.join("animation.kdl");
        let mut rotator = AnimationRotator::new(dir.clone(), target, 500, false).unwrap();

        let result = rotator.select_by_name("nonexistent").await;
        assert!(result.is_ok());
        assert!(!result.unwrap()); // false = not found

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_select_by_name_case_insensitive() {
        let dir = setup_test_dir("select-case");
        create_test_file(&dir, "MyAnimation.kdl", "content myanim");

        let target = dir.join("animation.kdl");
        let mut rotator = AnimationRotator::new(dir.clone(), target.clone(), 500, false).unwrap();

        let result = rotator.select_by_name("myanimation").await;
        assert!(result.is_ok());
        assert!(result.unwrap());
        assert_eq!(rotator.current_file_stem(), Some("MyAnimation".to_string()));

        // Also test uppercase input
        let result = rotator.select_by_name("MYANIMATION").await;
        assert!(result.is_ok());
        assert!(result.unwrap());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_extensions_not_in_stems() {
        let dir = setup_test_dir("stem-ext");
        create_test_file(&dir, "cool_anim.kdl", "content");

        let target = dir.join("animation.kdl");
        let rotator = AnimationRotator::new(dir.clone(), target, 500, false).unwrap();

        // Stem should NOT include .kdl
        assert_eq!(rotator.current_file_stem(), Some("cool_anim".to_string()));
        assert_eq!(rotator.file_stems(), vec!["cool_anim"]);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_try_resume_from_target_matches_content() {
        let dir = setup_test_dir("resume-match");
        create_test_file(&dir, "alpha.kdl", "content A");
        create_test_file(&dir, "beta.kdl", "content B");
        create_test_file(&dir, "gamma.kdl", "content C");

        // Create target OUTSIDE animation dir (matches real-world layout)
        let target_dir = setup_test_dir("resume-match-target");
        let target = target_dir.join("animation.kdl");
        fs::write(&target, "content C").unwrap();

        let rotator = AnimationRotator::new(dir.clone(), target.clone(), 500, false).unwrap();
        // Without shuffle: order is alpha, beta, gamma — gamma matches, index 2
        assert_eq!(rotator.current_index, 2);
        assert_eq!(rotator.current_file_stem(), Some("gamma".to_string()));

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&target_dir);
    }

    #[test]
    fn test_try_resume_from_target_no_match() {
        let dir = setup_test_dir("resume-nomatch");
        create_test_file(&dir, "alpha.kdl", "content A");
        create_test_file(&dir, "beta.kdl", "content B");

        // Target content doesn't match any file
        let target_dir = setup_test_dir("resume-nomatch-target");
        let target = target_dir.join("animation.kdl");
        fs::write(&target, "unrelated content").unwrap();

        let rotator = AnimationRotator::new(dir.clone(), target, 500, false).unwrap();
        // No match → stays at index 0
        assert_eq!(rotator.current_index, 0);

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&target_dir);
    }

    #[test]
    fn test_try_resume_from_target_missing_file() {
        let dir = setup_test_dir("resume-missing");
        create_test_file(&dir, "alpha.kdl", "content A");

        // Target file does not exist
        let target = dir.join("nonexistent.kdl");

        let rotator = AnimationRotator::new(dir.clone(), target, 500, false).unwrap();
        // No target to read → stays at index 0
        assert_eq!(rotator.current_index, 0);

        let _ = fs::remove_dir_all(&dir);
    }
}
