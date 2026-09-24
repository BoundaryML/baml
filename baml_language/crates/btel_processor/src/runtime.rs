use std::{
    cell::RefCell,
    future::{Future, poll_fn},
    marker::PhantomData,
    num::NonZeroUsize,
    pin::pin,
    rc::Rc,
    sync::{
        Arc, Weak,
        atomic::{AtomicU64, Ordering},
    },
};

use bex_chunkedringbuffer::{
    ChunkPool, Config, Producer, SetupError, Stats, TransportFailed, WorkerSlot,
};
use btel_records::{CaptureDeferred, SpanRecord, TimingRecord};
use btel_types::TelemetryId;

use crate::Processor;

type Span = SpanRecord<CaptureDeferred, CaptureDeferred>;
type Pool = ChunkPool<TimingRecord, Span>;
type BoundProcessor = Processor<CaptureDeferred, CaptureDeferred>;

static NEXT_RUNTIME: AtomicU64 = AtomicU64::new(0);

struct Binding {
    runtime: u64,
    depth: usize,
    producer: Option<Producer<TimingRecord, Span>>,
    worker: Option<WorkerSlot>,
    owner: Weak<TelemetryRuntime>,
    // Last emitted context per independent stream, not the scheduled thread.
    // Silent invocations and chunk publication leave these unchanged.
    timing_thread: Option<TelemetryId>,
    span_thread: Option<TelemetryId>,
}

impl Drop for Binding {
    fn drop(&mut self) {
        // TLS retirement seals any active producer before releasing its slot.
        drop(self.producer.take());
        if let (Some(runtime), Some(worker)) = (self.owner.upgrade(), self.worker.take()) {
            runtime.pool.release_worker(worker);
        }
    }
}

thread_local! {
    // One predetermined slot per engine/OS worker, retained across polls. Weak
    // owners keep idle bindings from extending engine or pool lifetimes.
    static BINDINGS: RefCell<Vec<Binding>> = const { RefCell::new(Vec::new()) };
}

