//! Panic hook integration for the bounded Space Zoo journal.
//!
//! Persistent logging is attempted nonblockingly by `logging::panic`; the prior
//! hook is always retained as the stderr fallback.
pub struct LogGuard;

pub fn install() -> LogGuard {
    let previous=std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        crate::logging::panic(info);
        previous(info);
    }));
    LogGuard
}
