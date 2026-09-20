//! Experimental alternative to per-record SPSC publication.
//!
//! `write_*` stores records before returning; PRIVATE partial chunks are not
//! consumer-visible until `seal`, fullness, or producer release. Integration MUST
//! seal before suspension, completion, or going idle. A live producer that never
//! reaches a seal boundary can delay visibility indefinitely. No processor may
//! take its actively mutated allocation.
//!
//! One pool has one consumer, a FIFO of sealed chunks, and separate typed storage
//! quotas. Every allocation counts toward its quota while free, producer-owned,
//! ready, processor-owned, or publisher-owned. Producers spin on exhaustion and metadata contention;
//! only the idle consumer parks. Explicit recording disable abandons queued data
//! and makes writes return without storing; invariant failures remain terminal.
#![allow(
    clippy::inline_always,
    reason = "ordinary append must inline to private Vec writes"
)]

mod sync;

use std::{collections::VecDeque, marker::PhantomData, mem::size_of, num::NonZeroUsize, rc::Rc};

pub use btel_settings::transport::ChunkConfig as Config;
use sync::{
    Arc, AtomicBool, AtomicU8, AtomicUsize, Condvar, Mutex, MutexGuard, Ordering, Padded,
    TryLockError, thread,
};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SetupError {
    InvalidCapacity,
    InsufficientChunks,
    Closed,
    Failed,
    Disabled,
    ProducerLimit,
    ProducerAlreadyBound,
    ConsumerAlreadyBound,
}

/// Bounded, pool-local stream index, fixed for a registered OS worker. Reusing
/// a retired worker's slot continues its FIFO stream, not a new global identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct ProducerId(usize);
impl ProducerId {
    pub fn index(self) -> usize {
        self.0
    }
}

/// OS-thread-bound slot reservation, independent of a poll's active producer.
/// The owner must call `ChunkPool::release_worker` when retiring it. This token
/// deliberately does not retain the pool, so an idle TLS cache cannot keep an
/// engine's buffers alive. A stale token cannot be used without its original pool.
#[must_use = "release the worker reservation when its OS thread retires"]
pub struct WorkerSlot {
    id: ProducerId,
    pool: *const (),
    _thread_bound: PhantomData<Rc<()>>,
}

#[derive(Clone, Copy, Default)]
struct WorkerState {
    owner: Option<thread::ThreadId>,
    reserved: bool,
    active: bool,
}

/// Invariant-failure signal, distinct from nonfatal recording disable.
#[derive(Debug)]
pub struct TransportFailed;

