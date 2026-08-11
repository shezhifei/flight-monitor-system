use std::future::Future;

use futures::future::FutureExt;
use tokio::task::JoinHandle;
use tracing::{debug, error};
use uuid::Uuid;

/// Spawn a tracked task that logs panics instead of silently dropping them.
///
/// Addresses the "naked `tokio::spawn`" anti-pattern: any panic inside the
/// spawned future is caught, logged with a stable task id/name, and the task
/// is aborted cleanly.  The returned handle can be awaited if the caller needs
/// to wait for completion.
///
/// # Notes
///
/// - The workspace uses `panic = "abort"` for release builds, so in release
///   mode panics abort the process rather than being caught.  This wrapper is
///   still valuable in debug/tests and for observability (name + task id).
/// - Callers must not hold locks across `.await` points inside `fut`, or the
///   `AssertUnwindSafe` boundary would be unsound.
pub fn spawn_tracked<F>(name: &'static str, fut: F) -> JoinHandle<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    let task_id = Uuid::new_v4();
    tokio::spawn(async move {
        let task_name = name;
        let tid = task_id;
        match std::panic::AssertUnwindSafe(fut).catch_unwind().await {
            Ok(()) => {
                debug!(%task_name, %tid, "tracked task completed");
            }
            Err(panic_info) => {
                let panic_msg = panic_info
                    .downcast_ref::<String>()
                    .map(String::as_str)
                    .or_else(|| panic_info.downcast_ref::<&str>().copied())
                    .unwrap_or("unknown panic");
                error!(%task_name, %tid, %panic_msg, "tracked task panicked");
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn catches_panic_and_returns_ok() {
        // Without the wrapper, awaiting a JoinHandle after a panic returns Err.
        // The wrapper must catch the panic and complete the outer task normally.
        let handle = spawn_tracked("test:panic", async {
            panic!("intentional test panic");
        });

        let result = handle.await;
        assert!(result.is_ok(), "spawn_tracked should absorb the inner panic");
    }

    #[tokio::test]
    async fn normal_completion_returns_ok() {
        let handle = spawn_tracked("test:ok", async {});
        let result = handle.await;
        assert!(result.is_ok(), "spawn_tracked should complete normally");
    }
}
