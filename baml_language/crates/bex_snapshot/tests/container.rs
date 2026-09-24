//! Container-level behavior: an embedded program makes a snapshot
//! self-contained, and several threads share one object table.

#![cfg(not(target_arch = "wasm32"))]

mod common;

use std::sync::{Arc, atomic::AtomicBool};

use bex_snapshot::{ParkedAt, ThreadInput, WriteOptions};
use bex_vm::{BexVm, VmExecState};
use bex_vm_types::{Program, Value};
use borsh::BorshDeserialize;
use common::Harness;

const PROGRAM: &str = r#"
function main() -> string[] {
    let seen: string[] = ["start"];
    let i = 0;
    while (i < 2) {
        baml.io.println("round " + i.to_string());
        seen.push("round " + i.to_string());
        i += 1;
    }
    seen
}
"#;

#[test]
fn a_snapshot_with_an_embedded_program_is_self_contained() {
    let harness = Harness::new(PROGRAM);
    let expected = harness.run_plain("user.main");
    let program_bytes = borsh::to_vec(&harness.program).expect("program serializes");
    assert_eq!(
        bex_snapshot::program_hash_of_bytes(&program_bytes),
        harness.program_hash
    );

    let (vm, _op, args) = harness.run_to_sysop("user.main", 1);
    let threads = [ThreadInput {
        thread_id: 1,
        parent_thread: None,
        name: "main".to_string(),
        vm: &vm,
        parked: ParkedAt::runnable(),
        extra_roots: args,
        settles_future: None,
        cancel: bex_snapshot::ThreadCancel::default(),
        user_cancels: Vec::new(),
        group: None,
        engine_state: Vec::new(),
    }];
    let (bytes, _) = bex_snapshot::write_snapshot(
        &vm.heap,
        &threads,
        WriteOptions {
            header: harness.header(3),
            embed_program: Some(program_bytes.clone()),
            compress: true,
            run_state: b"call_id_counter=7".to_vec(),
            future_id_span: 0,
        },
    )
    .expect("snapshot");
    drop(vm);

    // The receiving process has only the file.
    let embedded = bex_snapshot::read_embedded_program(&bytes)
        .expect("container")
        .expect("program is embedded");
    assert_eq!(embedded, program_bytes);
    // The run state is readable before any heap exists.
    assert_eq!(
        bex_snapshot::read_run_state(&bytes).expect("run state"),
        b"call_id_counter=7"
    );
    let program = Program::try_from_slice(&embedded).expect("program decodes");
    let hash = bex_snapshot::program_hash(&program).expect("hash");
    assert_eq!(hash, harness.program_hash, "the hash survives a round trip");

    let mut vm =
        BexVm::from_program(program, Arc::new(AtomicBool::new(false))).expect("program loads");
    let mut restored = bex_snapshot::restore_into(&vm.heap, &bytes, hash).expect("restores");
    assert_eq!(restored.header.seq, 3);
    assert_eq!(restored.run_state, b"call_id_counter=7");
    assert_eq!(
        restored.embedded_program.as_deref(),
        Some(&program_bytes[..])
    );
    let state = std::mem::take(&mut restored.threads[0].state);
    vm.import_thread_state(state).expect("imports");

    vm.stack.push(Value::NULL);
    let result = loop {
        match vm.exec().expect("exec") {
            VmExecState::Complete(value) => break common::render(&vm, value),
            VmExecState::SysOp { .. } => vm.stack.push(Value::NULL),
            VmExecState::EarlyYield => {}
            other => panic!("unexpected yield: {other:?}"),
        }
    };
    assert_eq!(result, expected);
}

#[test]
fn threads_share_one_object_table() {
    let harness = Harness::new(PROGRAM);
    let (vm, _op, args) = harness.run_to_sysop("user.main", 0);

    // Two thread entries over the same VM stand in for two threads that hold
    // the same objects (a parent and a spawned child, say).
    let thread = |id: u64, parent: Option<u64>| ThreadInput {
        thread_id: id,
        parent_thread: parent,
        name: format!("t{id}"),
        vm: &vm,
        parked: ParkedAt::runnable(),
        extra_roots: args.clone(),
        settles_future: None,
        cancel: bex_snapshot::ThreadCancel::default(),
        user_cancels: Vec::new(),
        group: None,
        engine_state: Vec::new(),
    };
    let single = [thread(1, None)];
    let double = [thread(1, None), thread(2, Some(1))];
    let options = |seq| WriteOptions {
        header: harness.header(seq),
        embed_program: None,
        compress: false,
        run_state: Vec::new(),
        future_id_span: 0,
    };
    let (_, single_stats) =
        bex_snapshot::write_snapshot(&vm.heap, &single, options(0)).expect("snapshot");
    let (bytes, double_stats) =
        bex_snapshot::write_snapshot(&vm.heap, &double, options(1)).expect("snapshot");
    assert_eq!(
        single_stats.objects, double_stats.objects,
        "shared objects are written once"
    );

    let target = harness.new_vm();
    let restored =
        bex_snapshot::restore_into(&target.heap, &bytes, harness.program_hash).expect("restores");
    assert_eq!(restored.threads.len(), 2);
    assert_eq!(restored.threads[1].thread_id, 2);
    assert_eq!(restored.threads[1].parent_thread, Some(1));
    assert_eq!(restored.threads[1].name, "t2");
    // Identity is preserved: both restored threads point at the same objects.
    assert_eq!(
        restored.threads[0].state.stack,
        restored.threads[1].state.stack
    );
    assert_eq!(
        restored.threads[0].extra_roots,
        restored.threads[1].extra_roots
    );
    assert!(
        restored.threads[0]
            .state
            .stack
            .iter()
            .any(|value| value.is_object())
    );
}
