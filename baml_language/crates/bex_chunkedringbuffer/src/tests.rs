use super::*;
fn nz(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}
fn config(cap: usize, chunks: usize) -> Config {
    Config {
        chunk_capacity: nz(cap),
        timing_chunks: nz(chunks),
        span_chunks: nz(chunks),
        max_producers: nz(1),
        preallocate: false,
    }
}
fn model(f: impl Fn() + Send + Sync + 'static) {
    #[cfg(baml_loom)]
    {
        let mut m = loom::model::Builder::new();
        m.preemption_bound = Some(2);
        m.max_branches = 10_000;
        m.check(f);
    }
    #[cfg(not(baml_loom))]
    f();
}

#[test]
fn partial_is_private_until_sealed_and_release_flushes_both_streams() {
    model(|| {
        let pool = ChunkPool::<usize, usize>::new(config(4, 1)).unwrap();
        let mut reader = pool.bind_consumer().unwrap();
        let mut writer = pool.register_producer().unwrap();
        writer.write_timing(1);
        writer.write_span(2);
        assert_eq!(
            reader
                .drain(nz(8), |_, _| unreachable!(), |_, _| unreachable!())
                .records,
            0
        );
        assert_eq!(pool.stats().allocated_timing_chunks, 1);
        assert_eq!(pool.stats().allocated_span_chunks, 1);
        writer.seal();
        let mut seen = Vec::new();
        reader.drain(nz(8), |_, v| seen.push(v), |_, v| assert_eq!(v, 2));
        assert_eq!(seen, [1]);
        assert_eq!(pool.stats().free_chunks, 2);
        writer.write_timing(3);
        writer.write_span(4);
        assert_eq!(writer.stats().allocations, 2);
        pool.close_admission();
        drop(writer);
        let end = reader.drain(nz(8), |_, v| assert_eq!(v, 3), |_, v| assert_eq!(v, 4));
        assert_eq!(end.records, 2);
        assert!(end.complete);
        assert!(!pool.is_failed());
    });
}

#[test]
fn full_chunk_handoff_wakeup_and_recycling() {
    model(|| {
        let pool = ChunkPool::<usize, usize>::new(config(1, 1)).unwrap();
        let copy = pool.clone();
        let worker = thread::spawn(move || {
            let mut c = copy.bind_consumer().unwrap();
            let mut expected = 0;
            loop {
                let s = c.drain(
                    nz(1),
                    |_, v| {
                        assert_eq!(v, expected);
                        expected += 1;
                    },
                    |_, _| unreachable!(),
                );
                if s.complete {
                    break;
                }
                if s.records == 0 {
                    c.wait();
                }
            }
            assert_eq!(expected, 2);
        });
        let mut p = pool.register_producer().unwrap();
        p.write_timing(0);
        p.write_timing(1);
        assert_eq!(p.stats().allocations, 1);
        drop(p);
        pool.close_admission();
        worker.join().unwrap();
        assert_eq!(pool.stats().free_chunks, 1);
    });
}

#[test]
fn close_racing_admission_cannot_lose_partial_chunk() {
    model(|| {
        let pool = ChunkPool::<usize, usize>::new(config(2, 1)).unwrap();
        let copy = pool.clone();
        let worker = thread::spawn(move || match copy.register_producer() {
            Ok(mut p) => {
                p.write_timing(7);
                1
            }
            Err(SetupError::Closed) => 0,
            Err(e) => panic!("unexpected {e:?}"),
        });
        pool.close_admission();
        let count = worker.join().unwrap();
        let mut reader = pool.bind_consumer().unwrap();
        let result = reader.drain(nz(2), |_, v| assert_eq!(v, 7), |_, _| unreachable!());
        assert_eq!(result.records, count);
        assert!(result.complete);
    });
}

#[test]
fn closing_an_empty_pool_wakes_the_consumer() {
    model(|| {
        let pool = ChunkPool::<usize, usize>::new(config(1, 1)).unwrap();
        let copy = pool.clone();
        let worker = thread::spawn(move || {
            let mut reader = copy.bind_consumer().unwrap();
            reader.wait();
            assert!(
                reader
                    .drain(nz(8), |_, _| unreachable!(), |_, _| unreachable!())
                    .complete
            );
        });
        pool.close_admission();
        worker.join().unwrap();
        assert!(!pool.is_failed());
    });
}

#[test]
fn batches_preserve_handoff_order_and_drain_limit() {
    model(|| {
        let pool = ChunkPool::<usize, usize>::new(config(1, 6)).unwrap();
        let mut reader = pool.bind_consumer().unwrap();
        let mut writer = pool.register_producer().unwrap();
        for i in 0..6 {
            writer.write_timing(i * 2);
            writer.write_span(i * 2 + 1);
        }
        drop(writer);
        pool.close_admission();
        let seen = std::cell::RefCell::new(Vec::new());
        let first = reader.drain(
            nz(3),
            |_, v| seen.borrow_mut().push(v),
            |_, v| seen.borrow_mut().push(v),
        );
        assert_eq!(first.chunks, 3);
        assert_eq!(first.records, 3);
        assert!(!first.complete);
        assert_eq!(pool.stats().ready_chunks, 9);
        assert_eq!(pool.stats().free_chunks, 3);
        let end = reader.drain(
            nz(20),
            |_, v| seen.borrow_mut().push(v),
            |_, v| seen.borrow_mut().push(v),
        );
        assert_eq!(end.chunks, 9);
        assert!(end.complete);
        assert_eq!(*seen.borrow(), (0..12).collect::<Vec<_>>());
        assert_eq!(pool.stats().free_chunks, 12);
    });
}

