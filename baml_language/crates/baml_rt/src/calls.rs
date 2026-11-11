//! Values fixed for the lifetime of a running fiber, passed through generated
//! calls so LLVM can keep them in registers and reuse inlined stack guards.
use std::cell::Cell;

use crate::R;

#[derive(Clone, Copy)]
pub struct Context {
    stack_threshold: usize,
    poll_word: *const Cell<u8>,
}

pub struct ErrorFrame {
    pub source: &'static crate::Source,
    pub caller_pc: u32,
    pub parent: *const ErrorFrame,
}

impl Context {
    /// # Safety
    /// Use only on the current fiber, inside its scoped runtime binding. Do not
    /// retain this context after that binding ends or pass it to another task.
    #[inline(always)]
    pub unsafe fn current() -> Self {
        Self {
            // A running fiber has at least this reserve above its valid stack
            // mapping's lower bound. Pre-add it so each call needs one compare.
            stack_threshold: if crate::ENTRY_ERROR.get().is_null() {
                crate::STACK_LIMIT.get() + crate::language::STACK_RESERVE
            } else {
                usize::MAX
            },
            poll_word: crate::POLL_WORD.get(),
        }
    }

    #[inline(always)]
    pub fn check_stack(self) -> R<()> {
        crate::language::check_stack_before(self.stack_threshold)
    }

    /// # Safety
    /// The context must still belong to the running fiber. Managed pointers
    /// must obey the same stack-rooting contract as `crate::safepoint`.
    #[inline(always)]
    pub unsafe fn poll(self) -> R<()> {
        if unsafe { (*self.poll_word).get() } == 0 {
            return Ok(());
        }
        // The slow path keeps using the scoped binding: it can yield, deliver
        // cancellation and collect, with exactly the ordinary poll's errors.
        crate::language::poll()
    }
}

/// Only programs observing error contexts carry immutable BAML frame links.
#[derive(Clone, Copy)]
pub struct ErrorContext {
    base: Context,
    pub(crate) frame: *const ErrorFrame,
    pub(crate) handling: usize,
}
impl ErrorContext {
    /// # Safety
    /// Same fiber and lifetime requirements as Context::current.
    #[inline(always)]
    pub unsafe fn current() -> Self {
        Self {
            base: unsafe { Context::current() },
            frame: std::ptr::null(),
            handling: 0,
        }
    }
    #[inline(always)]
    pub fn check_stack(self) -> R<()> {
        self.base.check_stack()
    }
    /// # Safety
    /// Same rooting requirements as Context::poll.
    #[inline(always)]
    pub unsafe fn poll(self) -> R<()> {
        unsafe { self.base.poll() }
    }
    pub fn error_frame(self, source: &'static crate::Source, site: usize) -> ErrorFrame {
        ErrorFrame {
            source,
            caller_pc: crate::telemetry::caller_pc(site),
            parent: self.frame,
        }
    }
    /// # Safety
    /// The frame must outlive all calls using the returned context.
    pub unsafe fn with_frame(self, frame: &ErrorFrame) -> Self {
        Self { frame, ..self }
    }
    pub fn with_handling(self, handling: usize) -> Self {
        Self { handling, ..self }
    }
    pub fn handling(self) -> usize {
        self.handling
    }
}
