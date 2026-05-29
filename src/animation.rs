use anyhow::{Context, Result};
use rand::seq::SliceRandom;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// Manages the set of animation files and rotates through them.
pub struct AnimationRotator {
    files: Vec<PathBuf>,
    current_index: usize,
    animation_dir: PathBuf,
    animation_target: PathBuf,
}

impl AnimationRotator {
    /// Create a new rotator by scanning the animation directory.
    ///
    /// Files are shuffled randomly so the rotation order differs each session.
    /// Returns an error if no `.kdl` files are found.
    pub fn new(animation_dir: PathBuf, animation_target: PathBuf) -> Result<Self> {
        let files = Self::scan_directory(&animation_dir)?;

        if files.is_empty() {
            anyhow::bail!(
                "No .kdl files found in {}",
                animation_dir.display()
            );
        }

        let mut rotator = AnimationRotator {
            files,
            current_index: 0,
            animation_dir,
            animation_target,
        };

        rotator.shuffle();

        info!(
            count = rotator.files.len(),
            animation_dir = %rotator.animation_dir.display(),
            animation_target = %rotator.animation_target.display(),
            "Animation rotator initialized"
        );

        Ok(rotator)
    }

    /// Create a rotator with an empty file list (used when the directory is empty at startup
    /// and we expect the watcher to populate it later).
    pub fn empty(animation_dir: PathBuf, animation_target: PathBuf) -> Self {
        info!(
            animation_target = %animation_target.display(),
            "Animation rotator initialized with empty file list (waiting for files)"
        );
        AnimationRotator {
            files: Vec::new(),
            current_index: 0,
            animation_dir,
            animation_target,
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
        let path = self.files[self.current_index].clone();
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

        let path = &self.files[self.current_index];
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

    /// Rescan the animation directory and rebuild the shuffled file list.
    ///
    /// If the previously active file still exists, it remains at the current index.
    /// Otherwise, the list starts fresh from index 0.
    pub async fn refresh(&mut self) {
        let old_active = if self.files.is_empty() {
            None
        } else {
            Some(self.files[self.current_index].clone())
        };

        match Self::scan_directory(&self.animation_dir) {
            Ok(new_files) if new_files.is_empty() => {
                warn!(
                    animation_dir = %self.animation_dir.display(),
                    "No .kdl files found after refresh"
                );
                self.files.clear();
                self.current_index = 0;
            }
            Ok(new_files) => {
                let mut rotator = AnimationRotator {
                    files: new_files,
                    current_index: 0,
                    animation_dir: self.animation_dir.clone(),
                    animation_target: self.animation_target.clone(),
                };
                rotator.shuffle();

                // Try to preserve the previously active animation
                if let Some(ref old_path) = old_active {
                    if let Some(pos) = rotator.files.iter().position(|p| p == old_path) {
                        rotator.current_index = pos;
                        debug!(
                            file = %old_path.display(),
                            "Preserved current animation after refresh"
                        );
                    }
                }

                info!(
                    count = rotator.files.len(),
                    "Refreshed animation file list"
                );

                *self = rotator;
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

    /// Shuffle the file list randomly.
    fn shuffle(&mut self) {
        self.files.shuffle(&mut rand::thread_rng());
        self.current_index = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Create a unique temp directory for each test to avoid interference.
    fn setup_test_dir(test_name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("niri-animation-rotate-test-{}-{}", test_name, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn create_test_file(dir: &Path, name: &str, content: &str) {
        fs::write(dir.join(name), content).unwrap();
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

    #[test]
    fn test_new_succeeds_with_valid_dir() {
        let dir = setup_test_dir("new-valid");
        create_test_file(&dir, "a.kdl", "content a");
        create_test_file(&dir, "b.kdl", "content b");
        create_test_file(&dir, "c.kdl", "content c");

        let target = dir.join("animation.kdl");
        let rotator = AnimationRotator::new(dir.clone(), target).unwrap();
        assert_eq!(rotator.file_count(), 3);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_new_fails_with_empty_dir() {
        let dir = setup_test_dir("new-empty");
        let target = dir.join("animation.kdl");
        let result = AnimationRotator::new(dir.clone(), target);
        assert!(result.is_err(), "Should fail with empty directory");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_empty_rotator_is_empty() {
        let dir = setup_test_dir("empty");
        let target = dir.join("animation.kdl");
        let rotator = AnimationRotator::empty(dir.clone(), target);
        assert!(rotator.is_empty());
        assert_eq!(rotator.file_count(), 0);

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_rotate_advances_index() {
        let dir = setup_test_dir("rotate-advance");
        create_test_file(&dir, "a.kdl", "content a");
        create_test_file(&dir, "b.kdl", "content b");
        create_test_file(&dir, "c.kdl", "content c");

        let target = dir.join("animation.kdl");
        let mut rotator = AnimationRotator::new(dir.clone(), target.clone()).unwrap();

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
        let mut rotator = AnimationRotator::new(dir.clone(), target.clone()).unwrap();

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
    async fn test_refresh_picks_up_new_file() {
        let dir = setup_test_dir("refresh-add");
        create_test_file(&dir, "a.kdl", "content a");

        let target = dir.join("animation.kdl");
        let mut rotator = AnimationRotator::new(dir.clone(), target).unwrap();
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
        let mut rotator = AnimationRotator::new(dir.clone(), target).unwrap();
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
        let mut rotator = AnimationRotator::new(dir.clone(), target.clone()).unwrap();

        let _ = rotator.rotate().await.unwrap();

        let current_file = rotator.files[rotator.current_index].clone();
        fs::remove_file(&current_file).unwrap();

        let result = rotator.rotate().await;
        assert!(result.is_ok());
        assert!(!rotator.is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_scan_directory_fails_on_nonexistent() {
        let dir = PathBuf::from("/nonexistent/path");
        let result = AnimationRotator::scan_directory(&dir);
        assert!(result.is_err());
    }
}