/// Engine-owned chunk transport and processor lifetime. VM instances retain an
/// Arc while executing; the worker owns only the pool, never the engine/runtime.
/// The last runtime owner closes admission, consumes outstanding chunks and joins.
pub struct TelemetryRuntime {
    id: u64,
    pool: Pool,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl TelemetryRuntime {
    /// Share a producer and private partial chunks across one future poll.
    /// The thread-bound scope lives on the poll stack, never in the suspended
    /// future. Full chunks publish immediately; partial chunks publish when the
    /// outermost scope exits on Pending, completion or unwinding. A later poll
    /// can safely bind on a different OS thread.
    pub async fn scope<F: Future>(self: &Arc<Self>, future: F) -> F::Output {
        let mut future = pin!(future);
        poll_fn(|cx| {
            let _scope = self.enter();
            future.as_mut().poll(cx)
        })
        .await
    }

    /// Lazy chunks, bounded to two chunks per lane per admitted OS thread.
    /// The floor accommodates host thread pools larger than available CPUs.
    pub fn new() -> std::io::Result<Arc<Self>> {
        let producers = std::thread::available_parallelism().map_or(64, |n| n.get().max(64));
        Self::with_config(Config {
            chunk_capacity: NonZeroUsize::new(256).unwrap(),
            timing_chunks: NonZeroUsize::new(producers * 2).unwrap(),
            span_chunks: NonZeroUsize::new(producers * 2).unwrap(),
            max_producers: NonZeroUsize::new(producers).unwrap(),
            preallocate: false,
        })
    }

    pub fn with_config(config: Config) -> std::io::Result<Arc<Self>> {
        let id = NEXT_RUNTIME
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .expect("telemetry runtime identity space exhausted");
        let pool = Pool::new(config).map_err(|e| std::io::Error::other(format!("{e:?}")))?;
        let worker = {
            let copy = pool.clone();
            let (ready, bound) = std::sync::mpsc::sync_channel(1);
            let worker = std::thread::Builder::new()
                .name("btel-processor".into())
                .spawn(move || {
                    let consumer = match copy.bind_consumer() {
                        Ok(consumer) => consumer,
                        Err(error) => {
                            let _ = ready.send(Err(error));
                            return;
                        }
                    };
                    if ready.send(Ok(())).is_err() {
                        return;
                    }
                    // The consumer's guard marks failure before a panic escapes.
                    let _ = BoundProcessor::new(consumer, NonZeroUsize::new(8).unwrap()).run();
                })?;
            match bound.recv() {
                Ok(Ok(())) => worker,
                result => {
                    pool.fail();
                    let _ = worker.join();
                    return Err(std::io::Error::other(format!(
                        "processor startup: {result:?}"
                    )));
                }
            }
        };
        Ok(Arc::new(Self {
            id,
            pool,
            worker: Some(worker),
        }))
    }

    /// Bind on the current OS thread for a synchronous VM operation. The scope
    /// is !Send; the outermost scope releases/seals, including unwinding. It must
    /// not survive an async suspension. No outcome-sensitive events originate
    /// in its destructor; the VM/engine still explicitly complete invocations.
    /// The worker slot and stream selectors survive across polls. Only the
    /// active producer and its private payload chunks are released on suspension.
    pub fn enter(self: &Arc<Self>) -> ExecutionScope {
        BINDINGS.with_borrow_mut(|bindings| {
            if let Some(binding) = bindings.iter_mut().find(|b| b.runtime == self.id) {
                if binding.depth == 0 {
                    binding.producer = Some(self.bind_worker(binding.worker.as_ref().unwrap()));
                }
                binding.depth += 1;
                return;
            }
            // Cold first attachment only. Dead engines leave no pool ownership;
            // remove their tiny TLS entries before attaching another engine.
            bindings.retain(|binding| binding.owner.strong_count() != 0);
            let worker = loop {
                match self.pool.reserve_worker() {
                    Ok(worker) => break worker,
                    Err(SetupError::ProducerLimit) => std::hint::spin_loop(),
                    Err(_) => {
                        self.pool.fail();
                        std::panic::panic_any(TransportFailed);
                    }
                }
            };
            let producer = self.bind_worker(&worker);
            bindings.push(Binding {
                runtime: self.id,
                depth: 1,
                producer: Some(producer),
                worker: Some(worker),
                owner: Arc::downgrade(self),
                timing_thread: None,
                span_thread: None,
            });
        });
        ExecutionScope {
            runtime: Arc::clone(self),
            _thread_bound: PhantomData,
        }
    }

    fn bind_worker(&self, worker: &WorkerSlot) -> Producer<TimingRecord, Span> {
        self.pool.bind_worker(worker).unwrap_or_else(|_| {
            self.pool.fail();
            std::panic::panic_any(TransportFailed);
        })
    }

    #[inline]
    pub fn write_timing(&self, thread: TelemetryId, record: TimingRecord) {
        BINDINGS.with_borrow_mut(|bindings| {
            let binding = bindings
                .iter_mut()
                .rev()
                .find(|b| b.runtime == self.id)
                .expect("telemetry emission outside execution scope");
            let producer = binding
                .producer
                .as_mut()
                .expect("telemetry emission outside execution scope");
            if binding.timing_thread != Some(thread) {
                producer.write_timing(TimingRecord::ThreadSelected { thread_id: thread });
                binding.timing_thread = Some(thread);
            }
            producer.write_timing(record);
        });
    }

    #[inline]
    pub fn write_span(&self, thread: TelemetryId, record: Span) {
        BINDINGS.with_borrow_mut(|bindings| {
            let binding = bindings
                .iter_mut()
                .rev()
                .find(|b| b.runtime == self.id)
                .expect("telemetry emission outside execution scope");
            let producer = binding
                .producer
                .as_mut()
                .expect("telemetry emission outside execution scope");
            if binding.span_thread != Some(thread) {
                producer.write_span(SpanRecord::ThreadSelected { thread_id: thread });
                binding.span_thread = Some(thread);
            }
            producer.write_span(record);
        });
    }

    pub fn stats(&self) -> Stats {
        self.pool.stats()
    }
}

/// Synchronous ownership boundary; stores no references into a VM or its heap.
pub struct ExecutionScope {
    runtime: Arc<TelemetryRuntime>,
    _thread_bound: PhantomData<Rc<()>>,
}

impl Drop for ExecutionScope {
    fn drop(&mut self) {
        let released = BINDINGS.with_borrow_mut(|bindings| {
            let index = bindings
                .iter()
                .rposition(|b| b.runtime == self.runtime.id)
                .unwrap();
            bindings[index].depth -= 1;
            (bindings[index].depth == 0)
                .then(|| bindings[index].producer.take())
                .flatten()
        });
        // Release outside the TLS borrow. Producer drop publishes partial chunks
        // and deactivates before shutdown can wait. Its worker slot stays bound.
        drop(released);
        if !std::thread::panicking() && self.runtime.pool.is_failed() {
            std::panic::panic_any(TransportFailed);
        }
    }
}

impl Drop for TelemetryRuntime {
    fn drop(&mut self) {
        self.pool.close_admission();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        panic::{AssertUnwindSafe, catch_unwind},
        task::{Context, Poll, Waker},
    };