#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub allocated_timing_chunks: usize,
    pub allocated_span_chunks: usize,
    pub free_chunks: usize,
    pub ready_chunks: usize,
    pub active_producers: usize,
    pub sealed_chunks: usize,
    pub sealed_records: usize,
    pub wakeups: usize,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct ProducerStats {
    pub allocations: usize,
    pub exhausted_waits: usize,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DrainStatus {
    pub chunks: usize,
    pub records: usize,
    /// Input is exhausted after admission closes and producers finish. Owned
    /// span chunks may still be held downstream; this is not a delivery barrier.
    pub complete: bool,
}

struct Lane<T> {
    free: Vec<Vec<T>>,
    allocated: usize,
    max: usize,
}
impl<T> Lane<T> {
    fn new(cap: usize, max: usize, preallocate: bool) -> Self {
        let mut free = Vec::with_capacity(max);
        if preallocate {
            for _ in 0..max {
                free.push(Vec::with_capacity(cap));
            }
        }
        Self {
            free,
            allocated: if preallocate { max } else { 0 },
            max,
        }
    }
}
enum Records<T, S> {
    Timing(Vec<T>),
    Span(Vec<S>),
}
struct Chunk<T, S> {
    producer: ProducerId,
    records: Records<T, S>,
}
struct State<T, S> {
    timing: Lane<T>,
    span: Lane<S>,
    ready: VecDeque<Chunk<T, S>>,
    workers: Box<[WorkerState]>,
    active_producers: usize,
    closed: bool,
    bound: bool,
    sleeping: bool,
    sealed_chunks: usize,
    sealed_records: usize,
    wakeups: usize,
}
const ACTIVE: u8 = 0;
const DISABLED: u8 = 1;
const FAILED: u8 = 2;

struct Shared<T, S> {
    state: Mutex<State<T, S>>,
    wake: Condvar,
    status: AtomicU8,
    // Advisory only: sleeping always rechecks state under the mutex.
    ready_hint: Padded<AtomicBool>,
    timing_free: Padded<AtomicUsize>,
    span_free: Padded<AtomicUsize>,
    config: Config,
}

#[cold]
#[inline(never)]
fn terminal() -> ! {
    std::panic::panic_any(TransportFailed)
}

impl<T, S> Shared<T, S> {
    #[inline(always)]
    fn check(&self) -> bool {
        match self.status.load(Ordering::Acquire) {
            ACTIVE => true,
            DISABLED => false,
            _ => terminal(),
        }
    }
    /// All producer metadata access is nonblocking try-lock plus CPU spin.
    fn lock(&self) -> MutexGuard<'_, State<T, S>> {
        loop {
            match self.state.try_lock() {
                Ok(g) => return g,
                Err(TryLockError::WouldBlock) => sync::spin(),
                Err(TryLockError::Poisoned(e)) => return e.into_inner(),
            }
        }
    }
    fn fail(&self) {
        // Serialize with the consumer's condition-variable check/wait transition.
        let guard = self.lock();
        let _ = self
            .status
            .compare_exchange(ACTIVE, FAILED, Ordering::AcqRel, Ordering::Acquire);
        drop(guard);
        self.wake.notify_all();
    }
    #[cold]
    fn take<R>(
        &self,
        lane: impl Fn(&mut State<T, S>) -> &mut Lane<R>,
        stats: &mut ProducerStats,
        available: &AtomicUsize,
    ) -> Option<Vec<R>> {
        let mut exhausted = false;
        loop {
            if !self.check() {
                return None;
            }
            let mut state = self.lock();
            let lane = lane(&mut state);
            if let Some(chunk) = lane.free.pop() {
                // All free-list mutations are serialized by state. Publish its
                // exact length; no read-modify-write is needed.
                available.store(lane.free.len(), Ordering::Release);
                return Some(chunk);
            }
            if lane.allocated < lane.max {
                // Reserved allocations also count: two writers cannot both
                // allocate the last slot while allocation runs outside the lock.
                lane.allocated += 1;
                drop(state);
                stats.allocations += 1;
                let mut guard = FailureGuard {
                    shared: self,
                    armed: true,
                };
                let chunk = Vec::with_capacity(self.config.chunk_capacity.get());
                guard.armed = false;
                return Some(chunk);
            }
            let notify = state.sleeping;
            if notify {
                state.sleeping = false;
                state.wakeups += 1;
            }
            drop(state);
            if notify {
                self.wake.notify_one();
            }
            if !exhausted {
                stats.exhausted_waits += 1;
                exhausted = true;
            }
            // Do not fight the recycler for its mutex while the pool is empty.
            // The consumer publishes availability after returning the allocation.
            // Another writer may win it first; the locked recheck above decides.
            while available.load(Ordering::Acquire) == 0 {
                if !self.check() {
                    return None;
                }
                sync::spin();
            }
        }
    }
    fn seal(&self, producer: ProducerId, records: Records<T, S>) -> bool {
        let len = match &records {
            Records::Timing(v) => v.len(),
            Records::Span(v) => v.len(),
        };
        debug_assert!(len > 0);
        let mut state = self.lock();
        if self.status.load(Ordering::Acquire) != ACTIVE {
            drop(state);
            return false;
        }
        // Descriptor capacity was reserved for EVERY allocated chunk. No ready
        // queue allocation is needed even when all chunks are sealed at once.
        debug_assert!(state.ready.len() < state.ready.capacity());
        state.ready.push_back(Chunk { producer, records });
        if state.ready.len() == 1 {
            self.ready_hint.0.store(true, Ordering::Relaxed);
        }
        state.sealed_chunks += 1;
        state.sealed_records += len;
        let notify = state.sleeping;
        if notify {
            state.sleeping = false;
            state.wakeups += 1;
        }
        drop(state);
        if notify {
            self.wake.notify_one();
        }
        true
    }
}

