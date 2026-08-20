//! The owner/snapshot substrate's two load-bearing properties, exercised
//! directly against `GlobalState` with a real thread pool.

use std::{
    path::PathBuf,
    sync::{Arc, Barrier, atomic::AtomicUsize},
    time::Duration,
};

use baml_base::{Name, SourceRootKind};
use baml_lsp::{
    LspError, OwnerEvent,
    executor::{Executors, ThreadPool, spawn_read},
    mutation::{RootSpec, SourceMutation},
    snapshot::RequestCx,
    state::GlobalState,
};

fn workspace(state: &mut GlobalState, root: &str, files: &[(&str, &str)]) {
    let applied = state.apply(vec![SourceMutation::UpsertRoot {
        spec: RootSpec {
            path: PathBuf::from(root),
            package: Name::new(baml_type::RESERVED_USER_PACKAGE),
            kind: SourceRootKind::Workspace,
        },
        files: files
            .iter()
            .map(|(name, text)| (PathBuf::from(root).join(name), (*text).to_owned()))
            .collect(),
    }]);
    assert!(applied.rejected.is_empty(), "{:?}", applied.rejected);
}

/// A read that reaches Salsa after the owner has started a mutation is
/// unwound with `PendingWrite` and reported as `ContentModified`; the
/// next read at the new revision succeeds.
#[test]
fn mid_read_mutation_yields_content_modified() {
    let mut state = GlobalState::new(Executors::single(Arc::new(ThreadPool::new(2))), None);
    workspace(&mut state, "/ws", &[("main.baml", "class A { x int }\n")]);

    // Job entered its query → barrier 1; owner starts `set_*` (blocks until
    // the job's snapshot drops); helper releases barrier 2 → the job's next
    // query entry unwinds.
    let entered = Arc::new(Barrier::new(2));
    let resume = Arc::new(Barrier::new(2));
    let handle = state.handle();
    let (tx, rx) = std::sync::mpsc::channel();
    {
        let entered = Arc::clone(&entered);
        let resume = Arc::clone(&resume);
        let snap = state.snapshot(RequestCx::default());
        spawn_read(
            state.request_executor(),
            snap,
            move |snap| {
                let files = baml_compiler2_hir::compiler2_all_files(snap.db());
                entered.wait();
                resume.wait();
                // Any query entry after the owner's set_* has begun unwinds.
                for file in files {
                    let _ = baml_compiler2_hir::file_package::file_package(snap.db(), file);
                    let _ = baml_db::check::check_file(snap.db(), file);
                }
                Ok(serde_json::Value::Null)
            },
            move |outcome| {
                handle.post(OwnerEvent::RequestDone {
                    session: baml_lsp::SessionKey(1),
                    request_id: lsp_server::RequestId::from(1),
                    respond: Box::new(move |r| tx.send(r).unwrap()),
                    outcome,
                });
            },
        );
    }
    entered.wait();
    // Release the job just after the owner enters `apply` (set_* blocks the
    // owner until the job unwinds and drops its snapshot).
    let releaser = std::thread::spawn({
        let resume = Arc::clone(&resume);
        move || {
            std::thread::sleep(Duration::from_millis(50));
            resume.wait();
        }
    });
    let applied = state.apply(vec![SourceMutation::SetOverlay {
        path: PathBuf::from("/ws/main.baml"),
        text: "class A { x int y int }\n".to_owned(),
        version: Some(2),
    }]);
    releaser.join().unwrap();
    assert!(applied.rejected.is_empty());

    let event = state
        .events()
        .recv_timeout(Duration::from_secs(10))
        .expect("job reports back");
    let OwnerEvent::RequestDone {
        respond, outcome, ..
    } = event
    else {
        panic!("unexpected event");
    };
    let result: Result<serde_json::Value, LspError> = outcome
        .map_err(LspError::from)
        .and_then(std::convert::identity);
    respond(result);
    match rx.recv().unwrap() {
        Err(LspError::ContentModified(_)) => {}
        other => panic!("expected ContentModified, got {other:?}"),
    }

    // The state is intact: a fresh read at the new revision succeeds.
    let (tx2, rx2) = std::sync::mpsc::channel::<Result<usize, LspError>>();
    let snap = state.snapshot(RequestCx::default());
    spawn_read(
        state.request_executor(),
        snap,
        |snap| Ok(baml_compiler2_hir::compiler2_all_files(snap.db()).len()),
        move |outcome| {
            tx2.send(
                outcome
                    .map_err(LspError::from)
                    .and_then(std::convert::identity),
            )
            .unwrap();
        },
    );
    let n = rx2.recv_timeout(Duration::from_secs(10)).unwrap().unwrap();
    assert!(n > 1, "stdlib + workspace file present");
}

/// A panicking read job is reported as `Internal`; the pool thread and the
/// owner state survive and serve the next request.
#[test]
fn injected_panic_is_internal_error_and_state_survives() {
    let mut state = GlobalState::new(Executors::single(Arc::new(ThreadPool::new(1))), None);
    workspace(&mut state, "/ws", &[("main.baml", "class A { x int }\n")]);
    let (tx, rx) = std::sync::mpsc::channel::<Result<serde_json::Value, LspError>>();
    let counter = Arc::new(AtomicUsize::new(0));

    let snap = state.snapshot(RequestCx::default());
    spawn_read(
        state.request_executor(),
        snap,
        |_snap| -> Result<serde_json::Value, LspError> { panic!("injected") },
        {
            let tx = tx.clone();
            move |outcome| {
                tx.send(
                    outcome
                        .map_err(LspError::from)
                        .and_then(std::convert::identity),
                )
                .unwrap();
            }
        },
    );
    match rx.recv_timeout(Duration::from_secs(10)).unwrap() {
        Err(LspError::Internal(message)) => assert!(message.contains("injected"), "{message}"),
        other => panic!("expected Internal, got {other:?}"),
    }

    // Same (single) pool thread, next job runs fine and the DB is usable.
    let snap = state.snapshot(RequestCx::default());
    let counter2 = Arc::clone(&counter);
    spawn_read(
        state.request_executor(),
        snap,
        move |snap| {
            counter2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(serde_json::json!(
                baml_compiler2_hir::compiler2_all_files(snap.db()).len()
            ))
        },
        move |outcome| {
            tx.send(
                outcome
                    .map_err(LspError::from)
                    .and_then(std::convert::identity),
            )
            .unwrap();
        },
    );
    let value = rx.recv_timeout(Duration::from_secs(10)).unwrap().unwrap();
    assert!(value.as_u64().unwrap() > 1);
    assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 1);
    // And mutations still apply after the panic.
    let applied = state.apply(vec![SourceMutation::SetOverlay {
        path: PathBuf::from("/ws/main.baml"),
        text: "class B { y int }\n".to_owned(),
        version: Some(3),
    }]);
    assert!(applied.rejected.is_empty());
}
