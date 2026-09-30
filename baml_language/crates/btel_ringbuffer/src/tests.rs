use super::*;

fn nz(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).unwrap()
}
fn config(capacity: usize) -> Config {
    Config {
        timing_capacity: nz(capacity),
        span_capacity: nz(capacity),
        processors: nz(1),
        max_pairs: nz(4),
    }
}

fn model(f: impl Fn() + Send + Sync + 'static) {
    #[cfg(baml_loom)]
    {
        let mut model = loom::model::Builder::new();
        model.preemption_bound = Some(2);
        model.max_branches = 10_000;
        model.check(f);
    }
    #[cfg(not(baml_loom))]
    f();
}

#[test]
fn slot_handoff_wraparound_and_unread_destruction() {
    model(|| {
        struct Owned(usize, Arc<AtomicUsize>);
        impl Drop for Owned {
            fn drop(&mut self) {
                self.1.fetch_add(1, Ordering::Relaxed);
            }
        }
        let drops = Arc::new(AtomicUsize::new(0));
        let ring = Arc::new(Ring::new(1));
        ring.seed_empty(usize::MAX);
        let mut cursor = ring.writer();
        let producer_ring = ring.clone();
        let producer_drops = drops.clone();
        let producer = thread::spawn(move || {
            let mut cursor = producer_ring.writer();
            for value in [11, 22] {
                while !producer_ring.has_space(&mut cursor) {
                    sync::spin();
                }
                producer_ring.write(&mut cursor, Owned(value, producer_drops.clone()));
            }
        });
        let value = loop {
            if let Some(value) = ring.read(&mut cursor) {
                break value;
            }
            sync::spin();
        };
        assert_eq!(value.0, 11);
        drop(value);
        producer.join().unwrap();
        assert_eq!(drops.load(Ordering::Relaxed), 1);
        drop(ring); // Second value remains unread, after wrapping the cursors.
        assert_eq!(drops.load(Ordering::Relaxed), 2);
    });
}

#[test]
fn notify_racing_arm_recheck_and_park() {
    model(|| {
        let wake = Arc::new(Wake::new());
        let published = Arc::new(AtomicBool::new(false));
        let consumer_wake = wake.clone();
        let consumer_value = published.clone();
        let consumer = thread::spawn(move || {
            consumer_wake.bind();
            consumer_wake.arm();
            if !consumer_value.load(Ordering::Acquire) {
                Wake::park();
            }
            consumer_wake.disarm();
            // Park may return spuriously. Repeat the actual protocol until data.
            while !consumer_value.load(Ordering::Acquire) {
                consumer_wake.arm();
                if !consumer_value.load(Ordering::Acquire) {
                    Wake::park();
                }
                consumer_wake.disarm();
            }
        });
        published.store(true, Ordering::Release);
        wake.notify();
        consumer.join().unwrap();
    });
}

#[test]
fn registration_close_and_final_drain() {
    model(|| {
        let registry = RingRegistry::<usize, usize>::new(config(1)).unwrap();
        let worker_registry = registry.clone();
        let consumer = thread::spawn(move || {
            let mut reader = worker_registry.bind_consumer(ProcessorId(0)).unwrap();
            let mut total = 0;
            loop {
                let status = reader.drain(
                    nz(1),
                    |_, _, v| total += v,
                    |_, _, _| panic!("wrong stream"),
                );
                if status.complete {
                    break;
                }
                if status.records == 0 {
                    reader.wait();
                }
            }
            assert_eq!(total, 42);
        });
        let mut writer = registry.register_producer().unwrap();
        registry.close_admission();
        writer.write_timing(21); // Final events are legal after admission closes.
        writer.write_timing(21); // Full-buffer spin and a second publication.
        drop(writer);
        consumer.join().unwrap();
        assert!(!registry.is_failed());
        assert!(matches!(
            registry.register_producer(),
            Err(SetupError::Closed)
        ));
    });
}

#[cfg(not(baml_loom))]
mod native {
    use std::{
        panic::{AssertUnwindSafe, catch_unwind},
        sync::mpsc,
        time::Duration,
    };

    use btel_types::allocate_telemetry_id;

    use super::*;

