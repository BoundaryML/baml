//! Recording-only error stack. Each link is owned by its generated stack frame.
use super::*;
use btel_records::{ErrorFrame, RaiseKind, RaiseStack, UnwindResult};

/// Marks a native builtin's frame that stays on the stack while its callee
/// runs (`reflect.call_any`): the interpreter reports it between the frames.
const NATIVE_FRAME: u8 = u8::MAX;

/// Run `call` beneath a native frame for `function` (a telemetry function
/// index) when error evidence is being recorded frame by frame.
///
/// # Safety
/// Called from generated code on the owner thread with a live session, if any.
pub unsafe fn native_frame<T>(function: usize, site: usize, call: impl FnOnce() -> R<T>) -> R<T> {
    let ptr = CURRENT.get();
    if ptr.is_null() || unsafe { (*ptr).evidence_frame.is_null() } {
        return call();
    }
    let parent = unsafe { (*ptr).evidence_frame };
    // The caller's frame reports the native call site.
    unsafe { (*parent).pc = (*ptr).sites[site].pc };
    let mut frame = EvidenceFrame {
        parent,
        function,
        pc: 0,
        mode: NATIVE_FRAME,
    };
    unsafe { (*ptr).evidence_frame = &mut frame };
    struct Restore(*mut State, *mut EvidenceFrame);
    impl Drop for Restore {
        fn drop(&mut self) {
            unsafe { (*self.0).evidence_frame = self.1 };
        }
    }
    let _restore = Restore(ptr, parent);
    call()
}

pub(super) struct EvidenceFrame {
    parent: *mut EvidenceFrame,
    function: usize,
    pc: u32,
    mode: u8,
}
#[derive(Debug, Clone)]
pub struct Raise {
    id: TelemetryId,
    origin: Option<TelemetryId>,
    unwound: u32,
    frame: usize,
    may_promote: bool,
    depth: u32,
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

fn propagated_origin(
    state: &mut State,
    error: &Error,
    kind: RaiseKind,
    raise_id: TelemetryId,
) -> Option<TelemetryId> {
    let via = match kind {
        RaiseKind::Rethrow => btel_records::OriginVia::Rethrow,
        RaiseKind::Await => btel_records::OriginVia::Await,
        _ => return None,
    };
    if let Some((origin, previous)) = error.telemetry_caught {
        state.span(SpanRecord::ErrorRaiseOrigin {
            raise_id,
            origin: btel_records::RaiseOrigin::Proven {
                origin,
                previous: Some(previous),
                via,
            },
        });
        Some(origin)
    } else {
        if kind == RaiseKind::Await {
            state.span(SpanRecord::ErrorRaiseOrigin {
                raise_id,
                origin: btel_records::RaiseOrigin::Unresolved {
                    reason: btel_records::UnresolvedOrigin::FutureLinkMissing,
                    via,
                },
            });
        }
        None
    }
}

/// Called before the first frame completion; all unwound frames share its clock.
#[cold]
pub fn raised(error: &mut Error, source: &crate::Source, site: usize, native: Option<usize>) {
    raised_with_kind(error, source, site, native, None);
}

/// An erased native adapter receives its caller's immutable telemetry site.
/// Its async completion has no native bytecode source table of its own.
pub fn raised_at_call_site(error: &mut Error, site: usize, kind: RaiseKind) {
    let state = CURRENT.get();
    if state.is_null() {
        return;
    }
    let sites: &'static [super::Site] = unsafe { (*state).sites };
    let Some(site) = sites.get(site) else {
        return;
    };
    let source = crate::Source {
        file: "",
        function: "",
        lines: &[],
        pcs: std::slice::from_ref(&site.pc),
        kinds: &[],
    };
    raised_with_kind(error, &source, 0, None, Some(kind));
}

pub fn raised_with_kind(
    error: &mut Error,
    source: &crate::Source,
    site: usize,
    native: Option<usize>,
    kind_override: Option<RaiseKind>,
) {
    if crate::ADMISSION_CANCELLED.replace(false) {
        return;
    }
    let ptr = CURRENT.get();
    if ptr.is_null() || error.telemetry_raise.is_some() || error.class == "InternalError" {
        return;
    }
    let state = unsafe { &mut *ptr };
    let frame = state.evidence_frame;
    if !state.evidence {
        return;
    }
    if frame.is_null() {
        raised_on_path(state, error, source, site, kind_override);
        return;
    }
    let pc = source.pcs.get(site).copied().unwrap_or(0);
    unsafe {
        (*frame).pc = pc;
    }
    let kind = kind_override.unwrap_or_else(|| {
        source
            .kinds
            .get(site)
            .copied()
            .unwrap_or(RaiseKind::Runtime)
    });
    let kind = if kind == RaiseKind::Await && error.class == "baml.panics.Cancelled" {
        RaiseKind::AwaitCancelled
    } else {
        kind
    };
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
        let native = current.mode == NATIVE_FRAME;
        observed &= current.mode != 0;
        if frames.len() < btel_records::MAX_ERROR_FRAMES {
            frames.push(ErrorFrame {
                function: Some(state.functions[current.function]),
                pc: (!native).then_some(current.pc),
                native,
            });
        }
        cursor = current.parent;
    }
    let raise_id = allocate_telemetry_id();
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
    let origin = propagated_origin(state, error, kind, raise_id);
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
        origin,
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
    if matches!(result, UnwindResult::Caught | UnwindResult::Unhandled) {
        error.telemetry_caught = Some((raise.origin.unwrap_or(raise.id), raise.id));
    }
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
fn raised_on_path(
    state: &mut State,
    error: &mut Error,
    source: &crate::Source,
    site: usize,
    kind_override: Option<RaiseKind>,
) {
    let Some(function) = state.active_function else {
        return;
    };
    let raise_id = allocate_telemetry_id();
    let mode = super::mode(state.words[function]) & 3;
    let raised_at = *error.telemetry_at.get_or_insert_with(|| state.read());
    let kind = kind_override.unwrap_or_else(|| {
        source
            .kinds
            .get(site)
            .copied()
            .unwrap_or(RaiseKind::Runtime)
    });
    let kind = if kind == RaiseKind::Await && error.class == "baml.panics.Cancelled" {
        RaiseKind::AwaitCancelled
    } else {
        kind
    };
    let origin = propagated_origin(state, error, kind, raise_id);
    state.span(SpanRecord::ErrorRaised {
        raise_id,
        raised_at,
        kind,
        function: Some(state.functions[function]),
        pc: source.pcs.get(site).copied(),
        raise_call: if mode == 2 { state.active_span } else { None },
        raise_frame_may_promote: mode == 3,
        call_path: state.active,
        frame_count: state.depth,
    });
    error.telemetry_raise = Some(Raise {
        id: raise_id,
        origin,
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

/// End producer-side evidence before transferring a future's saved error.
pub(crate) fn unhandled(error: &mut Error) {
    end(error, UnwindResult::Unhandled, None);
}
/// A defer's rethrow preserves the language context and starts a new raise.
pub fn rethrow(mut error: Box<Error>, source: &'static crate::Source, site: usize) -> Box<Error> {
    error.telemetry_at = None;
    raised(&mut error, source, site, None);
    error
}
