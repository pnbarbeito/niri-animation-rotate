mod animation;
mod config;
mod duration;
mod niri;

use anyhow::Result;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{Mutex, mpsc};

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = config::Config::load()?;

    tracing::info!(
        animation_dir = %config.animation_dir.display(),
        animation_target = %config.animation_target.display(),
        "Configuration loaded"
    );

    // Initialize the animation rotator
    let rotator = match animation::AnimationRotator::new(
        config.animation_dir.clone(),
        config.animation_target.clone(),
        config.duration_fallback_ms,
        config.random_order,
    ) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(
                animation_dir = %config.animation_dir.display(),
                error = %e,
                "Starting with empty animation list (waiting for files)"
            );
            animation::AnimationRotator::empty(
                config.animation_dir.clone(),
                config.animation_target.clone(),
                config.duration_fallback_ms,
                config.random_order,
            )
        }
    };

    tracing::info!(
        file_count = rotator.file_count(),
        "Animation rotator ready, starting event loop"
    );

    // Run the event loop
    let animator = Arc::new(Mutex::new(rotator));
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(run_event_loop(config, animator))
}

async fn run_event_loop(
    config: config::Config,
    animator: Arc<Mutex<animation::AnimationRotator>>,
) -> Result<()> {
    run_unified_event_loop(config, animator).await
}