    // All cross-thread integration cases have a watchdog. A broken wake protocol
    // fails the test rather than leaving the suite waiting indefinitely.
    fn checked(f: impl FnOnce() + Send + 'static) {
        let (send, receive) = mpsc::channel();
        std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(f));
            send.send(result).unwrap();
        });
        match receive
            .recv_timeout(Duration::from_secs(if cfg!(miri) { 180 } else { 20 }))
            .expect("ring test timed out")
        {
            Ok(()) => (),
            Err(error) => std::panic::resume_unwind(error),
        }
    }

    #[test]
    fn independent_contexts_partial_write_and_reuse() {
        checked(|| {
            #[derive(Debug, PartialEq)]
            enum Record {
                Context(TelemetryId),
                Value(usize),
            }
            let registry = RingRegistry::<Record, Record>::new(config(1)).unwrap();
            let mut consumer = registry.bind_consumer(ProcessorId(0)).unwrap();
            let id = allocate_telemetry_id();
            let worker_registry = registry.clone();
            let (progress, observed) = mpsc::channel();
            let (advance, proceed) = mpsc::channel();
            let producer = std::thread::spawn(move || {
                let mut writer = worker_registry.register_producer().unwrap();
                writer.write_timing_in(id, Record::Value(1), Record::Context);
                progress.send(1).unwrap();
                proceed.recv().unwrap();
                writer.write_span_in(id, Record::Value(2), Record::Context);
                progress.send(2).unwrap();
                writer.pair_id()
            });
            let mut timings = Vec::new();
            let mut spans = Vec::new();
            // With one slot, only context can be published. VM continuation
            // must not run until that slot is released for the associated event.
            loop {
                consumer.refresh();
                if consumer
                    .pairs
                    .iter()
                    .any(|r| !r.pair.timing.is_empty(&r.timing))
                {
                    break;
                }
                std::thread::yield_now();
            }
            assert!(observed.try_recv().is_err());
            consumer.drain(nz(1), |_, _, v| timings.push(v), |_, _, _| unreachable!());
            assert_eq!(timings, [Record::Context(id)]);
            assert_eq!(observed.recv_timeout(Duration::from_secs(5)).unwrap(), 1);
            advance.send(()).unwrap();
            while consumer.pairs.iter().all(|r| r.pair.span.is_empty(&r.span)) {
                std::thread::yield_now();
            }
            assert!(observed.try_recv().is_err());
            consumer.drain(
                nz(1),
                |_, ctx, v| {
                    ctx.thread = Some(id);
                    timings.push(v);
                },
                |_, _, v| spans.push(v),
            );
            assert_eq!(observed.recv_timeout(Duration::from_secs(5)).unwrap(), 2);
            let pair = producer.join().unwrap();
            consumer.drain(
                nz(4),
                |_, _, v| timings.push(v),
                |_, ctx, v| {
                    ctx.thread = Some(id);
                    spans.push(v);
                },
            );
            assert_eq!(timings, [Record::Context(id), Record::Value(1)]);
            assert_eq!(spans, [Record::Context(id), Record::Value(2)]);
            let mut writer = registry.register_producer().unwrap();
            assert_eq!(writer.pair_id(), pair);
            // Both writer and reader context must be fresh on reuse.
            writer.write_timing(Record::Context(id));
            consumer.drain(
                nz(1),
                |_, ctx, _| assert_eq!(ctx.thread, None),
                |_, _, _| unreachable!(),
            );
            drop(writer);
            registry.close_admission();
            assert!(consumer.drain(nz(1), |_, _, _| (), |_, _, _| ()).complete);
        });
    }

    #[test]
    fn active_empty_and_detached_unread_are_not_reusable() {
        let mut limits = config(2);
        limits.max_pairs = nz(1);
        let registry = RingRegistry::<usize, usize>::new(limits).unwrap();
        let mut consumer = registry.bind_consumer(ProcessorId(0)).unwrap();
        let mut producer = registry.register_producer().unwrap();
        assert!(matches!(
            registry.register_producer(),
            Err(SetupError::ProducerAlreadyBound)
        ));
        let clone = registry.clone();
        std::thread::spawn(move || {
            assert!(matches!(
                clone.register_producer(),
                Err(SetupError::PairLimit)
            ));
        })
        .join()
        .unwrap();
        producer.write_span(7);
        drop(producer);
        assert!(matches!(
            registry.register_producer(),
            Err(SetupError::PairLimit)
        ));
        consumer.drain(nz(1), |_, _, _| unreachable!(), |_, _, v| assert_eq!(v, 7));
        let producer = registry.register_producer().unwrap();
        assert_eq!(registry.allocated_pairs(), 1);
        drop(producer);
        registry.close_admission();
        assert!(consumer.drain(nz(1), |_, _, _| (), |_, _, _| ()).complete);
    }

    #[test]
    fn failure_releases_spinning_writer_without_success_and_drops_once() {
        checked(|| {
            struct Owned(Arc<AtomicUsize>);
            impl Drop for Owned {
                fn drop(&mut self) {
                    self.0.fetch_add(1, Ordering::Relaxed);
                }
            }
            let drops = Arc::new(AtomicUsize::new(0));
            let registry = RingRegistry::<Owned, Owned>::new(config(1)).unwrap();
            let consumer = registry.bind_consumer(ProcessorId(0)).unwrap();
            let (send, recv) = mpsc::channel();
            let worker_registry = registry.clone();
            let worker_drops = drops.clone();
            let writer = std::thread::spawn(move || {
                let mut writer = worker_registry.register_producer().unwrap();
                writer.write_span(Owned(worker_drops.clone()));
                send.send(()).unwrap();
                let result = catch_unwind(AssertUnwindSafe(|| {
                    writer.write_span(Owned(worker_drops));
                    panic!("write returned successfully after failure");
                }));
                assert!(result.unwrap_err().is::<TransportFailed>());
            });
            recv.recv().unwrap();
            drop(consumer); // Unexpected exit is terminal for the entire registry.
            writer.join().unwrap();
            assert!(registry.is_failed());
            assert_eq!(drops.load(Ordering::Relaxed), 1); // Unpublished pending record.
            drop(registry);
            assert_eq!(drops.load(Ordering::Relaxed), 2); // Published unread record.
        });
    }

    #[test]
    fn caught_consumer_callback_panic_is_still_terminal() {
        let registry = RingRegistry::<usize, usize>::new(config(1)).unwrap();
        let mut consumer = registry.bind_consumer(ProcessorId(0)).unwrap();
        let mut writer = registry.register_producer().unwrap();
        writer.write_timing(1);
        assert!(
            catch_unwind(AssertUnwindSafe(|| consumer.drain(
                nz(1),
                |_, _, _| panic!("processor failed"),
                |_, _, _| ()
            )))
            .is_err()
        );
        assert!(registry.is_failed());
        let result = catch_unwind(AssertUnwindSafe(|| writer.write_timing(2)));
        assert!(result.unwrap_err().is::<TransportFailed>());
    }

    #[test]
    fn many_producers_two_consumers_fifo_and_sparse_wakeup() {
        checked(|| {
            let mut settings = config(4);
            settings.processors = nz(3); // Also exercise a consumer with no assigned pair.
            settings.max_pairs = nz(2);
            let registry = RingRegistry::<(usize, usize), (usize, usize)>::new(settings).unwrap();
            let consumers: Vec<_> = (0..3)
                .map(|processor| {
                    let registry = registry.clone();
                    std::thread::spawn(move || {
                        let mut consumer = registry.bind_consumer(ProcessorId(processor)).unwrap();
                        let mut timing_next = [0; 2];
                        let mut span_next = [0; 2];
                        loop {
                            let status = consumer.drain(
                                nz(3),
                                |_, _, (p, n)| {
                                    assert_eq!(timing_next[p], n);
                                    timing_next[p] += 1;
                                },
                                |_, _, (p, n)| {
                                    assert_eq!(span_next[p], n);
                                    span_next[p] += 1;
                                },
                            );
                            if status.complete {
                                return timing_next.iter().sum::<usize>()
                                    + span_next.iter().sum::<usize>();
                            }
                            if status.records == 0 {
                                consumer.wait();
                            }
                        }
                    })
                })
                .collect();
            let producers: Vec<_> = (0..2)
                .map(|p| {
                    let registry = registry.clone();
                    std::thread::spawn(move || {
                        let mut writer = registry.register_producer().unwrap();
                        for n in 0..1_000 {
                            writer.write_timing((p, n));
                            writer.write_span((p, n));
                            if n == 0 {
                                std::thread::sleep(Duration::from_millis(5));
                            }
                        }
                    })
                })
                .collect();
            for producer in producers {
                producer.join().unwrap();
            }
            registry.close_admission();
            assert_eq!(
                consumers
                    .into_iter()
                    .map(|c| c.join().unwrap())
                    .sum::<usize>(),
                4_000
            );
            assert!(!registry.is_failed());
        });
    }

    #[test]
    fn engines_are_isolated_and_invalid_capacity_rejected() {
        let one = RingRegistry::<usize, usize>::new(config(2)).unwrap();
        let two = RingRegistry::<usize, usize>::new(config(2)).unwrap();
        let _a = one.register_producer().unwrap();
        let _b = two.register_producer().unwrap();
        one.fail();
        assert!(!two.is_failed());
        assert!(matches!(
            RingRegistry::<usize, usize>::new(config(3)),
            Err(SetupError::InvalidCapacity)
        ));
        let mut huge = config(1);
        huge.timing_capacity = nz(1 << (usize::BITS - 1));
        assert!(matches!(
            RingRegistry::<usize, usize>::new(huge),
            Err(SetupError::InvalidCapacity)
        ));
    }
}

