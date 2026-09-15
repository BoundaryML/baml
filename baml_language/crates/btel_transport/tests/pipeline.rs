use std::{cell::RefCell, rc::Rc};

use btel_core::stage::{Aggregator, EventBuilder, MarkerBatch, MarkerRange, Publisher};
use btel_transport::{Pipeline, RangeHandler, TransportConfig, TransportError, transport};

struct UnusedBuilder;
impl EventBuilder for UnusedBuilder {
    type Event = u64;
    fn build(&mut self, _: MarkerRange<'_>, _: impl FnMut(u64)) {
        panic!("discard emitted a batch");
    }
}
struct UnusedAggregator;
impl Aggregator<u64> for UnusedAggregator {
    type Aggregate = u64;
    fn observe(&mut self, _: &u64, _: impl FnMut(u64)) {
        panic!("discard emitted an event");
    }
}
struct UnusedPublisher;
impl Publisher<u64, u64> for UnusedPublisher {
    fn event(&mut self, _: u64) {
        panic!("unexpected publication");
    }
    fn aggregate(&mut self, _: u64) {
        panic!("unexpected aggregation");
    }
}

#[derive(Clone)]
struct Recorder(Rc<RefCell<Vec<(u64, u64)>>>);
impl RangeHandler for Recorder {
    fn accept(&mut self, range: MarkerRange<'_>, _: impl FnMut(MarkerBatch)) {
        for bytes in range.bytes.as_chunks::<8>().0 {
            self.0
                .borrow_mut()
                .push((range.source_id, u64::from_le_bytes(*bytes)));
        }
    }
}
fn config() -> TransportConfig {
    TransportConfig {
        segment_bytes: 64,
        freelist_segments: 2,
        memory_bytes: 32 * 1024 * 1024,
    }
}

#[test]
fn finish_drains_more_than_sixteen_segments_exactly_once() {
    let (factory, drainer) = transport(config()).unwrap();
    let mut producer = factory.producer(7).unwrap();
    for n in 0u64..4097 {
        assert!(producer.write(&n.to_le_bytes()));
    }
    drop(producer);
    let log = Recorder(Rc::default());
    let observed = log.clone();
    drainer
        .finish(Pipeline::new(
            log,
            UnusedBuilder,
            UnusedAggregator,
            UnusedPublisher,
        ))
        .unwrap();
    assert_eq!(
        *observed.0.borrow(),
        (0..4097).map(|n| (7, n)).collect::<Vec<_>>()
    );
    assert!(matches!(factory.producer(8), Err(TransportError::Closed)));
}

#[test]
fn a_hot_source_does_not_reset_the_round_robin_cursor() {
    let (factory, mut drainer) = transport(config()).unwrap();
    let mut hot = factory.producer(1).unwrap();
    let mut quiet = factory.producer(2).unwrap();
    for n in 0u64..4097 {
        assert!(hot.write(&n.to_le_bytes()));
    }
    assert!(quiet.write(&99u64.to_le_bytes()));
    let log = Recorder(Rc::default());
    let observed = log.clone();
    let mut pipeline = Pipeline::new(log, UnusedBuilder, UnusedAggregator, UnusedPublisher);
    assert!(drainer.drain(&mut pipeline, 1));
    assert!(observed.0.borrow().iter().all(|(source, _)| *source == 1));
    assert!(drainer.drain(&mut pipeline, 1));
    assert_eq!(observed.0.borrow().last(), Some(&(2, 99)));
    drop(hot);
    drop(quiet);
    drainer.finish(pipeline).unwrap();
    assert_eq!(observed.0.borrow().len(), 4098);
}

#[test]
fn finishing_with_a_live_writer_is_rejected() {
    let (factory, drainer) = transport(config()).unwrap();
    let _producer = factory.producer(1).unwrap();
    let pipeline = Pipeline::new(
        Recorder(Rc::default()),
        UnusedBuilder,
        UnusedAggregator,
        UnusedPublisher,
    );
    assert!(matches!(
        drainer.finish(pipeline),
        Err(TransportError::ActiveProducers)
    ));
}

