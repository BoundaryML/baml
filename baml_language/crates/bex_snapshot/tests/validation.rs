//! Negative tests: runs that cannot be snapshotted report a clean error with a
//! path, and bytes that are not a valid snapshot fail validation instead of
//! crashing.

#![cfg(not(target_arch = "wasm32"))]

mod common;

use std::sync::Arc;

use bex_snapshot::{ParkedAt, SnapshotError};
use bex_vm_types::Value;
use common::Harness;
use sha2::{Digest, Sha256};

/// The callback of `.map` runs under a native continuation frame. Its sys-op
/// yield therefore happens with a `Frame::Native` on the call stack.
const NATIVE_FRAME_PROGRAM: &str = r#"
function main() -> int[] {
    [1, 2, 3].map((n) -> { baml.io.println("in map"); n * 2 })
}
"#;

#[test]
fn a_native_continuation_frame_blocks_the_snapshot_cleanly() {
    let harness = Harness::new(NATIVE_FRAME_PROGRAM);
    let (vm, _op, args) = harness.run_to_sysop("user.main", 0);
    assert!(
        vm.frames
            .iter()
            .any(|frame| matches!(frame, bex_vm::Frame::Native(_))),
        "the scenario must park with a native frame on the stack"
    );

    let err = harness
        .write(&vm, ParkedAt::runnable(), args.clone(), false, 0)
        .expect_err("a native frame is not a clean point");
    match &err {
        SnapshotError::Blocked { reason, path } => {
            assert!(reason.contains("native continuation"), "reason: {reason}");
            assert_eq!(path, &vec!["thread 1 (main)".to_string()]);
        }
        other => panic!("expected Blocked, got {other:?}"),
    }
    assert!(vm.export_thread_state().is_err());

    // The state dump still works, so that a blocked pause can be inspected.
    let threads = [bex_snapshot::ThreadInput {
        thread_id: 1,
        parent_thread: None,
        name: "main".to_string(),
        vm: &vm,
        parked: ParkedAt::runnable(),
        extra_roots: args,
    }];
    let dump = bex_snapshot::state_dump(&vm.heap, &threads, "r-test", 1);
    assert!(
        !dump["threads"][0]["frames"]
            .as_array()
            .expect("frames")
            .is_empty()
    );
}

const MARKER_PROGRAM: &str = r#"
class Holder {
    label string
    payload string[]
}

function main() -> int {
    let marker = Holder { label: "held", payload: ["a", "b"] };
    baml.io.println("parked");
    marker.payload.length()
}
"#;

#[test]
fn an_unserializable_value_is_reported_with_its_path_from_a_named_local() {
    let harness = Harness::new_unoptimized(MARKER_PROGRAM);
    let (mut vm, _op, args) = harness.run_to_sysop("user.main", 0);

    // Plant a host resource inside the structure that the local `marker`
    // holds: marker.payload[1] = <RustData>.
    let resource = vm.tlab.alloc_rust_data(Arc::new(42u32));
    let holder = vm
        .stack
        .0
        .iter()
        .filter_map(Value::as_object_ptr)
        .find(|ptr| matches!(vm.get_object(*ptr), bex_vm_types::Object::Instance(_)))
        .expect("the Holder instance is on the stack");
    let bex_vm_types::Object::Instance(instance) = vm.get_object(holder) else {
        unreachable!();
    };
    let payload = instance.fields[1]
        .load()
        .as_object_ptr()
        .expect("payload array");
    let bex_vm_types::Object::Array(array) = vm.get_object(payload) else {
        panic!("payload is an array");
    };
    array.data.lock_mut()[1] = Value::object(resource);

    let err = harness
        .write(&vm, ParkedAt::runnable(), args, false, 0)
        .expect_err("RustData cannot be serialized");
    let SnapshotError::Blocked { reason, path } = err else {
        panic!("expected Blocked");
    };
    assert!(reason.contains("host resource"), "reason: {reason}");
    assert_eq!(
        path,
        vec![
            "thread 1 (main)".to_string(),
            "frame user.main".to_string(),
            "local `marker`".to_string(),
            "field `payload`".to_string(),
            "[1]".to_string(),
            "rust_data <rust_data>".to_string(),
        ]
    );
}

fn small_snapshot(harness: &Harness, compress: bool) -> Vec<u8> {
    let (vm, op, args) = harness.run_to_sysop("user.main", 0);
    let parked = ParkedAt {
        kind: "sysop".to_string(),
        payload: op.into_bytes(),
    };
    harness
        .write(&vm, parked, args, compress, 0)
        .expect("snapshot")
        .0
}

