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
