//! Monotonic clock shim.
//!
//! `std::time::Instant::now()` panics on `wasm32-unknown-unknown` (there is no
//! monotonic clock). Every monotonic-timing site in the compression hot path
//! (CCR TTL bookkeeping, SmartCrusher/diff timing instrumentation) imports
//! [`Instant`] from here instead of `std::time` so the wasm build gets a
//! zero-cost stand-in whose `elapsed()` is always `Duration::ZERO`. Timing
//! metrics simply read as 0 on wasm; correctness is unaffected.

#[cfg(not(target_arch = "wasm32"))]
pub(crate) use std::time::Instant;

#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy)]
pub(crate) struct Instant;

#[cfg(target_arch = "wasm32")]
impl Instant {
    pub(crate) fn now() -> Self {
        Instant
    }

    pub(crate) fn elapsed(&self) -> std::time::Duration {
        std::time::Duration::ZERO
    }
}
