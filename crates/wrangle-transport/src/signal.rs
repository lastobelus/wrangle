use std::sync::atomic::{AtomicBool, Ordering};

static SIGNAL_RECEIVED: AtomicBool = AtomicBool::new(false);

pub struct SignalGuard {
    child_pid: u32,
}

impl Drop for SignalGuard {
    fn drop(&mut self) {
        if SIGNAL_RECEIVED.load(Ordering::SeqCst) && self.child_pid > 0 {
            #[cfg(unix)]
            unsafe {
                libc::kill(self.child_pid as i32, libc::SIGTERM);
            }
        }
    }
}

pub fn setup_signal_handler(child_pid: u32) -> SignalGuard {
    static INSTALLED: AtomicBool = AtomicBool::new(false);
    if !INSTALLED.swap(true, Ordering::SeqCst) {
        let _ = ctrlc::set_handler(move || {
            SIGNAL_RECEIVED.store(true, Ordering::SeqCst);
        });
    }
    SignalGuard { child_pid }
}