/// One fixed processor's chunk pool. Clone handles, not endpoints. Multiple
/// processors use separate pools; this crate does not reassign chunks or workers.
pub struct ChunkPool<T, S> {
    shared: Arc<Shared<T, S>>,
}
impl<T, S> Clone for ChunkPool<T, S> {
    fn clone(&self) -> Self {
        Self {
            shared: self.shared.clone(),
        }
    }
}
impl<T: Send, S: Send> ChunkPool<T, S> {
    pub fn new(config: Config) -> Result<Self, SetupError> {
        let capacity = config.chunk_capacity.get();
        let timing = config.timing_chunks.get();
        let span = config.span_chunks.get();
        let chunks = timing
            .checked_add(span)
            .ok_or(SetupError::InvalidCapacity)?;
        let fits = |n: usize, size: usize| {
            n.checked_mul(size)
                .is_some_and(|b| isize::try_from(b).is_ok())
        };
        if !fits(capacity, size_of::<T>())
            || !fits(capacity, size_of::<S>())
            || !fits(chunks, size_of::<Chunk<T, S>>())
            || !fits(timing, size_of::<Vec<T>>())
            || !fits(span, size_of::<Vec<S>>())
            || !fits(config.max_producers.get(), size_of::<WorkerState>())
        {
            return Err(SetupError::InvalidCapacity);
        }
        if timing < config.max_producers.get() || span < config.max_producers.get() {
            return Err(SetupError::InsufficientChunks);
        }
        Ok(Self {
            shared: Arc::new(Shared {
                state: Mutex::new(State {
                    timing: Lane::new(capacity, timing, config.preallocate),
                    span: Lane::new(capacity, span, config.preallocate),
                    ready: VecDeque::with_capacity(chunks),
                    workers: vec![WorkerState::default(); config.max_producers.get()]
                        .into_boxed_slice(),
                    active_producers: 0,
                    closed: false,
                    bound: false,
                    sleeping: false,
                    sealed_chunks: 0,
                    sealed_records: 0,
                    wakeups: 0,
                }),
                wake: Condvar::new(),
                status: AtomicU8::new(ACTIVE),
                ready_hint: Padded(AtomicBool::new(false)),
                timing_free: Padded(AtomicUsize::new(if config.preallocate {
                    timing
                } else {
                    0
                })),
                span_free: Padded(AtomicUsize::new(if config.preallocate { span } else { 0 })),
                config,
            }),
        })
    }
    pub fn register_producer(&self) -> Result<Producer<T, S>, SetupError> {
        // Convenience for a single producer scope. Runtime workers instead keep
        // the reservation across polls and bind it directly on every entry.
        let worker = self.reserve_worker()?;
        let producer = self.bind_worker(&worker);
        self.release_worker(worker);
        producer
    }

    pub fn reserve_worker(&self) -> Result<WorkerSlot, SetupError> {
        let mut state = self.shared.lock();
        match self.shared.status.load(Ordering::Acquire) {
            FAILED => return Err(SetupError::Failed),
            DISABLED => return Err(SetupError::Disabled),
            _ => {}
        }
        if state.closed {
            return Err(SetupError::Closed);
        }
        let owner = thread::current().id();
        if state
            .workers
            .iter()
            .any(|worker| worker.owner == Some(owner))
        {
            return Err(SetupError::ProducerAlreadyBound);
        }
        let index = state
            .workers
            .iter()
            .position(|worker| worker.owner.is_none())
            .ok_or(SetupError::ProducerLimit)?;
        state.workers[index] = WorkerState {
            owner: Some(owner),
            reserved: true,
            active: false,
        };
        Ok(WorkerSlot {
            id: ProducerId(index),
            pool: Arc::as_ptr(&self.shared).cast::<()>(),
            _thread_bound: PhantomData,
        })
    }

