use baml_rt_tasks::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};
fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
}
fn scheduler(rt: &tokio::runtime::Runtime) -> TaskLoop {
    TaskLoop::new(
        Config::default(),
        rt.handle().clone(),
        Rc::new(RefCell::new(())),
    )
}
fn error(s: &str) -> Err {
    Err::Error {
        message: s.into(),
        trace: vec!["producer".into()],
    }
}
#[test]
fn parent_continues_children_fifo_at_pending_await() {
    let rt = runtime();
    let mut lp = scheduler(&rt);
    let log = Rc::new(RefCell::new(vec![]));
    let out = log.clone();
    let root = lp.spawn(move |cx| {
        out.borrow_mut().push(0);
        let a = out.clone();
        let first = cx.spawn(move |_| {
            a.borrow_mut().push(2);
            Ok(Value::int(1))
        });
        out.borrow_mut().push(1);
        let a = out.clone();
        cx.spawn(move |_| {
            a.borrow_mut().push(3);
            Ok(Value::int(2))
        });
        assert_eq!(&*out.borrow(), &[0, 1]);
        let v = cx.await_future(&first)?;
        out.borrow_mut().push(4);
        Ok(v)
    });
    assert_eq!(lp.run_until(&root).unwrap().as_int(), 1);
    assert_eq!(&*log.borrow(), &[0, 1, 2, 3, 4]);
}
#[test]
fn completion_and_io_are_child_start_boundaries() {
    let rt = runtime();
    let mut lp = scheduler(&rt);
    let seen = Rc::new(Cell::new(false));
    let flag = seen.clone();
    let root = lp.spawn(move |cx| {
        cx.spawn(move |_| {
            flag.set(true);
            Ok(Value::int(0))
        });
        Ok(Value::int(0))
    });
    lp.run_until(&root).unwrap();
    assert!(!seen.get());
    lp.run_until_idle();
    assert!(seen.get());
    let seen = Rc::new(Cell::new(false));
    let flag = seen.clone();
    let root = lp.spawn(move |cx| {
        let f = flag.clone();
        cx.spawn(move |_| {
            f.set(true);
            Ok(Value::int(0))
        });
        assert!(!flag.get());
        cx.io(|_| async { Ok(OwnedValue::Unit) })?;
        assert!(flag.get());
        Ok(Value::int(0))
    });
    lp.run_until(&root).unwrap();
}

