//! Cooperative cancellation for long renders.
//!
//! A job holder wraps its render call in [`scope`] with an `AtomicBool`;
//! the CPU renderers check [`requested`] between layers (and after expensive
//! child work) and abort with [`RenderError::Cancelled`]. Checks run on the
//! calling thread only — rayon row workers intentionally do not see the
//! token, so the finest abort granularity is one layer. No signal handlers,
//! no detached threads: cancellation is always cooperative and scoped.

use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::reference::RenderError;

thread_local! {
    static TOKEN: Cell<*const AtomicBool> = const { Cell::new(std::ptr::null()) };
}

struct Guard;
impl Drop for Guard {
    fn drop(&mut self) {
        TOKEN.with(|t| t.set(std::ptr::null()));
    }
}

/// Run `f` with `token` installed as the current render's cancel signal.
/// The token must outlive the call (callers hold an `Arc<AtomicBool>`).
pub fn scope<R>(token: &AtomicBool, f: impl FnOnce() -> R) -> R {
    let _guard = Guard;
    TOKEN.with(|t| t.set(token as *const AtomicBool));
    f()
}

/// True when the current scope's token has been cancelled.
pub fn requested() -> bool {
    TOKEN.with(|t| {
        let ptr = t.get();
        !ptr.is_null() && unsafe { &*ptr }.load(Ordering::Relaxed)
    })
}

/// Typed check for render loops: [`RenderError::Cancelled`] when cancelled.
pub fn check() -> Result<(), RenderError> {
    if requested() {
        Err(RenderError::Cancelled)
    } else {
        Ok(())
    }
}
