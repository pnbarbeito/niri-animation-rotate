use anyhow::Result;
use std::process::Stdio;
use tracing::{debug, warn};

/// Reload the Niri configuration by sending the reload action.
///
/// This spawns `niri msg action reload` as a subprocess and lets it run in the background.
/// Errors are logged but not propagated since a reload failure shouldn't crash the daemon.
pub async fn reload_niri() -> Result<()> {
    debug!("Reloading Niri configuration...");

    let result = tokio::process::Command::new("niri")
        .args(["msg", "action", "reload"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();

    match result {
        Ok(mut child) => {
            // Don't wait for the child — fire and forget
            // But we need to avoid zombie processes, so detach
            tokio::spawn(async move {
                let _ = child.wait().await;
            });
            debug!("Niri reload command spawned");
            Ok(())
        }
        Err(e) => {
            warn!(
                error = %e,
                "Failed to spawn 'niri msg action reload'. Is 'niri' in PATH?"
            );
            // Return Ok so the daemon doesn't crash on reload failure
            Ok(())
        }
    }
}
