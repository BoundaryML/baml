//! Call-depth accounting for recursive functions.
//!
//! The VM caps the call stack at `bex_vm::MAX_FRAMES` frames and throws
//! `baml.panics.StackOverflow` past it. Native code has no frame list, so a
//! function on a call cycle holds a [`Guard`] for its duration and the guard
//! throws the same panic at the same depth. Functions off every cycle cannot
//! recurse and pay nothing: a program's depth is bounded by its cycles.

use std::cell::Cell;

use crate::{Panic, Thrown};

/// The VM's `MAX_FRAMES`: the deepest call the language allows.
pub const MAX_DEPTH: u32 = 256;

thread_local! {
    static DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// One live frame of a recursive function. Dropping it leaves the frame.
#[must_use = "the frame is counted only while the guard lives"]
pub struct Guard(());

impl Guard {
    /// Enter a frame, or throw `StackOverflow` when [`MAX_DEPTH`] frames are
    /// already live.
    #[inline]
    pub fn enter() -> Result<Guard, Thrown> {
        DEPTH.with(|depth| {
            let current = depth.get();
            if current >= MAX_DEPTH {
                return Err(Thrown::from(Panic::StackOverflow));
            }
            depth.set(current + 1);
            Ok(Guard(()))
        })
    }
}

impl Drop for Guard {
    #[inline]
    fn drop(&mut self) {
        DEPTH.with(|depth| depth.set(depth.get() - 1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descend(n: u32) -> Result<u32, Thrown> {
        let _frame = Guard::enter()?;
        if n == 0 {
            Ok(0)
        } else {
            descend(n - 1).map(|d| d + 1)
        }
    }

    #[test]
    fn the_limit_is_the_vms_frame_count() {
        assert_eq!(descend(MAX_DEPTH - 1).unwrap(), MAX_DEPTH - 1);
        let thrown = descend(MAX_DEPTH).unwrap_err();
        assert_eq!(thrown.class_fqn(), "baml.panics.StackOverflow");
        // Unwinding released every frame.
        assert_eq!(descend(0).unwrap(), 0);
    }
}
