//! Only VM-entered frames have pending declaration hooks. Direct records do not.
//! Resolve each hook before the compiled body starts, using the interpreter's hook primitives.

use bex_vm_types::Value;
use btel_types::InvocationMode;

use super::FrameRecord;
use crate::{
    errors::VmError,
    telemetry::TraceConfig,
    vm::{BexVm, CallTrace},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceHookMode {
    Hidden,
    Timing,
    Span,
    Rich,
    EmptySpan,
}

impl TraceHookMode {
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
    #[inline]
    fn frame_hook_pending(&self, frame: &FrameRecord) -> Option<usize> {
        let idx = Self::record_vm_frame(frame)?;
        self.pending_trace_hooks
            .last()
            .is_some_and(|hook| hook.frame == idx)
            .then_some(idx)
    }

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

    /// Return hook settings (or true without settings), or null to skip the hook.
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