#[test]
fn stages_flush_in_order_and_events_and_aggregates_take_separate_branches() {
    type Log = Rc<RefCell<Vec<String>>>;
    struct H;
    impl RangeHandler for H {
        fn accept(&mut self, r: MarkerRange<'_>, mut emit: impl FnMut(MarkerBatch)) {
            emit(MarkerBatch {
                source_id: r.source_id,
                bytes: r.bytes.to_vec(),
            });
        }
        fn finish(&mut self, mut emit: impl FnMut(MarkerBatch)) {
            emit(MarkerBatch {
                source_id: 0,
                bytes: vec![2],
            });
        }
    }
    struct B;
    impl EventBuilder for B {
        type Event = u8;
        fn build(&mut self, r: MarkerRange<'_>, mut emit: impl FnMut(u8)) {
            for b in r.bytes {
                emit(*b);
            }
        }
        fn finish(&mut self, mut emit: impl FnMut(u8)) {
            emit(3);
        }
    }
    struct A;
    impl Aggregator<u8> for A {
        type Aggregate = u64;
        fn observe(&mut self, e: &u8, mut emit: impl FnMut(u64)) {
            emit(u64::from(*e) * 10);
        }
        fn finish(&mut self, mut emit: impl FnMut(u64)) {
            emit(99);
        }
    }
    struct P(Log);
    impl Publisher<u8, u64> for P {
        fn event(&mut self, e: u8) {
            self.0.borrow_mut().push(format!("event {e}"));
        }
        fn aggregate(&mut self, a: u64) {
            self.0.borrow_mut().push(format!("aggregate {a}"));
        }
        fn finish(&mut self) {
            self.0.borrow_mut().push("finish".into());
        }
    }
    let log = Log::default();
    let mut pipeline = Pipeline::new(H, B, A, P(log.clone()));
    let (factory, mut drainer) = transport(config()).unwrap();
    let mut producer = factory.producer(1).unwrap();
    assert!(producer.write(&[1]));
    assert!(drainer.drain(&mut pipeline, 1));
    drop(producer);
    drainer.finish(pipeline).unwrap();
    assert_eq!(
        *log.borrow(),
        [
            "aggregate 10",
            "event 1",
            "aggregate 20",
            "event 2",
            "aggregate 30",
            "event 3",
            "aggregate 99",
            "finish"
        ]
    );
}

#[test]
fn concurrent_endpoints_preserve_every_source_record_after_factory_drop() {
    let (factory, mut drainer) = transport(config()).unwrap();
    let writers: Vec<_> = (1..=2)
        .map(|source| {
            let factory = factory.clone();
            std::thread::spawn(move || {
                let mut producer = factory.producer(source).unwrap();
                for seq in 0u64..1025 {
                    assert!(producer.write(&seq.to_le_bytes()));
                }
            })
        })
        .collect();
    // The drainer and producers must keep the instance's context alive.
    drop(factory);
    let log = Recorder(Rc::default());
    let observed = log.clone();
    let mut pipeline = Pipeline::new(log, UnusedBuilder, UnusedAggregator, UnusedPublisher);
    while writers.iter().any(|writer| !writer.is_finished()) {
        drainer.drain(&mut pipeline, 1);
        std::thread::yield_now();
    }
    for writer in writers {
        writer.join().unwrap();
    }
    drainer.finish(pipeline).unwrap();
    let log = observed.0.borrow();
    assert_eq!(log.len(), 2050);
    for source in 1..=2 {
        let actual: Vec<_> = log
            .iter()
            .filter(|(id, _)| *id == source)
            .map(|(_, seq)| *seq)
            .collect();
        assert_eq!(actual, (0..1025).collect::<Vec<_>>());
    }
}

#[test]
fn empty_rings_consume_a_turn_without_resetting_the_cursor() {
    let (factory, mut drainer) = transport(config()).unwrap();
    let idle = factory.producer(1).unwrap();
    let mut active = factory.producer(2).unwrap();
    assert!(active.write(&99u64.to_le_bytes()));
    let log = Recorder(Rc::default());
    let observed = log.clone();
    let mut pipeline = Pipeline::new(log, UnusedBuilder, UnusedAggregator, UnusedPublisher);
    assert!(!drainer.drain(&mut pipeline, 1));
    assert!(observed.0.borrow().is_empty());
    assert!(drainer.drain(&mut pipeline, 1));
    assert_eq!(*observed.0.borrow(), [(2, 99)]);
    drop(idle);
    drop(active);
    drainer.finish(pipeline).unwrap();
}
