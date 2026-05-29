use anyhow::{Context, Result};
use clap::Parser;
use std::path::PathBuf;

/// Operation mode for the animation rotator.
#[derive(Debug, Clone, Copy, PartialEq, clap::ValueEnum)]
pub enum Mode {
    /// Listen to Niri compositor events and rotate automatically (default).
    Auto,
    /// Only rotate when a "rotate" command is received on the control socket.
    Manual,
}

/// Configuration file parsed from KDL.
///
/// All options available as CLI flags can also be set in the config file.
/// Expected format (each line is a child node with an argument):
///
/// ```kdl
/// animation-dir "/home/user/.../animations"
/// animation-target "/home/user/.../animation.kdl"
/// log-socket true
/// no-reload true
/// cooldown-ms 2000
/// mode "manual"
/// control-socket "/home/user/.../control.sock"
/// ```
#[derive(knuffel::Decode, Debug, Default, Clone)]
struct KdlConfig {
    #[knuffel(child, unwrap(argument))]
    animation_dir: Option<String>,

    #[knuffel(child, unwrap(argument))]
    animation_target: Option<String>,

    #[knuffel(child, unwrap(argument))]
    log_socket: Option<bool>,

    #[knuffel(child, unwrap(argument))]
    no_reload: Option<bool>,

    #[knuffel(child, unwrap(argument))]
    cooldown_ms: Option<u64>,

    #[knuffel(child, unwrap(argument))]
    mode: Option<String>,

    #[knuffel(child, unwrap(argument))]
    control_socket: Option<String>,
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
    /// KDL format. All options available as CLI flags can also be set here.
    /// Example:
    ///
    /// ```kdl
    /// animation-dir "/home/user/.config/niri/niri-animation-rotate/animations"
    /// animation-target "/home/user/.config/niri/niri-animation-rotate/animation.kdl"
    /// log-socket true
    /// no-reload true
    /// cooldown-ms 2000
    /// mode "manual"
    /// control-socket "/home/user/.config/niri/niri-animation-rotate/control.sock"
    /// ```
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

    /// Log all raw messages received from the Niri socket to stderr.
    ///
    /// Useful for debugging event reception issues.
    #[arg(long)]
    pub log_socket: bool,

    /// Skip the `niri msg action reload` call after rotating.
    ///
    /// Use this if your shell/compositor setup handles config reload automatically
    /// (e.g., NixOS nh, NixOS noctalia, or similar auto-reloading environments).
    #[arg(long)]
    pub no_reload: bool,

    /// Minimum time in milliseconds to wait before allowing another rotation.
    ///
    /// Prevents animation swaps while a previous animation is still playing.
    /// Set to 0 (default) for no cooldown.
    #[arg(long)]
    pub cooldown_ms: Option<u64>,

    /// Operation mode: auto (listen to Niri events) or manual (control socket).
    ///
    /// In manual mode, the daemon listens on a Unix socket for "rotate" commands
    /// instead of reacting to Niri compositor events. Use together with a Niri
    /// keybind that sends "rotate" to the control socket.
    #[arg(long, value_enum)]
    pub mode: Option<Mode>,

    /// Path to the control socket (used in manual mode).
    ///
    /// [default: ~/.config/niri/niri-animation-rotate/control.sock]
    #[arg(long)]
    pub control_socket: Option<PathBuf>,
}

/// Resolved application configuration after merging CLI args, config file, and defaults.
#[derive(Debug, Clone)]
pub struct Config {
    pub animation_dir: PathBuf,
    pub animation_target: PathBuf,
    pub log_socket: bool,
    pub no_reload: bool,
    pub cooldown_ms: u64,
    pub mode: Mode,
    pub control_socket: PathBuf,
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

        let log_socket = cli.log_socket || kdl_config.log_socket.unwrap_or(false);

        let no_reload = cli.no_reload || kdl_config.no_reload.unwrap_or(false);

        let cooldown_ms = cli
            .cooldown_ms
            .or(kdl_config.cooldown_ms)
            .unwrap_or(0);

        let mode = cli
            .mode
            .or_else(|| parse_mode_from_kdl(kdl_config.mode.as_deref()))
            .unwrap_or(Mode::Auto);

        let control_socket = cli
            .control_socket
            .or_else(|| kdl_config.control_socket.as_ref().map(PathBuf::from))
            .unwrap_or_else(default_control_socket);

        Ok(Config {
            animation_dir,
            animation_target,
            log_socket,
            no_reload,
            cooldown_ms,
            mode,
            control_socket,
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

fn default_control_socket() -> PathBuf {
    dirs::config_dir()
        .expect("Could not determine config directory")
        .join("niri")
        .join("niri-animation-rotate")
        .join("control.sock")
}

/// Parse the mode string from a KDL config file into a `Mode` variant.
/// Returns `None` if the value is invalid (a warning is logged).
fn parse_mode_from_kdl(value: Option<&str>) -> Option<Mode> {
    match value? {
        "auto" | "Auto" => Some(Mode::Auto),
        "manual" | "Manual" => Some(Mode::Manual),
        s => {
            tracing::warn!(mode = %s, "Invalid mode value in config file, using default");
            None
        }
    }
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
                .contains("niri/niri-animation-rotate/animation")
        );
    }

    #[test]
    fn test_config_file_not_found_returns_defaults() {
        let result = load_config_file(&PathBuf::from("/nonexistent/path/config.kdl")).unwrap();
        assert!(result.animation_dir.is_none());
        assert!(result.animation_target.is_none());
    }
}
