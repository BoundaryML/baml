//! Typed SPSC telemetry transport. No VM, record processor, or storage format.
//!
//! A producer writes synchronously: a full ring wakes its consumer and SPINS,
//! never yielding, parking, or dropping records. Idle consumers may park. A
//! terminal registry failure panics with [`TransportFailed`]; engine integration
//! must treat that as terminal execution failure, never as a catchable BAML error.
//! Endpoints are OS-thread-bound. Single-threaded WASM can compile this crate but
//! must not use a full-buffer write without an independently progressing consumer.
#![allow(unsafe_code)]
#![allow(
    clippy::inline_always,
    reason = "measured producer writes must not add an out-of-line emission layer"
)]

mod ring;
mod sync;
mod wake;

use std::{marker::PhantomData, num::NonZeroUsize, rc::Rc};

use btel_types::TelemetryId;
use ring::{Cursor, Ring};
use sync::{Arc, AtomicBool, AtomicUsize, Mutex, Ordering, Padded, thread};
use wake::Wake;

const ACTIVE: usize = 0;
const DETACHED: usize = 1;
const REUSABLE: usize = 2;

/// Stable index of one processor within a registry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessorId(pub usize);

/// Stable allocation identity within a registry, retained across producer reuse.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PairId(usize);

impl PairId {
    pub fn index(self) -> usize {
        self.0
    }
}

/// Capacities are slot counts, not byte counts. No implicit memory allocation defaults.
#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub timing_capacity: NonZeroUsize,
    pub span_capacity: NonZeroUsize,
    pub processors: NonZeroUsize,
    /// Bounds both active and retained reusable pairs.
    pub max_pairs: NonZeroUsize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SetupError {
    InvalidCapacity,
    InvalidProcessor,
    ConsumerAlreadyBound,
    ProducerAlreadyBound,
    PairLimit,
    Closed,
    Failed,
}

/// Out-of-band terminal signal; not an ordinary runtime/BAML exception.
#[derive(Debug)]
pub struct TransportFailed;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DrainStatus {
    pub records: usize,
    /// Admission is closed and this processor has drained all released producers.
    pub complete: bool,
}

/// Reader context is separate for each stream and reset before pair reuse.
/// A concrete record processor updates it when decoding its context record.
#[derive(Default)]
pub struct ReadContext {
    pub thread: Option<TelemetryId>,
}

struct Pair<T, S> {
    id: PairId,
    processor: ProcessorId,
    state: Padded<AtomicUsize>,
    timing: Ring<T>,
    span: Ring<S>,
}

struct Inventory<T, S> {
    pairs: Vec<Arc<Pair<T, S>>>,
    producers: Vec<(thread::ThreadId, PairId)>,
    bound: Vec<bool>,
}

struct Shared<T, S> {
    config: Config,
    inventory: Mutex<Inventory<T, S>>,
    pair_count: AtomicUsize,
    closed: AtomicBool,
    failed: Padded<AtomicBool>,
    wake: Box<[Arc<Wake>]>,
}

impl<T, S> Shared<T, S> {
    #[inline(always)]
    fn check(&self) {
        if self.failed.0.load(Ordering::Acquire) {
            terminal_failure();
        }
    }

    fn fail(&self) {
        self.failed.0.store(true, Ordering::Release);
        for wake in &self.wake {
            wake.notify();
        }
    }
}

#[cold]
#[inline(never)]
fn terminal_failure() -> ! {
    std::panic::panic_any(TransportFailed)
}

/// Engine-owned inventory. Clones refer to the same registry.
pub struct RingRegistry<T, S> {
    shared: Arc<Shared<T, S>>,
}

impl<T, S> Clone for RingRegistry<T, S> {
    fn clone(&self) -> Self {
        Self {
            shared: self.shared.clone(),
        }
    }
}

