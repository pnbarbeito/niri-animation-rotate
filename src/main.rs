mod animation;
mod config;
mod niri;

use anyhow::Result;
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, Mutex};

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
    match config.mode {
        config::Mode::Auto => run_auto_event_loop(config, animator).await,
        config::Mode::Manual => run_manual_event_loop(config, animator).await,
    }
}

/// Rotate the animation and optionally reload Niri config.
async fn rotate_and_reload(
    animator: &Arc<Mutex<animation::AnimationRotator>>,
    no_reload: bool,
) {
    let mut anim = animator.lock().await;
    if let Err(e) = anim.rotate().await {
        tracing::warn!(error = %e, "Failed to rotate animation");
    } else if !no_reload {
        if let Err(e) = niri::reload_niri().await {
            tracing::warn!(error = %e, "Failed to reload Niri config");
        }
    }
}

/// Auto mode: listen to Niri compositor events and rotate automatically.
async fn run_auto_event_loop(
    config: config::Config,
    animator: Arc<Mutex<animation::AnimationRotator>>,
) -> Result<()> {
    // Check for Niri socket
    let socket_path = std::env::var("NIRI_SOCKET").expect(
        "NIRI_SOCKET environment variable not set. Are you running inside a Niri session?",
    );

    tracing::info!(socket = %socket_path, "Connecting to Niri event stream");

    // Connect to Niri socket and subscribe to event stream
    let mut stream = UnixStream::connect(&socket_path).await?;

    // Subscribe to the event stream.
    // Events arrive on the same connection where we send the command,
    // so we use a single socket: write the command, then read events.
    stream.write_all(b"\"EventStream\"\n").await?;
    // Shut down the write half so Niri knows we're done sending commands.
    stream.shutdown().await?;

    let reader = BufReader::new(stream);
    let mut lines = reader.lines();

    // Spawn filesystem watcher
    let (watcher_tx, mut watcher_rx) = mpsc::unbounded_channel::<()>();
    let watcher_config = config.clone();
    tokio::spawn(async move {
        run_watcher(watcher_config, watcher_tx).await;
    });

    tracing::info!("Event loop started, waiting for events...");

    // Main event loop with signal handling
    let mut initial_events_seen = 0;
    const EXPECTED_INITIAL_EVENTS: usize = 5;
    let mut last_rotation: Option<Instant> = None;

    loop {
        tokio::select! {
            line = lines.next_line() => {
                match line {
                    Ok(Some(line)) => {
                        if config.log_socket {
                            eprintln!("[socket] {}", line);
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
                        if matches!(
                            event_type.as_str(),
                            "WindowOpenedOrChanged" | "WindowClosed" | "WorkspaceActivated"
                        ) {
                            // Cooldown: skip if the last rotation was too recent
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

                            tracing::debug!(event = %event_type, "Triggering animation rotation");
                            rotate_and_reload(&animator, config.no_reload).await;
                            last_rotation = Some(Instant::now());
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
            Some(()) = watcher_rx.recv() => {
                tracing::info!("Filesystem change detected, refreshing animation list");
                let mut anim = animator.lock().await;
                anim.refresh().await;
            }
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("Received SIGINT, shutting down gracefully");
                break;
            }
        }
    }

    Ok(())
}

/// Manual mode: listen on a control socket for "rotate" commands.
async fn run_manual_event_loop(
    config: config::Config,
    animator: Arc<Mutex<animation::AnimationRotator>>,
) -> Result<()> {
    // Remove old socket file if it exists, then bind
    let _ = tokio::fs::remove_file(&config.control_socket).await;
    let listener = UnixListener::bind(&config.control_socket)?;

    tracing::info!(
        control_socket = %config.control_socket.display(),
        "Manual mode: waiting for 'rotate' commands on control socket"
    );

    // Spawn filesystem watcher
    let (watcher_tx, mut watcher_rx) = mpsc::unbounded_channel::<()>();
    let watcher_config = config.clone();
    tokio::spawn(async move {
        run_watcher(watcher_config, watcher_tx).await;
    });

    let mut last_rotation: Option<Instant> = None;

    loop {
        tokio::select! {
            result = listener.accept() => {
                let (mut stream, _) = result?;
                let mut buf = String::new();
                let mut reader = BufReader::new(&mut stream);
                reader.read_line(&mut buf).await?;

                let command = buf.trim();
                if command == "rotate" {
                    // Cooldown: skip if the last rotation was too recent
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

                    tracing::info!("Received 'rotate' command on control socket");
                    rotate_and_reload(&animator, config.no_reload).await;
                    last_rotation = Some(Instant::now());
                } else {
                    tracing::debug!(command = %command, "Unknown command on control socket");
                }
            }
            Some(()) = watcher_rx.recv() => {
                tracing::info!("Filesystem change detected, refreshing animation list");
                let mut anim = animator.lock().await;
                anim.refresh().await;
            }
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
