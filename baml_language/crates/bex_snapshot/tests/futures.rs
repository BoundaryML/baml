//! Futures, cancel tokens, task groups, and thread links in snapshots
//! (format version 2), at the VM level.
//!
//! The engine normally creates these values. Here the test allocates them in
//! the heap of a parked VM, hands them to the writer as values the engine
//! holds (`extra_roots`), restores into a fresh heap, and inspects the result.

#![cfg(not(target_arch = "wasm32"))]
#![allow(unsafe_code)]

mod common;

use std::sync::Arc;

use bex_heap::CollectionLevel;
use bex_snapshot::{
    GroupMembership, ParkedAt, RestoreOptions, SettledFuture, SnapshotError, ThreadCancel,
    ThreadInput, WriteOptions,
};
use bex_vm::BexVm;
use bex_vm_types::{
    CancelTokenData, Future, HeapPtr, Object, RealizedTy, TaskGroupInner, Value,
    types::{CancellationToken, FutureId, FutureRead},
};
use common::{Harness, collect_garbage, render};

const PROGRAM: &str = r#"
class Pair {
    left string
    right int[]
}

function main() -> string {
    let pair = Pair { left: "kept", right: [1, 2, 3] };
    baml.io.println("parked");
    pair.left + pair.right.length().to_string()
}
"#;

fn pending(vm: &mut BexVm, id: usize) -> HeapPtr {
    vm.tlab.alloc(Object::Future(Future::pending(
        FutureId::from_usize(id),
        RealizedTy::int(),
        RealizedTy::never(),
        CancellationToken::new(),
    )))
}

fn future_of(ptr: HeapPtr) -> &'static Future {
    // SAFETY: single-threaded test, no collection between allocation and use.
    match unsafe { ptr.get() } {
        Object::Future(future) => future,
        other => panic!("expected a future, got {other}"),
    }
}

/// The local `pair` of the parked `main`, as a value to store in a future.
fn pair_value(vm: &BexVm) -> Value {
    *vm.stack
        .0
        .iter()
        .find(|value| {
            value
                .as_object_ptr()
                .is_some_and(|ptr| matches!(unsafe { ptr.get() }, Object::Instance(_)))
        })
        .expect("the instance is on the stack")
}

fn options(harness: &Harness, future_id_span: u64) -> WriteOptions {
    WriteOptions {
        header: harness.header(0),
        embed_program: None,
        compress: true,
        run_state: Vec::new(),
        future_id_span,
    }
}

