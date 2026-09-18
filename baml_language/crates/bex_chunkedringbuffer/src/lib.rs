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
//! ready, or processor-owned. Producers spin on exhaustion and metadata contention;
//! only the idle consumer parks. No production VM integration is provided here.
#![allow(
    clippy::inline_always,
    reason = "ordinary append must inline to private Vec writes"
)]

mod sync;

use std::{collections::VecDeque, marker::PhantomData, mem::size_of, num::NonZeroUsize, rc::Rc};

use sync::{
    Arc, AtomicBool, AtomicUsize, Condvar, Mutex, MutexGuard, Ordering, Padded, TryLockError,
    thread,
};

#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub chunk_capacity: NonZeroUsize,
    pub timing_chunks: NonZeroUsize,
    pub span_chunks: NonZeroUsize,
    /// Each lane needs at least this many chunks, otherwise writers can exhaust
    /// it with PRIVATE partial chunks that the consumer cannot reclaim.
    pub max_producers: NonZeroUsize,
    pub preallocate: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SetupError {
    InvalidCapacity,
    InsufficientChunks,
    Closed,
    Failed,
    ProducerLimit,
    ProducerAlreadyBound,
    ConsumerAlreadyBound,
    IdentityExhausted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct ProducerId(usize);
impl ProducerId {
    pub fn index(self) -> usize {
        self.0
    }
}

/// Terminal signal, never a normal BAML exception. Engine integration is pending.
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
    producers: Vec<(thread::ThreadId, ProducerId)>,
    next_id: usize,
    closed: bool,
    bound: bool,
    sleeping: bool,
    sealed_chunks: usize,
    sealed_records: usize,
    wakeups: usize,
}
struct Shared<T, S> {
    state: Mutex<State<T, S>>,
    wake: Condvar,
    failed: AtomicBool,
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
    fn check(&self) {
        if self.failed.load(Ordering::Acquire) {
            terminal();
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
        self.failed.store(true, Ordering::Release);
        drop(guard);
        self.wake.notify_all();
    }
    #[cold]
    fn take<R>(
        &self,
        lane: impl Fn(&mut State<T, S>) -> &mut Lane<R>,
        stats: &mut ProducerStats,
        available: &AtomicUsize,
    ) -> Vec<R> {
        let mut exhausted = false;
        loop {
            self.check();
            let mut state = self.lock();
            let lane = lane(&mut state);
            if let Some(chunk) = lane.free.pop() {
                // All free-list mutations are serialized by state. Publish its
                // exact length; no read-modify-write is needed.
                available.store(lane.free.len(), Ordering::Release);
                return chunk;
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
                return chunk;
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
                self.check();
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
        if self.failed.load(Ordering::Acquire) {
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
            || !fits(
                config.max_producers.get(),
                size_of::<(thread::ThreadId, ProducerId)>(),
            )
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
                    producers: Vec::with_capacity(config.max_producers.get()),
                    next_id: 0,
                    closed: false,
                    bound: false,
                    sleeping: false,
                    sealed_chunks: 0,
                    sealed_records: 0,
                    wakeups: 0,
                }),
                wake: Condvar::new(),
                failed: AtomicBool::new(false),
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
        let mut state = self.shared.lock();
        if self.shared.failed.load(Ordering::Acquire) {
            return Err(SetupError::Failed);
        }
        if state.closed {
            return Err(SetupError::Closed);
        }
        let owner = thread::current().id();
        if state.producers.iter().any(|(id, _)| *id == owner) {
            return Err(SetupError::ProducerAlreadyBound);
        }
        if state.producers.len() == self.shared.config.max_producers.get() {
            return Err(SetupError::ProducerLimit);
        }
        let id = ProducerId(state.next_id);
        state.next_id = state
            .next_id
            .checked_add(1)
            .ok_or(SetupError::IdentityExhausted)?;
        state.producers.push((owner, id));
        Ok(Producer {
            shared: self.shared.clone(),
            id,
            owner,
            timing: None,
            span: None,
            stats: ProducerStats::default(),
            _thread_bound: PhantomData,
        })
    }
    pub fn bind_consumer(&self) -> Result<Consumer<T, S>, SetupError> {
        let mut state = self.shared.lock();
        if self.shared.failed.load(Ordering::Acquire) {
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
    pub fn is_failed(&self) -> bool {
        self.shared.failed.load(Ordering::Acquire)
    }
    pub fn stats(&self) -> Stats {
        let s = self.shared.lock();
        Stats {
            allocated_timing_chunks: s.timing.allocated,
            allocated_span_chunks: s.span.allocated,
            free_chunks: s.timing.free.len() + s.span.free.len(),
            ready_chunks: s.ready.len(),
            active_producers: s.producers.len(),
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
        self.shared.check();
        if self.timing.is_none() {
            self.timing = Some(self.shared.take(
                |s| &mut s.timing,
                &mut self.stats,
                &self.shared.timing_free.0,
            ));
        }
        let chunk = self.timing.as_mut().unwrap();
        chunk.push(record);
        if chunk.len() == self.shared.config.chunk_capacity.get() {
            self.seal_timing();
        }
    }
    #[inline(always)]
    pub fn write_span(&mut self, record: S) {
        self.shared.check();
        if self.span.is_none() {
            self.span = Some(self.shared.take(
                |s| &mut s.span,
                &mut self.stats,
                &self.shared.span_free.0,
            ));
        }
        let chunk = self.span.as_mut().unwrap();
        chunk.push(record);
        if chunk.len() == self.shared.config.chunk_capacity.get() {
            self.seal_span();
        }
    }
    #[cold]
    fn seal_timing(&mut self) {
        if let Some(chunk) = self.timing.take() {
            if !self.shared.seal(self.id, Records::Timing(chunk)) {
                terminal();
            }
        }
    }
    #[cold]
    fn seal_span(&mut self) {
        if let Some(chunk) = self.span.take() {
            if !self.shared.seal(self.id, Records::Span(chunk)) {
                terminal();
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
        state.producers.retain(|(id, _)| *id != self.owner);
        drop(state);
        self.shared.wake.notify_one();
    }
}

pub struct Consumer<T, S> {
    shared: Arc<Shared<T, S>>,
    finished: bool,
    _thread_bound: PhantomData<Rc<()>>,
}
impl<T, S> Consumer<T, S> {
    /// Consume sealed chunks in FIFO handoff order, moving records into callbacks
    /// without copying allocations. Recycling happens in groups of at most eight
    /// chunks; callbacks run outside the pool lock. No ordering across pools.
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
            |producer, records| {
                for record in records {
                    span(producer, record);
                }
            },
        )
    }

    /// Exclusive access to whole sealed chunks, in FIFO handoff order.
    ///
    /// Each callback receives an ownership-moving iterator over one chunk. It
    /// may consume records or simply drop the iterator to discard the contents.
    /// Unconsumed records are destroyed without zeroing the backing storage;
    /// records without destructors need no per-record work. The allocation stays
    /// with the pool and is recycled after the callback returns. Iterators cannot
    /// outlive the callback. Do not forget them: doing so leaks unconsumed payloads.
    ///
    /// Callbacks run outside the pool mutex. A panic fails the transport and
    /// destroys remaining owned records exactly once, as with `drain`.
    pub fn drain_chunks(
        &mut self,
        max_chunks: NonZeroUsize,
        mut timing: impl FnMut(ProducerId, std::vec::Drain<'_, T>),
        mut span: impl FnMut(ProducerId, std::vec::Drain<'_, S>),
    ) -> DrainStatus {
        const BATCH: usize = 8;
        self.shared.check();
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
                self.finished = state.closed && state.producers.is_empty();
                break;
            }
            drop(state);
            // Callbacks run outside the mutex. On panic, remaining owned records
            // drop exactly once and FailureGuard makes the transport terminal.
            for slot in &mut batch[..count] {
                let Chunk { producer, records } = slot.as_mut().unwrap();
                match records {
                    Records::Timing(v) => {
                        status.records += v.len();
                        timing(*producer, v.drain(..));
                    }
                    Records::Span(v) => {
                        status.records += v.len();
                        span(*producer, v.drain(..));
                    }
                }
            }
            let mut state = self
                .shared
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            for slot in &mut batch[..count] {
                match slot.take().unwrap().records {
                    Records::Timing(v) => {
                        debug_assert!(v.is_empty());
                        state.timing.free.push(v);
                    }
                    Records::Span(v) => {
                        debug_assert!(v.is_empty());
                        state.span.free.push(v);
                    }
                }
            }
            // Serialized exact counts, published once per recycled batch.
            self.shared
                .timing_free
                .0
                .store(state.timing.free.len(), Ordering::Release);
            self.shared
                .span_free
                .0
                .store(state.span.free.len(), Ordering::Release);
            status.chunks += count;
        }
        self.shared.check();
        status.complete = self.finished;
        guard.armed = false;
        status
    }
    /// Only consumers park. The condition-variable predicate and notification
    /// share the metadata mutex, so sparse partial seals cannot miss a wake.
    pub fn wait(&mut self) {
        for _ in 0..if cfg!(baml_loom) { 1 } else { 64 } {
            self.shared.check();
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
            && !(s.closed && s.producers.is_empty())
            && !self.shared.failed.load(Ordering::Acquire)
        {
            s.sleeping = true;
            s = self
                .shared
                .wake
                .wait(s)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            s.sleeping = false;
        }
        drop(s);
        self.shared.check();
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