#[test]
fn cancellation_during_handoff_resumes_parent_behind_ready_siblings() {
    for pending in [false, true] {
        let rt = runtime();
        let mut lp = scheduler(&rt);
        let events = Rc::new(RefCell::new(vec![]));
        let output = events.clone();
        let parent = lp.spawn(move |cx| {
            let cancel = cx.cancel();
            let child_events = output.clone();
            let child = cx.spawn(move |cx| {
                child_events.borrow_mut().push(1);
                cancel.cancel();
                if pending {
                    cx.shield(|cx| cx.sleep(Duration::from_secs(60)))?;
                }
                Ok(Value::int(0))
            });
            let sibling_events = output.clone();
            cx.spawn_with(
                SpawnOptions {
                    detached: true,
                    ..SpawnOptions::default()
                },
                move |_| {
                    sibling_events.borrow_mut().push(2);
                    Ok(Value::int(0))
                },
            );
            assert!(matches!(cx.await_future(&child), Err(Err::Cancelled)));
            output.borrow_mut().push(3);
            Ok(Value::int(0))
        });
        lp.run_until(&parent).unwrap();
        assert_eq!(&*events.borrow(), &[1, 2, 3], "pending={pending}");
    }
}
#[test]
fn await_any_scans_input_order_and_carries_trace() {
    let rt = runtime();
    let mut lp = scheduler(&rt);
    let root = lp.spawn(|cx| {
        let a = cx.spawn(|_| Ok(Value::int(10)));
        let b = cx.spawn(|_| Ok(Value::int(20)));
        cx.sleep(Duration::from_millis(2))?;
        let (i, v) = cx.await_any(&[b, a])?;
        assert_eq!(i, 0);
        assert_eq!(v.as_int(), 20);
        let bad = cx.spawn(|_| Err(error("bad")));
        assert_eq!(cx.await_future(&bad).unwrap_err(), error("bad"));
        Ok(Value::int(1))
    });
    lp.run_until(&root).unwrap();
    assert!(lp.shutdown(Duration::ZERO).unhandled.is_empty());
}
#[test]
fn await_any_pending_wakes_once_and_unregisters_losers() {
    let rt = runtime();
    let mut lp = scheduler(&rt);
    let root = lp.spawn(|cx| {
        let a = cx.spawn(|cx| {
            cx.sleep(Duration::from_millis(20))?;
            Ok(Value::int(10))
        });
        let b = cx.spawn(|_| Ok(Value::int(20)));
        let (i, v) = cx.await_any(&[a.clone(), b])?;
        assert_eq!(i, 1);
        assert_eq!(v.as_int(), 20);
        cx.await_future(&a)
    });
    assert_eq!(lp.run_until(&root).unwrap().as_int(), 10);
}
fn chain(cx: &mut TaskCx<'_>, depth: usize) -> Outcome {
    if depth == 0 {
        return Ok(Value::int(0));
    }
    let f = cx.spawn(move |cx| chain(cx, depth - 1));
    let n = cx.await_future(&f)?.as_int();
    Ok(Value::int(n + 1))
}
#[test]
fn nested_spawn_and_stack_pool_reuse() {
    let rt = runtime();
    let mut lp = scheduler(&rt);
    let f = lp.spawn(|cx| chain(cx, 100));
    assert_eq!(lp.run_until(&f).unwrap().as_int(), 100);
    assert_eq!(lp.task_count(), 0);
    assert!(lp.pooled_stacks() > 0);
    for _ in 0..100 {
        let f = lp.spawn(|_| Ok(Value::int(1)));
        lp.run_until(&f).unwrap();
    }
    assert_eq!(lp.pooled_stacks(), 101);
}
#[test]
fn cancel_pending_redelivery_shield_and_detached_cleanup_child() {
    let rt = runtime();
    let mut lp = scheduler(&rt);
    let f = lp.spawn(|cx| {
        cx.cancel().cancel();
        assert_eq!(cx.poll(), Err(Err::Cancelled));
        assert_eq!(cx.poll(), Err(Err::Cancelled));
        cx.shield(|cx| {
            cx.poll()?;
            let f = cx.spawn(|cx| {
                cx.poll()?;
                Ok(Value::int(7))
            });
            assert_eq!(cx.await_future(&f)?.as_int(), 7);
            cx.io(|token| async move {
                assert!(!token.is_cancelled());
                Ok(OwnedValue::Unit)
            })?;
            Ok::<_, Err>(())
        })?;
        assert_eq!(cx.poll(), Err(Err::Cancelled));
        Err(Err::Cancelled)
    });
    assert!(matches!(f.state(), FutureState::Pending));
    assert_eq!(lp.run_until(&f).unwrap_err(), Err::Cancelled);
    assert!(matches!(f.state(), FutureState::Cancelled));
    let ran = Rc::new(Cell::new(false));
    let flag = ran.clone();
    let f = lp.spawn(move |_| {
        flag.set(true);
        Ok(Value::int(0))
    });
    f.request_cancel();
    assert!(matches!(f.state(), FutureState::Pending));
    assert_eq!(lp.run_until(&f).unwrap_err(), Err::Cancelled);
    assert!(!ran.get());
}
#[test]
fn cancellation_in_flight_replaces_io_token_and_ignores_late_result() {
    let rt = runtime();
    let mut lp = scheduler(&rt);
    let f = lp.spawn(|cx| {
        let cancel = cx.cancel();
        cx.spawn(move |cx| {
            cx.sleep(Duration::from_millis(5))?;
            cancel.cancel();
            Ok(Value::int(0))
        });
        let result = cx.io(|_| async {
            tokio::time::sleep(Duration::from_secs(60)).await;
            Ok(OwnedValue::Int(99))
        });
        assert_eq!(result.unwrap_err(), Err::Cancelled);
        cx.shield(|cx| {
            cx.io(|token| async move {
                assert!(!token.is_cancelled());
                tokio::time::sleep(Duration::from_millis(5)).await;
                Ok(OwnedValue::Int(42))
            })
        })
    });
    assert_eq!(lp.run_until(&f).unwrap().as_int(), 42);
}
#[test]
fn deadline_inherited_sources_and_failure_cascade() {
    let rt = runtime();
    let mut lp = scheduler(&rt);
    let linked = CancellationToken::new();
    let opts = SpawnOptions {
        linked: vec![linked.clone()],
        deadline: Some(Instant::now() + Duration::from_millis(5)),
        ..Default::default()
    };
    let f = lp.spawn_with(opts, |cx| {
        let f = cx.spawn(|cx| {
            cx.sleep(Duration::from_secs(60))?;
            Ok(Value::int(0))
        });
        cx.await_future(&f)
    });
    assert_eq!(lp.run_until(&f).unwrap_err(), Err::Cancelled);
    lp.run_until_idle();
    assert_eq!(lp.task_count(), 0);
    let child = Rc::new(RefCell::new(None));
    let save = child.clone();
    let f = lp.spawn_with(
        SpawnOptions {
            linked: vec![linked.clone()],
            ..Default::default()
        },
        move |cx| {
            *save.borrow_mut() = Some(cx.spawn(|cx| {
                cx.sleep(Duration::from_secs(60))?;
                Ok(Value::int(0))
            }));
            Ok(Value::int(0))
        },
    );
    lp.run_until(&f).unwrap();
    linked.cancel();
    let c = child.borrow().clone().unwrap();
    assert_eq!(lp.run_until(&c).unwrap_err(), Err::Cancelled);
    let save = child.clone();
    let f = lp.spawn(move |cx| {
        *save.borrow_mut() = Some(cx.spawn(|_| panic!("cancelled before entry")));
        Err(error("parent"))
    });
    assert_eq!(lp.run_until(&f).unwrap_err(), error("parent"));
    let c = child.borrow().clone().unwrap();
    assert_eq!(lp.run_until(&c).unwrap_err(), Err::Cancelled);
}
#[test]
fn gate_registers_synchronously_and_releases_on_cancel() {
    let rt = runtime();
    let mut lp = scheduler(&rt);
    let gate = Gate::new(1);
    let g = gate.clone();
    let f = lp.spawn(move |cx| {
        let a = cx.spawn_with(
            SpawnOptions {
                gate: Some(g.clone()),
                ..Default::default()
            },
            |cx| {
                cx.sleep(Duration::from_secs(60))?;
                Ok(Value::int(1))
            },
        );
        let b = cx.spawn_with(
            SpawnOptions {
                gate: Some(g.clone()),
                ..Default::default()
            },
            |_| panic!("queued cancelled body ran"),
        );
        assert_eq!(g.counts(), (0, 2));
        cx.sleep(Duration::from_millis(1))?;
        assert_eq!(g.counts(), (1, 1));
        b.request_cancel();
        assert_eq!(cx.await_future(&b).unwrap_err(), Err::Cancelled);
        assert_eq!(g.counts(), (1, 0));
        a.request_cancel();
        assert_eq!(cx.await_future(&a).unwrap_err(), Err::Cancelled);
        Ok(Value::int(0))
    });
    lp.run_until(&f).unwrap();
    assert_eq!(gate.counts(), (0, 0));
}
#[test]
fn tokio_and_cross_thread_host_submission() {
    let rt = runtime();
    let mut lp = scheduler(&rt);
    let submit = lp.submission_handle();
    let owner = std::thread::current().id();
    let thread = std::thread::spawn(move || {
        submit
            .submit(vec![OwnedValue::Int(17)], None, move |cx, args| {
                assert_eq!(std::thread::current().id(), owner);
                cx.io(move |_| async move {
                    assert_ne!(std::thread::current().id(), owner);
                    Ok(args[0].clone())
                })
            })
            .recv()
            .unwrap()
    });
    while !thread.is_finished() {
        lp.tick(false);
        std::thread::yield_now();
    }
    assert_eq!(thread.join().unwrap().unwrap(), OwnedValue::Int(17));
}
#[test]
fn poll_yield_fairness_cpu_and_io() {
    let rt = runtime();
    let mut lp = scheduler(&rt);
    let done = Rc::new(Cell::new(false));
    let flag = done.clone();
    let root = lp.spawn(move |cx| {
        let f = flag.clone();
        let io = cx.spawn(move |cx| {
            cx.io(|_| async {
                tokio::time::sleep(Duration::from_millis(5)).await;
                Ok(OwnedValue::Unit)
            })?;
            f.set(true);
            Ok(Value::int(1))
        });
        let start = Instant::now();
        let mut spins = 0u64;
        while !flag.get() {
            spins += 1;
            if spins.is_multiple_of(16) {
                cx.poll()?;
            }
            assert!(start.elapsed() < Duration::from_secs(2));
        }
        assert!(spins > 0);
        cx.await_future(&io)
    });
    assert_eq!(lp.run_until(&root).unwrap().as_int(), 1);
    assert!(done.get());
}
#[test]
fn panics_become_internal_error_and_unhandled_errors_report_once() {
    let rt = runtime();
    let mut lp = scheduler(&rt);
    let f = lp.spawn(|_| panic!("user panic"));
    assert!(matches!(lp.run_until(&f), Err(Err::InternalError(_))));
    let f = lp.spawn(|cx| {
        cx.spawn(|_| Err(error("unhandled")));
        Ok(Value::int(0))
    });
    lp.run_until(&f).unwrap();
    lp.run_until_idle();
    let report = lp.shutdown(Duration::ZERO);
    assert_eq!(report.unhandled.len(), 1);
    assert_eq!(report.unhandled[0].1, error("unhandled"));
    assert!(lp.shutdown(Duration::ZERO).unhandled.is_empty());
}
#[test]
fn io_panics_settle_internal_error() {
    let rt = runtime();
    let mut lp = scheduler(&rt);
    let f = lp.spawn(|cx| {
        cx.io(|_| async {
            panic!("I/O panic");
        })
    });
    assert!(matches!(lp.run_until(&f), Err(Err::InternalError(_))));
}
#[test]
fn shutdown_detects_leaks_and_unwinds_stack_drops() {
    struct Guard(Rc<Cell<usize>>);
    impl Drop for Guard {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let rt = runtime();
    let mut lp = scheduler(&rt);
    let drops = Rc::new(Cell::new(0));
    let d = drops.clone();
    let f = lp.spawn(move |cx| {
        let _guard = Guard(d);
        cx.shield(|cx| cx.sleep(Duration::from_secs(60)))?;
        Ok(Value::int(0))
    });
    lp.run_until_idle();
    let report = lp.shutdown(Duration::ZERO);
    assert_eq!(report.leaked, vec![f.id()]);
    assert_eq!(drops.get(), 1);
    assert!(matches!(f.state(), FutureState::Cancelled));
    assert_eq!(lp.task_count(), 0);
    assert_eq!(lp.pooled_stacks(), 0);
}
struct RootProbe {
    word_addr: Rc<Cell<usize>>,
    hits: Rc<Cell<usize>>,
    precise: Rc<Cell<usize>>,
}
impl LoopHeap for RootProbe {
    fn collect(&mut self, source: &dyn RootSource) {
        let target = self.word_addr.get();
        source.conservative_ranges(&mut |lo, hi| {
            assert_eq!(lo as usize % std::mem::size_of::<usize>(), 0);
            assert!(lo <= hi);
            if lo as usize <= target && target < hi as usize {
                // SAFETY: target is a live, explicitly address-taken usize local.
                assert_eq!(unsafe { *(target as *const usize) }, 0x1234_abcd);
                self.hits.set(self.hits.get() + 1);
            }
        });
        source.precise_roots(&mut |slot| {
            // SAFETY: callback only reads the live UnsafeCell-backed exact slot.
            assert_eq!(unsafe { *slot }, 0x1234_abcd);
            self.precise.set(self.precise.get() + 1);
        });
    }
}
#[test]
fn roots_cover_parked_and_running_stack_words_and_precise_slots() {
    let rt = runtime();
    let addr = Rc::new(Cell::new(0));
    let hits = Rc::new(Cell::new(0));
    let precise = Rc::new(Cell::new(0));
    let heap = RootProbe {
        word_addr: addr.clone(),
        hits: hits.clone(),
        precise: precise.clone(),
    };
    let mut lp = TaskLoop::new(
        Config::default(),
        rt.handle().clone(),
        Rc::new(RefCell::new(heap)),
    );
    let address = addr.clone();
    let f = lp.spawn_with(
        SpawnOptions {
            roots: vec![Value::pointer(0x1234_abcd)],
            ..Default::default()
        },
        move |cx| {
            let word: usize = std::hint::black_box(0x1234_abcd);
            address.set(&word as *const usize as usize);
            cx.collect();
            cx.sleep(Duration::from_secs(60))?;
            std::hint::black_box(word);
            Ok(Value::int(0))
        },
    );
    lp.run_until_idle();
    assert_eq!(hits.get(), 1);
    assert_eq!(precise.get(), 1);
    let probe = lp.spawn(|cx| {
        cx.collect();
        Ok(Value::int(0))
    });
    lp.run_until(&probe).unwrap();
    assert_eq!(hits.get(), 2);
    assert_eq!(precise.get(), 2);
    f.request_cancel();
    lp.run_until(&f).unwrap_err();
}
#[test]
fn roots_cover_ready_fiber_after_timeslice_yield_and_stack_reuse() {
    let rt = runtime();
    let addr = Rc::new(Cell::new(0));
    let hits = Rc::new(Cell::new(0));
    let heap = RootProbe {
        word_addr: addr.clone(),
        hits: hits.clone(),
        precise: Rc::new(Cell::new(0)),
    };
    let mut lp = TaskLoop::new(
        Config {
            time_slice: Duration::ZERO,
            ..Config::default()
        },
        rt.handle().clone(),
        Rc::new(RefCell::new(heap)),
    );
    for round in 0..3 {
        let address = addr.clone();
        let owner = lp.spawn(move |cx| {
            let word = std::hint::black_box(0x1234_abcdusize);
            address.set(&word as *const usize as usize);
            cx.poll()?;
            std::hint::black_box(word);
            Ok(Value::int(0))
        });
        // FIFO runs this collector while the owner is Ready, not sleeping.
        let probe = lp.spawn(|cx| {
            cx.collect();
            Ok(Value::int(0))
        });
        lp.run_until(&probe).unwrap();
        assert_eq!(hits.get(), round + 1);
        lp.run_until(&owner).unwrap();
        // Completed and pooled stacks must no longer contribute root ranges.
        lp.conservative_ranges(&mut |lo, hi| {
            assert!(!(lo as usize <= addr.get() && addr.get() < hi as usize));
        });
    }
}

#[test]
fn ten_thousand_parked_tasks() {
    let rt = runtime();
    let mut lp = scheduler(&rt);
    for _ in 0..10_000 {
        lp.spawn(|cx| {
            cx.sleep(Duration::from_secs(3600))?;
            Ok(Value::int(0))
        });
    }
    lp.run_until_idle();
    assert_eq!(lp.task_count(), 10_000);
    let mut ranges = 0;
    lp.conservative_ranges(&mut |_, _| ranges += 1);
    assert!(ranges >= 10_001);
    let report = lp.shutdown(Duration::ZERO);
    assert_eq!(report.leaked.len(), 10_000);
    assert_eq!(lp.task_count(), 0);
}
#[test]
fn closed_submission_returns_owned_error_and_foreign_future_is_rejected() {
    let rt = runtime();
    let mut a = scheduler(&rt);
    let mut b = scheduler(&rt);
    let f = a.spawn(|_| Ok(Value::int(0)));
    assert!(matches!(b.run_until(&f), Err(Err::InternalError(_))));
    let submit = a.submission_handle();
    let queued = submit.submit(vec![], None, |_, _| {
        panic!("abandoned preparation executed")
    });
    a.shutdown(Duration::ZERO);
    assert!(matches!(queued.recv().unwrap(), Err(Err::InternalError(_))));
    assert!(matches!(
        submit
            .submit(vec![], None, |_, _| Ok(Value::int(0)))
            .recv()
            .unwrap(),
        Err(Err::InternalError(_))
    ));
}
#[test]
fn ready_await_does_not_yield_and_poll_obeys_slice() {
    let rt = runtime();
    for (slice, should_run) in [(Duration::ZERO, true), (Duration::from_secs(60), false)] {
        let mut lp = TaskLoop::new(
            Config {
                time_slice: slice,
                ..Default::default()
            },
            rt.handle().clone(),
            Rc::new(RefCell::new(())),
        );
        let done = lp.spawn(|_| Ok(Value::int(0)));
        lp.run_until(&done).unwrap();
        let seen = Rc::new(Cell::new(false));
        let flag = seen.clone();
        let root = lp.spawn(move |cx| {
            let child_flag = flag.clone();
            cx.spawn(move |_| {
                child_flag.set(true);
                Ok(Value::int(0))
            });
            cx.await_future(&done)?;
            assert!(!flag.get());
            cx.poll()?;
            assert_eq!(flag.get(), should_run);
            Ok(Value::int(0))
        });
        lp.run_until(&root).unwrap();
        lp.run_until_idle();
        assert!(seen.get());
    }
}
#[test]
fn io_runtime_shutdown_does_not_strand_a_task() {
    let rt = runtime();
    let mut lp = scheduler(&rt);
    let f = lp.spawn(|cx| {
        cx.io(|_| async {
            tokio::time::sleep(Duration::from_secs(60)).await;
            Ok(OwnedValue::Unit)
        })
    });
    lp.run_until_idle();
    drop(rt);
    assert!(matches!(lp.run_until(&f), Err(Err::InternalError(_))));
}
#[test]
fn settled_future_payload_is_a_precise_root() {
    struct Probe(Rc<Cell<usize>>);
    impl LoopHeap for Probe {
        fn collect(&mut self, source: &dyn RootSource) {
            source.precise_roots(&mut |slot| {
                // SAFETY: callback reads an addressable live precise root slot.
                assert_eq!(unsafe { *slot }, 0xfeed);
                self.0.set(self.0.get() + 1);
            });
        }
    }
    let rt = runtime();
    let count = Rc::new(Cell::new(0));
    let mut lp = TaskLoop::new(
        Config::default(),
        rt.handle().clone(),
        Rc::new(RefCell::new(Probe(count.clone()))),
    );
    let f = lp.spawn(|_| Ok(Value::pointer(0xfeed)));
    lp.run_until(&f).unwrap();
    let root = lp.spawn(move |cx| {
        cx.collect();
        assert!(matches!(f.state(), FutureState::Ready(_)));
        Ok(Value::int(0))
    });
    lp.run_until(&root).unwrap();
    assert_eq!(count.get(), 1);
}
#[test]
fn retained_future_and_cancel_handle_survive_slot_reuse() {
    let rt = runtime();
    let mut lp = scheduler(&rt);
    let saved = Rc::new(RefCell::new(None));
    let save = saved.clone();
    let old = lp.spawn(move |cx| {
        *save.borrow_mut() = Some(cx.cancel());
        Ok(Value::int(11))
    });
    lp.run_until(&old).unwrap();
    let old_id = old.id();
    for _ in 0..200 {
        let saved = saved.clone();
        let f = lp.spawn(move |cx| {
            saved.borrow().as_ref().unwrap().cancel();
            cx.poll()?;
            Ok(Value::int(22))
        });
        assert_ne!(f.id(), old_id);
        assert_eq!(lp.run_until(&f).unwrap().as_int(), 22);
        assert_eq!(old.result().unwrap().unwrap().as_int(), 11);
    }
    drop(old);
    let f = lp.spawn(|cx| {
        cx.poll()?;
        Ok(Value::int(33))
    });
    saved.borrow().as_ref().unwrap().cancel();
    assert_eq!(lp.run_until(&f).unwrap().as_int(), 33);
}
#[test]
fn successful_parent_recycling_preserves_live_descendant_cancellation() {
    let rt = runtime();
    let mut lp = scheduler(&rt);
    let child = Rc::new(RefCell::new(None));
    let save = child.clone();
    let cancel = Rc::new(RefCell::new(None));
    let save_cancel = cancel.clone();
    let parent = lp.spawn(move |cx| {
        *save_cancel.borrow_mut() = Some(cx.cancel());
        *save.borrow_mut() = Some(cx.spawn(|cx| {
            cx.sleep(Duration::from_secs(60))?;
            Ok(Value::int(0))
        }));
        Ok(Value::int(1))
    });
    lp.run_until(&parent).unwrap();
    drop(parent);
    lp.run_until_idle();
    for _ in 0..100 {
        let f = lp.spawn(|_| Ok(Value::int(2)));
        lp.run_until(&f).unwrap();
    }
    cancel.borrow().as_ref().unwrap().cancel();
    assert_eq!(
        lp.run_until(child.borrow().as_ref().unwrap()).unwrap_err(),
        Err::Cancelled
    );
}
#[test]
fn external_cancellation_wakes_parked_waiter_from_another_thread() {
    let rt = runtime();
    let mut lp = scheduler(&rt);
    let linked = CancellationToken::new();
    let authority = linked.clone();
    let f = lp.spawn_with(
        SpawnOptions {
            linked: vec![linked],
            ..Default::default()
        },
        |cx| {
            let child = cx.spawn(|cx| {
                cx.sleep(Duration::from_secs(60))?;
                Ok(Value::int(0))
            });
            cx.await_future(&child)
        },
    );
    let thread = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(5));
        authority.cancel();
    });
    assert_eq!(lp.run_until(&f).unwrap_err(), Err::Cancelled);
    thread.join().unwrap();
    lp.run_until_idle();
    assert_eq!(lp.task_count(), 0);
}
#[test]
fn retired_io_messages_cannot_complete_a_reused_task_slot() {
    let rt = runtime();
    let mut lp = scheduler(&rt);
    for _ in 0..100 {
        let old = lp.spawn(|cx| {
            cx.io(|_| async {
                tokio::time::sleep(Duration::from_secs(60)).await;
                Ok(OwnedValue::Int(11))
            })
        });
        lp.run_until_idle();
        old.request_cancel();
        assert_eq!(lp.run_until(&old).unwrap_err(), Err::Cancelled);
        let new = lp.spawn(|cx| cx.io(|_| async { Ok(OwnedValue::Int(22)) }));
        assert_ne!(old.id(), new.id());
        assert_eq!(lp.run_until(&new).unwrap().as_int(), 22);
    }
}
#[test]
fn recycled_future_does_not_retain_a_precise_root() {
    let rt = runtime();
    let mut lp = scheduler(&rt);
    let f = lp.spawn(|_| Ok(Value::pointer(0xfeed)));
    lp.run_until(&f).unwrap();
    drop(f);
    let mut count = 0;
    lp.precise_roots(&mut |_| count += 1);
    assert_eq!(count, 0);
    for _ in 0..100 {
        let f = lp.spawn(|_| Ok(Value::int(0)));
        lp.run_until(&f).unwrap();
    }
    lp.precise_roots(&mut |_| count += 1);
    assert_eq!(count, 0);
}
#[test]
fn single_future_waiters_resume_fifo() {
    let rt = runtime();
    let mut lp = scheduler(&rt);
    let log = Rc::new(RefCell::new(Vec::new()));
    let output = log.clone();
    let root = lp.spawn(move |cx| {
        let producer = cx.spawn(|cx| {
            cx.sleep(Duration::from_millis(5))?;
            Ok(Value::int(7))
        });
        let mut joins = Vec::new();
        for index in 0..8 {
            let p = producer.clone();
            let out = output.clone();
            joins.push(cx.spawn(move |cx| {
                let v = cx.await_future(&p)?;
                out.borrow_mut().push(index);
                Ok(v)
            }));
        }
        for join in joins {
            assert_eq!(cx.await_future(&join)?.as_int(), 7);
        }
        Ok(Value::int(0))
    });
    lp.run_until(&root).unwrap();
    assert_eq!(*log.borrow(), (0..8).collect::<Vec<_>>());
}
#[test]
fn shutdown_does_not_reset_generations_for_local_reuse() {
    let rt = runtime();
    let mut lp = scheduler(&rt);
    let old = lp.spawn(|cx| {
        cx.io(|_| async {
            tokio::time::sleep(Duration::from_secs(60)).await;
            Ok(OwnedValue::Int(11))
        })
    });
    lp.run_until_idle();
    lp.shutdown(Duration::ZERO);
    let new = lp.spawn(|cx| cx.io(|_| async { Ok(OwnedValue::Int(22)) }));
    assert_ne!(old.id(), new.id());
    assert_eq!(lp.task_state(&old), TaskState::Done);
    assert_eq!(lp.run_until(&new).unwrap().as_int(), 22);
    assert!(matches!(old.state(), FutureState::Cancelled));
}
#[test]
fn deep_cancel_ancestry_is_released_on_a_small_fiber_without_recursion() {
    let rt = runtime();
    let mut lp = scheduler(&rt);
    let f = lp.spawn(|_| {
        let root = TaskCancel::default();
        let mut leaf = root.clone();
        for _ in 0..10_000 {
            leaf = leaf.child(vec![]);
        }
        root.cancel();
        assert!(leaf.is_cancelled());
        drop(leaf);
        Ok(Value::int(1))
    });
    assert_eq!(lp.run_until(&f).unwrap().as_int(), 1);
}