#[test]
fn futures_in_every_state_keep_their_state_value_flags_and_identity() {
    let harness = Harness::new(PROGRAM);
    let (mut vm, _op, _args) = harness.run_to_sysop("user.main", 0);
    let pair = pair_value(&vm);

    let still_pending = pending(&mut vm, 0);
    let ready = pending(&mut vm, 1);
    let failed = pending(&mut vm, 2);
    let failed_observed = pending(&mut vm, 3);
    let cancelled = pending(&mut vm, 4);
    let heap = Arc::clone(&vm.heap);
    unsafe {
        assert!(future_of(ready).settle_ready(heap.as_ref(), ready, pair));
        let trace = vec![bex_vm::StackFrame {
            function_name: "user.vendor".to_string(),
            file_path: "quotes.baml".to_string(),
            function_span: baml_type::Span::default(),
            error_line: 12,
        }];
        assert!(future_of(failed).settle_error(heap.as_ref(), failed, pair, trace.clone()));
        assert!(future_of(failed_observed).settle_error(
            heap.as_ref(),
            failed_observed,
            Value::int(7),
            trace
        ));
    }
    future_of(failed_observed).mark_observed();
    assert!(future_of(cancelled).request_cancel());

    // The same future twice: one object in the snapshot, one after the restore.
    let element_ty = RealizedTy::int();
    let shared = vm.tlab.alloc(Object::Array(bex_vm_types::types::Array::new(
        element_ty,
        vec![Value::object(ready), Value::object(ready)],
    )));
    let roots = vec![
        Value::object(still_pending),
        Value::object(ready),
        Value::object(failed),
        Value::object(failed_observed),
        Value::object(cancelled),
        Value::object(shared),
    ];

    let root = ThreadInput {
        name: "main".to_string(),
        extra_roots: roots,
        ..ThreadInput::new(1, &vm, ParkedAt::runnable())
    };
    // The thread that settles the pending future. Its VM stands in; only the
    // link matters here.
    let child = ThreadInput {
        parent_thread: Some(1),
        settles_future: Some(SettledFuture {
            id: FutureId::from_usize(0),
            object: Some(still_pending),
        }),
        cancel: ThreadCancel {
            cancelled: false,
            parent: Some(1),
        },
        ..ThreadInput::new(2, &vm, ParkedAt::runnable())
    };
    let (bytes, stats) =
        bex_snapshot::write_snapshot(&vm.heap, &[root, child], options(&harness, 5))
            .expect("futures are serializable");
    assert!(stats.objects >= 8, "{stats:?}");

    let mut target = harness.new_vm();
    let mut restored = bex_snapshot::restore_into_with(
        &target.heap,
        &bytes,
        harness.program_hash,
        RestoreOptions {
            future_id_base: 100,
        },
    )
    .expect("restores");
    assert_eq!(restored.future_id_span, 5);
    assert_eq!(restored.futures.len(), 5, "the shared future is one object");
    let pending_futures: Vec<_> = restored.futures.iter().filter(|f| f.pending).collect();
    assert_eq!(pending_futures.len(), 1);
    assert_eq!(pending_futures[0].id, FutureId::from_usize(100));
    assert_eq!(
        restored.threads[1].settles_future,
        Some(FutureId::from_usize(100)),
        "the thread finds its future under the shifted id"
    );
    assert_eq!(restored.threads[1].cancel.parent, Some(1));

    // Move every restored object once, so that stale pointers would show.
    let state = std::mem::take(&mut restored.threads[0].state);
    target.import_thread_state(state).expect("imports");
    let mut roots = restored.threads[0].extra_roots.clone();
    collect_garbage(&mut target, &mut roots, CollectionLevel::Major);

    let at = |index: usize| future_of(roots[index].as_object_ptr().expect("an object"));
    assert!(matches!(at(0).read(), FutureRead::Pending(id) if id.as_usize() == 100));
    let FutureRead::Ready(value) = at(1).read() else {
        panic!("ready stays ready");
    };
    assert_eq!(render(&target, value), render(&vm, pair));
    assert_eq!(at(1).id().as_usize(), 101);
    let FutureRead::Error(error) = at(2).read() else {
        panic!("failed stays failed");
    };
    assert_eq!(render(&target, error), render(&vm, pair));
    assert!(!at(2).is_observed(), "the error is still not observed");
    assert_eq!(at(2).error_trace()[0].function_name, "user.vendor");
    assert_eq!(at(2).error_trace()[0].error_line, 12);
    assert!(at(3).is_observed());
    assert!(matches!(at(4).read(), FutureRead::Cancelled));
    assert!(at(4).cancel_requested());
    assert!(at(4).cancel.is_cancelled());
    // A settled future wakes an awaiter at once.
    assert!(at(1).ready_waiter().get().is_some());
    assert!(at(0).ready_waiter().get().is_none());

    let Object::Array(array) = (unsafe { roots[5].as_object_ptr().unwrap().get() }) else {
        panic!("the array");
    };
    let items = array.data.lock().clone();
    assert_eq!(
        items[0], items[1],
        "identity of a shared future is preserved"
    );
    assert_eq!(items[0], roots[1]);
}

