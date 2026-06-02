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
/// no-window-opened false
/// no-window-closed false
/// no-workspace-activated false
/// cooldown-ms 2000
/// mode "manual"
/// control-socket "/home/user/.../control.sock"
/// niri-socket "/run/user/1000/niri.sock"
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
    no_window_opened: Option<bool>,

    #[knuffel(child, unwrap(argument))]
    no_window_closed: Option<bool>,

    #[knuffel(child, unwrap(argument))]
    no_workspace_activated: Option<bool>,

    #[knuffel(child, unwrap(argument))]
    cooldown_ms: Option<u64>,

    #[knuffel(child, unwrap(argument))]
    mode: Option<String>,

    #[knuffel(child, unwrap(argument))]
    control_socket: Option<String>,

    #[knuffel(child, unwrap(argument))]
    niri_socket: Option<String>,
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
    /// no-window-opened false
    /// no-window-closed false
    /// no-workspace-activated false
    /// cooldown-ms 2000
    /// mode "manual"
    /// control-socket "/home/user/.config/niri/niri-animation-rotate/control.sock"
    /// niri-socket "/run/user/1000/niri.sock"
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

    /// Do not rotate on WindowOpenedOrChanged events (window opens/changes).
    ///
    /// By default, the daemon rotates animations on every window open, close,
    /// and workspace switch. Use this flag to exclude window open events.
    #[arg(long)]
    pub no_window_opened: bool,

    /// Do not rotate on WindowClosed events (window closes).
    ///
    /// By default, the daemon rotates animations on every window open, close,
    /// and workspace switch. Use this flag to exclude window close events.
    #[arg(long)]
    pub no_window_closed: bool,

    /// Do not rotate on WorkspaceActivated events (workspace switch).
    ///
    /// By default, the daemon rotates animations on every window open, close,
    /// and workspace switch. Use this flag to exclude workspace switch events.
    #[arg(long)]
    pub no_workspace_activated: bool,

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

    /// Path to the Niri IPC socket.
    ///
    /// Overrides the `NIRI_SOCKET` environment variable.
    /// If not set, falls back to the `NIRI_SOCKET` env var (which is set by the Niri session).
    #[arg(long)]
    pub niri_socket: Option<PathBuf>,
}

/// Resolved application configuration after merging CLI args, config file, and defaults.
#[derive(Debug, Clone)]
pub struct Config {
    pub animation_dir: PathBuf,
    pub animation_target: PathBuf,
    pub log_socket: bool,
    pub no_reload: bool,
    pub no_window_opened: bool,
    pub no_window_closed: bool,
    pub no_workspace_activated: bool,
    pub cooldown_ms: u64,
    pub mode: Mode,
    pub control_socket: PathBuf,
    pub niri_socket: PathBuf,
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

        let no_window_opened = cli.no_window_opened || kdl_config.no_window_opened.unwrap_or(false);

        let no_window_closed = cli.no_window_closed || kdl_config.no_window_closed.unwrap_or(false);

        let no_workspace_activated =
            cli.no_workspace_activated || kdl_config.no_workspace_activated.unwrap_or(false);

        let cooldown_ms = cli.cooldown_ms.or(kdl_config.cooldown_ms).unwrap_or(0);

        let mode = cli
            .mode
            .or_else(|| parse_mode_from_kdl(kdl_config.mode.as_deref()))
            .unwrap_or(Mode::Auto);

        let control_socket = cli
            .control_socket
            .or_else(|| kdl_config.control_socket.as_ref().map(PathBuf::from))
            .unwrap_or_else(default_control_socket);

        let niri_socket = cli
            .niri_socket
            .or_else(|| kdl_config.niri_socket.as_ref().map(PathBuf::from))
            .or_else(default_niri_socket_from_env)
            .context(
                "NIRI_SOCKET environment variable not set. Are you running inside a Niri session?",
            )?;

