//! `$id` / identity contract tests for the VM-minted call-id system.
//!
//! Salvaged from `tracing.rs` when PR #3616 removed the legacy span/event
//! layer (`HostSpanContext`, `RuntimeEvent`, the event store, and the
//! `EventSink` constructor parameter). The span-stream assertions died with
//! that layer; what survives here are the tests that pin the *identity*
//! contract of the surviving system:
//!
//! - root `$id` reads decode to the host-created `BoundaryId`, while child
//!   calls without an override decode to the current call's default `CallRef`
//!   (`process_euid` / `engine_id` / `thread_id` / `call_id`),
//! - `baml.id.new()` / `baml.id.set()` / `$id = ...` override semantics
//!   (per-call, stack-scoped, not inherited by callees),
//! - identity correctness across `spawn`, nested calls, and caught
//!   exceptions,
//! - `ProgramMetadata` function-table derivation (owner types),
//! - sink-independence and multi-engine scoping of minted ids.
//!
//! Ring-level lifecycle/linkage coverage lives in the segmented-backend
//! integration tests.

mod common;

use std::sync::Arc;

use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use bex_events::{
    ids::{BoundaryId, RuntimeId},
    prof::backend::{ProfilerConfig, ProfilerSession},
};
use common::compile_for_engine;
use sys_native::SysOpsExt;

#[tokio::test]
async fn baml_id_inside_spawn_uses_child_thread_root_call() {
    let source = r#"
        function main() -> string {
            let f = spawn { $id };
            await f
        }
    "#;

    let snapshot = compile_for_engine(source);
    let (profiler_session, diagnostic) = ProfilerSession::from_config(ProfilerConfig {
        enabled: false,
        ..ProfilerConfig::default()
    });
    assert!(diagnostic.is_none());
    let engine = Arc::new(
        BexEngine::new_with_profiler_session(
            snapshot,
            Arc::new(sys_native::SysOps::native()),
            Vec::new(),
            profiler_session,
        )
        .unwrap(),
    );

    let boundary_id = BoundaryId::from_bytes([1; 16]);
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_boundary_id(boundary_id)
        .build();
    let value = engine
        .call_function("main", vec![], call_ctx, true)
        .await
        .unwrap();

    let BexExternalValue::String(id) = value else {
        panic!("expected spawned $id string");
    };
    let RuntimeId::DefaultCall(call_ref) = RuntimeId::decode(id.as_str()).unwrap() else {
        panic!("expected default call runtime ID");
    };
    assert_eq!(call_ref.process_euid, engine.process_euid());
    assert_eq!(call_ref.engine_id, engine.engine_id());
    assert_eq!(call_ref.thread_id, bex_engine::BexThreadId(2));
    assert_eq!(call_ref.call_id, bex_engine::BexCallId(1));
}

#[tokio::test]
async fn call_function_with_trace_surfaces_entry_call_ref() {
    let source = r#"
        function main() -> string {
            $id
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new()).unwrap(),
    );

    let boundary_id = BoundaryId::from_bytes([1; 16]);
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_boundary_id(boundary_id)
        .build();
    let result = engine
        .call_function_with_trace("main", vec![], call_ctx, true)
        .await
        .unwrap();

    assert_eq!(result.entry_call_ref.process_euid, engine.process_euid());
    assert_eq!(result.entry_call_ref.engine_id, engine.engine_id());
    assert_eq!(result.entry_call_ref.thread_id, bex_engine::BexThreadId(1));
    assert_eq!(result.entry_call_ref.call_id, bex_engine::BexCallId(1));

    let BexExternalValue::String(id) = result.value.unwrap() else {
        panic!("expected $id string result");
    };
    let RuntimeId::Boundary(actual_boundary_id) = RuntimeId::decode(id.as_str()).unwrap() else {
        panic!("expected root boundary runtime ID");
    };
    assert_eq!(actual_boundary_id, boundary_id);
}

