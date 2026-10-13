//! Console-exclusive VT guard. This is a per-process, per-VT lock, NOT a
//! global keyboard remap: Linux asks us before switching away; we say no.
//! Only exists during the game. Ctrl+C remains an intentional clean exit.
#[cfg(target_os = "linux")]
mod platform {
    use std::io;
    use std::sync::atomic::{AtomicBool, Ordering};
    use libc::{self, c_int};

    const VT_GETMODE: libc::c_ulong = 0x5601;
    const VT_SETMODE: libc::c_ulong = 0x5602;
    const VT_RELDISP: libc::c_ulong = 0x5605;
    const VT_AUTO: u8 = 0;
    const VT_PROCESS: u8 = 1;
    static RELEASE_REQUESTED: AtomicBool = AtomicBool::new(false);

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct VtMode { mode: u8, waitv: u8, relsig: i16, acqsig: i16, frsig: i16 }

    extern "C" fn release_signal(_sig: c_int) {
        RELEASE_REQUESTED.store(true, Ordering::Relaxed);
    }

    /// Physical Linux VT detection; PTYs (SSH and terminal emulators) fail VT_GETMODE.
    pub fn is_virtual_console() -> bool {
        let mut mode = VtMode::default();
        unsafe { libc::ioctl(libc::STDIN_FILENO, VT_GETMODE, &mut mode) == 0 }
    }

    pub struct Guard {
        fd: c_int,
        original: VtMode,
        previous_handler: libc::sighandler_t,
    }

    impl Guard {
        /// Ok(None) if not running on a Linux virtual console (SSH, GNOME, etc.).
        pub fn install() -> io::Result<Option<Self>> {
            let fd = libc::STDIN_FILENO;
            let mut original = VtMode::default();
            let rc = unsafe { libc::ioctl(fd, VT_GETMODE, &mut original) };
            if rc < 0 {
                let e = io::Error::last_os_error();
                if matches!(e.raw_os_error(), Some(libc::ENOTTY | libc::EINVAL)) {
                    return Ok(None);
                }
                return Err(e);
            }
            if original.mode != VT_AUTO {
                return Err(io::Error::other("console already managed by another VT process"));
            }
            RELEASE_REQUESTED.store(false, Ordering::Relaxed);
            let handler = unsafe {
                libc::signal(libc::SIGUSR1, release_signal as libc::sighandler_t)
            };
            if handler == libc::SIG_ERR { return Err(io::Error::last_os_error()); }
            let mode = VtMode { mode: VT_PROCESS, waitv: 0,
                relsig: libc::SIGUSR1 as i16, acqsig: 0, frsig: 0 };
            if unsafe { libc::ioctl(fd, VT_SETMODE, &mode) } < 0 {
                let e = io::Error::last_os_error();
                unsafe { libc::signal(libc::SIGUSR1, handler); }
                return Err(e);
            }
            Ok(Some(Self { fd, original, previous_handler: handler }))
        }

        /// Deny any pending Alt-Fn / Ctrl-Alt-Fn switch request.
        pub fn poll(&self) {
            if RELEASE_REQUESTED.swap(false, Ordering::Relaxed) {
                unsafe { libc::ioctl(self.fd, VT_RELDISP, 0); }
            }
        }
    }

    impl Drop for Guard {
        fn drop(&mut self) {
            // Always reset to auto before restoring the previous handler.
            unsafe {
                libc::ioctl(self.fd, VT_SETMODE, &self.original);
                libc::signal(libc::SIGUSR1, self.previous_handler);
            }
        }
    }
}

#[cfg(target_os = "linux")]
pub use platform::Guard as VtGuard;
#[cfg(target_os = "linux")]
pub use platform::is_virtual_console;

#[cfg(not(target_os = "linux"))]
pub struct VtGuard;
#[cfg(not(target_os = "linux"))]
impl VtGuard {
    pub fn install() -> std::io::Result<Option<Self>> { Ok(None) }
    pub fn poll(&self) {}
}
