//! Loop-owned telemetry. Frames and site caches contain no managed pointers.
use crate::{Error, R};
use bex_chunkedringbuffer::{ChunkPool, Producer};
use btel_clock::{ClockEpoch, ClockRuntime};
use btel_processor::{NoSinkPublisher, Processor};
use btel_records::{AggregateDelta, SpanRecord, TimingRecord};
use btel_snapshot::Snapshot;
use btel_types::{
    AwaitDuration, CallPathEdge, CallPathId, CallPathNodeId, ClockInstant, FunctionId,
    InvocationOutcome, TelemetryId, allocate_telemetry_id,
};
use std::{
    cell::Cell,
    collections::HashMap,
    num::NonZeroUsize,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
};

pub mod capture;
pub mod evidence;
pub use btel_records::RaiseKind;
pub use btel_types as types;
pub type Policy = btel_settings::policy::TelemetryPolicy<std::time::Duration>;
pub struct Function {
    pub captures: Option<&'static capture::Metadata>,
    pub metadata: fn(FunctionId) -> btel_types::FunctionMetadata,
    pub word: &'static AtomicU8,
    pub hidden: bool,
    pub policy: Policy,
}
#[derive(Clone, Copy)]
pub struct Site {
    pub caller: Option<usize>,
    pub pc: u32,
    pub callee: usize,
}
#[derive(Clone, Copy, Default)]
struct Cache {
    parent: Option<CallPathId>,
    path: CallPathId,
    slot: *mut AggregateDelta,
}
#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct Key {
    parent: CallPathId,
    caller: Option<usize>,
    pc: u32,
    callee: usize,
}
type Span = SpanRecord<Snapshot, Snapshot>;
type Pool = ChunkPool<TimingRecord, Span>;
thread_local! { static CURRENT: Cell<*mut State> = const { Cell::new(std::ptr::null_mut()) }; }

pub(crate) fn current_raw() -> *mut () {
    CURRENT.get().cast()
}
/// The pointer is null or an owner-thread state that outlives the binding.
pub(crate) unsafe fn replace_current_raw(state: *mut ()) {
    CURRENT.set(state.cast());
}

#[derive(Clone, Copy)]
enum DeltaClock {
    Epoch,
    #[cfg(target_arch = "aarch64")]
    OrderedCounter,
    #[cfg(all(target_arch = "aarch64", target_os = "linux"))]
    SelfSynchronizedCounter,
}
impl DeltaClock {
    fn for_epoch(clock: &ClockEpoch) -> Self {
        #[cfg(target_arch = "aarch64")]
        if clock.metadata().source == btel_clock::Source::SystemCounter {
            #[cfg(target_os = "linux")]
            if unsafe { libc::getauxval(libc::AT_HWCAP2) } & (1 << 19) != 0 {
                return Self::SelfSynchronizedCounter;
            }
            return Self::OrderedCounter;
        }
        let _ = clock;
        Self::Epoch
    }
}

struct State {
    capture_metadata: Option<&'static capture::Metadata>,
    capture_scratch: Option<capture::Scratch>,
    evidence: bool,
    depth: u32,
    evidence_frame: *mut evidence::EvidenceFrame,
    producer: Producer<TimingRecord, Span>,
    clock: Arc<ClockEpoch>,
    functions: Vec<FunctionId>,
    names: Vec<btel_records::ThreadName>,
    words: Vec<&'static AtomicU8>,
    name: Option<btel_records::ThreadName>,
    sites: &'static [Site],
    caches: Vec<Cache>,
    paths: HashMap<Key, (CallPathId, *mut AggregateDelta)>,
    histories: Vec<Vec<Cache>>,
    // Stable addresses let on-stack frames update their slot after nested calls
    // grow the table. These pointers never address the managed heap.
    #[allow(clippy::vec_box, reason = "stable addresses held by frames and caches")]
    slots: Vec<Box<[AggregateDelta; 2]>>,
    reader: DeltaClock,
    active_span: Option<TelemetryId>,
    policies: Vec<Policy>,
    active: CallPathId,
    active_function: Option<usize>,
    active_slot: *mut AggregateDelta,
    next_path: u32,
    gc_function: FunctionId,
    gc_started: Option<ClockInstant>,
    thread: TelemetryId,
    started: ClockInstant,
}
impl State {
    #[inline(always)]
    fn read(&self) -> ClockInstant {
        match self.reader {
            DeltaClock::Epoch => self.clock.read(),
            #[cfg(target_arch = "aarch64")]
            DeltaClock::OrderedCounter => {
                let ticks;
                unsafe {
                    std::arch::asm!("isb", "mrs {}, cntvct_el0", out(reg) ticks, options(nostack, preserves_flags));
                }
                ClockInstant::from_ticks(ticks)
            }
            #[cfg(all(target_arch = "aarch64", target_os = "linux"))]
            DeltaClock::SelfSynchronizedCounter => {
                let ticks;
                // FEAT_ECV is checked once. Same count/domain as Quanta's
                // CNTVCT_EL0, but reads occur in program order without an ISB.
                unsafe {
                    std::arch::asm!("mrs {}, S3_3_C14_C0_6", out(reg) ticks, options(nostack, preserves_flags));
                }
                ClockInstant::from_ticks(ticks)
            }
        }
    }
    fn span(&mut self, record: Span) {
        if self.producer.span_remaining() < 3 {
            self.producer.seal();
        }
        if self.producer.span_is_empty() {
            self.producer.write_span(SpanRecord::ThreadSelected {
                thread_id: self.thread,
            });
            self.producer.write_span(SpanRecord::ContextCleared);
        }
        self.producer.write_span(record);
    }