#[tokio::test]
async fn boundary_id_current_matches_root_id() {
    let source = r#"
        function main() -> string {
            boundary.id.current() + "|" + $id
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new()).unwrap(),
    );

    let boundary_id = BoundaryId::from_bytes([3; 16]);
    let value = engine
        .call_function(
            "main",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next())
                .with_boundary_id(boundary_id)
                .build(),
            true,
        )
        .await
        .unwrap();

    let BexExternalValue::String(result) = value else {
        panic!("expected boundary id string result");
    };
    let parts: Vec<&str> = result.as_str().split('|').collect();
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0], parts[1]);
    assert_eq!(parts[0], boundary_id.to_wire_string());
    let RuntimeId::Boundary(actual_boundary_id) = RuntimeId::decode(parts[0]).unwrap() else {
        panic!("expected root boundary runtime ID");
    };
    assert_eq!(actual_boundary_id, boundary_id);
}

#[tokio::test]
async fn explicit_local_id_is_visible_only_in_callee_and_restores_caller_id() {
    let source = r#"
        function explicit_identity_leaf() -> string {
            boundary.id.current()
        }

        function main() -> string {
            let before = boundary.id.current()
            let inside = explicit_identity_leaf($id = boundary.id())
            let after = boundary.id.current()
            before + "|" + inside + "|" + after
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new()).unwrap(),
    );
    let boundary_id = BoundaryId::from_bytes([25; 16]);
    let value = engine
        .call_function(
            "main",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next())
                .with_boundary_id(boundary_id)
                .build(),
            true,
        )
        .await
        .unwrap();
    let BexExternalValue::String(result) = value else {
        panic!("expected identity string")
    };
    let parts = result.split('|').collect::<Vec<_>>();
    assert_eq!(parts.len(), 3);
    assert_eq!(parts[0], boundary_id.to_wire_string());
    assert_ne!(
        parts[1], parts[0],
        "callee must observe the explicit LocalId"
    );
    assert_eq!(
        parts[2], parts[0],
        "caller identity must be restored after return"
    );
    assert!(matches!(
        RuntimeId::decode(parts[1]),
        Ok(RuntimeId::Boundary(_))
    ));
}

#[tokio::test]
async fn call_callable_with_trace_surfaces_callable_entry_call_ref() {
    let source = r#"
        function get_callable() -> () -> string throws never {
            callable
        }

        function callable() -> string {
            $id
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new()).unwrap(),
    );

    let handle = match engine
        .call_function(
            "get_callable",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            false,
        )
        .await
        .unwrap()
    {
        BexExternalValue::Handle(handle) => handle,
        other => panic!("expected callable handle, got {other:?}"),
    };

    let boundary_id = BoundaryId::from_bytes([2; 16]);
    let result = engine
        .call_callable_with_trace(
            handle,
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next())
                .with_boundary_id(boundary_id)
                .build(),
            true,
        )
        .await
        .unwrap();

    assert_eq!(result.entry_call_ref.process_euid, engine.process_euid());
    assert_eq!(result.entry_call_ref.engine_id, engine.engine_id());
    assert_eq!(result.entry_call_ref.thread_id, bex_engine::BexThreadId(2));
    assert_eq!(result.entry_call_ref.call_id, bex_engine::BexCallId(1));

    let BexExternalValue::String(id) = result.value.unwrap() else {
        panic!("expected callable $id string result");
    };
    let RuntimeId::Boundary(actual_boundary_id) = RuntimeId::decode(id.as_str()).unwrap() else {
        panic!("expected callable root boundary runtime ID");
    };
    assert_eq!(actual_boundary_id, boundary_id);
}

#[tokio::test]
async fn call_function_with_trace_keeps_entry_call_ref_for_runtime_error() {
    let source = r#"
        function main() -> int {
            throw "boom"
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new()).unwrap(),
    );

    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next()).build();
    let result = engine
        .call_function_with_trace("main", vec![], call_ctx, true)
        .await
        .unwrap();

    assert_eq!(result.entry_call_ref.process_euid, engine.process_euid());
    assert_eq!(result.entry_call_ref.engine_id, engine.engine_id());
    assert_eq!(result.entry_call_ref.thread_id, bex_engine::BexThreadId(1));
    assert_eq!(result.entry_call_ref.call_id, bex_engine::BexCallId(1));
    assert!(
        result.value.is_err(),
        "runtime failure should be the traced outcome, not a pre-entry error"
    );
}