#[test]
fn integration_root_leases_are_loop_owned_and_released_without_borrowing() {
    let rt = runtime();
    let lp = scheduler(&rt);
    let integration = lp.integration_roots();
    let root = unsafe { integration.root(0) };
    let mut slots = Vec::new();
    lp.precise_roots(&mut |slot| slots.push(slot));
    assert_eq!(slots.len(), 1);
    assert_eq!(root.get(), 0);
    integration.with_roots(|source| {
        // Drop may occur while the loop is borrowed during forced fiber unwind.
        drop(root);
        source.precise_roots(&mut |_| panic!("released root still visited"));
    });
    for _ in 0..1000 {
        let root = unsafe { integration.root(0) };
        let mut count = 0;
        integration.with_roots(|source| source.precise_roots(&mut |_| count += 1));
        assert_eq!(count, 1);
        drop(root);
    }
    lp.precise_roots(&mut |_| panic!("expired integration root"));
}

#[test]
fn idle_poll_word_is_clear_and_local_cancellation_is_immediate_and_sticky() {
    let rt = runtime();
    let mut lp = scheduler(&rt);
    let word = lp.integration_roots().poll_word();
    let word_in = word.clone();
    let future = lp.spawn(move |cx| {
        assert_eq!(word_in.get() & 1, 0);
        for _ in 0..1000 {
            cx.poll()?;
        }
        assert_eq!(word_in.get() & 1, 0);
        cx.cancel().cancel();
        assert_eq!(word_in.get() & 1, 1);
        assert!(matches!(cx.poll(), Err(Err::Cancelled)));
        assert!(matches!(cx.poll(), Err(Err::Cancelled)));
        Ok(Value::int(1))
    });
    lp.run_until(&future).unwrap();
}

