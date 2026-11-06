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

pub use btel_types as types;
pub type Policy = btel_settings::policy::TelemetryPolicy<std::time::Duration>;
pub struct Function {
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
    slot: usize,
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

struct State {
    producer: Producer<TimingRecord, Span>,
    clock: Arc<ClockEpoch>,
    functions: Vec<FunctionId>,
    names: Vec<btel_records::ThreadName>,
    words: Vec<&'static AtomicU8>,
    name: Option<btel_records::ThreadName>,
    sites: &'static [Site],
    caches: Vec<Cache>,
    paths: HashMap<Key, (CallPathId, usize)>,
    slots: Vec<AggregateDelta>,
    active: CallPathId,
    active_function: Option<usize>,
    active_slot: usize,
    next_path: u32,
    thread: TelemetryId,
    started: ClockInstant,
}
impl State {
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
        for slot in &mut self.slots {
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
    fn path(&mut self, site: Site) -> (CallPathId, usize) {
        let key = Key {
            parent: self.active,
            caller: site.caller,
            pc: site.pc,
            callee: site.callee,
        };
        if let Some(value) = self.paths.get(&key) {
            return *value;
        }
        self.next_path = self.next_path.checked_add(1).expect("call path exhausted");
        let path = CallPathId::new_non_root(self.next_path).unwrap();
        let slot = self.slots.len();
        for reentry in [false, true] {
            self.slots.push(AggregateDelta {
                node: CallPathNodeId::new(path, reentry),
                ..Default::default()
            });
        }
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
    saved_slot: usize,
    slot: usize,
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
    word.load(Ordering::Relaxed) != 0
}
/// # Safety
/// A session must own the current task, and the caller must finish this frame
/// before leaving that task. The site must belong to the compiled program.
#[inline(always)]
pub unsafe fn enter_observed(site_id: usize) -> Frame {
    let ptr = CURRENT.get();
    let state = unsafe { &mut *ptr };
    let site = state.sites[site_id];
    let entered_at = state.clock.read();
    let reentry = site.caller == Some(site.callee) && state.active_function == Some(site.callee);
    let (path, slot) = if reentry {
        // The active node's adjacent slot is its reentry population.
        let base = state.active_slot;
        (state.active, base + 1)
    } else {
        let cache = state.caches[site_id];
        if cache.parent == Some(state.active) {
            (cache.path, cache.slot)
        } else {
            let (path, slot) = state.path(site);
            state.caches[site_id] = Cache {
                parent: Some(state.active),
                path,
                slot,
            };
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
    state.active = path;
    state.active_slot = slot & !1;
    state.active_function = Some(site.callee);
    frame
}
impl Frame {
    #[inline(always)]
    pub fn finish<T>(self, result: &mut R<T>) {
        let state = unsafe { &mut *self.state };
        let (exited_at, outcome) = match result {
            Ok(_) => (state.clock.read(), InvocationOutcome::Ok),
            Err(error) => (
                *error.telemetry_at.get_or_insert_with(|| state.clock.read()),
                outcome(error),
            ),
        };
        let sample = AggregateDelta {
            node: state.slots[self.slot].node,
            count: 1,
            total_duration: self.entered_at.elapsed_until(exited_at),
            total_self_await: self.await_duration,
            errored: u64::from(outcome.is_error()),
            cancelled: u64::from(outcome == InvocationOutcome::Cancelled),
            panicked: u64::from(outcome == InvocationOutcome::Panicked),
        };
        debug_assert_eq!(sample.node.is_reentry(), self.flags != 0);
        if let Some(sum) = state.slots[self.slot].checked_merge(sample) {
            state.slots[self.slot] = sum;
        } else {
            state.producer.write_timing(TimingRecord::AggregateDeltas(
                vec![state.slots[self.slot]].into_boxed_slice(),
            ));
            state.slots[self.slot] = sample;
        }
        state.active = self.saved_path;
        state.active_function = self.saved_function;
        state.active_slot = self.saved_slot;
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
        let words = functions
            .iter()
            .map(|function| {
                let word = observation(level, function.hidden, function.policy);
                if word > 1 {
                    return Err(Error::internal(
                        "unsupported in runtime v2: span telemetry or promotion (T2 pending)"
                            .into(),
                    ));
                }
                Ok(word)
            })
            .collect::<R<Vec<_>>>()?;
        for (function, word) in functions.iter().zip(words) {
            function.word.store(word, Ordering::Relaxed);
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
        let metadata = Arc::new(btel_types::FunctionMetadataTable {
            functions: functions
                .iter()
                .zip(&ids)
                .map(|(f, &id)| (f.metadata)(id))
                .collect(),
        });
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
            producer: pool.register_producer().unwrap(),
            clock,
            functions: ids,
            names,
            words: functions.iter().map(|f| f.word).collect(),
            name: None,
            sites,
            caches: vec![Cache::default(); sites.len()],
            paths: HashMap::new(),
            slots: Vec::new(),
            active: CallPathId::ROOT,
            active_function: None,
            active_slot: 0,
            next_path: 0,
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
        self.run_at(site, || {
            // Typed specialization of stringify(int, Declared, 0, false):
            // resolve the naming mode, then encode the scalar without a heap.
            let frame = unsafe { enter(word, site) };
            let naming_frame = unsafe { enter(alias_word, site + 1) };
            let mut aliased: R<bool> = Ok(false);
            if let Some(frame) = naming_frame {
                frame.finish(&mut aliased);
            }
            let mut result = aliased.map(|_| value.to_string());
            if let Some(frame) = frame {
                frame.finish(&mut result);
            }
            result
        })
    }
    fn run_at<T>(&mut self, site: usize, call: impl FnOnce() -> R<T>) -> R<T> {
        let Some(state) = &mut self.state else {
            return call();
        };
        state.name = state.sites.get(site).map(|s| state.names[s.callee].clone());
        state.clock = self.clocks.start_run();
        state.clock.attach_thread();
        state.thread = allocate_telemetry_id();
        state.started = state.clock.read();
        state.paths.clear();
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
        let result = call();
        drop(binding);
        state.flush();
        state.span(SpanRecord::ThreadSpanCompletion {
            id: state.thread,
            parent_id: None,
            spawn_call_path: CallPathId::ROOT,
            started_at: state.started,
            completed_at: state.clock.read(),
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
    pub fn finish(mut self, success: bool) {
        self.exit.set(btel_types::ProcessExit {
            status: if success {
                btel_types::ProcessStatus::Success
            } else {
                btel_types::ProcessStatus::Error
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
// compiler passthrough functions remain hidden. Span/promotion is T2.
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
        let allocator = btel_types::FunctionIdAllocator::default();
        let clock = ClockRuntime::new(btel_clock::ClockMode::Monotonic).start_run();
        let mut state = Box::new(State {
            producer: pool.register_producer().unwrap(),
            clock,
            functions: vec![allocator.allocate().unwrap(), allocator.allocate().unwrap()],
            names: vec![],
            words: vec![],
            name: None,
            sites: SITES,
            caches: vec![Cache::default(); 3],
            paths: HashMap::new(),
            slots: Vec::new(),
            active: CallPathId::ROOT,
            active_function: None,
            active_slot: 0,
            next_path: 0,
            thread: allocate_telemetry_id(),
            started: ClockInstant::from_ticks(0),
        });
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
        before_yield();
        assert!(state.slots.iter().all(|s| s.count == 0));
        let child = unsafe { enter(&WORD, 1) }.unwrap();
        assert_eq!(state.active, path);
        assert_eq!(state.paths.len(), 2);
        child.finish(&mut Ok(()));
        root.finish(&mut Ok(()));
        assert_eq!(state.active, CallPathId::ROOT);
        CURRENT.set(std::ptr::null_mut());
        state.flush();
        processor.process_available();
        processor.flush();
        assert_eq!(processor.publisher().stats().function_completions, 4);
    }
}