    /// Resume this worker's predetermined slot. No thread search or slot selection.
    pub fn bind_worker(&self, worker: &WorkerSlot) -> Result<Producer<T, S>, SetupError> {
        assert_eq!(
            worker.pool,
            Arc::as_ptr(&self.shared).cast::<()>(),
            "wrong worker pool"
        );
        let mut state = self.shared.lock();
        match self.shared.status.load(Ordering::Acquire) {
            FAILED => return Err(SetupError::Failed),
            DISABLED => return Err(SetupError::Disabled),
            _ => {}
        }
        if state.closed {
            return Err(SetupError::Closed);
        }
        let owner = thread::current().id();
        let slot = &mut state.workers[worker.id.index()];
        assert_eq!(slot.owner, Some(owner));
        assert!(slot.reserved);
        if slot.active {
            return Err(SetupError::ProducerAlreadyBound);
        }
        slot.active = true;
        state.active_producers += 1;
        Ok(Producer {
            shared: self.shared.clone(),
            id: worker.id,
            owner,
            timing: None,
            span: None,
            stats: ProducerStats::default(),
            _thread_bound: PhantomData,
        })
    }

    /// Retire a worker. An active producer keeps the slot until its final seal;
    /// otherwise the next worker may reuse it immediately. FIFO orders old and
    /// new chunks, and each new writer must announce its initial stream context.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "retirement consumes the non-Clone slot token so it cannot be rebound"
    )]
    pub fn release_worker(&self, worker: WorkerSlot) {
        assert_eq!(
            worker.pool,
            Arc::as_ptr(&self.shared).cast::<()>(),
            "wrong worker pool"
        );
        let mut state = self.shared.lock();
        let slot = &mut state.workers[worker.id.index()];
        assert_eq!(slot.owner, Some(thread::current().id()));
        slot.reserved = false;
        if !slot.active {
            slot.owner = None;
        }
    }
    pub fn bind_consumer(&self) -> Result<Consumer<T, S>, SetupError> {
        let mut state = self.shared.lock();
        if self.shared.status.load(Ordering::Acquire) == FAILED {
            return Err(SetupError::Failed);
        }
        if state.bound {
            return Err(SetupError::ConsumerAlreadyBound);
        }
        state.bound = true;
        Ok(Consumer {
            shared: self.shared.clone(),
            finished: false,
            _thread_bound: PhantomData,
        })
    }
    pub fn close_admission(&self) {
        let mut state = self.shared.lock();
        state.closed = true;
        drop(state);
        self.shared.wake.notify_one();
    }
    pub fn fail(&self) {
        self.shared.fail();
    }
    /// Permanently abandon recording. Release queued allocations and wake the
    /// consumer; producers blocked on capacity return without writing. Unlike
    /// an invariant failure, this must not unwind application execution.
    pub fn disable(&self) {
        let mut state = self.shared.lock();
        if self
            .shared
            .status
            .compare_exchange(ACTIVE, DISABLED, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        state.closed = true;
        let ready = std::mem::take(&mut state.ready);
        let timing = std::mem::take(&mut state.timing.free);
        let span = std::mem::take(&mut state.span.free);
        self.shared.ready_hint.0.store(false, Ordering::Release);
        drop(state);
        self.shared.wake.notify_all();
        // Destruct payloads outside the metadata lock. Private chunks belong to
        // their producers and are released when those scopes exit.
        drop((ready, timing, span));
    }
    pub fn is_disabled(&self) -> bool {
        self.shared.status.load(Ordering::Acquire) == DISABLED
    }
    pub fn is_failed(&self) -> bool {
        self.shared.status.load(Ordering::Acquire) == FAILED
    }
    pub fn stats(&self) -> Stats {
        let s = self.shared.lock();
        Stats {
            allocated_timing_chunks: s.timing.allocated,
            allocated_span_chunks: s.span.allocated,
            free_chunks: s.timing.free.len() + s.span.free.len(),
            ready_chunks: s.ready.len(),
            active_producers: s.active_producers,
            sealed_chunks: s.sealed_chunks,
            sealed_records: s.sealed_records,
            wakeups: s.wakeups,
        }
    }
}