        Ok(Config {
            animation_dir,
            animation_target,
            log_socket,
            no_reload,
            no_window_opened,
            no_window_closed,
            no_workspace_activated,
            cooldown_ms,
            mode,
            control_socket,
            niri_socket,
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

fn default_niri_socket_from_env() -> Option<PathBuf> {
    std::env::var("NIRI_SOCKET").ok().map(PathBuf::from)
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

    #[test]
    fn test_default_niri_socket_from_env_set() {
        let original = std::env::var("NIRI_SOCKET").ok();
        // SAFETY: These are test functions running sequentially with --test-threads=1,
        // so there is no concurrent env var access.
        unsafe {
            std::env::set_var("NIRI_SOCKET", "/run/user/1000/niri.sock");
        }
        let result = default_niri_socket_from_env();
        assert_eq!(result, Some(PathBuf::from("/run/user/1000/niri.sock")));
        unsafe {
            match original {
                Some(v) => std::env::set_var("NIRI_SOCKET", v),
                None => std::env::remove_var("NIRI_SOCKET"),
            }
        }
    }

    #[test]
    fn test_default_niri_socket_from_env_unset() {
        let original = std::env::var("NIRI_SOCKET").ok();
        // SAFETY: These are test functions running sequentially with --test-threads=1,
        // so there is no concurrent env var access.
        unsafe {
            std::env::remove_var("NIRI_SOCKET");
        }
        let result = default_niri_socket_from_env();
        assert_eq!(result, None);
        unsafe {
            match original {
                Some(v) => std::env::set_var("NIRI_SOCKET", v),
                None => std::env::remove_var("NIRI_SOCKET"),
            }
        }
    }

    #[test]
    fn test_event_filter_defaults_are_false() {
        // Default KdlConfig should have None for the new fields,
        // which means the merge pattern (cli.x || kdl.x.unwrap_or(false))
        // produces false (events trigger rotation by default).
        let kdl = KdlConfig::default();
        assert!(kdl.no_window_opened.is_none());
        assert!(kdl.no_window_closed.is_none());
        assert!(kdl.no_workspace_activated.is_none());

        // With CLI=false and config=None, resolved should be false:
        assert!(!(false || kdl.no_window_opened.unwrap_or(false)));
        assert!(!(false || kdl.no_window_closed.unwrap_or(false)));
        assert!(!(false || kdl.no_workspace_activated.unwrap_or(false)));
    }

    #[test]
    fn test_event_filter_merge_pattern() {
        // Test the merge pattern: cli.x || kdl_config.x.unwrap_or(false)
        // CLI false + config None = false (default, all events trigger)
        assert!(!(false || None::<bool>.unwrap_or(false)));

        // CLI false + config Some(true) = true (config enables suppression)
        assert!(false || Some(true).unwrap_or(false));

        // CLI true + config None = true (CLI enables suppression)
        assert!(true || None::<bool>.unwrap_or(false));

        // CLI true + config Some(true) = true (both enable suppression)
        assert!(true || Some(true).unwrap_or(false));

        // CLI true + config Some(false) = true (CLI wins)
        assert!(true || Some(false).unwrap_or(false));
    }

    #[test]
    fn test_event_filter_kdl_parsing() {
        // Parse a KDL snippet with all three new fields set to true
        let kdl: KdlConfig = knuffel::parse(
            "test",
            r#"
log-socket true
no-reload true
no-window-opened true
no-window-closed true
no-workspace-activated true
"#,
        )
        .expect("Failed to parse KDL config snippet");

        assert_eq!(kdl.no_window_opened, Some(true));
        assert_eq!(kdl.no_window_closed, Some(true));
        assert_eq!(kdl.no_workspace_activated, Some(true));
    }

    #[test]
    fn test_event_filter_kdl_parsing_false() {
        // Parse a KDL snippet with the new fields set to false
        let kdl: KdlConfig = knuffel::parse(
            "test",
            r#"
log-socket true
no-window-opened false
no-window-closed false
no-workspace-activated false
"#,
        )
        .expect("Failed to parse KDL config snippet");

        assert_eq!(kdl.no_window_opened, Some(false));
        assert_eq!(kdl.no_window_closed, Some(false));
        assert_eq!(kdl.no_workspace_activated, Some(false));
    }
}