#[test]
fn a_pending_future_without_its_thread_and_an_internal_error_block_with_a_path() {
    let harness = Harness::new(PROGRAM);
    let (mut vm, _op, _args) = harness.run_to_sysop("user.main", 0);

    let orphan = pending(&mut vm, 0);
    let thread = ThreadInput {
        name: "main".to_string(),
        extra_roots: vec![Value::object(orphan)],
        ..ThreadInput::new(1, &vm, ParkedAt::runnable())
    };
    let error = bex_snapshot::write_snapshot(&vm.heap, &[thread], options(&harness, 1))
        .expect_err("nothing would ever settle the restored future");
    let SnapshotError::Blocked { reason, path } = error else {
        panic!("expected Blocked");
    };
    assert!(reason.contains("no thread of the run settles"), "{reason}");
    assert_eq!(path[0], "thread 1 (main)");
    assert!(
        path.last().unwrap().contains("future #0 (pending)"),
        "{path:?}"
    );

    let broken = pending(&mut vm, 1);
    assert!(future_of(broken).settle_internal_error("engine bug".into()));
    let thread = ThreadInput {
        name: "main".to_string(),
        extra_roots: vec![Value::object(broken)],
        ..ThreadInput::new(1, &vm, ParkedAt::runnable())
    };
    let error = bex_snapshot::write_snapshot(&vm.heap, &[thread], options(&harness, 2))
        .expect_err("a Rust error value cannot move");
    let SnapshotError::Blocked { reason, path } = error else {
        panic!("expected Blocked");
    };
    assert!(reason.contains("internal engine error"), "{reason}");
    assert!(path.last().unwrap().contains("internal error"), "{path:?}");
}

#[tokio::test]
async fn cancel_tokens_and_task_groups_are_rebuilt_once_per_identity() {
    let harness = Harness::new(PROGRAM);
    let (mut vm, _op, _args) = harness.run_to_sysop("user.main", 0);

    let plain = CancelTokenData::new();
    let fired = CancelTokenData::new();
    fired.token().cancel();
    let either = CancelTokenData::composite(vec![Arc::clone(&plain), Arc::clone(&fired)]);
    let lonely = CancelTokenData::composite(vec![Arc::clone(&plain)]);
    let group = TaskGroupInner::new(2, Some("quotes".to_string()));
    let tickets: Vec<_> = (0..4)
        .map(|_| group.register(CancellationToken::new()))
        .collect();

    // Two heap handles of `plain`: they must restore to one token.
    let rust_data = |vm: &mut BexVm, data: bex_vm_types::snapshot_ctx::RustDataRef| {
        Value::object(vm.tlab.alloc(Object::RustData(data)))
    };
    let roots = vec![
        rust_data(&mut vm, plain.clone()),
        rust_data(&mut vm, plain.clone()),
        rust_data(&mut vm, either.clone()),
        rust_data(&mut vm, group.clone()),
    ];

    let root = ThreadInput {
        extra_roots: roots,
        ..ThreadInput::new(1, &vm, ParkedAt::runnable())
    };
    let member = |thread_id: u64, future: usize, ticket: usize, queued: bool| ThreadInput {
        parent_thread: Some(1),
        settles_future: Some(SettledFuture {
            id: FutureId::from_usize(future),
            object: None,
        }),
        cancel: ThreadCancel {
            cancelled: thread_id == 3,
            parent: Some(1),
        },
        // `lonely` is reachable from no heap value, only from this thread.
        user_cancels: vec![Arc::clone(&lonely)],
        group: Some(GroupMembership {
            group: Arc::clone(&group),
            member_id: tickets[ticket].member_id(),
        }),
        ..ThreadInput::new(
            thread_id,
            &vm,
            ParkedAt {
                kind: if queued { "queued" } else { "runnable" }.to_string(),
                payload: Vec::new(),
            },
        )
    };
    // Threads in another order than the group's members.
    let threads = [root, member(2, 0, 3, true), member(3, 1, 0, false)];
    let (bytes, _) = bex_snapshot::write_snapshot(&vm.heap, &threads, options(&harness, 2))
        .expect("tokens and groups are serializable");

    let target = harness.new_vm();
    let restored =
        bex_snapshot::restore_into(&target.heap, &bytes, harness.program_hash).expect("restores");
    let handle = |index: usize| {
        let ptr = restored.threads[0].extra_roots[index]
            .as_object_ptr()
            .unwrap();
        match unsafe { ptr.get() } {
            Object::RustData(data) => Arc::clone(data),
            other => panic!("expected rust data, got {other}"),
        }
    };
    let plain_a = handle(0).downcast::<CancelTokenData>().expect("a token");
    let plain_b = handle(1).downcast::<CancelTokenData>().expect("a token");
    assert!(Arc::ptr_eq(&plain_a, &plain_b), "one identity, one token");
    assert!(!plain_a.is_cancelled());
    let either = handle(2).downcast::<CancelTokenData>().expect("a token");
    assert_eq!(either.sources().len(), 2);
    assert!(Arc::ptr_eq(&either.sources()[0], &plain_a));
    assert!(
        either.sources()[1].is_cancelled(),
        "a fired token stays fired"
    );
    assert_eq!(restored.cancel_tokens.len(), 4);

    // The composite's links are the caller's to start. Its second source had
    // already fired, so the watcher fires the composite.
    for token in &restored.cancel_tokens {
        for watcher in token.watchers() {
            tokio::spawn(watcher);
        }
    }
    either.token().cancelled().await;
    assert!(!plain_a.is_cancelled(), "links are one-directional");

    let group = handle(3).downcast::<TaskGroupInner>().expect("a group");
    assert_eq!(group.limit(), 2);
    assert_eq!(group.name(), Some("quotes"));
    assert_eq!(group.active_count(), 0, "the caller registers the members");
    let queued = restored.threads[1].group.as_ref().expect("a seat");
    let running = restored.threads[2].group.as_ref().expect("a seat");
    assert!(Arc::ptr_eq(&queued.group, &group));
    assert!(!queued.active && running.active);
    assert_eq!((running.order, queued.order), (0, 3));
    assert!(restored.threads[2].cancel.cancelled);
    assert_eq!(restored.threads[1].user_cancels.len(), 1);
    assert!(Arc::ptr_eq(
        &restored.threads[1].user_cancels[0],
        &restored.threads[2].user_cancels[0]
    ));
    assert!(Arc::ptr_eq(
        &restored.threads[1].user_cancels[0].sources()[0],
        &plain_a
    ));
    drop(tickets);
}

