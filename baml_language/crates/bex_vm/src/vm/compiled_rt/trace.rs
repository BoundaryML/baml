//! Declaration trace hooks of compiled bodies.
//!
//! A hooked function's bytecode starts with a hook prologue (`BeginTraceHook`,
//! the hook call, `EndTraceHook`) or a built-in mode marker (`TraceHookHidden`
//! and friends). The VM's call path pushes a pending hook when it enters such
//! a function, and the prologue resolves it before the body starts. A compiled
//! body carries the same prologue and resolves the pending hook through these
//! entry points, which do what the corresponding opcodes do.
//!
//! A pending hook exists only when the VM's call path entered the frame; a
//! compiled caller that runs a compiled callee in place does so with
//! telemetry off, where bytecode's prologue has been disabled as well. Each
//! entry point therefore acts only on the frame's own pending hook.

use bex_vm_types::Value;
use btel_types::InvocationMode;

use crate::{
    errors::VmError,
    telemetry::TraceConfig,
    vm::{BexVm, CallTrace},
};

/// A standard mode a declaration applies without a hook call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceHookMode {
    Hidden,
    Timing,
    Span,
    Rich,
    EmptySpan,
}

impl TraceHookMode {
    /// The trace controls the mode's opcode builds.
    pub(crate) fn config(self) -> TraceConfig {
        let mode = match self {
            Self::Hidden => InvocationMode::Hidden,
            Self::Timing => InvocationMode::Timing,
            Self::Span | Self::Rich | Self::EmptySpan => InvocationMode::Span,
        };
        let (inputs, output, error) = match self {
            Self::EmptySpan => (Some(false), Some(false), Some(false)),
            Self::Rich => (Some(true), Some(true), Some(true)),
            Self::Span => (None, Some(true), Some(true)),
            Self::Hidden | Self::Timing => (None, None, None),
        };
        TraceConfig {
            mode: Some(mode),
            inputs,
            output,
            error,
            ..Default::default()
        }
    }
}

impl BexVm {
    /// Whether the call path left a hook pending for the compiled frame at `depth`.
    #[inline]
    fn frame_hook_pending(&self, depth: usize) -> bool {
        self.pending_trace_hooks
            .last()
            .is_some_and(|hook| hook.frame + 1 == depth)
    }

    /// Resolve the pending hook of the frame at `depth` with a standard mode,
    /// as `TraceHookHidden` and the other mode opcodes do.
    #[inline]
    pub fn builtin_trace_hook(
        &mut self,
        depth: usize,
        mode: TraceHookMode,
    ) -> Result<(), Box<VmError>> {
        if !self.frame_hook_pending(depth) {
            return Ok(());
        }
        self.builtin_trace_hook_pending(depth, mode)
    }

    #[cold]
    #[inline(never)]
    fn builtin_trace_hook_pending(
        &mut self,
        depth: usize,
        mode: TraceHookMode,
    ) -> Result<(), Box<VmError>> {
        let returned = (self.telemetry.is_some() && !self.hooks_suppressed()).then(|| CallTrace {
            telemetry: mode.config(),
            context: None,
            reservation: None,
        });
        self.finish_trace_hook_config(depth - 1, returned)
            .map_err(Box::new)
    }

    /// Start the declaration hook of the frame at `depth`, as `BeginTraceHook`
    /// does: the hook's settings (or `true` when it takes none), or null when
    /// the hook is not to run.
    #[inline]
    pub fn begin_trace_hook(
        &mut self,
        depth: usize,
        with_settings: bool,
    ) -> Result<Value, Box<VmError>> {
        if !self.frame_hook_pending(depth) {
            return Ok(Value::NULL);
        }
        self.begin_trace_hook_pending(depth, with_settings)
    }

    #[cold]
    #[inline(never)]
    fn begin_trace_hook_pending(
        &mut self,
        depth: usize,
        with_settings: bool,
    ) -> Result<Value, Box<VmError>> {
        self.begin_trace_hook_in(depth - 1, with_settings)
            .map_err(Box::new)
    }

    /// Apply what the declaration hook of the frame at `depth` returned, as
    /// `EndTraceHook` does.
    #[cold]
    #[inline(never)]
    pub fn finish_trace_hook(&mut self, depth: usize, result: Value) -> Result<(), Box<VmError>> {
        self.finish_trace_hook_in(depth - 1, result)
            .map_err(Box::new)
    }
}