#[tokio::test]
async fn baml_id_inside_nested_expression_call_uses_nested_call_id() {
    let source = r#"
        function inner() -> string {
            $id
        }

        function main() -> string {
            inner()
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new()).unwrap(),
    );

    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next()).build();
    let value = engine
        .call_function("main", vec![], call_ctx, true)
        .await
        .unwrap();

    let BexExternalValue::String(id) = value else {
        panic!("expected nested $id string");
    };
    let RuntimeId::DefaultCall(call_ref) = RuntimeId::decode(id.as_str()).unwrap() else {
        panic!("expected default call runtime ID");
    };
    assert_eq!(call_ref.process_euid, engine.process_euid());
    assert_eq!(call_ref.engine_id, engine.engine_id());
    assert_eq!(call_ref.thread_id, bex_engine::BexThreadId(1));
    assert_eq!(call_ref.call_id, bex_engine::BexCallId(2));
}

#[test]
fn function_metadata_derives_owner_type_for_class_methods() {
    let source = r#"
        class Holder {
            value int

            function unwrap(self) -> int {
                self.value
            }
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine =
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new()).unwrap();

    let method = engine
        .program_metadata()
        .function_table
        .functions
        .iter()
        .find(|metadata| metadata.fqn == "user.Holder.unwrap")
        .expect("expected method metadata");
    assert_eq!(
        method.owner_type,
        Some(bex_events::DefinitionKey("class:user.Holder".to_string()))
    );
}

#[test]
fn program_identity_is_uuid_v7_with_or_without_a_source_hash() {
    fn assert_uuid_v7(program_id: bex_events::ids::ProgramId) {
        assert_eq!(program_id.0[6] >> 4, 7);
        assert_eq!(program_id.0[8] >> 6, 2);
    }

    let with_hash = compile_for_engine("function main() -> null { null }");
    let source_hash = with_hash
        .source_content_hash
        .expect("the compiler should stamp a source-content hash");
    let with_hash_engine = BexEngine::new(
        with_hash,
        Arc::new(sys_native::SysOps::native()),
        Vec::new(),
    )
    .unwrap();
    assert_uuid_v7(with_hash_engine.program_metadata().program_id);
    assert_eq!(
        with_hash_engine.program_metadata().source_snapshot_id,
        Some(bex_events::ids::SourceSnapshotId(source_hash))
    );

    let mut without_hash = compile_for_engine("function main() -> null { null }");
    without_hash.source_content_hash = None;
    let without_hash_engine = BexEngine::new(
        without_hash,
        Arc::new(sys_native::SysOps::native()),
        Vec::new(),
    )
    .unwrap();
    assert_uuid_v7(without_hash_engine.program_metadata().program_id);
    assert_ne!(
        with_hash_engine.program_metadata().program_id,
        without_hash_engine.program_metadata().program_id
    );
    assert_eq!(
        without_hash_engine.program_metadata().source_snapshot_id,
        None
    );
}

#[tokio::test]
async fn baml_id_current_new_and_set_roundtrip() {
    let source = r#"
        function main() -> string {
            let before = $id;
            let next = baml.id.new();
            let set_result = baml.id.set(next);
            let after = $id;
            before + "|" + next + "|" + set_result + "|" + after
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new()).unwrap(),
    );

    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next()).build();
    let value = engine
        .call_function("main", vec![], call_ctx, true)
        .await
        .unwrap();

    let BexExternalValue::String(result) = value else {
        panic!("expected string result");
    };
    let parts: Vec<&str> = result.as_str().split('|').collect();
    assert_eq!(parts.len(), 4);

    let RuntimeId::Boundary(root_boundary_id) = RuntimeId::decode(parts[0]).unwrap() else {
        panic!("expected root boundary runtime ID");
    };

    assert_eq!(parts[1], parts[2]);
    assert_eq!(parts[2], parts[3]);
    let RuntimeId::Boundary(override_id) = RuntimeId::decode(parts[3]).unwrap() else {
        panic!("expected override runtime ID");
    };
    assert_ne!(root_boundary_id, override_id);

    // Structural runtime-ID annotation coverage lives in profiling_backend.rs.
}

#[tokio::test]
async fn baml_id_assignment_overrides_current_id() {
    let source = r#"
        function main() -> string {
            let before = $id;
            let next = baml.id.new();
            $id = next;
            let after = $id;
            before + "|" + next + "|" + after
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new()).unwrap(),
    );

    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next()).build();
    let value = engine
        .call_function("main", vec![], call_ctx, true)
        .await
        .unwrap();

    let BexExternalValue::String(result) = value else {
        panic!("expected string result");
    };
    let parts: Vec<&str> = result.as_str().split('|').collect();
    assert_eq!(parts.len(), 3);

    let RuntimeId::Boundary(root_boundary_id) = RuntimeId::decode(parts[0]).unwrap() else {
        panic!("expected root boundary runtime ID");
    };

    assert_eq!(parts[1], parts[2]);
    let RuntimeId::Boundary(override_id) = RuntimeId::decode(parts[2]).unwrap() else {
        panic!("expected override runtime ID");
    };
    assert_ne!(root_boundary_id, override_id);

    // Structural runtime-ID annotation coverage lives in profiling_backend.rs.
}