#[test]
fn a_format_version_1_snapshot_is_refused_with_a_clear_error() {
    let harness = Harness::new(PROGRAM);
    let (vm, _op, args) = harness.run_to_sysop("user.main", 0);
    let (mut bytes, _) = harness
        .write(&vm, ParkedAt::runnable(), args, false, 0)
        .expect("snapshot");
    assert_eq!(bex_snapshot::FORMAT_VERSION, 2);
    assert_eq!(bex_snapshot::read_header(&bytes).unwrap().format_version, 2);

    // The version field follows the 8-byte magic.
    bytes[8..12].copy_from_slice(&1u32.to_le_bytes());
    let error = bex_snapshot::read_header(&bytes).unwrap_err();
    assert!(matches!(error, SnapshotError::Mismatch(_)), "{error:?}");
    let message = error.to_string();
    assert!(message.contains("format version 1"), "{message}");
    assert!(message.contains("reads version 2"), "{message}");
    assert!(message.contains("older runtime"), "{message}");
    // A real version 1 file has a valid checksum (SHA-256 of everything
    // before it, in the last 32 bytes).
    let body = bytes.len() - 32;
    let checksum = <sha2::Sha256 as sha2::Digest>::digest(&bytes[..body]);
    bytes[body..].copy_from_slice(&checksum);
    let target = harness.new_vm();
    let error = bex_snapshot::restore_into(&target.heap, &bytes, harness.program_hash)
        .err()
        .expect("version 1 is refused");
    assert!(error.to_string().contains("format version 1"), "{error}");
}