#[test]
fn await_settled_selects_failures_without_observing_them() {
    let rt = runtime();
    let mut lp = scheduler(&rt);
    let root = lp.spawn(|cx| {
        let success = cx.spawn(|_| Ok(Value::int(1)));
        let failure = cx.spawn(|_| Err(error("not consumed")));
        cx.await_future(&success)?;
        assert_eq!(cx.await_settled(&[failure.clone(), success])?, 0);
        assert_eq!(cx.await_settled(&[failure.clone()])?, 0);
        assert_eq!(cx.await_settled(&[failure])?, 0);
        Ok(Value::int(0))
    });
    lp.run_until(&root).unwrap();
    let report = lp.shutdown(Duration::ZERO);
    assert_eq!(report.unhandled.len(), 1);
    assert_eq!(report.unhandled[0].1, error("not consumed"));
}

#[test]
fn pending_await_handoff_scans_the_waiting_parent_stack() {
    let rt = runtime();
    let address = Rc::new(Cell::new(0));
    let hits = Rc::new(Cell::new(0));
    let heap = RootProbe {
        word_addr: address.clone(),
        hits: hits.clone(),
        precise: Rc::new(Cell::new(0)),
    };
    let mut lp = TaskLoop::new(
        Config::default(),
        rt.handle().clone(),
        Rc::new(RefCell::new(heap)),
    );
    let root = lp.spawn(move |cx| {
        let word = std::hint::black_box(0x1234_abcdusize);
        address.set(&word as *const usize as usize);
        let child = cx.spawn(|cx| {
            cx.collect();
            cx.sleep(Duration::from_millis(1))?;
            cx.collect();
            Ok(Value::int(1))
        });
        let result = cx.await_future(&child)?;
        std::hint::black_box(word);
        Ok(result)
    });
    assert_eq!(lp.run_until(&root).unwrap().as_int(), 1);
    assert_eq!(hits.get(), 2);
}