/// Exclusive, OS-thread-bound private chunks. Append does not publish a partial
/// chunk. Seal before suspending a logical thread or leaving this producer idle.
pub struct Producer<T, S> {
    shared: Arc<Shared<T, S>>,
    id: ProducerId,
    owner: thread::ThreadId,
    timing: Option<Vec<T>>,
    span: Option<Vec<S>>,
    stats: ProducerStats,
    _thread_bound: PhantomData<Rc<()>>,
}
impl<T, S> Producer<T, S> {
    pub fn id(&self) -> ProducerId {
        self.id
    }
    pub fn stats(&self) -> ProducerStats {
        self.stats
    }
    #[inline(always)]
    pub fn write_timing(&mut self, record: T) {
        if !self.shared.check() {
            return;
        }
        if self.timing.is_none() {
            self.timing = self.shared.take(
                |s| &mut s.timing,
                &mut self.stats,
                &self.shared.timing_free.0,
            );
        }
        let Some(chunk) = self.timing.as_mut() else {
            return;
        };
        chunk.push(record);
        if chunk.len() == self.shared.config.chunk_capacity.get() {
            self.seal_timing();
        }
    }
    #[inline(always)]
    pub fn write_span(&mut self, record: S) {
        if !self.shared.check() {
            return;
        }
        if self.span.is_none() {
            self.span =
                self.shared
                    .take(|s| &mut s.span, &mut self.stats, &self.shared.span_free.0);
        }
        let Some(chunk) = self.span.as_mut() else {
            return;
        };
        chunk.push(record);
        if chunk.len() == self.shared.config.chunk_capacity.get() {
            self.seal_span();
        }
    }
    #[cold]
    fn seal_timing(&mut self) {
        if let Some(chunk) = self.timing.take() {
            if !self.shared.seal(self.id, Records::Timing(chunk)) {
                self.shared.check();
            }
        }
    }
    #[cold]
    fn seal_span(&mut self) {
        if let Some(chunk) = self.span.take() {
            if !self.shared.seal(self.id, Records::Span(chunk)) {
                self.shared.check();
            }
        }
    }
    /// Make both partial streams visible. Does not reacquire empty chunks, so an
    /// idle producer holds no payload allocation after this call.
    pub fn seal(&mut self) {
        self.shared.check();
        self.seal_timing();
        self.seal_span();
    }
}
impl<T, S> Drop for Producer<T, S> {
    fn drop(&mut self) {
        // This publishes already-produced data; it does NOT generate invocation
        // outcomes or completion records. Explicit VM finalization remains separate.
        if let Some(c) = self.timing.take() {
            self.shared.seal(self.id, Records::Timing(c));
        }
        if let Some(c) = self.span.take() {
            self.shared.seal(self.id, Records::Span(c));
        }
        let mut state = self.shared.lock();
        let slot = &mut state.workers[self.id.index()];
        debug_assert_eq!(slot.owner, Some(self.owner));
        slot.active = false;
        if !slot.reserved {
            slot.owner = None;
        }
        state.active_producers -= 1;
        drop(state);
        self.shared.wake.notify_one();
    }
}

/// Exclusive ownership of a sealed span allocation. Moving this lease never
/// copies its records. It may outlive the consumer and cross threads when its
/// record types are Send. It continues to count against the pool's span quota.
///
/// Drop destroys remaining payloads and returns the allocation without zeroing
/// storage. Forgetting a lease permanently withholds its capacity from producers.
pub struct SpanChunk<T, S> {
    shared: Arc<Shared<T, S>>,
    records: Vec<S>,
}

impl<T, S> SpanChunk<T, S> {
    pub fn as_slice(&self) -> &[S] {
        &self.records
    }

    /// Move records out when needed; the backing allocation stays with the lease.
    pub fn drain(&mut self) -> std::vec::Drain<'_, S> {
        self.records.drain(..)
    }
}