#[test]
fn header_is_readable_and_mismatches_are_refused() {
    let harness = Harness::new(MARKER_PROGRAM);
    let bytes = small_snapshot(&harness, true);

    let header = bex_snapshot::read_header(&bytes).expect("header");
    assert_eq!(header, harness.header(0));
    assert_eq!(
        bex_snapshot::read_embedded_program(&bytes).expect("container"),
        None
    );

    // Wrong program hash.
    let vm = harness.new_vm();
    let err = bex_snapshot::restore_into(&vm.heap, &bytes, [0u8; 32]).err();
    assert!(matches!(err, Some(SnapshotError::Mismatch(_))), "{err:?}");

    // A different program with the right hash supplied by a confused caller:
    // the compile-time region differs, or a frame's function name does.
    let other = Harness::new(NATIVE_FRAME_PROGRAM);
    let other_vm = other.new_vm();
    match bex_snapshot::restore_into(&other_vm.heap, &bytes, harness.program_hash) {
        Err(SnapshotError::Mismatch(_) | SnapshotError::Format(_)) => {}
        Err(other) => panic!("unexpected error kind: {other:?}"),
        Ok(mut restored) => {
            let state = std::mem::take(&mut restored.threads[0].state);
            let mut other_vm = other_vm;
            assert!(other_vm.import_thread_state(state).is_err());
        }
    }

    // Unsupported format version.
    let mut wrong_version = bytes;
    wrong_version[8] = 0xEE;
    assert!(matches!(
        bex_snapshot::read_header(&wrong_version),
        Err(SnapshotError::Mismatch(_))
    ));
}

#[test]
fn corrupt_bytes_fail_validation_instead_of_crashing() {
    let harness = Harness::new(MARKER_PROGRAM);
    let vm = harness.new_vm();
    let restore = |bytes: &[u8]| bex_snapshot::restore_into(&vm.heap, bytes, harness.program_hash);

    for compress in [false, true] {
        let bytes = small_snapshot(&harness, compress);
        assert!(restore(&bytes).is_ok(), "the pristine snapshot restores");

        // Not a snapshot at all.
        assert!(matches!(restore(b""), Err(SnapshotError::Format(_))));
        assert!(matches!(
            restore(b"BAMLSNAP"),
            Err(SnapshotError::Format(_))
        ));
        assert!(matches!(
            restore(&vec![0xAB; 4096]),
            Err(SnapshotError::Format(_))
        ));
        assert!(bex_snapshot::read_header(&vec![0xAB; 4096]).is_err());

        // Every truncation fails.
        for len in 0..bytes.len() {
            assert!(restore(&bytes[..len]).is_err(), "truncated to {len}");
        }

        // Every single-byte change is caught (by the checksum at the latest).
        for index in 0..bytes.len() {
            let mut damaged = bytes.clone();
            damaged[index] ^= 0x5A;
            assert!(restore(&damaged).is_err(), "byte {index} flipped");
        }
    }
}

/// Damage that the checksum does not catch (the attacker, or a buggy writer,
/// recomputed it): the structural validation has to hold on its own. No
/// outcome may panic, and an `Ok` must still pass `import_thread_state`'s
/// checks or be refused there.
#[test]
fn damage_under_a_valid_checksum_never_panics() {
    let harness = Harness::new(MARKER_PROGRAM);
    let bytes = small_snapshot(&harness, false);
    let body_len = bytes.len() - 32;

    let mut refused = 0usize;
    let mut accepted = 0usize;
    // One shared target heap: refused restores leave unreachable placeholder
    // objects behind, which is allowed.
    let vm = harness.new_vm();
    for index in 12..body_len {
        for mask in [0x01u8, 0x80, 0xFF] {
            let mut damaged = bytes[..body_len].to_vec();
            damaged[index] ^= mask;
            let checksum = Sha256::digest(&damaged);
            damaged.extend_from_slice(&checksum);

            match bex_snapshot::restore_into(&vm.heap, &damaged, harness.program_hash) {
                Err(_) => refused += 1,
                Ok(_) => {
                    accepted += 1;
                    // Installing the state must not panic either. Building a
                    // VM per case is slow, so sample the accepted cases.
                    if accepted % 16 != 1 {
                        continue;
                    }
                    let mut target = harness.new_vm();
                    let mut restored =
                        bex_snapshot::restore_into(&target.heap, &damaged, harness.program_hash)
                            .expect("accepted once, accepted again");
                    if let Some(thread) = restored.threads.first_mut() {
                        let state = std::mem::take(&mut thread.state);
                        // A refusal and a success are both fine.
                        let _ = target.import_thread_state(state);
                    }
                }
            }
        }
    }
    assert!(refused > 0, "some damage must be refused");
    // Flips inside string payloads and similar data are legitimately accepted.
    assert!(accepted > 0, "some damage only changes data");
}

/// The dispatch loop reads bytecode without bounds checks, so a program
/// counter that points into the middle of an instruction must be refused at
/// import, before it can be executed.
#[test]
fn a_program_counter_inside_an_instruction_is_refused_at_import() {
    let harness = Harness::new(MARKER_PROGRAM);
    let bytes = small_snapshot(&harness, false);

    let import_with_delta = |delta: usize| {
        let mut vm = harness.new_vm();
        let mut restored = bex_snapshot::restore_into(&vm.heap, &bytes, harness.program_hash)
            .expect("the pristine snapshot restores");
        let mut state = std::mem::take(&mut restored.threads[0].state);
        state.frames.last_mut().expect("a frame").instruction_ptr += delta;
        vm.import_thread_state(state)
    };

    import_with_delta(0).expect("the untouched state imports");
    let refused: Vec<String> = (1..=8)
        .filter_map(|delta| import_with_delta(delta).err())
        .collect();
    assert!(
        !refused.is_empty(),
        "at least one of eight consecutive byte offsets is not an instruction start"
    );
    assert!(
        refused
            .iter()
            .any(|message| message.contains("not the start of an instruction")),
        "refusals: {refused:?}"
    );
}
