use anyhow::{Context, Result};
use clap::Parser;
use std::path::PathBuf;

/// Configuration file parsed from KDL.
///
/// Expected format (each line is a child node with a string argument):
///   animation-dir "/home/user/.config/niri/niri-animation-rotate/animations"
///   animation-target "/home/user/.config/niri/niri-animation-rotate/animation.kdl"
#[derive(knuffel::Decode, Debug, Default, Clone)]
struct KdlConfig {
    #[knuffel(child, unwrap(argument))]
    animation_dir: Option<String>,

    #[knuffel(child, unwrap(argument))]
    animation_target: Option<String>,
}

/// niri-animation-rotate — Rotates Niri window animations on compositor events.
///
/// Connects to the Niri compositor's IPC event stream and rotates between
/// animation KDL files every time a window is opened/closed or a workspace
/// is activated. Each animation file is randomly shuffled on startup so that
/// the order of animations is different every session.
///
/// The program watches the animation directory for changes in real time,
/// automatically picking up new, modified, or removed animation files.
///
/// Setup:
///   1. Place animation .kdl files in the animation directory.
///   2. Create a config file at ~/.config/niri/niri-animation-rotate/config.kdl
///      (see --help output for format).
///   3. Add to your main Niri config:
///        include "niri-animation-rotate/animation.kdl"
///   4. Run this program inside your Niri session.
#[derive(Parser, Debug, Clone)]
#[command(name = "niri-animation-rotate", version, about)]
pub struct Cli {
    /// Path to the configuration file.
    ///
    /// KDL format with optional "animation-dir" and "animation-target" properties.
    /// Example:
    ///   animation-dir "/home/user/.config/niri/niri-animation-rotate/animations"
    ///   animation-target "/home/user/.config/niri/niri-animation-rotate/animation.kdl"
    ///
    /// [default: ~/.config/niri/niri-animation-rotate/config.kdl]
    #[arg(long)]
    pub config: Option<PathBuf>,

    /// Directory containing animation .kdl files.
    ///
    /// All .kdl files in this directory will be shuffled and rotated through.
    /// Overrides the value from the config file.
    ///
    /// [default: ~/.config/niri/niri-animation-rotate/animations]
    #[arg(long)]
    pub animation_dir: Option<PathBuf>,

    /// Path to the animation output file to write.
    ///
    /// This is the file that Niri reads via `include` in its config.
    /// Overrides the value from the config file.
    ///
    /// [default: ~/.config/niri/niri-animation-rotate/animation.kdl]
    #[arg(long)]
    pub animation_target: Option<PathBuf>,
}

/// Resolved application configuration after merging CLI args, config file, and defaults.
#[derive(Debug, Clone)]
pub struct Config {
    pub animation_dir: PathBuf,
    pub animation_target: PathBuf,
}

impl Config {
    /// Build the final configuration by merging:
    ///   1. Hardcoded defaults
    ///   2. Config file values (if the file exists)
    ///   3. CLI argument overrides
    pub fn load() -> Result<Self> {
        let cli = Cli::parse();

        // Determine config file path
        let config_path = cli.config.clone().unwrap_or_else(default_config_path);

        // Try to load the config file
        let kdl_config = load_config_file(&config_path)?;

        // Merge: defaults → config file → CLI
        let animation_dir = cli
            .animation_dir
            .or_else(|| kdl_config.animation_dir.as_ref().map(PathBuf::from))
            .unwrap_or_else(default_animation_dir);

        let animation_target = cli
            .animation_target
            .or_else(|| kdl_config.animation_target.as_ref().map(PathBuf::from))
            .unwrap_or_else(default_animation_target);

        Ok(Config {
            animation_dir,
            animation_target,
        })
    }
}

fn load_config_file(path: &PathBuf) -> Result<KdlConfig> {
    if !path.exists() {
        tracing::debug!(
            config_path = %path.display(),
            "Config file not found, using defaults"
        );
        return Ok(KdlConfig::default());
    }

    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read config file: {}", path.display()))?;

    let kdl: KdlConfig = knuffel::parse(path.to_str().unwrap_or("config"), &content)
        .context("Failed to parse config file (expected KDL)")?;

    eprintln!("Loaded config file: {}", path.display());
    Ok(kdl)
}

fn default_config_path() -> PathBuf {
    dirs::config_dir()
        .expect("Could not determine config directory")
        .join("niri")
        .join("niri-animation-rotate")
        .join("config.kdl")
}

fn default_animation_dir() -> PathBuf {
    dirs::config_dir()
        .expect("Could not determine config directory")
        .join("niri")
        .join("niri-animation-rotate")
        .join("animations")
}

fn default_animation_target() -> PathBuf {
    dirs::config_dir()
        .expect("Could not determine config directory")
        .join("niri")
        .join("niri-animation-rotate")
        .join("animation.kdl")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults_are_under_niri_config() {
        let dir = default_animation_dir();
        assert!(
            dir.to_string_lossy()
                .contains("niri/niri-animation-rotate/animations")
        );

        let target = default_animation_target();
        assert!(
            target
                .to_string_lossy()
                .contains("niri-animation-rotate/animations")
        );
    }

    #[test]
    fn test_config_file_not_found_returns_defaults() {
        let result = load_config_file(&PathBuf::from("/nonexistent/path/config.kdl")).unwrap();
        assert!(result.animation_dir.is_none());
        assert!(result.animation_target.is_none());
    }
}