/// Unified event loop: control socket always available, Niri events filtered by mode.
///
/// Supports hot-switching between auto and manual mode via `mode auto` / `mode manual`
/// commands on the control socket.
async fn run_unified_event_loop(
    config: config::Config,
    animator: Arc<Mutex<animation::AnimationRotator>>,
) -> Result<()> {
    // --- Control socket (always available) ---
    let _ = tokio::fs::remove_file(&config.control_socket).await;
    let listener = UnixListener::bind(&config.control_socket)?;

    // --- Niri IPC connection (always active, events filtered by mode) ---
    let mut stream = UnixStream::connect(&config.niri_socket).await?;
    // Subscribe to the event stream.
    // Events arrive on the same connection where we send the command.
    stream.write_all(b"\"EventStream\"\n").await?;
    stream.shutdown().await?;
    let reader = BufReader::new(stream);
    let mut lines = reader.lines();

    // --- Filesystem watcher (always active) ---
    let (watcher_tx, mut watcher_rx) = mpsc::unbounded_channel::<()>();
    let watcher_config = config.clone();
    tokio::spawn(async move {
        run_watcher(watcher_config, watcher_tx).await;
    });

    // --- Mode state (mutable, hot-swappable) ---
    let mut mode = config.mode;

    // --- Auto-mode debounce state ---
    let mut initial_events_seen = 0;
    const EXPECTED_INITIAL_EVENTS: usize = 5;
    let mut bloqueo_hasta: Option<Instant> = None;
    // Startup init (only meaningful if starting in auto mode)
    if config.animation_target.exists()
        && let Some(duracion) = duration::parse_animation_duration(&config.animation_target)
    {
        bloqueo_hasta = Some(Instant::now() + Duration::from_millis(duracion + config.cooldown_ms));
        tracing::info!(
            duration_ms = duracion,
            cooldown_ms = config.cooldown_ms,
            "Startup: blocking rotations until current animation finishes"
        );
    }

    // --- Manual-mode cooldown state ---
    let mut last_rotation: Option<Instant> = None;

    tracing::info!(
        mode = ?mode,
        control_socket = %config.control_socket.display(),
        niri_socket = %config.niri_socket.display(),
        "Unified event loop started"
    );

    loop {
        tokio::select! {
            // --- Niri events (processed only in auto mode) ---
            line = lines.next_line() => {
                match line {
                    Ok(Some(line)) => {
                        if config.log_socket {
                            eprintln!("[socket] {}", line);
                        }
                        if mode != config::Mode::Auto {
                            continue;
                        }

                        let event_type = match extract_event_type(&line) {
                            Some(t) => t.to_string(),
                            None => {
                                tracing::debug!(line = %line, "Could not parse event type, skipping");
                                continue;
                            }
                        };

                        // Skip initial state events
                        if initial_events_seen < EXPECTED_INITIAL_EVENTS {
                            initial_events_seen += 1;
                            tracing::debug!(
                                event = %event_type,
                                "Initial state event received (skipped)"
                            );
                            continue;
                        }

                        // Check if this is a rotation-triggering event
                        let should_rotate = match event_type.as_str() {
                            "WindowOpenedOrChanged" => !config.no_window_opened,
                            "WindowClosed" => !config.no_window_closed,
                            _ => false,
                        };
                        if should_rotate {
                            // Debounce-with-reset
                            let ahora = Instant::now();
                            let debe_rotar = match bloqueo_hasta {
                                None => true,
                                Some(t) => ahora >= t,
                            };

                            if debe_rotar {
                                tracing::debug!(event = %event_type, "Triggering animation rotation");
                                rotate_and_reload(&animator, config.no_reload).await;

                                let nueva_duracion = {
                                    let anim = animator.lock().await;
                                    anim.current_duration_ms()
                                };
                                bloqueo_hasta = Some(
                                    ahora + Duration::from_millis(nueva_duracion + config.cooldown_ms)
                                );
                            } else {
                                let duracion_vigente = {
                                    let anim = animator.lock().await;
                                    anim.current_duration_ms()
                                };
                                bloqueo_hasta = Some(
                                    ahora + Duration::from_millis(duracion_vigente + config.cooldown_ms)
                                );
                                tracing::debug!(
                                    event = %event_type,
                                    "Rotation blocked: animation still playing (block extended)"
                                );
                            }
                        } else {
                            tracing::trace!(event = %event_type, "Event ignored");
                        }
                    }
                    Ok(None) => {
                        tracing::warn!("Niri event stream ended (socket closed)");
                        break;
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "Error reading from Niri event stream");
                        break;
                    }
                }
            }

            // --- Control socket commands (always available) ---
            result = listener.accept() => {
                let (mut stream, _) = result?;
                let mut buf = String::new();
                {
                    let mut reader = BufReader::new(&mut stream);
                    reader.read_line(&mut buf).await?;
                }

                let command = buf.trim();

                // Mode-switch commands (always available, in any mode)
                if command == "mode auto" {
                    mode = config::Mode::Auto;
                    // Re-initialize auto-mode debounce from current target
                    if config.animation_target.exists()
                        && let Some(duracion) = duration::parse_animation_duration(&config.animation_target)
                    {
                        bloqueo_hasta = Some(Instant::now() + Duration::from_millis(duracion + config.cooldown_ms));
                    } else {
                        bloqueo_hasta = None;
                    }
                    last_rotation = None;
                    initial_events_seen = 0; // reset skip counter
                    tracing::info!("Switched to auto mode");
                    let _ = stream.write_all(b"ok\n").await;
                    continue;
                }
                if command == "mode manual" {
                    mode = config::Mode::Manual;
                    last_rotation = None;
                    bloqueo_hasta = None;
                    tracing::info!("Switched to manual mode");
                    let _ = stream.write_all(b"ok\n").await;
                    continue;
                }

                // In auto mode, only allow query commands (no rotation)
                if mode == config::Mode::Auto {
                    match command {
                        "current" => {
                            let anim = animator.lock().await;
                            match anim.current_file_stem() {
                                Some(stem) => {
                                    let _ = stream.write_all(format!("{}\n", stem).as_bytes()).await;
                                }
                                None => {
                                    let _ = stream.write_all(b"error: no animations available\n").await;
                                }
                            }
                        }
                        "list" => {
                            let anim = animator.lock().await;
                            for stem in anim.file_stems() {
                                let _ = stream.write_all(format!("{}\n", stem).as_bytes()).await;
                            }
                        }
                        _ => {
                            let _ = stream.write_all(b"error: not in manual mode\n").await;
                        }
                    }
                    continue;
                }

                // Manual mode: full command set
                // Cooldown check for rotation commands
                let is_rotation_cmd = matches!(command, "next" | "rotate" | "prev");
                if is_rotation_cmd
                    && let Some(t) = last_rotation
                {
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

                match command {
                    "next" | "rotate" => {
                        tracing::info!("Received '{}' command on control socket", command);
                        rotate_and_reload(&animator, config.no_reload).await;
                        last_rotation = Some(Instant::now());
                    }
                    "prev" => {
                        tracing::info!("Received 'prev' command on control socket");
                        rotate_prev_and_reload(&animator, config.no_reload).await;
                        last_rotation = Some(Instant::now());
                    }
                    "current" => {
                        let anim = animator.lock().await;
                        match anim.current_file_stem() {
                            Some(stem) => {
                                let _ = stream.write_all(format!("{}\n", stem).as_bytes()).await;
                            }
                            None => {
                                let _ = stream.write_all(b"error: no animations available\n").await;
                            }
                        }
                    }
                    "list" => {
                        let anim = animator.lock().await;
                        for stem in anim.file_stems() {
                            let _ = stream.write_all(format!("{}\n", stem).as_bytes()).await;
                        }
                    }
                    "select" => {
                        let _ = stream.write_all(b"error: missing name\n").await;
                    }
                    cmd if cmd.starts_with("select ") => {
                        let name = cmd["select ".len()..].trim();
                        if name.is_empty() {
                            let _ = stream.write_all(b"error: missing name\n").await;
                        } else {
                            let ahora = Instant::now();
                            let mut anim = animator.lock().await;
                            match anim.select_by_name(name).await {
                                Ok(true) => {
                                    drop(anim);
                                    if !config.no_reload
                                        && let Err(e) = niri::reload_niri().await
                                    {
                                        tracing::warn!(error = %e, "Failed to reload Niri config");
                                    }
                                    last_rotation = Some(ahora);
                                    let _ = stream.write_all(b"ok\n").await;
                                }
                                Ok(false) => {
                                    let _ = stream.write_all(b"error: not found\n").await;
                                }
                                Err(e) => {
                                    let _ = stream
                                        .write_all(format!("error: {}\n", e).as_bytes())
                                        .await;
                                }
                            }
                        }
                    }
                    _ => {
                        let _ = stream
                            .write_all(
                                format!("error: unknown command: {}\n", command).as_bytes(),
                            )
                            .await;
                    }
                }
            }

            // --- Filesystem watcher ---
            Some(()) = watcher_rx.recv() => {
                tracing::info!("Filesystem change detected, refreshing animation list");
                let mut anim = animator.lock().await;
                anim.refresh().await;
            }

            // --- Shutdown ---
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("Received SIGINT, shutting down gracefully");
                break;
            }
        }
    }

    // Clean up the socket file on shutdown
    let _ = tokio::fs::remove_file(&config.control_socket).await;

    Ok(())
}

