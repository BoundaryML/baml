use std::{cell::RefCell, num::NonZeroUsize, time::Duration};

use btel_processor::{AggregateDelta, Publisher};
use btel_publisher::{RecordingConfig, RecordingPublisher, proto};
use btel_types::{AwaitDuration, CallPathId, CallPathNodeId, ClockDuration};
use prost::Message;

use super::*;

fn files(id: RecordingId, count: usize) -> Vec<SealedFile> {
    let output = RefCell::new(Vec::new());
    let mut publisher = RecordingPublisher::new(id, RecordingConfig::default(), |f| {
        output.borrow_mut().push(f);
    })
    .unwrap();
    for _ in 0..count {
        publisher.aggregate(AggregateDelta {
            node: CallPathNodeId::new(CallPathId::new_non_root(7).unwrap(), false),
            count: 1,
            total_duration: ClockDuration::from_ticks(3),
            total_io_duration: AwaitDuration::ZERO,
        });
        publisher.flush();
    }
    drop(publisher);
    output.into_inner()
}

#[test]
fn local_publisher_finishes_encoding_before_disk_drain() {
    let root = tempfile::tempdir().unwrap();
    let id = RecordingId::generate();
    let (entered, started) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let expected = files(id, 1).pop().unwrap();
    let expected_bytes = expected.bytes().to_vec();
    let sink = FileSink::start(
        root.path().to_owned(),
        FileSinkConfig::default(),
        move |file| {
            entered.send(()).unwrap();
            released.recv_timeout(Duration::from_secs(5)).unwrap();
            assert_eq!(file.bytes(), expected_bytes);
            Ok(())
        },
        |_| {},
    )
    .unwrap();
    let builder = btel_publisher::RecordingBuilder::new(id, RecordingConfig::default()).unwrap();
    let mut publisher = LocalPublisher::new(builder, sink);
    let sink = publisher.sink().unwrap().clone();
    publisher.aggregate(AggregateDelta {
        node: CallPathNodeId::new(CallPathId::new_non_root(7).unwrap(), false),
        count: 1,
        total_duration: ClockDuration::from_ticks(3),
        total_io_duration: AwaitDuration::ZERO,
    });
    publisher.finish();
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(sink.result(), None);
    drop(publisher);
    release.send(()).unwrap();
    sink.finish().unwrap();
    assert_eq!(sink.result(), Some(Ok(())));
}

#[test]
fn writes_exact_publisher_bytes_and_finish_is_idempotent() {
    let root = tempfile::tempdir().unwrap();
    let id = RecordingId::generate();
    let mut sink = FileSink::create(root.path(), id, FileSinkConfig::default()).unwrap();
    let sender = sink.take_sender();
    let produced = files(id, 3);
    let expected: Vec<_> = produced
        .iter()
        .map(|f| (f.sequence(), f.bytes().to_vec()))
        .collect();
    for file in produced {
        sender.send(file).unwrap();
    }
    std::thread::scope(|scope| {
        scope.spawn(|| sink.finish().unwrap());
        scope.spawn(|| sink.finish().unwrap());
    });
    assert_eq!(sink.result(), Some(Ok(())));
    assert_eq!(fs::read_dir(sink.directory()).unwrap().count(), 3);
    for (sequence, bytes) in &expected {
        let path = sink.directory().join(format!("{sequence:020}.btel"));
        assert_eq!(&fs::read(path).unwrap(), bytes);
    }
    assert!(sender.send(files(id, 1).pop().unwrap()).is_err());
    assert!(FileSink::create(root.path(), id, FileSinkConfig::default()).is_err());
    let read = read_directory(sink.directory()).unwrap();
    assert_eq!(read.files.len(), 3);
    assert!(!read.has_recording_end);
    assert_eq!(
        read.issues,
        vec![ReadIssue::UnresolvedReference {
            kind: "call path",
            id: 7
        }]
    );
}