impl<T: Send, S: Send> RingRegistry<T, S> {
    pub fn new(config: Config) -> Result<Self, SetupError> {
        if !Ring::<T>::valid_capacity(config.timing_capacity.get())
            || !Ring::<S>::valid_capacity(config.span_capacity.get())
        {
            return Err(SetupError::InvalidCapacity);
        }
        Ok(Self {
            shared: Arc::new(Shared {
                inventory: Mutex::new(Inventory {
                    pairs: Vec::new(),
                    producers: Vec::new(),
                    bound: vec![false; config.processors.get()],
                }),
                pair_count: AtomicUsize::new(0),
                closed: AtomicBool::new(false),
                failed: Padded(AtomicBool::new(false)),
                wake: (0..config.processors.get())
                    .map(|_| Arc::new(Wake::new()))
                    .collect(),
                config,
            }),
        })
    }

    /// Cold operation, once per engine/OS-thread binding. A logical VM thread
    /// must use the destination OS thread's lease after migration.
    pub fn register_producer(&self) -> Result<ProducerLease<T, S>, SetupError> {
        let mut inventory = self
            .shared
            .inventory
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if self.shared.failed.0.load(Ordering::Acquire) {
            return Err(SetupError::Failed);
        }
        if self.shared.closed.load(Ordering::Acquire) {
            return Err(SetupError::Closed);
        }
        let owner = thread::current().id();
        if inventory.producers.iter().any(|(id, _)| *id == owner) {
            return Err(SetupError::ProducerAlreadyBound);
        }
        let pair = if let Some(pair) = inventory
            .pairs
            .iter()
            .find(|p| p.state.0.load(Ordering::Acquire) == REUSABLE)
        {
            pair.state.0.store(ACTIVE, Ordering::Release);
            pair.clone()
        } else {
            if inventory.pairs.len() == self.shared.config.max_pairs.get() {
                return Err(SetupError::PairLimit);
            }
            let index = inventory.pairs.len();
            let pair = Arc::new(Pair {
                id: PairId(index),
                processor: ProcessorId(index % self.shared.config.processors.get()),
                state: Padded(AtomicUsize::new(ACTIVE)),
                timing: Ring::new(self.shared.config.timing_capacity.get()),
                span: Ring::new(self.shared.config.span_capacity.get()),
            });
            inventory.pairs.push(pair.clone());
            self.shared
                .pair_count
                .store(inventory.pairs.len(), Ordering::Release);
            pair
        };
        inventory.producers.push((owner, pair.id));
        drop(inventory);
        let writer = ProducerLease {
            timing: pair.timing.writer(),
            span: pair.span.writer(),
            timing_thread: None,
            span_thread: None,
            wake: self.shared.wake[pair.processor.0].clone(),
            shared: self.shared.clone(),
            pair,
            owner,
            _thread_bound: PhantomData,
        };
        writer.wake().notify();
        Ok(writer)
    }

    /// Bind on the actual processor thread, before entering its drain/wait loop.
    /// Each processor is bound at most once, including after failure.
    pub fn bind_consumer(&self, processor: ProcessorId) -> Result<Consumer<T, S>, SetupError> {
        let mut inventory = self
            .shared
            .inventory
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if self.shared.failed.0.load(Ordering::Acquire) {
            return Err(SetupError::Failed);
        }
        let bound = inventory
            .bound
            .get_mut(processor.0)
            .ok_or(SetupError::InvalidProcessor)?;
        if *bound {
            return Err(SetupError::ConsumerAlreadyBound);
        }
        *bound = true;
        self.shared.wake[processor.0].bind();
        Ok(Consumer {
            shared: self.shared.clone(),
            processor,
            pairs: Vec::new(),
            known_pairs: 0,
            finished: false,
            _thread_bound: PhantomData,
        })
    }

    /// Stops admission, not existing writes. Consumers must remain alive until
    /// all leases are released and their final records have been drained.
    pub fn close_admission(&self) {
        let inventory = self
            .shared
            .inventory
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.shared.closed.store(true, Ordering::Release);
        drop(inventory);
        for wake in &self.shared.wake {
            wake.notify();
        }
    }

    pub fn fail(&self) {
        self.shared.fail();
    }
    pub fn is_failed(&self) -> bool {
        self.shared.failed.0.load(Ordering::Acquire)
    }
    pub fn allocated_pairs(&self) -> usize {
        self.shared.pair_count.load(Ordering::Acquire)
    }
}