    use btel_types::allocate_telemetry_id;

    use super::*;

    fn config() -> Config {
        Config {
            chunk_capacity: NonZeroUsize::new(8).unwrap(),
            timing_chunks: NonZeroUsize::new(4).unwrap(),
            span_chunks: NonZeroUsize::new(4).unwrap(),
            max_producers: NonZeroUsize::new(2).unwrap(),
            preallocate: true,
        }
    }

    // Keep the actual records for assertions; no production observer/sink hook.
    fn manual_runtime() -> (Arc<TelemetryRuntime>, Pool) {
        let pool = Pool::new(config()).unwrap();
        let runtime = Arc::new(TelemetryRuntime {
            id: NEXT_RUNTIME.fetch_add(1, Ordering::Relaxed),
            pool: pool.clone(),
            worker: None,
        });
        (runtime, pool)
    }

    #[test]
    fn selectors_follow_emitted_records_independently_per_stream() {
        use btel_types::{AwaitDuration, CallPathId, ClockInstant, InvocationOutcome};
        let (runtime, pool) = manual_runtime();
        let mut consumer = pool.bind_consumer().unwrap();
        let a = allocate_telemetry_id();
        let b = allocate_telemetry_id();
        let timing = || TimingRecord::FunctionTimingCompletion {
            call_path: CallPathId::ROOT,
            entered_at: ClockInstant::from_ticks(1),
            exited_at: ClockInstant::from_ticks(2),
            await_time: AwaitDuration::ZERO,
            outcome: InvocationOutcome::Ok,
            reentry: false,
        };
        let span = |parent_id| SpanRecord::FunctionSpanAnnouncement {
            id: allocate_telemetry_id(),
            parent_id,
            call_path: CallPathId::ROOT,
            entered_at: ClockInstant::from_ticks(1),
            captured_inputs: None,
        };
        {
            let _poll = runtime.enter();
            runtime.write_timing(a, timing());
            // A different invocation can run without producing any records.
            // Binding/resuming alone must not alter either stream's context.
            {
                let _silent_invocation = runtime.enter();
            }
            runtime.write_timing(a, timing());
            runtime.write_span(b, span(b));
            runtime.write_timing(a, timing()); // Span B did not change Timing A.
            runtime.write_timing(b, timing());
            runtime.write_timing(a, timing());
            runtime.write_span(b, span(b));
            runtime.write_span(a, span(a));
            // Timing filled a chunk. A new chunk of the SAME producer keeps
            // its stream context; it does not need a redundant selector.
            assert_eq!(pool.stats().sealed_chunks, 1);
            runtime.write_timing(a, timing());
        }
        pool.close_admission();
        let (mut timing_current, mut span_current) = (None, None);
        let (mut timing_selectors, mut span_selectors) = (Vec::new(), Vec::new());
        let (mut timings, mut spans) = (Vec::new(), Vec::new());
        let drained = consumer.drain_chunks(
            NonZeroUsize::new(8).unwrap(),
            |_, records| {
                for record in records {
                    match record {
                        TimingRecord::ThreadSelected { thread_id } => {
                            timing_current = Some(thread_id);
                            timing_selectors.push(thread_id);
                        }
                        TimingRecord::FunctionTimingCompletion { .. } => {
                            timings.push(timing_current.expect("timing context"));
                        }
                    }
                }
            },
            |_, mut records| {
                for record in records.drain() {
                    match record {
                        SpanRecord::ThreadSelected { thread_id } => {
                            span_current = Some(thread_id);
                            span_selectors.push(thread_id);
                        }
                        SpanRecord::FunctionSpanAnnouncement { parent_id, .. } => {
                            assert_eq!(span_current, Some(parent_id));
                            spans.push(span_current.expect("span context"));
                        }
                        _ => unreachable!(),
                    }
                }
            },
        );
        assert!(drained.complete);
        assert_eq!(timing_selectors, [a, b, a]);
        assert_eq!(timings, [a, a, a, b, a, a]);
        assert_eq!(span_selectors, [b, a]);
        assert_eq!(spans, [b, b, a]);
        assert!(!pool.is_failed());
    }