/// Rotate the animation and optionally reload Niri config.
async fn rotate_and_reload(animator: &Arc<Mutex<animation::AnimationRotator>>, no_reload: bool) {
    let mut anim = animator.lock().await;
    if let Err(e) = anim.rotate().await {
        tracing::warn!(error = %e, "Failed to rotate animation");
    } else if !no_reload {
        if let Err(e) = niri::reload_niri().await {
            tracing::warn!(error = %e, "Failed to reload Niri config");
        }
    }
}

/// Rotate to the previous animation and optionally reload Niri config.
async fn rotate_prev_and_reload(
    animator: &Arc<Mutex<animation::AnimationRotator>>,
    no_reload: bool,
) {
    let mut anim = animator.lock().await;
    if let Err(e) = anim.rotate_prev().await {
        tracing::warn!(error = %e, "Failed to rotate animation");
    } else if !no_reload
        && let Err(e) = niri::reload_niri().await
    {
        tracing::warn!(error = %e, "Failed to reload Niri config");
    }
}

fn extract_event_type(line: &str) -> Option<&str> {
    // Niri events are always {"EventName": ...}
    // Extract the first JSON key without full parsing
    let trimmed = line.trim();
    if !trimmed.starts_with('{') {
        return None;
    }

    let after_brace = &trimmed[1..];
    if !after_brace.starts_with('"') {
        return None;
    }

    let after_open_quote = &after_brace[1..];
    let end_quote = after_open_quote.find('"')?;
    Some(&after_open_quote[..end_quote])
}

