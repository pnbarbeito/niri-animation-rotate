mod animation;
mod config;
mod niri;

use anyhow::Result;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
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

    // Apply the initial animation
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(rotator.apply_current())?;

    tracing::info!(
        file_count = rotator.file_count(),
        "Animation rotator ready, starting event loop"
    );

    // Run the event loop
    let animator = Arc::new(Mutex::new(rotator));

    rt.block_on(run_event_loop(config, animator))
}

async fn run_event_loop(
    config: config::Config,
    animator: Arc<Mutex<animation::AnimationRotator>>,
) -> Result<()> {
    // Check for Niri socket
    let socket_path = std::env::var("NIRI_SOCKET").expect(
        "NIRI_SOCKET environment variable not set. Are you running inside a Niri session?",
    );

    tracing::info!(socket = %socket_path, "Connecting to Niri event stream");

    // Connect to Niri socket and subscribe to event stream
    let stream = UnixStream::connect(&socket_path).await?;
    let mut write_stream = UnixStream::connect(&socket_path).await?;

    // Subscribe to the event stream
    write_stream.write_all(b"\"EventStream\"\n").await?;
    // Drop the write end so Niri knows we're done writing
    drop(write_stream);

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

    loop {
        tokio::select! {
            line = lines.next_line() => {
                match line {
                    Ok(Some(line)) => {
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
                            tracing::debug!(event = %event_type, "Triggering animation rotation");
                            let mut anim = animator.lock().await;
                            if let Err(e) = anim.rotate().await {
                                tracing::warn!(error = %e, "Failed to rotate animation");
                            } else if let Err(e) = niri::reload_niri().await {
                                tracing::warn!(error = %e, "Failed to reload Niri config");
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