/// Exclusive producer access. Neither Send nor Sync: it belongs to an OS thread,
/// not a logical BAML thread, and must not be carried across async suspension.
pub struct ProducerLease<T, S> {
    shared: Arc<Shared<T, S>>,
    pair: Arc<Pair<T, S>>,
    owner: thread::ThreadId,
    wake: Arc<Wake>,
    timing: Cursor,
    span: Cursor,
    timing_thread: Option<TelemetryId>,
    span_thread: Option<TelemetryId>,
    _thread_bound: PhantomData<Rc<()>>,
}

impl<T, S> ProducerLease<T, S> {
    pub fn pair_id(&self) -> PairId {
        self.pair.id
    }
    pub fn processor_id(&self) -> ProcessorId {
        self.pair.processor
    }
    fn wake(&self) -> &Wake {
        &self.wake
    }

    /// Publishes before returning. Full means spin, never yield or park.
    #[inline(always)]
    pub fn write_timing(&mut self, record: T) {
        write(
            &self.shared,
            &self.wake,
            &self.pair.timing,
            &mut self.timing,
            record,
        );
    }

    /// Publishes before returning. Full means spin, never yield or park.
    #[inline(always)]
    pub fn write_span(&mut self, record: S) {
        write(
            &self.shared,
            &self.wake,
            &self.pair.span,
            &mut self.span,
            record,
        );
    }

    /// The context constructor is called only when this stream changes thread.
    /// Exclusive ownership is retained through BOTH publications, even if full
    /// after writing context. Records carrying their own context use `write_timing`.
    #[inline(always)]
    pub fn write_timing_in(
        &mut self,
        thread: TelemetryId,
        record: T,
        context: impl FnOnce(TelemetryId) -> T,
    ) {
        if self.timing_thread != Some(thread) {
            self.write_timing(context(thread));
            self.timing_thread = Some(thread);
        }
        self.write_timing(record);
    }

    #[inline(always)]
    pub fn write_span_in(
        &mut self,
        thread: TelemetryId,
        record: S,
        context: impl FnOnce(TelemetryId) -> S,
    ) {
        if self.span_thread != Some(thread) {
            self.write_span(context(thread));
            self.span_thread = Some(thread);
        }
        self.write_span(record);
    }
}

#[inline(always)]
fn write<T, S, R>(
    shared: &Shared<T, S>,
    wake: &Wake,
    ring: &Ring<R>,
    cursor: &mut Cursor,
    record: R,
) {
    shared.check();
    if !ring.has_space(cursor) {
        wait_for_space(shared, wake, ring, cursor);
    }
    ring.write(cursor, record);
    wake.notify();
}

/// A full producer wakes once, then exclusively spins until its slot is free.
/// Outlining keeps this retry loop and terminal failure handling off the normal
/// write instruction path. No scheduler yield, sleep, or producer parking.
#[cold]
#[inline(never)]
fn wait_for_space<T, S, R>(
    shared: &Shared<T, S>,
    wake: &Wake,
    ring: &Ring<R>,
    cursor: &mut Cursor,
) {
    wake.notify();
    loop {
        shared.check();
        if ring.has_space(cursor) {
            return;
        }
        sync::spin();
    }
}

impl<T, S> Drop for ProducerLease<T, S> {
    fn drop(&mut self) {
        let mut inventory = self
            .shared
            .inventory
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        inventory
            .producers
            .retain(|(owner, _)| *owner != self.owner);
        self.pair.state.0.store(DETACHED, Ordering::Release);
        drop(inventory);
        self.wake().notify();
    }
}

struct Reader<T, S> {
    pair: Arc<Pair<T, S>>,
    timing: Cursor,
    span: Cursor,
    timing_context: ReadContext,
    span_context: ReadContext,
}

/// One OS-thread-bound consumer for a fixed assignment of ring pairs.
/// Dropping without observing complete draining fails the entire registry.
pub struct Consumer<T, S> {
    shared: Arc<Shared<T, S>>,
    processor: ProcessorId,
    pairs: Vec<Reader<T, S>>,
    known_pairs: usize,
    finished: bool,
    _thread_bound: PhantomData<Rc<()>>,
}