#[test]
fn switch_hooks_restore_bindings_across_handoff_and_parked_resume() {
    thread_local! { static CURRENT_ID: Cell<usize> = const { Cell::new(0) }; }
    unsafe fn enter(data: *mut ()) {
        CURRENT_ID.set(unsafe { *data.cast::<usize>() });
    }
    unsafe fn leave(_: *mut ()) {
        CURRENT_ID.set(0);
    }
    fn bound(cx: &mut TaskCx<'_>, body: impl FnOnce(&mut TaskCx<'_>) -> Outcome) -> Outcome {
        struct Binding<'a>(*mut TaskCx<'a>, Box<usize>);
        impl Drop for Binding<'_> {
            fn drop(&mut self) {
                unsafe {
                    (*self.0).set_switch_hook(None);
                }
                CURRENT_ID.set(0);
            }
        }
        let mut binding = Binding(cx, Box::new(cx.id() + 1));
        let hook = SwitchHook {
            data: (&mut *binding.1 as *mut usize).cast(),
            enter,
            leave,
        };
        unsafe {
            enter(hook.data);
            cx.set_switch_hook(Some(hook));
        }
        let result = body(cx);
        drop(binding);
        result
    }
    let rt = runtime();
    let mut lp = scheduler(&rt);
    let root = lp.spawn(|cx| {
        bound(cx, |cx| {
            let id = cx.id() + 1;
            assert_eq!(CURRENT_ID.get(), id);
            let child = cx.spawn(|cx| {
                bound(cx, |cx| {
                    let id = cx.id() + 1;
                    assert_eq!(CURRENT_ID.get(), id);
                    cx.sleep(Duration::from_millis(1))?;
                    assert_eq!(CURRENT_ID.get(), id);
                    Ok(Value::int(7))
                })
            });
            let result = cx.await_future(&child)?;
            assert_eq!(CURRENT_ID.get(), id);
            Ok(result)
        })
    });
    assert_eq!(lp.run_until(&root).unwrap().as_int(), 7);
    assert_eq!(CURRENT_ID.get(), 0);
}