impl<T, S> Drop for SpanChunk<T, S> {
    fn drop(&mut self) {
        let mut guard = FailureGuard {
            shared: &self.shared,
            armed: true,
        };
        // User destructors run outside the pool lock. A destructor panic makes
        // the pool terminal rather than silently losing a counted allocation.
        self.records.clear();
        let mut state = self.shared.lock();
        if self.shared.status.load(Ordering::Acquire) == ACTIVE {
            state.span.free.push(std::mem::take(&mut self.records));
        }
        self.shared
            .span_free
            .0
            .store(state.span.free.len(), Ordering::Release);
        guard.armed = false;
    }
}

pub struct Consumer<T, S> {
    shared: Arc<Shared<T, S>>,
    finished: bool,
    _thread_bound: PhantomData<Rc<()>>,
}
impl<T, S> Consumer<T, S> {
    /// Protect processing outside a drain callback, such as a publisher flush.
    /// A panic makes the pool terminal even if input draining already completed.
    pub fn guard_processing<R>(&self, process: impl FnOnce() -> R) -> R {
        self.shared.check();
        let mut guard = FailureGuard {
            shared: &self.shared,
            armed: true,
        };
        let result = process();
        guard.armed = false;
        result
    }

    /// Consume sealed chunks in FIFO handoff order, moving records into callbacks
    /// without copying allocations. Timing recycling happens in groups of at
    /// most eight chunks; spans recycle after their callback consumes the lease.
    /// Callbacks run outside the pool lock. No ordering across pools.
    pub fn drain(
        &mut self,
        max_chunks: NonZeroUsize,
        mut timing: impl FnMut(ProducerId, T),
        mut span: impl FnMut(ProducerId, S),
    ) -> DrainStatus {
        self.drain_chunks(
            max_chunks,
            |producer, records| {
                for record in records {
                    timing(producer, record);
                }
            },
            |producer, mut records| {
                for record in records.drain() {
                    span(producer, record);
                }
            },
        )
    }

