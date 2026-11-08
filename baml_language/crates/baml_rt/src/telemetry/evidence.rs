//! Recording-only error stack. Each link is owned by its generated stack frame.
use super::*;
use btel_records::{ErrorFrame, RaiseKind, RaiseStack, UnwindResult};

pub(super) struct EvidenceFrame {
    parent: *mut EvidenceFrame,
    function: usize,
    pc: u32,
    mode: u8,
}
#[derive(Debug)]
pub struct Raise {
    id: TelemetryId,
    unwound: u32,
    frame: usize,
    may_promote: bool,
    depth: u32,
}

#[derive(Debug, Default)]
pub struct History {
    origin: Option<TelemetryId>,
    previous: Option<TelemetryId>,
}

fn origin(state: &mut State, error: &Error, id: TelemetryId) {
    if let Some(origin) = error.telemetry_history.origin {
        state.span(SpanRecord::ErrorRaiseOrigin {
            raise_id: id,
            origin: btel_records::RaiseOrigin::Proven {
                origin,
                previous: error.telemetry_history.previous,
                via: btel_records::OriginVia::Rethrow,
            },
        });
    }
}

/// # Safety
/// The current session owns this stack and `site` belongs to its program.
#[inline(never)]
pub unsafe fn observe<T>(site: usize, mode: u8, call: impl FnOnce() -> R<T>) -> R<T> {
    unsafe { observe_captured(site, mode, InputCaptures::default, |_| None, call) }
}
/// # Safety
/// Same session, site, and callback requirements as `super::observe_captured`.
#[inline(never)]
pub unsafe fn observe_captured<T>(
    site: usize,
    mode: u8,
    inputs: impl FnOnce() -> InputCaptures,
    output: impl FnOnce(&R<T>) -> Option<btel_snapshot::Snapshot>,
    call: impl FnOnce() -> R<T>,
) -> R<T> {
    let ptr = CURRENT.get();
    let state = unsafe { &mut *ptr };
    let site_info = state.sites[site];
    let parent = state.evidence_frame;
    if !parent.is_null() {
        unsafe {
            (*parent).pc = site_info.pc;
        }
    }
    let mut frame = EvidenceFrame {
        parent,
        function: site_info.callee,
        pc: 0,
        mode,
    };
    state.evidence_frame = &mut frame;
    struct Restore(*mut State, *mut EvidenceFrame);
    impl Drop for Restore {
        fn drop(&mut self) {
            unsafe {
                (*self.0).evidence_frame = self.1;
            }
        }
    }
    let restore = Restore(ptr, parent);
    let mut result = match mode {
        0 => call(),
        1 => {
            let timing = unsafe { enter_observed(site) };
            let mut result = call();
            timing.finish(&mut result);
            result
        }
        _ => unsafe { super::observe_captured(site, mode, inputs, output, call) },
    };
    drop(restore);
    if let Err(error) = &mut result
        && let Some(raise) = &mut error.telemetry_raise
    {
        raise.unwound += 1;
        if raise.may_promote && raise.frame == (&raw const frame as usize) {
            unsafe { &mut *ptr }.span(SpanRecord::ErrorRaiseFrameCompleted { raise_id: raise.id });
            raise.may_promote = false;
        }
    }
    result
}

