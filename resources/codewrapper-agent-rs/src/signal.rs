//! Signal handling for graceful shutdown

use std::sync::atomic::{AtomicBool, Ordering};
use tracing::{info, warn};

/// Global flag for signal received
static SIGNAL_RECEIVED: AtomicBool = AtomicBool::new(false);

/// Check if a termination signal has been received
pub fn is_signal_received() -> bool {
    SIGNAL_RECEIVED.load(Ordering::SeqCst)
}

/// Signal handler guard - kills child process on drop if signal received
pub struct SignalGuard {
    child_pid: u32,
}

impl Drop for SignalGuard {
    fn drop(&mut self) {
        if is_signal_received() && self.child_pid > 0 {
            warn!(pid = self.child_pid, "Killing child process due to signal");
            #[cfg(unix)]
            {
                // Send SIGTERM first
                unsafe {
                    libc::kill(self.child_pid as i32, libc::SIGTERM);
                }
            }
            #[cfg(windows)]
            {
                // On Windows, we rely on the process being killed when the handle is dropped
                let _ = self.child_pid; // suppress unused warning
            }
        }
    }
}

/// Setup signal handler and return a guard
pub fn setup_signal_handler(child_pid: u32) -> SignalGuard {
    // Setup handler only once
    static HANDLER_INSTALLED: AtomicBool = AtomicBool::new(false);

    if !HANDLER_INSTALLED.swap(true, Ordering::SeqCst) {
        ctrlc::set_handler(move || {
            info!("Received termination signal");
            SIGNAL_RECEIVED.store(true, Ordering::SeqCst);
        })
        .expect("Error setting signal handler");
    }

    SignalGuard { child_pid }
}

/// Wait for graceful shutdown with timeout
#[allow(dead_code)]
pub async fn wait_for_graceful_shutdown(
    child: &mut tokio::process::Child,
    timeout_secs: u64,
) -> std::io::Result<std::process::ExitStatus> {
    use tokio::time::{Duration, timeout};

    // First, try graceful shutdown (SIGTERM on Unix)
    #[cfg(unix)]
    if let Some(pid) = child.id() {
        unsafe {
            libc::kill(pid as i32, libc::SIGTERM);
        }
    }

    #[cfg(not(unix))]
    let _ = child; // suppress unused warning on non-unix

    // Wait for process to exit with timeout
    match timeout(Duration::from_secs(timeout_secs), child.wait()).await {
        Ok(result) => result,
        Err(_) => {
            // Timeout - force kill
            warn!("Graceful shutdown timed out, force killing");
            child.kill().await?;
            child.wait().await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_flag() {
        assert!(!is_signal_received());
        SIGNAL_RECEIVED.store(true, Ordering::SeqCst);
        assert!(is_signal_received());
        SIGNAL_RECEIVED.store(false, Ordering::SeqCst);
    }
}