// ── §2.2 contract: `$id` override persistence (T4-T7) ──────────────────────

/// T4: an override set via `$id = ...` must survive a nested bytecode call.
/// The override is VM-owned, keyed by the overridden call's `call_id`, so
/// re-entering `main` after `helper()` returns must restore it.
#[tokio::test]
async fn id_override_survives_nested_call() {
    let source = r#"
        function helper() -> int {
            1
        }

        function main() -> string {
            let next = baml.id.new();
            $id = next;
            let mid = $id;
            let x = helper();
            let after = $id;
            next + "|" + mid + "|" + after
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new()).unwrap(),
    );

    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next()).build();
    let value = engine
        .call_function("main", vec![], call_ctx, true)
        .await
        .unwrap();

    let BexExternalValue::String(result) = value else {
        panic!("expected string result");
    };
    let parts: Vec<&str> = result.as_str().split('|').collect();
    assert_eq!(parts.len(), 3);

    assert_eq!(parts[0], parts[1], "mid should equal the override (next)");
    assert_eq!(
        parts[1], parts[2],
        "after a nested call, $id should still be the override"
    );
}

/// T5: same as T4 but inside a `spawn` body (each child thread has its own
/// span state; the override must persist there too).
#[tokio::test]
async fn id_override_survives_nested_call_in_spawn() {
    let source = r#"
        function helper() -> int {
            1
        }

        function main() -> string {
            let f = spawn {
                let next = baml.id.new();
                $id = next;
                let mid = $id;
                let x = helper();
                let after = $id;
                next + "|" + mid + "|" + after
            };
            await f
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new()).unwrap(),
    );

    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next()).build();
    let value = engine
        .call_function("main", vec![], call_ctx, true)
        .await
        .unwrap();

    let BexExternalValue::String(result) = value else {
        panic!("expected string result");
    };
    let parts: Vec<&str> = result.as_str().split('|').collect();
    assert_eq!(parts.len(), 3);

    assert_eq!(parts[0], parts[1], "mid should equal the override (next)");
    assert_eq!(
        parts[1], parts[2],
        "after a nested call in spawn, $id should still be the override"
    );
}

/// T6: the inverse scoping rule — an override on the *caller* must NOT leak
/// into the callee. The callee's `$id` is its own default `CallRef`.
#[tokio::test]
async fn id_override_not_inherited_by_nested_call() {
    let source = r#"
        function helper() -> string {
            $id
        }

        function main() -> string {
            let next = baml.id.new();
            $id = next;
            let inner = helper();
            next + "|" + inner
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new()).unwrap(),
    );

    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next()).build();
    let value = engine
        .call_function("main", vec![], call_ctx, true)
        .await
        .unwrap();

    let BexExternalValue::String(result) = value else {
        panic!("expected string result");
    };
    let parts: Vec<&str> = result.as_str().split('|').collect();
    assert_eq!(parts.len(), 2);
    assert_ne!(
        parts[0], parts[1],
        "helper's $id must not inherit the caller's override"
    );

    let RuntimeId::DefaultCall(call_ref) = RuntimeId::decode(parts[1]).unwrap() else {
        panic!("helper's $id should be a default CallRef, got {}", parts[1]);
    };
    assert_eq!(call_ref.thread_id, bex_engine::BexThreadId(1));
    // Call ids count sys-op calls too (the ring records them as call pairs,
    // minted unconditionally in `prof_enter_sysop`): main = 1,
    // baml.id.new() = 2, the `$id =` set-op = 3, helper = 4.
    assert_eq!(call_ref.call_id, bex_engine::BexCallId(4));
}