/// Called before the first frame completion; all unwound frames share its clock.
#[cold]
pub fn raised(error: &mut Error, source: &crate::Source, site: usize, native: Option<usize>) {
    let ptr = CURRENT.get();
    if ptr.is_null() || error.telemetry_raise.is_some() {
        return;
    }
    let state = unsafe { &mut *ptr };
    let frame = state.evidence_frame;
    if !state.evidence {
        return;
    }
    if frame.is_null() {
        raised_on_path(state, error, source, site);
        return;
    }
    let pc = source.pcs.get(site).copied().unwrap_or(0);
    unsafe {
        (*frame).pc = pc;
    }
    let kind = source
        .kinds
        .get(site)
        .copied()
        .unwrap_or(RaiseKind::Runtime);
    let mut frames = Vec::new();
    if let Some(function) = native {
        frames.push(ErrorFrame {
            function: Some(state.functions[function]),
            pc: None,
            native: true,
        });
    }
    let mut cursor = frame;
    let mut observed = true;
    let mut frame_count = u32::from(native.is_some());
    while !cursor.is_null() {
        let current = unsafe { &*cursor };
        frame_count += 1;
        observed &= current.mode != 0;
        if frames.len() < btel_records::MAX_ERROR_FRAMES {
            frames.push(ErrorFrame {
                function: Some(state.functions[current.function]),
                pc: Some(current.pc),
                native: false,
            });
        }
        cursor = current.parent;
    }
    let raise_id = allocate_telemetry_id();
    origin(state, error, raise_id);
    if !observed {
        state.span(SpanRecord::ErrorRaiseStack(Box::new(RaiseStack {
            raise_id,
            frames,
            inherited: Vec::new(),
            inherited_count: 0,
        })));
    }
    let raised_at = *error.telemetry_at.get_or_insert_with(|| state.read());
    let may_promote = unsafe { (*frame).mode } == 3;
    state.span(SpanRecord::ErrorRaised {
        raise_id,
        raised_at,
        kind,
        function: Some(state.functions[unsafe { (*frame).function }]),
        pc: Some(pc),
        raise_call: (unsafe { (*frame).mode } == 2)
            .then_some(state.active_span)
            .flatten(),
        raise_frame_may_promote: may_promote,
        call_path: if observed {
            state.active
        } else {
            CallPathId::ROOT
        },
        frame_count,
    });
    error.telemetry_raise = Some(Raise {
        id: raise_id,
        unwound: 0,
        frame: frame as usize,
        may_promote,
        depth: state.depth,
    });
}

#[cold]
pub fn caught(error: &mut Error, pc: u32) {
    end(error, UnwindResult::Caught, Some(pc));
}
#[cold]
pub(super) fn end(error: &mut Error, result: UnwindResult, handler_pc: Option<u32>) {
    let Some(raise) = error.telemetry_raise.take() else {
        return;
    };
    let state = unsafe { &mut *CURRENT.get() };
    error.telemetry_history.origin.get_or_insert(raise.id);
    error.telemetry_history.previous = Some(raise.id);
    let handler_function = handler_pc.map(|_| {
        let function = if state.evidence_frame.is_null() {
            state.active_function.expect("observed handler")
        } else {
            unsafe { (*state.evidence_frame).function }
        };
        state.functions[function]
    });
    state.span(SpanRecord::ErrorUnwindEnded {
        raise_id: raise.id,
        result,
        handler_function,
        handler_pc,
        unwound_frames: raise.unwound,
    });
}

// On fully observed stacks no extra per-call frame or cold helper is needed:
// the active path carries caller PCs, and one depth counter preserves recursion.
#[cold]
fn raised_on_path(state: &mut State, error: &mut Error, source: &crate::Source, site: usize) {
    let Some(function) = state.active_function else {
        return;
    };
    let raise_id = allocate_telemetry_id();
    origin(state, error, raise_id);
    let mode = super::mode(state.words[function]) & 3;
    let raised_at = *error.telemetry_at.get_or_insert_with(|| state.read());
    state.span(SpanRecord::ErrorRaised {
        raise_id,
        raised_at,
        kind: source
            .kinds
            .get(site)
            .copied()
            .unwrap_or(RaiseKind::Runtime),
        function: Some(state.functions[function]),
        pc: source.pcs.get(site).copied(),
        raise_call: if mode == 2 { state.active_span } else { None },
        raise_frame_may_promote: mode == 3,
        call_path: state.active,
        frame_count: state.depth,
    });
    error.telemetry_raise = Some(Raise {
        id: raise_id,
        unwound: 0,
        frame: 0,
        may_promote: mode == 3,
        depth: state.depth,
    });
}
#[inline(always)]
pub(super) fn frame_completed<T>(state: &mut State, result: &mut R<T>) {
    if state.evidence_frame.is_null()
        && let Err(error) = result
        && let Some(raise) = &mut error.telemetry_raise
    {
        raise.unwound += 1;
        if raise.may_promote && raise.depth == state.depth {
            state.span(SpanRecord::ErrorRaiseFrameCompleted { raise_id: raise.id });
            raise.may_promote = false;
        }
    }
}