    #[test]
    fn poll_scope_publishes_before_pending_and_rebinds_after_migration() {
        let (runtime, pool) = manual_runtime();
        let mut consumer = pool.bind_consumer().unwrap();
        let id = allocate_telemetry_id();
        let mut first = true;
        let execution = poll_fn(|_| {
            // Independent VM operations within one poll reuse the same binding.
            for _ in 0..2 {
                let _scope = runtime.enter();
                runtime.write_timing(id, TimingRecord::ThreadSelected { thread_id: id });
            }
            assert_eq!(pool.stats().active_producers, 1);
            if std::mem::take(&mut first) {
                Poll::Pending
            } else {
                Poll::Ready(())
            }
        });
        let mut execution = pin!(runtime.scope(execution));
        assert!(
            execution
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
        assert_eq!(pool.stats().active_producers, 0);
        assert_eq!(pool.stats().sealed_chunks, 1);
        assert_eq!(pool.stats().sealed_records, 3);
        // Moving the pinned reference also proves the future remains Send:
        // the !Send producer guard must not be stored across suspension.
        std::thread::scope(|threads| {
            threads
                .spawn(|| {
                    assert!(
                        execution
                            .as_mut()
                            .poll(&mut Context::from_waker(Waker::noop()))
                            .is_ready()
                    );
                })
                .join()
                .unwrap();
        });
        assert_eq!(pool.stats().active_producers, 0);
        assert_eq!(pool.stats().sealed_chunks, 2);
        pool.close_admission();
        let mut producers = Vec::new();
        let drained = consumer.drain_chunks(
            NonZeroUsize::new(8).unwrap(),
            |producer, records| {
                producers.push(producer);
                assert_eq!(records.len(), 3);
            },
            |_, _| unreachable!(),
        );
        assert!(drained.complete);
        assert_ne!(producers[0], producers[1]);
    }

    #[test]
    fn poll_scope_releases_on_panic_and_abandonment() {
        let (runtime, pool) = manual_runtime();
        let mut consumer = pool.bind_consumer().unwrap();
        let id = allocate_telemetry_id();
        {
            let mut execution = pin!(runtime.scope(async {
                runtime.write_timing(id, TimingRecord::ThreadSelected { thread_id: id });
                std::future::pending::<()>().await;
            }));
            assert!(
                execution
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
            );
            assert_eq!(pool.stats().active_producers, 0);
            assert_eq!(pool.stats().sealed_records, 2);
        }
        let panic = catch_unwind(AssertUnwindSafe(|| {
            let mut execution = pin!(runtime.scope(async {
                runtime.write_timing(id, TimingRecord::ThreadSelected { thread_id: id });
                panic!("execution failed");
            }));
            let _ = execution
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()));
        }));
        assert!(panic.is_err());
        assert_eq!(pool.stats().active_producers, 0);
        assert_eq!(pool.stats().sealed_records, 3); // Same worker keeps its selector.
        pool.close_admission();
        assert!(
            consumer
                .drain_chunks(
                    NonZeroUsize::new(8).unwrap(),
                    |_, records| drop(records),
                    |_, _| unreachable!()
                )
                .complete
        );
        assert!(!pool.is_failed());
    }

    #[test]
    fn scopes_publish_context_and_deferred_capture_then_rebind_after_migration() {
        let (runtime, pool) = manual_runtime();
        let mut consumer = pool.bind_consumer().unwrap();
        let thread_id = allocate_telemetry_id();
        {
            let _scope = runtime.enter();
            runtime.write_timing(thread_id, TimingRecord::ThreadSelected { thread_id });
            let nested = runtime.enter();
            runtime.write_span(
                thread_id,
                SpanRecord::FunctionSpanAnnouncement {
                    id: allocate_telemetry_id(),
                    parent_id: thread_id,
                    call_path: btel_types::CallPathId::ROOT,
                    entered_at: btel_types::ClockInstant::from_ticks(1),
                    captured_inputs: Some(Box::new(CaptureDeferred)),
                },
            );
            drop(nested);
            assert_eq!(pool.stats().sealed_records, 0);
            assert_eq!(pool.stats().active_producers, 1);
        }
        assert_eq!(pool.stats().active_producers, 0);
        assert_eq!(pool.stats().sealed_records, 4);
        let copy = Arc::clone(&runtime);
        std::thread::spawn(move || {
            let _scope = copy.enter();
            copy.write_timing(thread_id, TimingRecord::ThreadSelected { thread_id });
        })
        .join()
        .unwrap();
        drop(runtime);
        let mut contexts = Vec::new();
        let mut spans = Vec::new();
        assert!(
            consumer
                .drain(
                    NonZeroUsize::new(8).unwrap(),
                    |producer, record| {
                        let TimingRecord::ThreadSelected {
                            thread_id: selected,
                        } = record
                        else {
                            panic!()
                        };
                        assert_eq!(selected, thread_id);
                        contexts.push(producer);
                    },
                    |_, record| spans.push(record)
                )
                .complete
        );
        assert_eq!(contexts.len(), 4);
        assert_ne!(
            contexts[0], contexts[2],
            "migration obtains a new exclusive producer"
        );
        assert!(
            matches!(&spans[0], SpanRecord::ThreadSelected { thread_id: id } if *id == thread_id)
        );
        assert!(matches!(
            &spans[1],
            SpanRecord::FunctionSpanAnnouncement {
                captured_inputs: Some(_),
                ..
            }
        ));
        assert!(!pool.is_failed());
    }