    fn flush(&mut self) {
        let mut batch = Vec::new();
        for slot in self.slots.iter_mut().flat_map(|pair| pair.iter_mut()) {
            if slot.count != 0 {
                batch.push(*slot);
                slot.count = 0;
                slot.total_duration = Default::default();
                slot.total_self_await = AwaitDuration::ZERO;
                slot.errored = 0;
                slot.cancelled = 0;
                slot.panicked = 0;
            }
        }
        if !batch.is_empty() {
            self.producer
                .write_timing(TimingRecord::AggregateDeltas(batch.into_boxed_slice()));
        }
        self.producer.seal();
    }
    #[cold]
    fn path(&mut self, site_id: usize) -> (CallPathId, *mut AggregateDelta) {
        // The one-entry cache is checked by the prologue. Alternating parents
        // reuse this compact site-local history, without hashing an existing edge.
        if let Some(cache) = self.histories[site_id]
            .iter()
            .find(|cache| cache.parent == Some(self.active))
        {
            return (cache.path, cache.slot);
        }
        let site = self.sites[site_id];
        let key = Key {
            parent: self.active,
            caller: site.caller,
            pc: site.pc,
            callee: site.callee,
        };
        if let Some(&(path, slot)) = self.paths.get(&key) {
            self.histories[site_id].push(Cache {
                parent: Some(self.active),
                path,
                slot,
            });
            return (path, slot);
        }
        self.next_path = self.next_path.checked_add(1).expect("call path exhausted");
        let path = CallPathId::new_non_root(self.next_path).unwrap();
        let mut pair = Box::new([false, true].map(|reentry| AggregateDelta {
            node: CallPathNodeId::new(path, reentry),
            ..Default::default()
        }));
        let slot = pair.as_mut_ptr();
        self.slots.push(pair);
        self.histories[site_id].push(Cache {
            parent: Some(self.active),
            path,
            slot,
        });
        self.paths.insert(key, (path, slot));
        self.span(SpanRecord::CallPathDefined {
            call_path: path,
            parent_call_path: self.active,
            visible_caller: site.caller.map(|f| self.functions[f]),
            caller_pc: site.pc,
            callee: self.functions[site.callee],
            edge: CallPathEdge::Synchronous,
        });
        (path, slot)
    }
}

/// Explicit completion, including errors, is required; Drop does not guess outcomes.
pub struct Frame {
    state: *mut State,
    entered_at: ClockInstant,
    saved_path: CallPathId,
    saved_function: Option<usize>,
    saved_slot: *mut AggregateDelta,
    slot: *mut AggregateDelta,
    pub await_duration: AwaitDuration,
    flags: u8,
}
/// # Safety
/// Called only within a Session's current task, with a compiler-emitted site.
#[inline(always)]
pub unsafe fn enter(word: &AtomicU8, site_id: usize) -> Option<Frame> {
    enabled(word).then(|| unsafe { enter_observed(site_id) })
}
#[inline(always)]
pub fn enabled(word: &AtomicU8) -> bool {
    mode(word) != 0
}
#[inline(always)]
pub fn mode(word: &AtomicU8) -> u8 {
    word.load(Ordering::Relaxed)
}