    /// Exclusive access to whole sealed chunks, in FIFO handoff order.
    ///
    /// Timing receives a scoped Drain and recycles after processing. Span receives
    /// an owned lease, which can be passed downstream and retained past callback
    /// return. Its allocation recycles only when the lease drops. Neither path
    /// copies records or zeroes storage; remaining owned payloads are destroyed.
    ///
    /// Callbacks run outside the pool mutex. A panic fails the transport and
    /// destroys remaining owned records exactly once, as with `drain`.
    pub fn drain_chunks(
        &mut self,
        max_chunks: NonZeroUsize,
        mut timing: impl FnMut(ProducerId, std::vec::Drain<'_, T>),
        mut span: impl FnMut(ProducerId, SpanChunk<T, S>),
    ) -> DrainStatus {
        use btel_settings::transport::DRAIN_BATCH_CHUNKS as BATCH;
        if !self.shared.check() {
            self.finished = true;
            return DrainStatus {
                complete: true,
                ..DrainStatus::default()
            };
        }
        let mut guard = FailureGuard {
            shared: &self.shared,
            armed: true,
        };
        let mut status = DrainStatus::default();
        // Bound stack scratch space and the number of allocations withheld from
        // producers between recycling passes. No allocation on the drain path.
        let mut batch: [Option<Chunk<T, S>>; BATCH] = std::array::from_fn(|_| None);
        while status.chunks < max_chunks.get() {
            let mut state = self
                .shared
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let count = state
                .ready
                .len()
                .min(BATCH)
                .min(max_chunks.get() - status.chunks);
            for slot in &mut batch[..count] {
                *slot = state.ready.pop_front();
            }
            if state.ready.is_empty() {
                self.shared.ready_hint.0.store(false, Ordering::Relaxed);
            }
            if count == 0 {
                self.finished = state.closed && state.active_producers == 0;
                break;
            }
            drop(state);
            // Callbacks run outside the mutex. On panic, remaining owned records
            // drop exactly once and FailureGuard makes the transport terminal.
            let mut has_timing = false;
            for slot in &mut batch[..count] {
                let Chunk { producer, records } = slot.take().unwrap();
                match records {
                    Records::Timing(mut v) => {
                        status.records += v.len();
                        timing(producer, v.drain(..));
                        *slot = Some(Chunk {
                            producer,
                            records: Records::Timing(v),
                        });
                        has_timing = true;
                    }
                    Records::Span(v) => {
                        status.records += v.len();
                        span(
                            producer,
                            SpanChunk {
                                shared: self.shared.clone(),
                                records: v,
                            },
                        );
                    }
                }
            }
            status.chunks += count;
            if !has_timing {
                continue;
            }
            let mut state = self
                .shared
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            for slot in &mut batch[..count] {
                if let Some(Chunk {
                    records: Records::Timing(v),
                    ..
                }) = slot.take()
                {
                    debug_assert!(v.is_empty());
                    if self.shared.status.load(Ordering::Acquire) == ACTIVE {
                        state.timing.free.push(v);
                    }
                }
            }
            // Serialized exact counts, published once per recycled batch.
            self.shared
                .timing_free
                .0
                .store(state.timing.free.len(), Ordering::Release);
        }
        if !self.shared.check() {
            self.finished = true;
        }
        status.complete = self.finished;
        guard.armed = false;
        status
    }
    /// Only consumers park. The condition-variable predicate and notification
    /// share the metadata mutex, so sparse partial seals cannot miss a wake.
    pub fn wait(&mut self) {
        self.wait_until(None);
    }

    /// Consumer-only deadline for periodic processing when no input arrives.
    /// The same locked predicate protects timed and untimed waits from lost wakes.
    pub fn wait_until(&mut self, deadline: Option<std::time::Instant>) {
        for _ in 0..if cfg!(baml_loom) {
            btel_settings::transport::MODEL_IDLE_PROBES
        } else {
            btel_settings::transport::IDLE_PROBES
        } {
            if !self.shared.check() {
                return;
            }
            // Avoid competing with publishers for the mutex during empty polls.
            // A stale hint only changes how soon we reach the locked recheck;
            // it never authorizes sleep or access to the queue itself.
            if self.shared.ready_hint.0.load(Ordering::Relaxed) {
                return;
            }
            sync::spin();
        }
        let mut s = self
            .shared
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while s.ready.is_empty()
            && !(s.closed && s.active_producers == 0)
            && self.shared.status.load(Ordering::Acquire) == ACTIVE
        {
            if deadline.is_some_and(|at| std::time::Instant::now() >= at) {
                break;
            }
            s.sleeping = true;
            s = if let Some(at) = deadline {
                self.shared
                    .wake
                    .wait_timeout(s, at.saturating_duration_since(std::time::Instant::now()))
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .0
            } else {
                self.shared
                    .wake
                    .wait(s)
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
            };
            s.sleeping = false;
        }
        drop(s);
        self.shared.check();
    }

    /// Readiness hint only. A producer may publish immediately after this read.
    pub fn has_ready_chunks(&self) -> bool {
        self.shared.ready_hint.0.load(Ordering::Acquire)
    }

    pub fn is_disabled(&self) -> bool {
        self.shared.status.load(Ordering::Acquire) == DISABLED
    }

    pub fn chunk_capacity(&self) -> usize {
        self.shared.config.chunk_capacity.get()
    }

    /// Fixed decoder capacity; every chunk carries an index below this bound.
    pub fn producer_capacity(&self) -> usize {
        self.shared.config.max_producers.get()
    }
}
impl<T, S> Drop for Consumer<T, S> {
    fn drop(&mut self) {
        if !self.finished {
            self.shared.fail();
        }
    }
}
struct FailureGuard<'a, T, S> {
    shared: &'a Shared<T, S>,
    armed: bool,
}
impl<T, S> Drop for FailureGuard<'_, T, S> {
    fn drop(&mut self) {
        if self.armed {
            self.shared.fail();
        }
    }
}

#[cfg(test)]
mod tests;
