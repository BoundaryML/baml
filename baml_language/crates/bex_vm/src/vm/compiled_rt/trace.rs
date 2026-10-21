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
//! compiled caller can also run an unhooked callee in place with telemetry on.
//! Each entry point therefore acts only on the frame's own pending hook.

use bex_vm_types::Value;
use btel_types::InvocationMode;

use super::FrameRecord;
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
    /// The VM frame of the body with record `frame` when the call path left
    /// a hook pending for it. Only the call path pushes hooked frames, so a
    /// record of a directly called body never has one.
    #[inline]
    fn frame_hook_pending(&self, frame: &FrameRecord) -> Option<usize> {
        let idx = Self::record_vm_frame(frame)?;
        self.pending_trace_hooks
            .last()
            .is_some_and(|hook| hook.frame == idx)
            .then_some(idx)
    }

    /// Resolve the pending hook of the frame of `frame` with a standard mode,
    /// as `TraceHookHidden` and the other mode opcodes do.
    #[inline]
    pub fn builtin_trace_hook(
        &mut self,
        frame: &FrameRecord,
        mode: TraceHookMode,
    ) -> Result<(), Box<VmError>> {
        let Some(idx) = self.frame_hook_pending(frame) else {
            return Ok(());
        };
        self.builtin_trace_hook_pending(idx, mode)
    }

    #[cold]
    #[inline(never)]
    fn builtin_trace_hook_pending(
        &mut self,
        idx: usize,
        mode: TraceHookMode,
    ) -> Result<(), Box<VmError>> {
        let returned = (self.telemetry.is_some() && !self.hooks_suppressed()).then(|| CallTrace {
            telemetry: mode.config(),
            context: None,
            reservation: None,
        });
        self.finish_trace_hook_config(idx, returned)
            .map_err(Box::new)
    }

    /// Start the declaration hook of the frame of `frame`, as `BeginTraceHook`
    /// does: the hook's settings (or `true` when it takes none), or null when
    /// the hook is not to run.
    #[inline]
    pub fn compiled_begin_trace_hook(
        &mut self,
        frame: &FrameRecord,
        with_settings: bool,
    ) -> Result<Value, Box<VmError>> {
        let Some(idx) = self.frame_hook_pending(frame) else {
            return Ok(Value::NULL);
        };
        self.begin_trace_hook_pending(idx, with_settings)
    }

    #[cold]
    #[inline(never)]
    fn begin_trace_hook_pending(
        &mut self,
        idx: usize,
        with_settings: bool,
    ) -> Result<Value, Box<VmError>> {
        self.begin_trace_hook(idx, with_settings).map_err(Box::new)
    }

    /// Apply what the declaration hook of the frame of `frame` returned, as
    /// `EndTraceHook` does.
    #[cold]
    #[inline(never)]
    pub fn compiled_finish_trace_hook(
        &mut self,
        frame: &FrameRecord,
        result: Value,
    ) -> Result<(), Box<VmError>> {
        let idx = Self::record_vm_frame(frame).expect("a hooked body has a VM frame");
        self.finish_trace_hook(idx, result).map_err(Box::new)
    }
}