#[test]
fn full_queue_waits_for_capacity_and_unblocks_on_writer_failure() {
    for fail in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let (entered, started) = mpsc::channel();
        let (release, released) = mpsc::channel();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let written = Arc::clone(&seen);
        let mut first = true;
        let mut sink = FileSink::start(
            root.path().to_owned(),
            FileSinkConfig {
                queue_files: NonZeroUsize::new(1).unwrap(),
            },
            move |file| {
                if first {
                    first = false;
                    entered.send(()).unwrap();
                    released.recv_timeout(Duration::from_secs(5)).unwrap();
                    if fail {
                        return Err(FileSinkError("injected disk failure".into()));
                    }
                }
                written.lock().unwrap().push(file.sequence().get());
                Ok(())
            },
            |_| {},
        )
        .unwrap();
        let sender = sink.take_sender();
        let mut files = files(RecordingId::generate(), 3).into_iter();
        sender.send(files.next().unwrap()).unwrap();
        started.recv_timeout(Duration::from_secs(5)).unwrap();
        sender.send(files.next().unwrap()).unwrap();
        let third = files.next().unwrap();
        let (attempting, attempted) = mpsc::channel();
        let (completed, completion) = mpsc::channel();
        let pending = std::thread::spawn(move || {
            attempting.send(()).unwrap();
            completed.send(sender.send(third)).unwrap();
        });
        attempted.recv_timeout(Duration::from_secs(5)).unwrap();
        let early = completion.recv_timeout(Duration::from_millis(50));
        // Always unblock the worker before asserting, including on a regression.
        release.send(()).unwrap();
        assert!(matches!(early, Err(mpsc::RecvTimeoutError::Timeout)));
        let admission = completion.recv_timeout(Duration::from_secs(5)).unwrap();
        pending.join().unwrap();
        let finished = sink.finish();
        if fail {
            let expected = Err(FileSinkError("injected disk failure".into()));
            assert_eq!(admission, expected);
            assert_eq!(finished, expected);
        } else {
            admission.unwrap();
            finished.unwrap();
            assert_eq!(*seen.lock().unwrap(), [1, 2, 3]);
        }
    }
}

#[test]
fn disk_failure_and_duplicate_sequence_are_observable() {
    let root = tempfile::tempdir().unwrap();
    let id = RecordingId::generate();
    let mut sink = FileSink::create(root.path(), id, FileSinkConfig::default()).unwrap();
    let sender = sink.take_sender();
    let original = files(id, 1).pop().unwrap();
    // Interfere after successful setup, so failure occurs asynchronously.
    fs::remove_dir(sink.directory()).unwrap();
    fs::write(sink.directory(), b"not a directory").unwrap();
    sender.send(original).unwrap();
    let error = sink.finish().unwrap_err();
    assert_eq!(sink.result(), Some(Err(error.clone())));
    assert_eq!(sink.finish(), Err(error));

    let id = RecordingId::generate();
    let mut sink = FileSink::create(root.path(), id, FileSinkConfig::default()).unwrap();
    let sender = sink.take_sender();
    let original = files(id, 1).pop().unwrap();
    let expected = original.bytes().to_vec();
    sender.send(original).unwrap();
    sender.send(files(id, 1).pop().unwrap()).unwrap();
    assert!(sink.finish().unwrap_err().to_string().contains("sequence"));
    assert_eq!(
        fs::read(sink.directory().join("00000000000000000001.btel")).unwrap(),
        expected
    );
}

#[test]
fn recovery_keeps_later_files_when_a_middle_file_is_missing_or_corrupt() {
    let root = tempfile::tempdir().unwrap();
    let id = RecordingId::generate();
    let mut sink = FileSink::create(root.path(), id, FileSinkConfig::default()).unwrap();
    let sender = sink.take_sender();
    for file in files(id, 4) {
        sender.send(file).unwrap();
    }
    sink.finish().unwrap();
    fs::remove_file(sink.directory().join("00000000000000000002.btel")).unwrap();
    fs::write(sink.directory().join("00000000000000000003.btel"), [0xff]).unwrap();
    fs::write(
        sink.directory().join("00000000000000000005.btel.part"),
        [0x08],
    )
    .unwrap();
    let read = read_directory(sink.directory()).unwrap();
    assert_eq!(
        read.files.iter().map(|f| f.sequence).collect::<Vec<_>>(),
        vec![1, 4]
    );
    assert!(
        read.issues
            .contains(&ReadIssue::MissingSequences { first: 2, last: 3 })
    );
    assert!(
        read.issues
            .iter()
            .any(|i| matches!(i, ReadIssue::InvalidFile { .. }))
    );
    assert!(
        read.issues
            .iter()
            .any(|i| matches!(i, ReadIssue::UnfinishedFile(_)))
    );
    assert!(!read.has_recording_end);
}

#[test]
fn later_definitions_resolve_a_live_prefix_and_wrong_identity_is_rejected() {
    let root = tempfile::tempdir().unwrap();
    let id = RecordingId::generate();
    let mut sink = FileSink::create(root.path(), id, FileSinkConfig::default()).unwrap();
    let sender = sink.take_sender();
    let original = files(id, 1).pop().unwrap();
    let original_bytes = original.bytes().to_vec();
    sender.send(original).unwrap();
    sink.finish().unwrap();
    assert!(!read_directory(sink.directory()).unwrap().issues.is_empty());
    let mut second = proto::RecordingFile::decode(original_bytes.as_slice()).unwrap();
    second.sequence = 2;
    second.aggregates = None;
    second.definitions = Some(proto::Definitions {
        functions: vec![proto::FunctionDefinition {
            function_id: 8,
            resolution: Some(proto::function_definition::Resolution::Unavailable(
                proto::MetadataUnavailable {},
            )),
        }],
        call_paths: vec![proto::CallPathDefinition {
            call_path_id: 7,
            thread_id: 5,
            callee_function_id: 8,
            edge: proto::CallPathEdge::Synchronous as i32,
            ..Default::default()
        }],
        threads: vec![proto::ThreadDefinition {
            thread_id: 5,
            clock_epoch_id: 3,
            ..Default::default()
        }],
        clock_epochs: vec![proto::ClockEpochDefinition {
            epoch_id: 3,
            domain_id: 3,
            source: proto::ClockSource::OsMonotonic as i32,
            multiplier: 1,
            ..Default::default()
        }],
    });
    let path = sink.directory().join("00000000000000000002.btel");
    fs::write(&path, second.encode_to_vec()).unwrap();
    assert!(read_directory(sink.directory()).unwrap().issues.is_empty());
    second.header.as_mut().unwrap().recording_id = RecordingId::generate().as_bytes().to_vec();
    fs::write(path, second.encode_to_vec()).unwrap();
    let read = read_directory(sink.directory()).unwrap();
    assert_eq!(read.files.len(), 1);
    assert!(read.issues.iter().any(
        |i| matches!(i, ReadIssue::InvalidFile { reason, .. } if reason.contains("identity"))
    ));
}