async fn run_watcher(config: config::Config, tx: mpsc::UnboundedSender<()>) {
    use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

    let watcher_result = RecommendedWatcher::new(
        move |result: Result<Event, notify::Error>| {
            if let Ok(event) = result {
                match event.kind {
                    EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(_) => {
                        let _ = tx.send(());
                    }
                    _ => {}
                }
            }
        },
        notify::Config::default(),
    );

    let mut watcher = match watcher_result {
        Ok(w) => w,
        Err(e) => {
            tracing::error!(error = %e, "Failed to create filesystem watcher");
            return;
        }
    };

    if let Err(e) = watcher.watch(&config.animation_dir, RecursiveMode::NonRecursive) {
        tracing::error!(
            error = %e,
            dir = %config.animation_dir.display(),
            "Failed to watch animation directory"
        );
        return;
    }

    tracing::info!(
        dir = %config.animation_dir.display(),
        "Filesystem watcher started"
    );

    // Keep the watcher alive by sleeping indefinitely
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pure-function debounce logic: determines whether to rotate and returns the
    /// new block-until timestamp.
    ///
    /// - If `bloqueo_hasta` is `None` (no block), rotation fires and the block is set.
    /// - If the block has expired (`ahora >= bloqueo_hasta`), rotation fires.
    /// - Otherwise, rotation is skipped and the block is extended.
    ///
    /// Returns `(should_rotate, new_bloqueo_hasta)`.
    fn compute_block(
        bloqueo_hasta: Option<Instant>,
        ahora: Instant,
        duration_ms: u64,
        cooldown_ms: u64,
    ) -> (bool, Option<Instant>) {
        let debe_rotar = match bloqueo_hasta {
            None => true,
            Some(t) => ahora >= t,
        };

        let new_block = Some(ahora + Duration::from_millis(duration_ms + cooldown_ms));

        (debe_rotar, new_block)
    }

    #[test]
    fn test_debounce_no_block_rotates() {
        let ahora = Instant::now();
        let (should_rotate, new_block) = compute_block(None, ahora, 400, 50);
        assert!(should_rotate);
        // Block set for 450ms from ahora
        assert_eq!(
            new_block,
            Some(ahora + Duration::from_millis(450))
        );
    }

    #[test]
    fn test_debounce_block_active_no_rotation() {
        let ahora = Instant::now();
        // Block is set 3 seconds from now (still active)
        let block = Some(ahora + Duration::from_secs(3));
        let (should_rotate, new_block) = compute_block(block, ahora, 400, 50);
        assert!(!should_rotate);
        // Block is extended from ahora, not from the old block
        assert_eq!(
            new_block,
            Some(ahora + Duration::from_millis(450))
        );
    }

    #[test]
    fn test_debounce_block_expired_rotates() {
        let ahora = Instant::now();
        // Block expired 1 second ago
        let block = Some(ahora - Duration::from_secs(1));
        let (should_rotate, new_block) = compute_block(block, ahora, 400, 50);
        assert!(should_rotate);
        assert_eq!(
            new_block,
            Some(ahora + Duration::from_millis(450))
        );
    }

    #[test]
    fn test_debounce_exact_expiry_moment() {
        let ahora = Instant::now();
        // Block expires exactly at ahora
        let block = Some(ahora);
        let (should_rotate, new_block) = compute_block(block, ahora, 400, 0);
        assert!(should_rotate, "Rotation should occur at the exact expiry moment");
        assert_eq!(new_block, Some(ahora + Duration::from_millis(400)));
    }

    #[test]
    fn test_extract_event_type_valid() {
        assert_eq!(
            extract_event_type(r#"{"WindowOpenedOrChanged": {"window": {}}}"#),
            Some("WindowOpenedOrChanged")
        );
    }

    #[test]
    fn test_extract_event_type_no_event() {
        assert_eq!(extract_event_type("not json"), None);
        assert_eq!(extract_event_type("{}"), None);
    }

    #[test]
    fn test_extract_event_type_workspace_activated() {
        assert_eq!(
            extract_event_type(r#"{"WorkspaceActivated": {"id": 1, "focused": true}}"#),
            Some("WorkspaceActivated")
        );
    }
}