/// §2.2 (T4-T7 companion): overrides nest. A callee that sets its own
/// override must shadow — never destroy — the caller's: after the callee
/// returns, the caller's `$id` is its own override again (the VM keeps a
/// per-call override stack, popped with the callee's frame).
#[tokio::test]
async fn id_override_survives_callee_override() {
    let source = r#"
        function helper() -> string {
            let mine = baml.id.new();
            $id = mine;
            $id
        }

        function main() -> string {
            let outer = baml.id.new();
            $id = outer;
            let inner = helper();
            let after = $id;
            outer + "|" + inner + "|" + after
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new()).unwrap(),
    );

    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next()).build();
    let value = engine
        .call_function("main", vec![], call_ctx, true)
        .await
        .unwrap();

    let BexExternalValue::String(result) = value else {
        panic!("expected string result");
    };
    let parts: Vec<&str> = result.as_str().split('|').collect();
    assert_eq!(parts.len(), 3);

    let RuntimeId::Boundary(_) = RuntimeId::decode(parts[0]).unwrap() else {
        panic!("outer should be an override ID");
    };
    let RuntimeId::Boundary(_) = RuntimeId::decode(parts[1]).unwrap() else {
        panic!("helper's $id should be the helper's own override");
    };
    assert_ne!(
        parts[0], parts[1],
        "helper's override is its own, not the caller's"
    );
    assert_eq!(
        parts[0], parts[2],
        "the caller's override must survive a callee that also overrides"
    );
}

/// T7 (adapted): the override is still in force when the overridden call
/// returns — `$id` read at the end of `main`, after a nested call, decodes
/// as the override UUID rather than the default `CallRef`. The per-record
/// emission-count/ordering semantics live in the segmented-backend tests.
#[tokio::test]
async fn id_override_read_at_return_is_override_uuid() {
    let source = r#"
        function helper() -> int {
            1
        }

        function main() -> string {
            let next = baml.id.new();
            $id = next;
            let x = helper();
            $id
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new()).unwrap(),
    );

    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next()).build();
    let value = engine
        .call_function("main", vec![], call_ctx, true)
        .await
        .unwrap();

    let BexExternalValue::String(id) = value else {
        panic!("expected string result");
    };
    let RuntimeId::Boundary(_override_id) = RuntimeId::decode(id.as_str()).unwrap() else {
        panic!("expected override runtime ID, got {id}");
    };

    // Structural runtime-ID annotation coverage lives in profiling_backend.rs.
}

// ── §2.1 contract: identity across caught exceptions (T2) ──────────────────

/// T2: `$id` read after a caught exception reflects the *current* call (the
/// root), not a stale unwound span.
#[tokio::test]
async fn id_is_correct_after_caught_exception() {
    let source = r#"
        function boom() -> int {
            throw "boom"
        }

        function safe() -> int {
            boom() catch (e) {
                _ => 0
            }
        }

        function main() -> string {
            let a = safe();
            $id
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new()).unwrap(),
    );

    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next()).build();
    let value = engine
        .call_function("main", vec![], call_ctx, true)
        .await
        .unwrap();

    let BexExternalValue::String(id) = value else {
        panic!("expected string result");
    };
    let RuntimeId::Boundary(_) = RuntimeId::decode(id.as_str()).unwrap() else {
        panic!("expected root BoundaryId after catch, got {id}");
    };
}

// ── §3.3 contract: baml.id.set throws clause (T25) ─────────────────────────

/// T25a/b: `baml.id.set` rejects non-override inputs with a *catchable*
/// `InvalidArgument` — including a default `CallRef` (so a call cannot adopt
/// another call's identity) and garbage strings.
#[tokio::test]
async fn baml_id_set_invalid_inputs_throw_catchable_invalid_argument() {
    let source = r#"
        function child_current() -> string {
            baml.id.set(baml.id.current()) catch (e) {
                baml.errors.InvalidArgument => "caught"
            }
        }

        function set_child_current() -> string {
            child_current()
        }

        function set_garbage() -> string {
            baml.id.set("garbage") catch (e) {
                baml.errors.InvalidArgument => "caught"
            }
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new()).unwrap(),
    );

    for entry in ["set_child_current", "set_garbage"] {
        let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next()).build();
        let value = engine
            .call_function(entry, vec![], call_ctx, true)
            .await
            .unwrap_or_else(|e| panic!("{entry}: InvalidArgument must be catchable: {e}"));
        assert_eq!(
            value,
            BexExternalValue::String("caught".into()),
            "{entry}: catch arm must run"
        );
    }
}