#[test]
fn forced_unwind_installs_bindings_until_the_fiber_unregisters_them() {
    thread_local! { static CURRENT_ID: Cell<usize> = const { Cell::new(0) }; }
    unsafe fn enter(data: *mut ()) {
        CURRENT_ID.set(unsafe { *data.cast::<usize>() });
    }
    unsafe fn leave(_: *mut ()) {
        CURRENT_ID.set(0);
    }
    struct Binding<'a> {
        cx: *mut TaskCx<'a>,
        id: Box<usize>,
        drops: Rc<Cell<usize>>,
    }
    impl Drop for Binding<'_> {
        fn drop(&mut self) {
            assert_eq!(CURRENT_ID.get(), *self.id);
            unsafe { (*self.cx).set_switch_hook(None) };
            CURRENT_ID.set(0);
            self.drops.set(self.drops.get() + 1);
        }
    }
    let rt = runtime();
    let mut lp = scheduler(&rt);
    let drops = Rc::new(Cell::new(0));
    let output = drops.clone();
    let future = lp.spawn(move |cx| {
        let mut binding = Binding {
            cx,
            id: Box::new(cx.id() + 1),
            drops: output,
        };
        let hook = SwitchHook {
            data: (&mut *binding.id as *mut usize).cast(),
            enter,
            leave,
        };
        unsafe {
            enter(hook.data);
            cx.set_switch_hook(Some(hook));
        }
        cx.shield(|cx| cx.sleep(Duration::from_secs(60)))?;
        drop(binding);
        Ok(Value::int(0))
    });
    lp.tick(false);
    assert_eq!(CURRENT_ID.get(), 0);
    let report = lp.shutdown(Duration::ZERO);
    assert_eq!(report.leaked.len(), 1);
    assert_eq!(drops.get(), 1);
    assert!(matches!(future.state(), FutureState::Cancelled));
    assert_eq!(CURRENT_ID.get(), 0);
}