#[test]
fn admission_racing_registration_cannot_lose_final_records() {
    model(|| {
        let registry = RingRegistry::<usize, usize>::new(config(1)).unwrap();
        let worker_registry = registry.clone();
        let worker = thread::spawn(move || {
            let mut consumer = worker_registry.bind_consumer(ProcessorId(0)).unwrap();
            let mut count = 0;
            loop {
                let status = consumer.drain(nz(1), |_, _, _| count += 1, |_, _, _| unreachable!());
                if status.complete {
                    return count;
                }
                if status.records == 0 {
                    consumer.wait();
                }
            }
        });
        let producer_registry = registry.clone();
        let producer = thread::spawn(move || match producer_registry.register_producer() {
            Ok(mut producer) => {
                producer.write_timing(9);
                1
            }
            Err(SetupError::Closed) => 0,
            Err(error) => panic!("unexpected admission result: {error:?}"),
        });
        registry.close_admission();
        assert_eq!(producer.join().unwrap(), worker.join().unwrap());
        assert!(!registry.is_failed());
    });
}

#[test]
fn competing_reuse_claims_have_exactly_one_writer() {
    model(|| {
        let mut limits = config(1);
        limits.max_pairs = nz(1);
        let registry = RingRegistry::<usize, usize>::new(limits).unwrap();
        let mut reader = registry.bind_consumer(ProcessorId(0)).unwrap();
        let original = registry.register_producer().unwrap();
        let pair = original.pair_id();
        drop(original);
        reader.drain(nz(1), |_, _, _| (), |_, _, _| ());
        let arrivals = Arc::new(AtomicUsize::new(0));
        let workers: Vec<_> = (0..2)
            .map(|_| {
                let registry = registry.clone();
                let arrivals = arrivals.clone();
                thread::spawn(move || {
                    let claim = registry.register_producer();
                    arrivals.fetch_add(1, Ordering::AcqRel);
                    while arrivals.load(Ordering::Acquire) != 2 {
                        sync::spin();
                    }
                    match claim {
                        Ok(mut writer) => {
                            assert_eq!(writer.pair_id(), pair);
                            writer.write_timing(1);
                            1
                        }
                        Err(SetupError::PairLimit) => 0,
                        Err(error) => panic!("unexpected reuse result: {error:?}"),
                    }
                })
            })
            .collect();
        assert_eq!(
            workers
                .into_iter()
                .map(|w| w.join().unwrap())
                .sum::<usize>(),
            1
        );
        registry.close_admission();
        let result = reader.drain(nz(1), |_, _, v| assert_eq!(v, 1), |_, _, _| unreachable!());
        assert_eq!(
            result,
            DrainStatus {
                records: 1,
                complete: true
            }
        );
    });
}