/// Global off has no producer installed. A hidden/unobserved function at low
/// still has one, so its descendants must keep their own observation policies.
#[inline(always)]
pub fn active() -> bool {
    !CURRENT.get().is_null()
}
/// # Safety
/// A session must own the current task, and the caller must finish this frame
/// before leaving that task. The site must belong to the compiled program.
#[inline(always)]
pub unsafe fn enter_observed(site_id: usize) -> Frame {
    let ptr = CURRENT.get();
    let state = unsafe { &mut *ptr };
    let site = unsafe { *state.sites.get_unchecked(site_id) };
    let entered_at = state.read();
    let reentry = site.caller == Some(site.callee) && state.active_function == Some(site.callee);
    let (path, slot) = if reentry {
        // The active node's adjacent slot is its reentry population.
        let base = state.active_slot;
        (state.active, unsafe { base.add(1) })
    } else {
        let cache = unsafe { *state.caches.get_unchecked(site_id) };
        if cache.parent == Some(state.active) {
            (cache.path, cache.slot)
        } else {
            let (path, slot) = state.path(site_id);
            unsafe {
                *state.caches.get_unchecked_mut(site_id) = Cache {
                    parent: Some(state.active),
                    path,
                    slot,
                };
            }
            (path, slot)
        }
    };
    let frame = Frame {
        state: ptr,
        entered_at,
        saved_path: state.active,
        saved_function: state.active_function,
        saved_slot: state.active_slot,
        slot,
        await_duration: AwaitDuration::ZERO,
        flags: u8::from(reentry),
    };
    state.depth += 1;
    state.active = path;
    state.active_slot = if reentry {
        unsafe { slot.sub(1) }
    } else {
        slot
    };
    state.active_function = Some(site.callee);
    frame
}
impl Frame {
    #[inline(always)]
    pub fn finish<T>(self, result: &mut R<T>) {
        let state = unsafe { &mut *self.state };
        let (exited_at, outcome) = match result {
            Ok(_) => (state.read(), InvocationOutcome::Ok),
            Err(error) => (
                *error.telemetry_at.get_or_insert_with(|| state.read()),
                outcome(error),
            ),
        };
        // Only two integer additions on a successful synchronous call. Keep
        // outcome/await handling and overflow transfer out of the common path.
        let slot = unsafe { &mut *self.slot };
        let duration = self.entered_at.elapsed_until(exited_at);
        debug_assert_eq!(slot.node.is_reentry(), self.flags != 0);
        if !(outcome == InvocationOutcome::Ok
            && self.await_duration == AwaitDuration::ZERO
            && add_ok(slot, duration))
        {
            complete_unusual(state, self.slot, duration, self.await_duration, outcome);
        }
        evidence::frame_completed(state, result);
        state.depth -= 1;
        state.active = self.saved_path;
        state.active_function = self.saved_function;
        state.active_slot = self.saved_slot;
    }
}
#[derive(Default)]
pub struct InputCaptures {
    pub inputs: Option<btel_snapshot::Snapshot>,
    pub types: Option<btel_snapshot::Snapshot>,
}
fn with_capture<T>(f: impl FnOnce(&mut capture::Scratch, &capture::Metadata) -> T) -> T {
    let pointer = CURRENT.get();
    assert!(!pointer.is_null(), "capture requires an active session");
    let state = unsafe { &mut *pointer };
    let metadata = state.capture_metadata.expect("compiler capture metadata");
    f(
        state.capture_scratch.get_or_insert_with(Default::default),
        metadata,
    )
}
/// # Safety
/// Values must be live objects of the current session throughout capture.
pub unsafe fn capture_arguments(values: &[crate::Dyn]) -> InputCaptures {
    InputCaptures {
        inputs: Some(with_capture(|s, m| unsafe {
            s.arguments(m.shapes, values)
        })),
        types: None,
    }
}
/// # Safety
/// The value must be live in the current session throughout capture.
pub unsafe fn capture_value(value: crate::Dyn) -> Option<btel_snapshot::Snapshot> {
    Some(with_capture(|s, m| unsafe { s.value(m.shapes, value) }))
}
/// # Safety
/// The error payload must be live in the current session.
pub unsafe fn capture_error(error: &Error) -> Option<btel_snapshot::Snapshot> {
    if error.value == 0 {
        return None;
    }
    let desc = unsafe { (*(error.value as *const baml_rt_gc::Header)).desc };
    unsafe {
        capture_value(crate::Dyn {
            tag: desc as u64 + 3,
            payload: error.value as u64,
        })
    }
}
pub fn capture_text(value: &str) -> Option<btel_snapshot::Snapshot> {
    Some(with_capture(|s, _| s.text(value)))
}
pub fn capture_type_args(args: &[(&str, &capture::Type)]) -> Option<btel_snapshot::Snapshot> {
    Some(with_capture(|s, _| s.type_args(args)))
}
/// Span and promotion work is kept out of the ordinary timing frame.
/// # Safety
/// The same session and site requirements as `enter_observed` apply.
#[inline(never)]
pub unsafe fn observe<T>(site: usize, mode: u8, call: impl FnOnce() -> R<T>) -> R<T> {
    unsafe { observe_captured(site, mode, InputCaptures::default, |_| None, call) }
}
/// # Safety
/// The same session and site requirements as `enter_observed` apply. Capture
/// callbacks must not run managed code or trigger collection.
#[inline(never)]
pub unsafe fn observe_captured<T>(
    site: usize,
    mode: u8,
    inputs: impl FnOnce() -> InputCaptures,
    output: impl FnOnce(&R<T>) -> Option<btel_snapshot::Snapshot>,
    call: impl FnOnce() -> R<T>,
) -> R<T> {
    let policy = unsafe {
        let s = &*CURRENT.get();
        s.policies[s.sites[site].callee]
    };
    let captured = if mode == 2 && policy.capture_inputs {
        inputs()
    } else {
        InputCaptures::default()
    };
    let needs_announcement = captured.inputs.is_some() || captured.types.is_some();
    let frame = unsafe { enter_observed(site) };
    let state = unsafe { &mut *frame.state };
    let saved_span = state.active_span;
    let parent_id = saved_span.unwrap_or(state.thread);
    let id = (mode == 2).then(allocate_telemetry_id);
    let call_path = state.active;
    let entered_at = frame.entered_at;
    if mode == 2 {
        let id = id.unwrap();
        state.active_span = Some(id);
        state.span(SpanRecord::FunctionSpanAnnouncement {
            id,
            parent_id,
            call_path,
            entered_at,
            captured_inputs: captured.inputs,
            captured_type_args: captured.types,
        });
    }
    let mut result = call();
    let state = unsafe { &mut *frame.state };
    let (exited_at, outcome) = match &mut result {
        Ok(_) => (state.read(), InvocationOutcome::Ok),
        Err(error) => (
            *error.telemetry_at.get_or_insert_with(|| state.read()),
            outcome(error),
        ),
    };
    let duration = entered_at.elapsed_until(exited_at);
    let promoted = mode == 2
        || policy.span_from_entry
        || policy.promote_errors && outcome.is_error()
        || policy
            .promotion_duration_threshold
            .is_some_and(|threshold| {
                state
                    .clock
                    .threshold(threshold)
                    .reached(duration, state.clock.domain())
            });
    if promoted {
        let captured_value = if (result.is_ok() && policy.capture_output)
            || (result.is_err() && policy.capture_error)
        {
            output(&result)
        } else {
            None
        };
        let state = unsafe { &mut *frame.state };
        let id = id.unwrap_or_else(allocate_telemetry_id);
        let await_time = frame.await_duration;
        macro_rules! completion {
            ($variant:ident) => {
                SpanRecord::$variant {
                    id,
                    parent_id,
                    call_path,
                    entered_at,
                    exited_at,
                    await_time,
                    captured_value,
                }
            };
        }
        let record = if mode == 2 && needs_announcement {
            match (frame.flags != 0, outcome) {
                (false, InvocationOutcome::Ok) => {
                    completion!(FunctionSpanCompletionOkNeedsAnnouncement)
                }
                (false, InvocationOutcome::Errored) => {
                    completion!(FunctionSpanCompletionErroredNeedsAnnouncement)
                }
                (false, InvocationOutcome::Cancelled) => {
                    completion!(FunctionSpanCompletionCancelledNeedsAnnouncement)
                }
                (false, InvocationOutcome::Panicked) => {
                    completion!(FunctionSpanCompletionPanickedNeedsAnnouncement)
                }
                (true, InvocationOutcome::Ok) => {
                    completion!(FunctionSpanCompletionOkReentryNeedsAnnouncement)
                }
                (true, InvocationOutcome::Errored) => {
                    completion!(FunctionSpanCompletionErroredReentryNeedsAnnouncement)
                }
                (true, InvocationOutcome::Cancelled) => {
                    completion!(FunctionSpanCompletionCancelledReentryNeedsAnnouncement)
                }
                (true, InvocationOutcome::Panicked) => {
                    completion!(FunctionSpanCompletionPanickedReentryNeedsAnnouncement)
                }
            }
        } else {
            match (mode == 2, frame.flags != 0, outcome) {
                (true, false, InvocationOutcome::Ok) => completion!(FunctionSpanCompletionOk),
                (true, false, InvocationOutcome::Errored) => {
                    completion!(FunctionSpanCompletionErrored)
                }
                (true, false, InvocationOutcome::Cancelled) => {
                    completion!(FunctionSpanCompletionCancelled)
                }
                (true, false, InvocationOutcome::Panicked) => {
                    completion!(FunctionSpanCompletionPanicked)
                }
                (true, true, InvocationOutcome::Ok) => completion!(FunctionSpanCompletionOkReentry),
                (true, true, InvocationOutcome::Errored) => {
                    completion!(FunctionSpanCompletionErroredReentry)
                }
                (true, true, InvocationOutcome::Cancelled) => {
                    completion!(FunctionSpanCompletionCancelledReentry)
                }
                (true, true, InvocationOutcome::Panicked) => {
                    completion!(FunctionSpanCompletionPanickedReentry)
                }
                (false, false, InvocationOutcome::Ok) => completion!(LateFunctionSpanCompletionOk),
                (false, false, InvocationOutcome::Errored) => {
                    completion!(LateFunctionSpanCompletionErrored)
                }
                (false, false, InvocationOutcome::Cancelled) => {
                    completion!(LateFunctionSpanCompletionCancelled)
                }
                (false, false, InvocationOutcome::Panicked) => {
                    completion!(LateFunctionSpanCompletionPanicked)
                }
                (false, true, InvocationOutcome::Ok) => {
                    completion!(LateFunctionSpanCompletionOkReentry)
                }
                (false, true, InvocationOutcome::Errored) => {
                    completion!(LateFunctionSpanCompletionErroredReentry)
                }
                (false, true, InvocationOutcome::Cancelled) => {
                    completion!(LateFunctionSpanCompletionCancelledReentry)
                }
                (false, true, InvocationOutcome::Panicked) => {
                    completion!(LateFunctionSpanCompletionPanickedReentry)
                }
            }
        };
        state.span(record);
    } else {
        complete_unusual(state, frame.slot, duration, frame.await_duration, outcome);
    }
    let state = unsafe { &mut *frame.state };
    evidence::frame_completed(state, &mut result);
    state.depth -= 1;
    state.active_span = saved_span;
    state.active = frame.saved_path;
    state.active_function = frame.saved_function;
    state.active_slot = frame.saved_slot;
    result
}