#[cfg(not(baml_loom))]
mod native {
    use std::{
        panic::{AssertUnwindSafe, catch_unwind},
        sync::{
            atomic::{AtomicUsize, Ordering as Order},
            mpsc,
        },
        time::Duration,
    };

    use super::*;

    #[test]
    fn whole_chunks_drop_remaining_payloads_and_reuse_allocations() {
        struct Owned(std::sync::Arc<AtomicUsize>);
        impl Drop for Owned {
            fn drop(&mut self) {
                self.0.fetch_add(1, Order::SeqCst);
            }
        }
        let drops = std::sync::Arc::new(AtomicUsize::new(0));
        let pool = ChunkPool::<u64, Owned>::new(config(4, 1)).unwrap();
        let mut writer = pool.register_producer().unwrap();
        let mut reader = pool.bind_consumer().unwrap();
        for i in 0..3 {
            writer.write_timing(i);
            writer.write_span(Owned(drops.clone()));
        }
        let timing_ptr = writer.timing.as_ref().unwrap().as_ptr();
        let span_ptr = writer.span.as_ref().unwrap().as_ptr();
        writer.seal();
        let status = reader.drain_chunks(
            nz(2),
            |_, records| {
                assert_eq!(records.as_slice(), [0, 1, 2]);
                // Discard the entire plain-data chunk without iterating it.
                drop(records);
            },
            |_, mut records| {
                // Moving one payload out and discarding the remainder must
                // destroy all three exactly once before recycling the chunk.
                drop(records.next().unwrap());
                assert_eq!(drops.load(Order::SeqCst), 1);
            },
        );
        assert_eq!(status.chunks, 2);
        assert_eq!(status.records, 6);
        assert_eq!(drops.load(Order::SeqCst), 3);
        writer.write_timing(9);
        writer.write_span(Owned(drops.clone()));
        assert_eq!(writer.timing.as_ref().unwrap().as_ptr(), timing_ptr);
        assert_eq!(writer.span.as_ref().unwrap().as_ptr(), span_ptr);
        assert_eq!(writer.timing.as_ref().unwrap().capacity(), 4);
        assert_eq!(writer.span.as_ref().unwrap().capacity(), 4);
        assert_eq!(writer.stats().allocations, 2);
        drop(writer);
        pool.close_admission();
        assert!(
            reader
                .drain_chunks(nz(8), |_, r| drop(r), |_, r| drop(r))
                .complete
        );
        assert_eq!(drops.load(Order::SeqCst), 4);
    }
    fn checked(f: impl FnOnce() + Send + 'static) {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let r = catch_unwind(AssertUnwindSafe(f));
            tx.send(r).unwrap();
        });
        match rx
            .recv_timeout(Duration::from_secs(if cfg!(miri) { 120 } else { 10 }))
            .expect("chunk transport timed out")
        {
            Ok(()) => (),
            Err(e) => std::panic::resume_unwind(e),
        }
    }
    #[test]
    fn processor_owned_chunk_counts_toward_limit() {
        checked(|| {
            let pool = ChunkPool::<usize, usize>::new(config(1, 1)).unwrap();
            let mut p = pool.register_producer().unwrap();
            p.write_timing(1);
            drop(p);
            let (held_tx, held_rx) = mpsc::channel();
            let (release_tx, release_rx) = mpsc::channel();
            let (done_tx, done_rx) = mpsc::channel();
            let copy = pool.clone();
            let consumer = thread::spawn(move || {
                let mut c = copy.bind_consumer().unwrap();
                loop {
                    let s = c.drain(
                        nz(1),
                        |_, v| {
                            if v == 1 {
                                held_tx.send(()).unwrap();
                                release_rx.recv().unwrap();
                            } else {
                                assert_eq!(v, 2);
                            }
                        },
                        |_, _| unreachable!(),
                    );
                    if s.complete {
                        break;
                    }
                    if s.records == 0 {
                        c.wait();
                    }
                }
            });
            held_rx.recv().unwrap();
            let copy = pool.clone();
            let writer = thread::spawn(move || {
                let mut p = copy.register_producer().unwrap();
                p.write_timing(2);
                done_tx.send(()).unwrap();
                p.stats()
            });
            thread::sleep(Duration::from_millis(2));
            assert!(done_rx.try_recv().is_err());
            assert_eq!(pool.stats().allocated_timing_chunks, 1);
            release_tx.send(()).unwrap();
            let stats = writer.join().unwrap();
            assert_eq!(stats.allocations, 0);
            pool.close_admission();
            consumer.join().unwrap();
            assert_eq!(pool.stats().free_chunks, 1);
        });
    }
    #[test]
    fn callback_failure_drops_owned_data_once_and_stops_pending_writes() {
        checked(|| {
            struct Owned(std::sync::Arc<AtomicUsize>);
            impl Drop for Owned {
                fn drop(&mut self) {
                    self.0.fetch_add(1, Order::SeqCst);
                }
            }
            let drops = std::sync::Arc::new(AtomicUsize::new(0));
            let pool = ChunkPool::<Owned, Owned>::new(config(2, 2)).unwrap();
            let mut c = pool.bind_consumer().unwrap();
            let mut p = pool.register_producer().unwrap();
            p.write_timing(Owned(drops.clone()));
            p.write_timing(Owned(drops.clone()));
            p.write_timing(Owned(drops.clone()));
            p.write_span(Owned(drops.clone()));
            p.seal();
            assert!(
                catch_unwind(AssertUnwindSafe(|| c.drain(
                    nz(8),
                    |_, _| panic!("processor failed"),
                    |_, _| unreachable!()
                )))
                .is_err()
            );
            assert!(pool.is_failed());
            let e = catch_unwind(AssertUnwindSafe(|| p.write_timing(Owned(drops.clone()))))
                .unwrap_err();
            assert!(e.is::<TransportFailed>());
            drop(p);
            drop(c);
            drop(pool);
            assert_eq!(drops.load(Order::SeqCst), 5);
        });
    }
    #[test]
    fn bounded_multi_producer_recycling_preserves_each_stream_fifo() {
        checked(|| {
            let mut cfg = config(3, 2);
            cfg.max_producers = nz(2);
            cfg.preallocate = true;
            let pool = ChunkPool::<(usize, usize), (usize, usize)>::new(cfg).unwrap();
            let copy = pool.clone();
            let consumer = thread::spawn(move || {
                let mut c = copy.bind_consumer().unwrap();
                let mut ts = [0; 2];
                let mut ss = [0; 2];
                loop {
                    let s = c.drain(
                        nz(2),
                        |_, (p, i)| {
                            assert_eq!(i, ts[p]);
                            ts[p] += 1;
                        },
                        |_, (p, i)| {
                            assert_eq!(i, ss[p]);
                            ss[p] += 1;
                        },
                    );
                    if s.complete {
                        break;
                    }
                    if s.records == 0 {
                        c.wait();
                    }
                }
                assert_eq!(ts, [40; 2]);
                assert_eq!(ss, [40; 2]);
            });
            let writers: Vec<_> = (0..2)
                .map(|i| {
                    let copy = pool.clone();
                    thread::spawn(move || {
                        let mut p = copy.register_producer().unwrap();
                        for n in 0..40 {
                            p.write_timing((i, n));
                            p.write_span((i, n));
                            if n % 7 == 0 {
                                p.seal();
                            }
                        }
                        assert_eq!(p.stats().allocations, 0);
                    })
                })
                .collect();
            for w in writers {
                w.join().unwrap();
            }
            pool.close_admission();
            consumer.join().unwrap();
            let s = pool.stats();
            assert_eq!(s.allocated_timing_chunks + s.allocated_span_chunks, 4);
            assert_eq!(s.free_chunks, 4);
            assert_eq!(s.sealed_records, 160);
        });
    }
    #[test]
    fn consumer_failure_stops_an_exhausted_writer() {
        checked(|| {
            let pool = ChunkPool::<usize, usize>::new(config(1, 1)).unwrap();
            let consumer = pool.bind_consumer().unwrap();
            let copy = pool.clone();
            let (ready, received) = mpsc::channel();
            let writer = thread::spawn(move || {
                let mut writer = copy.register_producer().unwrap();
                writer.write_timing(1);
                ready.send(()).unwrap();
                let error = catch_unwind(AssertUnwindSafe(|| writer.write_timing(2))).unwrap_err();
                assert!(error.is::<TransportFailed>());
            });
            received.recv().unwrap();
            drop(consumer);
            writer.join().unwrap();
            assert!(pool.is_failed());
            assert_eq!(pool.stats().ready_chunks, 1);
            assert_eq!(pool.stats().active_producers, 0);
        });
    }

    #[test]
    fn unsafe_pool_size_and_duplicate_ownership_are_rejected() {
        let mut cfg = config(2, 1);
        cfg.max_producers = nz(2);
        assert!(matches!(
            ChunkPool::<u64, u64>::new(cfg),
            Err(SetupError::InsufficientChunks)
        ));
        let pool = ChunkPool::<u64, u64>::new(config(2, 1)).unwrap();
        let _p = pool.register_producer().unwrap();
        assert!(matches!(
            pool.register_producer(),
            Err(SetupError::ProducerAlreadyBound)
        ));
        let copy = pool.clone();
        thread::spawn(move || {
            assert!(matches!(
                copy.register_producer(),
                Err(SetupError::ProducerLimit)
            ));
        })
        .join()
        .unwrap();
        let c = pool.bind_consumer().unwrap();
        assert!(matches!(
            pool.bind_consumer(),
            Err(SetupError::ConsumerAlreadyBound)
        ));
        drop(c);
        assert!(pool.is_failed());
    }
}