impl<T, S> Consumer<T, S> {
    fn refresh(&mut self) {
        if self.known_pairs == self.shared.pair_count.load(Ordering::Acquire) {
            return;
        }
        let inventory = self
            .shared
            .inventory
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for pair in &inventory.pairs[self.known_pairs..] {
            if pair.processor == self.processor {
                self.pairs.push(Reader {
                    pair: pair.clone(),
                    timing: Cursor::default(),
                    span: Cursor::default(),
                    timing_context: ReadContext::default(),
                    span_context: ReadContext::default(),
                });
            }
        }
        self.known_pairs = inventory.pairs.len();
    }

    fn complete(&mut self) -> bool {
        if !self.shared.closed.load(Ordering::Acquire) {
            return false;
        }
        // Admission's release precedes this refresh: no last registration can
        // be omitted just because this worker previously observed empty storage.
        self.refresh();
        self.pairs
            .iter()
            .all(|reader| reader.pair.state.0.load(Ordering::Acquire) == REUSABLE)
    }

    /// One fair sweep: at most `batch` records from EACH stream of EACH pair.
    /// Payload ownership passes to callbacks; no registry lock/heap permit is
    /// held. A callback panic fails the registry even if caught by its caller.
    pub fn drain(
        &mut self,
        batch: NonZeroUsize,
        mut timing: impl FnMut(PairId, &mut ReadContext, T),
        mut span: impl FnMut(PairId, &mut ReadContext, S),
    ) -> DrainStatus {
        self.shared.check();
        self.refresh();
        let mut guard = DrainGuard {
            shared: &self.shared,
            armed: true,
        };
        let mut records = 0;
        for reader in &mut self.pairs {
            // Observe detachment BEFORE reading final tails; a previous empty
            // observation alone must never certify reuse.
            let detached = reader.pair.state.0.load(Ordering::Acquire) == DETACHED;
            for _ in 0..batch.get() {
                let Some(value) = reader.pair.timing.read(&mut reader.timing) else {
                    break;
                };
                records += 1;
                timing(reader.pair.id, &mut reader.timing_context, value);
            }
            for _ in 0..batch.get() {
                let Some(value) = reader.pair.span.read(&mut reader.span) else {
                    break;
                };
                records += 1;
                span(reader.pair.id, &mut reader.span_context, value);
            }
            if detached
                && reader.pair.timing.is_empty(&reader.timing)
                && reader.pair.span.is_empty(&reader.span)
            {
                reader.timing_context = ReadContext::default();
                reader.span_context = ReadContext::default();
                reader.pair.state.0.store(REUSABLE, Ordering::Release);
            }
        }
        guard.armed = false;
        drop(guard);
        self.shared.check();
        self.finished = self.complete();
        DrainStatus {
            records,
            complete: self.finished,
        }
    }

    fn has_work(&self) -> bool {
        self.pairs.iter().any(|reader| {
            reader.pair.state.0.load(Ordering::Acquire) == DETACHED
                || !reader.pair.timing.is_empty(&reader.timing)
                || !reader.pair.span.is_empty(&reader.span)
        })
    }

    /// Briefly spin, then park only the idle CONSUMER. Arm then recheck all
    /// assigned queues and lifecycle state; a racing write deposits a token
    /// before or during park. Spurious returns are allowed.
    pub fn wait(&mut self) {
        self.shared.check();
        if self.finished {
            return;
        }
        // Fixed probe budget, no clock read. This avoids park/unpark ping-pong
        // during short producer gaps without permanently consuming an idle CPU.
        // Loom needs only one probe to explore the same arm/recheck protocol.
        let probes = if cfg!(baml_loom) { 1 } else { 64 };
        self.refresh();
        for _ in 0..probes {
            if self.has_work() {
                return;
            }
            sync::spin();
        }
        self.shared.wake[self.processor.0].arm();
        self.refresh();
        if !self.has_work() && !self.complete() {
            self.shared.check();
            Wake::park();
        }
        self.shared.wake[self.processor.0].disarm();
        self.shared.check();
    }
}

struct DrainGuard<'a, T, S> {
    shared: &'a Shared<T, S>,
    armed: bool,
}
impl<T, S> Drop for DrainGuard<'_, T, S> {
    fn drop(&mut self) {
        if self.armed {
            self.shared.fail();
        }
    }
}

impl<T, S> Drop for Consumer<T, S> {
    fn drop(&mut self) {
        if !self.finished {
            self.shared.fail();
        }
    }
}

#[cfg(test)]
mod tests;