/// # Safety
/// The same session and site requirements as `enter_observed` apply.
pub unsafe fn observed_call<T>(word: &AtomicU8, site: usize, call: impl FnOnce() -> R<T>) -> R<T> {
    unsafe { observed_captured(word, site, InputCaptures::default, |_| None, call) }
}
/// # Safety
/// Same session, site, and callback requirements as `observe_captured`.
pub unsafe fn observed_captured<T>(
    word: &AtomicU8,
    site: usize,
    inputs: impl FnOnce() -> InputCaptures,
    output: impl FnOnce(&R<T>) -> Option<btel_snapshot::Snapshot>,
    call: impl FnOnce() -> R<T>,
) -> R<T> {
    let mode = mode(word);
    if mode == 0 {
        return call();
    }
    if mode > 3 {
        return unsafe { evidence::observe_captured(site, mode & 3, inputs, output, call) };
    }
    match mode {
        1 => {
            let frame = unsafe { enter_observed(site) };
            let mut result = call();
            frame.finish(&mut result);
            result
        }
        mode => unsafe { observe_captured(site, mode, inputs, output, call) },
    }
}

#[inline(always)]
fn add_ok(slot: &mut AggregateDelta, duration: btel_types::ClockDuration) -> bool {
    let Some(count) = slot.count.checked_add(1) else {
        return false;
    };
    let Some(ticks) = slot.total_duration.get().checked_add(duration.get()) else {
        return false;
    };
    slot.count = count;
    slot.total_duration = btel_types::ClockDuration::from_ticks(ticks);
    true
}

#[cold]
fn complete_unusual(
    state: &mut State,
    slot: *mut AggregateDelta,
    duration: btel_types::ClockDuration,
    await_duration: AwaitDuration,
    outcome: InvocationOutcome,
) {
    let slot = unsafe { &mut *slot };
    let sample = AggregateDelta {
        node: slot.node,
        count: 1,
        total_duration: duration,
        total_self_await: await_duration,
        errored: u64::from(outcome.is_error()),
        cancelled: u64::from(outcome == InvocationOutcome::Cancelled),
        panicked: u64::from(outcome == InvocationOutcome::Panicked),
    };
    if let Some(sum) = slot.checked_merge(sample) {
        *slot = sum;
    } else {
        state.producer.write_timing(TimingRecord::AggregateDeltas(
            vec![*slot].into_boxed_slice(),
        ));
        *slot = sample;
    }
}

fn outcome(error: &Error) -> InvocationOutcome {
    if error.class == "baml.panics.Cancelled" {
        InvocationOutcome::Cancelled
    } else if error.class.starts_with("baml.panics.") || error.value == 0 {
        InvocationOutcome::Panicked
    } else {
        InvocationOutcome::Errored
    }
}
/// Flush before the task loop can suspend the current stack.
pub fn before_yield() {
    let ptr = CURRENT.get();
    if !ptr.is_null() {
        unsafe { &mut *ptr }.flush();
    }
}

fn gc_metadata(function_id: FunctionId) -> btel_types::FunctionMetadata {
    use btel_types::{FunctionMetadata, RuntimeFunctionKind, RuntimeFunctionOrigin};
    FunctionMetadata {
        function_id,
        fqn: "baml.gc".into(),
        display_name: "baml.gc".into(),
        source_file: None,
        source_span: None,
        kind: RuntimeFunctionKind::Native,
        origin: RuntimeFunctionOrigin::Builtin,
        owner_type: None,
        parent_function: None,
        lambda_path: None,
        definition_key: None,
        package_name: Some("baml".into()),
        namespace: vec![],
        argument_layout: None,
        source_map: None,
        model_invoke: false,
    }
}

/// Publish collector facts only after marking finishes; never traverse managed values.
pub fn collection(event: baml_rt_gc::CollectionEvent) {
    use baml_rt_gc::CollectionEvent;
    use btel_types::context::{Context, ContextPatch, ContextValue};
    let ptr = CURRENT.get();
    if ptr.is_null() {
        return;
    }
    let state = unsafe { &mut *ptr };
    let (live_slots, reclaimed_slots, live_payload_bytes) = match event {
        CollectionEvent::Started => {
            state.gc_started = Some(state.read());
            return;
        }
        CollectionEvent::Completed {
            live_slots,
            reclaimed_slots,
            live_payload_bytes,
        } => (live_slots, reclaimed_slots, live_payload_bytes),
    };
    let completed_at = state.read();
    let started_at = state.gc_started.take().expect("collection started");
    let mut patch = ContextPatch::default();
    for (key, value) in [("collection_level", "major"), ("trigger", "automatic")] {
        patch
            .metadata
            .insert(key.into(), Some(ContextValue::String(value.into())));
    }
    // This non-generational, non-moving collector performs no promotions.
    for (key, count) in [
        ("live_slots", live_slots),
        ("reclaimed_slots", reclaimed_slots),
        ("live_payload_bytes", live_payload_bytes),
        ("promoted_to_gen1", 0),
        ("promoted_to_gen2", 0),
    ] {
        patch.metadata.insert(
            key.into(),
            Some(ContextValue::Int(i64::try_from(count).unwrap())),
        );
    }
    let captured_context = btel_snapshot::context::capture(
        &Context::default().with_patch(&patch),
        &btel_snapshot::SnapshotPool::unbounded(Default::default()),
    );
    let thread = allocate_telemetry_id();
    state.next_path = state.next_path.checked_add(1).expect("call path exhausted");
    let path = CallPathId::new_non_root(state.next_path).unwrap();
    let name = Some(Arc::new("baml.gc".into()));
    // Fresh chunk holds the complete context and operation, with no interleaved
    // user records. The ordinary writer reselects the task on its next chunk.
    state.producer.seal();
    for record in [
        SpanRecord::ThreadSelected { thread_id: thread },
        SpanRecord::ContextSelected { captured_context },
        SpanRecord::CallPathDefined {
            call_path: path,
            parent_call_path: CallPathId::ROOT,
            visible_caller: None,
            caller_pc: 0,
            callee: state.gc_function,
            edge: CallPathEdge::Spawn,
        },
        SpanRecord::ThreadSpanAnnouncement {
            id: thread,
            parent_id: None,
            spawn_call_path: path,
            started_at,
            clock: state.clock.clone(),
            name: name.clone(),
        },
        SpanRecord::ThreadSpanCompletion {
            id: thread,
            parent_id: None,
            spawn_call_path: path,
            started_at,
            completed_at,
            outcome: InvocationOutcome::Ok,
            clock: state.clock.clone(),
            name,
        },
    ] {
        state.producer.write_span(record);
    }
    state.producer.seal();
}