// ── Remaining contract net: T28, two-engine scoping ────────────────────────

/// T28: `call_id` minting is sink-independent — `$id` returns exact
/// `(thread_id, call_id)` identities with **no** event sink configured.
/// This is the regression guard for any future "gate the per-call yield"
/// optimization: `$id` is a language feature and must survive tracing-off.
/// (Post-#3616 the engine takes no sink parameter at all, so every engine
/// is "sinkless"; the test still pins that minting needs no tracing wired.)
#[tokio::test]
async fn sinkless_engine_still_mints_correct_ids() {
    let source = r#"
        function deep() -> string {
            $id
        }

        function mid() -> string {
            deep() + "|" + $id
        }

        function main() -> string {
            let spawned = spawn { $id };
            mid() + "|" + $id + "|" + (await spawned)
        }
    "#;

    let snapshot = compile_for_engine(source);
    let (profiler_session, diagnostic) = ProfilerSession::from_config(ProfilerConfig {
        enabled: false,
        ..ProfilerConfig::default()
    });
    assert!(diagnostic.is_none());
    let engine = Arc::new(
        BexEngine::new_with_profiler_session(
            snapshot,
            Arc::new(sys_native::SysOps::native()),
            Vec::new(),
            profiler_session,
        )
        .unwrap(),
    );

    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next()).build();
    let value = engine
        .call_function("main", vec![], call_ctx, true)
        .await
        .unwrap();
    let BexExternalValue::String(result) = value else {
        panic!("expected string result");
    };
    let parts: Vec<&str> = result.as_str().split('|').collect();
    assert_eq!(parts.len(), 4);
    let ids: Vec<(u64, u64)> = [parts[0], parts[1], parts[3]]
        .iter()
        .map(|part| {
            let RuntimeId::DefaultCall(call_ref) = RuntimeId::decode(part).unwrap() else {
                panic!("expected default CallRef, got {part}");
            };
            (call_ref.thread_id.0, call_ref.call_id.0)
        })
        .collect();
    let RuntimeId::Boundary(_) = RuntimeId::decode(parts[2]).unwrap() else {
        panic!("main root should expose BoundaryId, got {}", parts[2]);
    };

    // deep (called by mid), mid, main root boundary, spawned child root.
    // Call ids on the main thread: main=1, spawn-closure-thread is separate,
    // mid=2, deep=3 (spawn dispatch happens before mid()).
    assert_eq!(ids[0].0, 1, "deep runs on the main thread: {ids:?}");
    assert_eq!(ids[1].0, 1, "mid runs on the main thread: {ids:?}");
    assert!(ids[0].1 > ids[1].1, "deep is called by mid: {ids:?}");
    assert_eq!(ids[2], (2, 1), "spawned body is thread 2 call 1: {ids:?}");
}

/// TICKET §11.2's two-engine row: two engines in one process get distinct
/// engine ids, and their thread-1/call-1 `CallRefs` encode to distinct strings
/// — the actual collision-avoidance mechanism behind header-only scoping.
#[tokio::test]
async fn two_engines_mint_distinct_call_refs() {
    let source = r#"
        function inner() -> string {
            $id
        }

        function main() -> string {
            inner()
        }
    "#;

    let mut encoded = Vec::new();
    let mut engine_ids = Vec::new();
    for _ in 0..2 {
        let snapshot = compile_for_engine(source);
        let engine = Arc::new(
            BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new()).unwrap(),
        );
        engine_ids.push(engine.engine_id());
        let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next()).build();
        let BexExternalValue::String(id) = engine
            .call_function("main", vec![], call_ctx, true)
            .await
            .unwrap()
        else {
            panic!("expected string");
        };
        encoded.push(id.to_string());
    }

    assert_ne!(engine_ids[0], engine_ids[1]);
    assert_ne!(
        encoded[0], encoded[1],
        "the same nested call in two engines must encode to distinct CallRefs"
    );
    for (id, engine_id) in encoded.iter().zip(&engine_ids) {
        let RuntimeId::DefaultCall(call_ref) = RuntimeId::decode(id).unwrap() else {
            panic!("expected default CallRef");
        };
        assert_eq!(call_ref.engine_id, *engine_id);
        assert_eq!(call_ref.thread_id, bex_engine::BexThreadId(1));
        assert_eq!(call_ref.call_id, bex_engine::BexCallId(2));
    }
}