fn snapshot(pool: &btel_snapshot::SnapshotPool, n: i64) -> btel_snapshot::Snapshot {
    pool.try_acquire()
        .unwrap()
        .finish_value(btel_snapshot::SnapshotValue::Int(n))
}

#[test]
fn cas_is_shared_across_recordings_and_atomic_under_concurrent_writers() {
    let root = tempfile::tempdir().unwrap();
    let cas = root.path().join("cas");
    let pool = btel_snapshot::SnapshotPool::new(3, btel_snapshot::Limits::default());
    let value = snapshot(&pool, 42);
    let id = value.id();
    let mut expected = Vec::new();
    value.write_blob(&mut expected).unwrap();
    drop(value);
    let mut a = FileSink::create_with_cas(
        root.path(),
        &cas,
        RecordingId::generate(),
        FileSinkConfig::default(),
        |_| {},
    )
    .unwrap();
    let mut b = FileSink::create_with_cas(
        root.path(),
        &cas,
        RecordingId::generate(),
        FileSinkConfig::default(),
        |_| {},
    )
    .unwrap();
    a.take_sender().send_snapshot(snapshot(&pool, 42)).unwrap();
    b.take_sender().send_snapshot(snapshot(&pool, 42)).unwrap();
    a.finish().unwrap();
    b.finish().unwrap();
    assert_eq!(pool.stats().in_use, 0);
    let path = cas_path(&cas, id);
    assert_eq!(fs::read(&path).unwrap(), expected);
    let filename = path.file_name().unwrap().to_str().unwrap();
    assert_eq!(filename.len(), 32);
    assert_eq!(
        path,
        cas.join("v1")
            .join(&filename[..2])
            .join(&filename[2..4])
            .join(&filename[4..6])
            .join(filename)
    );
    assert_eq!(
        fs::read_dir(path.parent().unwrap()).unwrap().count(),
        1,
        "no partials left behind"
    );
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    let mut c = FileSink::create_with_cas(
        root.path(),
        &cas,
        RecordingId::generate(),
        FileSinkConfig::default(),
        |_| {},
    )
    .unwrap();
    c.take_sender().send_snapshot(snapshot(&pool, 42)).unwrap();
    c.finish().unwrap();
    assert_eq!(
        fs::metadata(path).unwrap().modified().unwrap(),
        modified,
        "existing CAS entry was reused"
    );
}

#[test]
fn cas_failure_releases_full_queue_and_retained_snapshot_owners() {
    let root = tempfile::tempdir().unwrap();
    let pool = btel_snapshot::SnapshotPool::new(3, btel_snapshot::Limits::default());
    let (entered, started) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let mut sink = FileSink::start_with_snapshots(
        root.path().to_owned(),
        FileSinkConfig {
            queue_files: NonZeroUsize::new(1).unwrap(),
        },
        |_| Ok(()),
        move |_| {
            entered.send(()).unwrap();
            released.recv_timeout(Duration::from_secs(5)).unwrap();
            Err(FileSinkError("injected ENOSPC".into()))
        },
        |_| {},
    )
    .unwrap();
    let sender = sink.take_sender();
    sender.send_snapshot(snapshot(&pool, 1)).unwrap();
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    sender.send_snapshot(snapshot(&pool, 2)).unwrap();
    let third = snapshot(&pool, 3);
    let (completed, completion) = mpsc::channel();
    let waiting = std::thread::spawn(move || completed.send(sender.send_snapshot(third)).unwrap());
    let early = completion.recv_timeout(Duration::from_millis(50));
    release.send(()).unwrap();
    assert!(matches!(early, Err(mpsc::RecvTimeoutError::Timeout)));
    assert!(
        completion
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .is_err()
    );
    waiting.join().unwrap();
    assert!(sink.finish().is_err());
    assert_eq!(pool.stats().in_use, 0);
}
