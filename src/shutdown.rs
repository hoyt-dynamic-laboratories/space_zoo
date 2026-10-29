//! Defer catchable external termination to the ordinary game-loop teardown.
//! Signal handlers must never touch the framebuffer, VT, terminal or heap.
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);
static SHUTDOWN_SIGNAL: AtomicI32 = AtomicI32::new(0);

pub fn requested() -> bool {
    SHUTDOWN_REQUESTED.load(Ordering::Relaxed)
}

pub fn signal_name() -> Option<&'static str> {
    match SHUTDOWN_SIGNAL.load(Ordering::Relaxed) {
        libc::SIGINT => Some("SIGINT"),
        libc::SIGTERM => Some("SIGTERM"),
        _ => None,
    }
}

#[cfg(unix)]
mod platform {
    use super::*;
    use std::{io, mem};

    extern "C" fn request_shutdown(signal: libc::c_int) {
        SHUTDOWN_SIGNAL.store(signal, Ordering::Relaxed);
        SHUTDOWN_REQUESTED.store(true, Ordering::Relaxed);
    }

    /// Restores preexisting dispositions after the game has fully torn down.
    pub struct SignalGuard {
        previous_int: libc::sigaction,
        previous_term: libc::sigaction,
    }

    impl SignalGuard {
        pub fn install() -> io::Result<Self> {
            SHUTDOWN_REQUESTED.store(false, Ordering::Relaxed);
            SHUTDOWN_SIGNAL.store(0, Ordering::Relaxed);
            // SAFETY: All fields in a zeroed sigaction are valid before the
            // signal mask is explicitly initialized by sigemptyset.
            let mut action: libc::sigaction = unsafe { mem::zeroed() };
            action.sa_sigaction = request_shutdown as *const () as libc::sighandler_t;
            action.sa_flags = 0; // Do not restart blocking reads on termination.
            // SAFETY: action owns its initialized signal mask.
            if unsafe { libc::sigemptyset(&mut action.sa_mask) } < 0 {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: sigaction writes the old dispositions to valid structs.
            let mut previous_int: libc::sigaction = unsafe { mem::zeroed() };
            let mut previous_term: libc::sigaction = unsafe { mem::zeroed() };
            if unsafe { libc::sigaction(libc::SIGINT, &action, &mut previous_int) } < 0 {
                return Err(io::Error::last_os_error());
            }
            if unsafe { libc::sigaction(libc::SIGTERM, &action, &mut previous_term) } < 0 {
                let error = io::Error::last_os_error();
                unsafe { libc::sigaction(libc::SIGINT, &previous_int, std::ptr::null_mut()); }
                return Err(error);
            }
            Ok(Self { previous_int, previous_term })
        }
    }

    impl Drop for SignalGuard {
        fn drop(&mut self) {
            // Only restored by ordinary Rust teardown, never by the handler.
            let term=unsafe {libc::sigaction(libc::SIGTERM,&self.previous_term,std::ptr::null_mut())};
            if term<0 {crate::logging::cleanup_failure("signal.SIGTERM_restore",&io::Error::last_os_error().to_string());}
            let int=unsafe {libc::sigaction(libc::SIGINT,&self.previous_int,std::ptr::null_mut())};
            if int<0 {crate::logging::cleanup_failure("signal.SIGINT_restore",&io::Error::last_os_error().to_string());}
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn handler_only_sets_shutdown_flag() {
            SHUTDOWN_REQUESTED.store(false, Ordering::Relaxed);
            request_shutdown(libc::SIGTERM);
            assert!(requested());
            SHUTDOWN_REQUESTED.store(false, Ordering::Relaxed);
            SHUTDOWN_SIGNAL.store(0, Ordering::Relaxed);
        }
    }
}

#[cfg(unix)]
pub use platform::SignalGuard;

#[cfg(not(unix))]
pub struct SignalGuard;
#[cfg(not(unix))]
impl SignalGuard {
    pub fn install() -> std::io::Result<Self> { Ok(Self) }
}