    #[test]
    fn worker_slots_survive_polls_retire_with_threads_and_do_not_keep_runtime_alive() {
        use btel_types::{AwaitDuration, CallPathId, ClockInstant, InvocationOutcome};
        let (runtime, pool) = manual_runtime();
        let mut consumer = pool.bind_consumer().unwrap();
        for _ in 0..8 {
            let copy = runtime.clone();
            std::thread::spawn(move || {
                let id = allocate_telemetry_id();
                for _ in 0..2 {
                    let _poll = copy.enter();
                    copy.write_timing(
                        id,
                        TimingRecord::FunctionTimingCompletion {
                            call_path: CallPathId::new_non_root(1).unwrap(),
                            entered_at: ClockInstant::from_ticks(1),
                            exited_at: ClockInstant::from_ticks(2),
                            await_time: AwaitDuration::ZERO,
                            outcome: InvocationOutcome::Ok,
                            reentry: false,
                        },
                    );
                }
            })
            .join()
            .unwrap();
            let mut chunks = Vec::new();
            consumer.drain_chunks(
                NonZeroUsize::new(8).unwrap(),
                |id, records| {
                    chunks.push((id.index(), records.len()));
                },
                |_, _| unreachable!(),
            );
            // Stable slot, no duplicate selector in poll two; TLS retirement
            // releases it so subsequent OS workers reuse the bounded table.
            assert_eq!(chunks, [(0, 2), (0, 1)]);
            assert_eq!(Arc::strong_count(&runtime), 1);
        }
        // An idle binding on a still-live OS thread is weak as well.
        drop(runtime.enter());
        let weak = Arc::downgrade(&runtime);
        drop(runtime);
        assert!(weak.upgrade().is_none());
        assert!(
            consumer
                .drain(
                    NonZeroUsize::new(8).unwrap(),
                    |_, _| unreachable!(),
                    |_, _| unreachable!()
                )
                .complete
        );
    }

    #[test]
    fn nested_engines_and_unwind_release_every_producer_before_shutdown() {
        let first = TelemetryRuntime::with_config(config()).unwrap();
        let second = TelemetryRuntime::with_config(config()).unwrap();
        let first_pool = first.pool.clone();
        let second_pool = second.pool.clone();
        let id = allocate_telemetry_id();
        let panic = catch_unwind(AssertUnwindSafe(|| {
            let _outer = first.enter();
            first.write_timing(id, TimingRecord::ThreadSelected { thread_id: id });
            let _other = second.enter();
            let _reentry = first.enter();
            first.write_timing(id, TimingRecord::ThreadSelected { thread_id: id });
            second.write_timing(id, TimingRecord::ThreadSelected { thread_id: id });
            panic!("execution failed");
        }));
        assert!(panic.is_err());
        assert_eq!(first.stats().active_producers, 0);
        assert_eq!(second.stats().active_producers, 0);
        drop(first);
        drop(second);
        // Runtime destruction joins the worker after all published records have
        // been consumed, without an engine reference cycle or idle TLS lease.
        assert_eq!(first_pool.stats().sealed_records, 3);
        assert_eq!(second_pool.stats().sealed_records, 2);
        assert_eq!(first_pool.stats().ready_chunks, 0);
        assert_eq!(second_pool.stats().ready_chunks, 0);
        assert_eq!(first_pool.stats().free_chunks, 8);
        assert!(!first_pool.is_failed());
        assert!(!second_pool.is_failed());
    }

    #[test]
    fn failed_processor_prevents_execution_from_rebinding() {
        let runtime = TelemetryRuntime::with_config(config()).unwrap();
        runtime.pool.fail();
        let panic = catch_unwind(AssertUnwindSafe(|| runtime.enter()))
            .err()
            .unwrap();
        assert!(panic.is::<TransportFailed>());
        assert_eq!(runtime.stats().active_producers, 0);
        // Also joins the failed worker instead of waiting forever for progress.
        drop(runtime);
    }
}