pub struct Session {
    state: Option<Box<State>>,
    pool: Option<Pool>,
    worker: Option<std::thread::JoinHandle<()>>,
    delivery: Option<Arc<btel_file::LocalDelivery>>,
    clocks: ClockRuntime,
    exit: Arc<btel_types::ProcessExitSlot>,
}
impl Session {
    pub fn new(functions: &'static [Function], sites: &'static [Site]) -> R<Self> {
        let level = btel_settings::mode::from_env().map_err(|e| Error::internal(e.to_string()))?;
        let evidence = level.is_some() && recording_root().is_some();
        let words = functions
            .iter()
            .map(|function| {
                let word = observation(level, function.hidden, function.policy);
                Ok(word)
            })
            .collect::<R<Vec<_>>>()?;
        // If every function is observed, its call path already carries the
        // stack. Only low/hidden observations need recording-only stack links.
        let explicit_stack = evidence && words.contains(&0);
        for (function, word) in functions.iter().zip(words) {
            function
                .word
                .store(word | if explicit_stack { 4 } else { 0 }, Ordering::Relaxed);
        }
        let clocks = ClockRuntime::new(btel_settings::clock::DEFAULT_MODE);
        let exit = Arc::new(btel_types::ProcessExitSlot::default());
        let mut session = Self {
            state: None,
            pool: None,
            worker: None,
            delivery: None,
            clocks,
            exit,
        };
        if level.is_none() {
            return Ok(session);
        }
        let pool = Pool::new(bex_chunkedringbuffer::Config::for_producers(
            NonZeroUsize::new(1).unwrap(),
        ))
        .map_err(|e| Error::internal(format!("telemetry transport: {e:?}")))?;
        let consumer_pool = pool.clone();
        let allocator = btel_types::FunctionIdAllocator::default();
        let ids: Vec<_> = functions
            .iter()
            .map(|_| allocator.allocate().unwrap())
            .collect();
        let gc_function = allocator.allocate().unwrap();
        let mut metadata = btel_types::FunctionMetadataTable {
            functions: functions
                .iter()
                .zip(&ids)
                .map(|(f, &id)| (f.metadata)(id))
                .collect(),
        };
        metadata.functions.push(gc_metadata(gc_function));
        let metadata = Arc::new(metadata);
        let names = metadata
            .functions
            .iter()
            .map(|f| Arc::new(f.fqn.clone().into_boxed_str()))
            .collect();
        let worker = if let Some(root) = recording_root() {
            let id = btel_types::RecordingId::generate();
            let delivery = btel_file::LocalDelivery::create_with_cas(
                &root.join("recordings"),
                &root.join("cas"),
                id,
                Default::default(),
                |_| {},
            )
            .map_err(|e| Error::internal(e.to_string()))?;
            let builder = btel_recorder::RecordingBuilder::new(id, Default::default())
                .map_err(|e| Error::internal(e.to_string()))?
                .with_function_metadata(metadata)
                .with_process(btel_recorder::ProcessRecording {
                    info: btel_types::ProcessInfo {
                        process_id: *id.as_bytes(),
                        baml_version: baml_version::CANONICAL_VERSION.into(),
                        host: "pack".into(),
                        ..Default::default()
                    },
                    sources: None,
                    context: btel_snapshot::context::capture(
                        &Default::default(),
                        &btel_snapshot::SnapshotPool::unbounded(Default::default()),
                    ),
                    exit: session.exit.clone(),
                });
            let publisher = btel_file::LocalPublisher::new(builder, delivery);
            session.delivery = publisher.delivery().cloned();
            std::thread::spawn(move || {
                Processor::with_publisher(
                    consumer_pool.bind_consumer().unwrap(),
                    NonZeroUsize::new(8).unwrap(),
                    publisher,
                )
                .run()
                .expect("telemetry processor")
            })
        } else {
            std::thread::spawn(move || {
                Processor::with_publisher(
                    consumer_pool.bind_consumer().unwrap(),
                    NonZeroUsize::new(8).unwrap(),
                    NoSinkPublisher::default(),
                )
                .run()
                .expect("telemetry processor")
            })
        };
        let clock = session.clocks.start_run();
        session.state = Some(Box::new(State {
            capture_metadata: functions.first().and_then(|f| f.captures),
            capture_scratch: None,
            evidence,
            depth: 0,
            evidence_frame: std::ptr::null_mut(),
            producer: pool.register_producer().unwrap(),
            clock: clock.clone(),
            functions: ids,
            names,
            words: functions.iter().map(|f| f.word).collect(),
            name: None,
            sites,
            caches: vec![Cache::default(); sites.len()],
            paths: HashMap::new(),
            histories: vec![Vec::new(); sites.len()],
            reader: DeltaClock::for_epoch(&clock),
            slots: Vec::new(),
            active_span: None,
            policies: functions.iter().map(|f| f.policy).collect(),
            active: CallPathId::ROOT,
            active_function: None,
            active_slot: std::ptr::null_mut(),
            next_path: 0,
            gc_function,
            gc_started: None,
            thread: allocate_telemetry_id(),
            started: ClockInstant::from_ticks(0),
        }));
        session.pool = Some(pool);
        session.worker = Some(worker);
        Ok(session)
    }
    pub fn run<T>(&mut self, call: impl FnOnce() -> R<T>) -> R<T> {
        self.run_at(0, call)
    }
    pub fn format_int(&mut self, value: i64, site: usize) -> R<String> {
        let Some(state) = &self.state else {
            return Ok(value.to_string());
        };
        let word = state.words[state.sites[site].callee];
        let alias_word = state.words[state.sites[site + 1].callee];
        let naming_tag = state.capture_metadata.map_or(0, |m| m.naming_tag);
        self.run_at(site, || {
            // Typed specialization of stringify(int, Declared, 0, false):
            // resolve the naming mode, then encode the scalar without a heap.
            unsafe {
                observed_captured(
                    word,
                    site,
                    || {
                        capture_arguments(&[
                            crate::Dyn {
                                tag: 0,
                                payload: value as u64,
                            },
                            crate::Dyn {
                                tag: u64::MAX,
                                payload: 0,
                            },
                            crate::Dyn {
                                tag: u64::MAX,
                                payload: 0,
                            },
                            crate::Dyn {
                                tag: u64::MAX,
                                payload: 0,
                            },
                        ])
                    },
                    |r: &R<String>| match r {
                        Ok(v) => capture_text(v),
                        Err(e) => capture_error(e),
                    },
                    || {
                        let aliased: R<bool> = observed_captured(
                            alias_word,
                            site + 1,
                            || {
                                capture_arguments(&[crate::Dyn {
                                    tag: naming_tag,
                                    payload: 0,
                                }])
                            },
                            |r| match r {
                                Ok(v) => capture_value(crate::Dyn {
                                    tag: 1,
                                    payload: *v as u64,
                                }),
                                Err(e) => capture_error(e),
                            },
                            || Ok(false),
                        );
                        aliased.map(|_| value.to_string())
                    },
                )
            }
        })
    }
    fn run_at<T>(&mut self, site: usize, call: impl FnOnce() -> R<T>) -> R<T> {
        let Some(state) = &mut self.state else {
            return call();
        };
        state.name = state.sites.get(site).map(|s| state.names[s.callee].clone());
        state.clock = self.clocks.start_run();
        state.reader = DeltaClock::for_epoch(&state.clock);
        state.clock.attach_thread();
        state.thread = allocate_telemetry_id();
        state.started = state.read();
        state.paths.clear();
        for history in &mut state.histories {
            history.clear();
        }
        state.slots.clear();
        state.caches.fill(Cache::default());
        state.producer.write_timing(TimingRecord::ThreadSelected {
            thread_id: state.thread,
        });
        state.producer.write_span(SpanRecord::ThreadSelected {
            thread_id: state.thread,
        });
        state.producer.write_span(SpanRecord::ContextCleared);
        state.span(SpanRecord::ThreadSpanAnnouncement {
            id: state.thread,
            parent_id: None,
            spawn_call_path: CallPathId::ROOT,
            started_at: state.started,
            clock: state.clock.clone(),
            name: state.name.clone(),
        });
        struct Binding(*mut State);
        impl Drop for Binding {
            fn drop(&mut self) {
                CURRENT.set(self.0);
            }
        }
        let binding = Binding(CURRENT.replace(&mut **state));
        let mut result = call();
        if let Err(error) = &mut result {
            evidence::end(error, btel_records::UnwindResult::Unhandled, None);
        }
        drop(binding);
        state.flush();
        state.span(SpanRecord::ThreadSpanCompletion {
            id: state.thread,
            parent_id: None,
            spawn_call_path: CallPathId::ROOT,
            started_at: state.started,
            completed_at: state.read(),
            outcome: result
                .as_ref()
                .map_or_else(|e| outcome(e), |_| InvocationOutcome::Ok),
            clock: state.clock.clone(),
            name: state.name.clone(),
        });
        self.clocks.validate(&state.clock, true);
        state.clock.finish_thread();
        state.producer.seal();
        result
    }
    pub fn finish<T>(mut self, result: &R<T>) {
        self.exit.set(btel_types::ProcessExit {
            status: match result {
                Ok(_) => btel_types::ProcessStatus::Success,
                Err(error) if error.value != 0 && outcome(error) == InvocationOutcome::Panicked => {
                    btel_types::ProcessStatus::Panicked
                }
                Err(_) => btel_types::ProcessStatus::Error,
            },
            at_unix_ns: 0,
        });
        self.shutdown();
    }
    fn shutdown(&mut self) {
        if let Some(mut state) = self.state.take() {
            state.flush();
        }
        if let Some(pool) = self.pool.take() {
            pool.close_admission();
        }
        if let Some(worker) = self.worker.take() {
            worker.join().expect("telemetry worker panic");
        }
        if let Some(delivery) = self.delivery.take() {
            delivery.finish().expect("telemetry delivery");
        }
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn recording_root() -> Option<std::path::PathBuf> {
    if let Some(home) = baml_env::os_var("BAML_HOME").filter(|h| !h.is_empty()) {
        return Some(std::path::PathBuf::from(home).join("btel"));
    }
    if let Ok(cwd) = std::env::current_dir() {
        for directory in cwd.ancestors() {
            if directory.join("baml.toml").is_file() {
                return Some(directory.join(".baml/btel"));
            }
        }
    }
    baml_env::os_var("HOME").map(|h| std::path::PathBuf::from(h).join(".baml/btel"))
}

// Resolve once. Explicit policies override auto-low, but global off and
// compiler passthrough functions remain hidden.
fn observation(
    level: Option<btel_settings::mode::AutoTelemetryLevel>,
    hidden: bool,
    policy: Policy,
) -> u8 {
    use btel_settings::mode::AutoTelemetryLevel;
    if level.is_none() || hidden {
        return 0;
    }
    if policy.span_from_entry || (policy == Policy::NONE && level == Some(AutoTelemetryLevel::High))
    {
        return 2;
    }
    if policy.promote_errors || policy.promotion_duration_threshold.is_some() {
        return 3;
    }
    u8::from(policy != Policy::NONE || level != Some(AutoTelemetryLevel::Low))
}

#[cfg(test)]
mod tests {
    use super::*;
    static WORD: AtomicU8 = AtomicU8::new(1);
    static SITES: &[Site] = &[
        Site {
            caller: None,
            pc: 0,
            callee: 0,
        },
        Site {
            caller: Some(0),
            pc: 10,
            callee: 1,
        },
        Site {
            caller: Some(1),
            pc: 20,
            callee: 1,
        },
    ];

    #[test]
    fn successful_slot_add_is_atomic_on_overflow() {
        use btel_types::ClockDuration;
        let mut slot = AggregateDelta {
            count: u64::MAX,
            ..Default::default()
        };
        assert!(!add_ok(&mut slot, ClockDuration::from_ticks(1)));
        assert_eq!(slot.count, u64::MAX);
        assert_eq!(slot.total_duration.get(), 0);
        slot.count = 7;
        slot.total_duration = ClockDuration::from_ticks(u64::MAX);
        assert!(!add_ok(&mut slot, ClockDuration::from_ticks(1)));
        assert_eq!(slot.count, 7);
        slot.total_duration = ClockDuration::from_ticks(10);
        assert!(add_ok(&mut slot, ClockDuration::from_ticks(5)));
        assert_eq!((slot.count, slot.total_duration.get()), (8, 15));
    }

    #[test]
    fn startup_observation_respects_off_low_and_explicit_policy() {
        use btel_settings::mode::AutoTelemetryLevel::{High, Low, Medium};
        let policy = Policy {
            capture_output: true,
            ..Policy::NONE
        };
        assert_eq!(observation(None, false, policy), 0);
        assert_eq!(observation(Some(Low), false, Policy::NONE), 0);
        assert_eq!(observation(Some(Medium), false, Policy::NONE), 1);
        assert_eq!(observation(Some(Low), false, policy), 1);
        assert_eq!(observation(Some(High), true, policy), 0);
        assert_eq!(observation(Some(High), false, Policy::NONE), 2);
        assert_eq!(
            observation(
                Some(Low),
                false,
                Policy {
                    promote_errors: true,
                    ..Policy::NONE
                }
            ),
            3
        );
    }
    #[test]
    fn hidden_does_not_need_a_bound_loop() {
        assert!(unsafe { enter(&AtomicU8::new(0), usize::MAX) }.is_none());
    }

    #[test]
    fn recursive_population_cached_sites_flush_and_shared_error_clock() {
        let pool = Pool::new(bex_chunkedringbuffer::Config::for_producers(
            NonZeroUsize::new(1).unwrap(),
        ))
        .unwrap();
        let mut processor = Processor::with_publisher(
            pool.bind_consumer().unwrap(),
            NonZeroUsize::new(8).unwrap(),
            NoSinkPublisher::default(),
        );
        let mut state = test_state(&pool);
        state.producer.write_timing(TimingRecord::ThreadSelected {
            thread_id: state.thread,
        });
        CURRENT.set(&mut *state);
        let root = unsafe { enter(&WORD, 0) }.unwrap();
        let child = unsafe { enter(&WORD, 1) }.unwrap();
        let recursive = unsafe { enter(&WORD, 2) }.unwrap();
        let mut error: R<()> = Err(Error::internal("test panic".into()));
        recursive.finish(&mut error);
        let at = error.as_ref().unwrap_err().telemetry_at;
        child.finish(&mut error);
        assert_eq!(error.as_ref().unwrap_err().telemetry_at, at);
        assert_eq!(state.paths.len(), 2);
        let path = state.caches[1].path;
        // Force a cache miss and a slot-table relocation while the root frame
        // is live. The site history must reuse the old stable slot without hashing.
        state.caches[1] = Cache::default();
        state.paths.clear();
        state.slots.reserve(512);
        before_yield();
        assert!(
            state
                .slots
                .iter()
                .flat_map(|p| p.iter())
                .all(|s| s.count == 0)
        );
        let child = unsafe { enter(&WORD, 1) }.unwrap();
        assert_eq!(state.active, path);
        assert!(state.paths.is_empty());
        child.finish(&mut Ok(()));
        root.finish(&mut Ok(()));
        assert_eq!(state.active, CallPathId::ROOT);
        CURRENT.set(std::ptr::null_mut());
        state.flush();
        processor.process_available();
        processor.flush();
        assert_eq!(processor.publisher().stats().function_completions, 4);
    }
    fn test_state(pool: &Pool) -> Box<State> {
        let allocator = btel_types::FunctionIdAllocator::default();
        let clock = ClockRuntime::new(btel_clock::ClockMode::Monotonic).start_run();
        Box::new(State {
            capture_metadata: None,
            capture_scratch: None,
            evidence: true,
            depth: 0,
            evidence_frame: std::ptr::null_mut(),
            producer: pool.register_producer().unwrap(),
            clock,
            functions: vec![allocator.allocate().unwrap(), allocator.allocate().unwrap()],
            names: vec![],
            words: vec![],
            name: None,
            sites: SITES,
            caches: vec![Cache::default(); 3],
            paths: HashMap::new(),
            histories: vec![Vec::new(); SITES.len()],
            reader: DeltaClock::Epoch,
            slots: Vec::new(),
            active_span: None,
            policies: vec![Policy::NONE; 2],
            active: CallPathId::ROOT,
            active_function: None,
            active_slot: std::ptr::null_mut(),
            next_path: 0,
            gc_function: allocator.allocate().unwrap(),
            gc_started: None,
            thread: allocate_telemetry_id(),
            started: ClockInstant::from_ticks(0),
        })
    }

    #[derive(Default)]
    struct Spans {
        announcements: Vec<(TelemetryId, TelemetryId)>,
        completions: Vec<(TelemetryId, TelemetryId, bool, bool, InvocationOutcome)>,
        count: u64,
        capture_counts: Vec<(bool, usize)>,
        dependencies: Vec<bool>,
        raises: Vec<(btel_records::RaiseKind, u32, CallPathId)>,
        stacks: Vec<usize>,
        ends: Vec<(u32, Option<u32>)>,
    }
    impl btel_processor::Publisher<Snapshot, Snapshot> for Spans {
        fn aggregate(&mut self, delta: AggregateDelta) {
            self.count += delta.count;
        }
        fn span(&mut self, _: TelemetryId, record: &mut Span) {
            match record {
                SpanRecord::ErrorRaised {
                    kind,
                    frame_count,
                    call_path,
                    ..
                } => self.raises.push((*kind, *frame_count, *call_path)),
                SpanRecord::ErrorRaiseStack(stack) => self.stacks.push(stack.frames.len()),
                SpanRecord::ErrorUnwindEnded {
                    unwound_frames,
                    handler_pc,
                    ..
                } => self.ends.push((*unwound_frames, *handler_pc)),
                _ => {}
            }
            self.capture_counts.push((
                matches!(record, SpanRecord::FunctionSpanAnnouncement { .. }),
                record.captures().count(),
            ));
            if let SpanRecord::FunctionSpanAnnouncement { id, parent_id, .. } = record {
                self.announcements.push((*id, *parent_id));
            }
            if let Some(done) = record.completion() {
                self.dependencies.push(done.requires_announcement);
                self.completions.push((
                    done.id,
                    done.parent_id,
                    done.late,
                    done.reentry,
                    done.outcome,
                ));
            }
        }
        fn flush(&mut self) {}
    }
    #[test]
    fn spans_and_late_promotion_restore_parent_and_count_once() {
        let pool = Pool::new(bex_chunkedringbuffer::Config::for_producers(
            NonZeroUsize::new(1).unwrap(),
        ))
        .unwrap();
        let mut processor = Processor::with_publisher(
            pool.bind_consumer().unwrap(),
            NonZeroUsize::new(8).unwrap(),
            Spans::default(),
        );
        let mut state = test_state(&pool);
        state.producer.write_timing(TimingRecord::ThreadSelected {
            thread_id: state.thread,
        });
        state.policies[1].promote_errors = true;
        CURRENT.set(&mut *state);
        let result = unsafe {
            observe(0, 2, || {
                let parent = (*CURRENT.get()).active_span;
                observe(1, 2, || observe(2, 2, || Ok(())))?;
                assert_eq!((*CURRENT.get()).active_span, parent);
                // No announcement or aggregate duplication for a late completion.
                let failed: R<()> = observe(1, 3, || Err(Error::internal("test".into())));
                assert!(failed.is_err());
                assert_eq!((*CURRENT.get()).active_span, parent);
                observe(1, 3, || Ok(()))?;
                (&mut (*CURRENT.get()).policies)[1].promotion_duration_threshold =
                    Some(std::time::Duration::ZERO);
                observe(1, 3, || Ok(()))
            })
        };
        assert!(result.is_ok());
        assert!(state.active_span.is_none());
        assert_eq!(state.active, CallPathId::ROOT);
        CURRENT.set(std::ptr::null_mut());
        state.flush();
        processor.process_available();
        processor.flush();
        let published = processor.publisher();
        assert_eq!(published.count, 6);
        assert_eq!(published.announcements.len(), 3);
        assert_eq!(published.completions.len(), 5);
        let root = published.announcements[0].0;
        let child = published.announcements[1].0;
        assert_eq!(published.announcements[0].1, state.thread);
        assert_eq!(published.announcements[1].1, root);
        assert_eq!(published.announcements[2].1, child);
        assert!(published.completions[0].3);
        assert!(published.completions[2].2);
        assert_eq!(published.completions[2].1, root);
        assert_eq!(published.completions[2].4, InvocationOutcome::Panicked);
    }

    #[test]
    fn captures_are_policy_gated_and_late_spans_never_capture_inputs() {
        let pool = Pool::new(bex_chunkedringbuffer::Config::for_producers(
            NonZeroUsize::new(1).unwrap(),
        ))
        .unwrap();
        let mut processor = Processor::with_publisher(
            pool.bind_consumer().unwrap(),
            NonZeroUsize::new(8).unwrap(),
            Spans::default(),
        );
        let mut state = test_state(&pool);
        state.producer.write_timing(TimingRecord::ThreadSelected {
            thread_id: state.thread,
        });
        CURRENT.set(&mut *state);
        let calls = Cell::new(0);
        let inputs = || {
            calls.set(calls.get() + 1);
            let mut scratch = capture::Scratch::default();
            InputCaptures {
                inputs: Some(unsafe {
                    scratch.arguments(&[], &[crate::Dyn { tag: 0, payload: 7 }])
                }),
                types: Some(scratch.type_args(&[("T", &capture::Type::Int)])),
            }
        };
        let outputs = |_: &R<()>| {
            calls.set(calls.get() + 1);
            Some(capture::Scratch::default().text("result"))
        };
        unsafe { observe_captured(0, 2, inputs, outputs, || Ok(())) }.unwrap();
        assert_eq!(calls.get(), 0);
        state.policies[0] = Policy {
            capture_inputs: true,
            capture_output: true,
            capture_error: true,
            promote_errors: true,
            ..Policy::NONE
        };
        unsafe { observe_captured(0, 2, inputs, outputs, || Ok(())) }.unwrap();
        assert_eq!(calls.get(), 2);
        let failed = unsafe {
            observe_captured(0, 3, inputs, outputs, || {
                Err(Error::internal("promotion".into()))
            })
        };
        assert!(failed.is_err());
        assert_eq!(calls.get(), 3);
        unsafe { observe_captured(0, 3, inputs, outputs, || Ok(())) }.unwrap();
        assert_eq!(calls.get(), 3);
        state.policies[0].capture_output = false;
        unsafe { observe_captured(0, 2, inputs, outputs, || Ok(())) }.unwrap();
        assert_eq!(calls.get(), 4);
        CURRENT.set(std::ptr::null_mut());
        state.flush();
        processor.process_available();
        processor.flush();
        let p = processor.publisher();
        assert_eq!(p.count, 5);
        assert_eq!(p.announcements.len(), 3);
        assert_eq!(p.dependencies, [false, true, false, true]);
        assert_eq!(
            p.capture_counts
                .iter()
                .filter(|(entry, _)| *entry)
                .map(|(_, n)| *n)
                .collect::<Vec<_>>(),
            [0, 2, 2]
        );
        assert_eq!(
            p.capture_counts
                .iter()
                .filter(|(entry, _)| !*entry)
                .map(|(_, n)| *n)
                .sum::<usize>(),
            2
        );
    }

    #[test]
    fn recording_errors_preserve_hidden_stacks_and_unwind_counts() {
        static SOURCE: crate::Source = crate::Source {
            file: "test.baml",
            function: "child",
            lines: &[7],
            pcs: &[17],
            kinds: &[RaiseKind::Throw],
        };
        static WORDS: [AtomicU8; 8] = [
            AtomicU8::new(0),
            AtomicU8::new(1),
            AtomicU8::new(2),
            AtomicU8::new(3),
            AtomicU8::new(4),
            AtomicU8::new(5),
            AtomicU8::new(6),
            AtomicU8::new(7),
        ];
        for (mode, explicit) in [
            (0, true),
            (1, true),
            (2, true),
            (3, true),
            (1, false),
            (2, false),
            (3, false),
        ] {
            let word = &WORDS[mode | if explicit { 4 } else { 0 }];
            let pool = Pool::new(bex_chunkedringbuffer::Config::for_producers(
                NonZeroUsize::new(1).unwrap(),
            ))
            .unwrap();
            let mut processor = Processor::with_publisher(
                pool.bind_consumer().unwrap(),
                NonZeroUsize::new(8).unwrap(),
                Spans::default(),
            );
            let mut state = test_state(&pool);
            state.policies[1].promote_errors = true;
            state.words = vec![word, word];
            state.producer.write_timing(TimingRecord::ThreadSelected {
                thread_id: state.thread,
            });
            CURRENT.set(&mut *state);
            let result = unsafe {
                observed_call(word, 0, || {
                    let child: R<()> = observed_call(word, 1, || {
                        Err(Error::internal("test".into()).at(&SOURCE, 0))
                    });
                    let mut error = child.unwrap_err();
                    assert!(error.telemetry_at.is_some());
                    evidence::caught(&mut error, 42);
                    assert!(error.telemetry_raise.is_none());
                    Ok(())
                })
            };
            assert!(result.is_ok());
            assert!(state.evidence_frame.is_null());
            CURRENT.set(std::ptr::null_mut());
            state.flush();
            processor.process_available();
            processor.flush();
            let published = processor.publisher();
            assert_eq!(published.count, if mode == 0 { 0 } else { 2 });
            assert_eq!(published.raises.len(), 1);
            assert_eq!(published.raises[0].0, RaiseKind::Throw);
            assert_eq!(published.raises[0].1, 2);
            assert_eq!(published.raises[0].2 == CallPathId::ROOT, mode == 0);
            assert_eq!(published.stacks, if mode == 0 { vec![2] } else { vec![] });
            assert_eq!(published.ends, vec![(1, Some(42))]);
        }
    }
}
