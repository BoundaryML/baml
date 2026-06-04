//! Native-bridge sysop impl for the `baml.host` namespace.
//!
//! Implements `IoNamespaceHost::call_host_value`, which:
//! 1. Encodes the inbound `args` to a type-rich `BamlOutboundValue` list
//!    (engine→host direction) — the host needs typed args.
//! 2. Allocates a call id and installs a `CompletionHandle` in the in-flight
//!    host-call table.
//! 3. Fires the registered `HostDispatchFn` (bridge-installed) with the
//!    host-value key, call id, and encoded args bytes.
//! 4. Returns `SysOpResult::Async` — the result is resolved when the host
//!    calls `complete_host_call(call_id, ...)`. On completion the host's
//!    returned value is validated against the expected return type
//!    (`type_arg_0`), raising `VmBamlError::HostCallable` (wrapped as
//!    `VmRustFnError::BamlError(...)`) on mismatch.
//!
//! ## In-flight lifetime / no timeout
//!
//! There is **no per-call wall-clock timeout**. The in-flight entry installed
//! in step 2 is removed by exactly one of:
//!
//! * the host calling `complete_host_call(call_id, ...)` (normal completion);
//!   or
//! * cancellation — the engine drops the returned async future (the
//!   `tokio::select!` cancel arm in `bex_engine`), which drops the
//!   [`host_dispatch::InflightGuard`] moved into it; the guard evicts the
//!   dangling entry so nothing leaks.
//!
//! A host that never completes a call and is never cancelled leaves the call
//! pending forever — see [`crate::host_dispatch`] for the rationale.

use std::sync::Arc;

use baml_type::Ty;
use bex_external_types::validate_host_return;
use bex_heap::BexHeap;
use bridge_ctypes::{
    CffiHandleTableOptions, baml_core::cffi::HostCallableErrorCategory, external_to_outbound,
};
use prost::Message as _;
use sys_ops::io::{
    self, BexExternalValue, CallId, SysOpContext, SysOpOutput, VmBamlError, VmRustFnError,
};
use sys_types::{OpError, SysOp, SysOpResult};

use crate::{NativeSysOps, host_dispatch};

impl io::IoNamespaceHost for NativeSysOps {
    fn call_host_value(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        handle: BexExternalValue,
        args: Vec<BexExternalValue>,
        type_arg_0: Ty,
        // `type_arg_1` is the declared throws contract `E`; the runtime
        // contract check that consumes it lands in a later phase.
        _type_arg_1: Ty,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<BexExternalValue> {
        // Extract the HostValueArc from the incoming handle.
        let host_arc = match handle {
            BexExternalValue::HostValue(arc) => arc,
            other => {
                return SysOpOutput::err(VmBamlError::InvalidArgument {
                    message: format!("expected HostValue, got {other:?}"),
                });
            }
        };

        // Encode the args as a type-rich `BamlOutboundValue` list. The host
        // receives the args as a single protobuf-encoded `BamlOutboundValue`
        // whose variant is a `BamlValueList` — there is no dedicated arg-list
        // proto message, so the list value is the canonical container.
        let options = CffiHandleTableOptions::for_wire();
        let arg_values = BexExternalValue::Array {
            element_type: Ty::unknown(),
            items: args,
        };
        let encoded: Vec<u8> = match external_to_outbound(&arg_values, &options) {
            Ok(value) => value.encode_to_vec(),
            Err(e) => {
                return SysOpOutput::err(VmBamlError::HostCallable {
                    class_name: String::new(),
                    message: format!("failed to encode host-call arguments: {e}"),
                    traceback: None,
                    language: None,
                    category: HostCallableErrorCategory::HostCallableInvalidArgument as i32,
                });
            }
        };

        // Allocate a fresh call id and create a CompletionHandle.
        let call_id = host_dispatch::next_call_id();
        let (result, completion) = SysOpResult::pending(SysOp::BamlHostCallHostValue);

        // On a (2^32-wrap) id collision `insert` returns `false` after failing
        // this call's `completion` with an error — and the live entry under
        // `call_id` belongs to the *other* call. So when it returns `false` we
        // must NOT fire the dispatch (the host callback would run for an
        // already-failed call) and must NOT build an `InflightGuard` (its drop
        // would evict the other call's entry). `result` already carries the
        // collision error, so the `match result` below returns it.
        let guard = if host_dispatch::insert(call_id, completion) {
            // RAII guard that evicts this call's in-flight table entry on drop.
            // Moved into the async future below so that if the engine drops the
            // future on cancellation (the `tokio::select!` cancel arm), the
            // dangling entry is removed — no leak, and a late
            // `complete_host_call` for this id hits the benign unknown-id path.
            // On normal completion the entry is already gone (taken by
            // `complete_host_call`), so the guard's `take` is a no-op. The guard
            // carries only the `Copy` `call_id`. No wall-clock timeout: see the
            // module docs.
            let guard = host_dispatch::InflightGuard::new(call_id);

            // Fire the dispatch callback. If no bridge has registered, fail
            // synchronously so the engine does not hang waiting on the oneshot.
            if !host_dispatch::fire_dispatch(host_arc.key, call_id, &encoded) {
                if let Some(c) = host_dispatch::take(call_id) {
                    c.complete(Err(OpError::new(
                        SysOp::BamlHostCallHostValue,
                        VmBamlError::NotImplemented {
                            message: "no host bridge registered for host-value dispatch"
                                .to_string(),
                        },
                    )));
                }
            }
            Some(guard)
        } else {
            None
        };

        // The completion already yields a typed `BexExternalValue` (decoded
        // from the inbound payload by `complete_host_call`). Validate the
        // host's returned value against the expected return type `type_arg_0`
        // before handing it back to the VM; on mismatch surface a catchable
        // `root.errors.HostCallable`.
        //
        // `SysOpResult::pending` always yields `Async`, so the `Ready` arms are
        // not reached in practice; the guard is moved into the async future to
        // get cancel-drop eviction. (The `Ready` arms drop the guard inline,
        // which evicts the entry too — correct, just unused.)
        match result {
            SysOpResult::Ready(Ok(value)) => match validate_return_value(&value, &type_arg_0) {
                Ok(()) => SysOpOutput::ok(value),
                Err(err) => SysOpOutput::err(err),
            },
            SysOpResult::Ready(Err(err)) => SysOpOutput::err(err.kind),
            SysOpResult::Async(fut) => SysOpOutput::async_op(async move {
                // Move the guard into the future so it is dropped — and the
                // in-flight entry evicted — if this future is cancelled. It is
                // `None` on the collision path (no entry of ours to evict); the
                // awaited `fut` then resolves to the collision error.
                let _guard = guard;
                // `fut.await` yields `Result<_, OpError>` and
                // `validate_return_value` yields `Result<_, VmRustFnError>`;
                // both propagate via the `From<…> for VmRustFnError` impls.
                let value = fut.await?;
                validate_return_value(&value, &type_arg_0)?;
                Ok(value)
            }),
        }
    }
}

/// Validate a host-returned value against the wrapper's declared return
/// type, producing a `VmBamlError::HostCallable` on mismatch.
///
/// Delegates to the shared, strict [`validate_host_return`] guard (shared
/// with the WASM bridge) so the native and WASM bridges enforce an identical
/// shape contract: scalar discrimination (`int` ≠ `float`), container
/// recursion, enum identity, and class-name identity. Class *field types* are
/// validated engine-side at the result-push site, where the resolved class
/// schema is available.
fn validate_return_value(value: &BexExternalValue, expected: &Ty) -> Result<(), VmRustFnError> {
    validate_host_return(value, expected)
        .map_err(|err| VmBamlError::HostCallable {
            class_name: "TypeError".to_string(),
            message: err.to_string(),
            traceback: None,
            language: None,
            category: HostCallableErrorCategory::HostCallableInvalidArgument as i32,
        })
        .map_err(VmRustFnError::from)
}

#[cfg(test)]
mod tests {
    use baml_type::{Ty, TyAttr};
    use sys_ops::io::{BexExternalValue, CallId, IoNamespaceHost as _, SysOpContext, SysOpOutput};
    use sys_types::{OpError, SysOp, SysOpResult, VmBamlError, VmRustFnError};

    use super::*;
    use crate::host_dispatch;

    fn make_heap() -> Arc<BexHeap> {
        BexHeap::new(vec![])
    }

    fn int_ty() -> Ty {
        Ty::Int {
            attr: TyAttr::default(),
        }
    }

    // -------------------------------------------------------------------------
    // Type-error path: non-HostValue arg → immediate TypeError
    // -------------------------------------------------------------------------
    #[tokio::test]
    async fn wrong_type_returns_type_error() {
        let ops = NativeSysOps;
        let heap = make_heap();
        let ctx = SysOpContext::empty();
        // Pass a String instead of a HostValue.
        let output = ops.call_host_value(
            &heap,
            CallId::next(),
            BexExternalValue::String("not a host value".to_string().into()),
            vec![],
            int_ty(),
            Ty::unknown(),
            &ctx,
        );
        let result = match output {
            SysOpOutput::Ready(r) => r,
            SysOpOutput::Async(fut) => fut.await,
        };
        let err = result.expect_err("expected type error");
        // The wrong-handle-type arg surfaces as a `VmBamlError::InvalidArgument`
        // (which the host SDK sees as `baml.errors.InvalidArgument`), wrapped
        // in the canonical `VmRustFnError::BamlError`.
        assert!(
            matches!(
                err,
                VmRustFnError::BamlError(VmBamlError::InvalidArgument { .. })
            ),
            "expected InvalidArgument, got {err:?}"
        );
    }

    // -------------------------------------------------------------------------
    // Table round-trip: insert + complete_with_value
    // -------------------------------------------------------------------------
    #[tokio::test]
    async fn completion_table_success_round_trip() {
        let (result, completion) = SysOpResult::pending(SysOp::BamlHostCallHostValue);
        let call_id = host_dispatch::next_call_id();
        assert!(
            host_dispatch::insert(call_id, completion),
            "fresh id must insert cleanly"
        );

        // Simulate the host calling back with an Int result.
        host_dispatch::complete_with_value(call_id, BexExternalValue::Int(42));

        let value = match result {
            SysOpResult::Async(fut) => fut.await.expect("expected Ok"),
            SysOpResult::Ready(Ok(v)) => v,
            SysOpResult::Ready(Err(e)) => panic!("unexpected error: {e}"),
        };
        assert!(
            matches!(value, BexExternalValue::Int(42)),
            "unexpected value: {value:?}"
        );
    }

    // -------------------------------------------------------------------------
    // Table round-trip: insert + complete_with_error (HostCallable)
    // -------------------------------------------------------------------------
    #[tokio::test]
    async fn completion_table_host_callable_error_round_trip() {
        let (result, completion) = SysOpResult::pending(SysOp::BamlHostCallHostValue);
        let call_id = host_dispatch::next_call_id();
        assert!(
            host_dispatch::insert(call_id, completion),
            "fresh id must insert cleanly"
        );

        // Simulate the host calling back with a HostCallable error.
        host_dispatch::complete_with_error(
            call_id,
            OpError::new(
                SysOp::BamlHostCallHostValue,
                VmBamlError::HostCallable {
                    class_name: "RuntimeError".to_string(),
                    message: "host raised".to_string(),
                    traceback: Some("at line 1".to_string()),
                    language: Some("python".to_string()),
                    category: 2,
                },
            ),
        );

        let err = match result {
            SysOpResult::Async(fut) => fut.await.expect_err("expected error"),
            SysOpResult::Ready(Err(e)) => e,
            SysOpResult::Ready(Ok(_)) => panic!("expected error"),
        };
        match &err.kind {
            VmRustFnError::BamlError(VmBamlError::HostCallable {
                class_name,
                language,
                category,
                ..
            }) => {
                assert_eq!(class_name, "RuntimeError");
                assert_eq!(language.as_deref(), Some("python"));
                assert_eq!(*category, 2);
            }
            other => panic!("expected HostCallable, got {other:?}"),
        }
    }

    // -------------------------------------------------------------------------
    // Table round-trip: unknown call_id does not panic
    // -------------------------------------------------------------------------
    #[test]
    fn completion_table_unknown_id_does_not_panic() {
        host_dispatch::complete_with_value(u32::MAX - 2, BexExternalValue::Null);
        host_dispatch::complete_with_error(
            u32::MAX - 3,
            OpError::new(
                SysOp::BamlHostCallHostValue,
                VmBamlError::NotImplemented {
                    message: "unknown id".to_string(),
                },
            ),
        );
    }

    // -------------------------------------------------------------------------
    // Return-type validation: matching value passes
    // -------------------------------------------------------------------------
    #[test]
    fn validate_return_value_accepts_matching_type() {
        validate_return_value(&BexExternalValue::Int(7), &int_ty())
            .expect("an Int should match the `int` return type");
    }

    // -------------------------------------------------------------------------
    // Return-type validation: mismatched value → HostCallable error
    // -------------------------------------------------------------------------
    #[test]
    fn validate_return_value_rejects_mismatched_type() {
        let err = validate_return_value(
            &BexExternalValue::String("oops".to_string().into()),
            &int_ty(),
        )
        .expect_err("a String should not match the `int` return type");
        match err {
            VmRustFnError::BamlError(VmBamlError::HostCallable { category, .. }) => {
                assert_eq!(
                    category,
                    HostCallableErrorCategory::HostCallableInvalidArgument as i32
                );
            }
            other => panic!("expected BamlError(HostCallable), got {other:?}"),
        }
    }

    // -------------------------------------------------------------------------
    // Return-type validation: `unknown` accepts anything
    // -------------------------------------------------------------------------
    #[test]
    fn validate_return_value_unknown_accepts_anything() {
        validate_return_value(
            &BexExternalValue::String("anything".to_string().into()),
            &Ty::unknown(),
        )
        .expect("`unknown` return type should accept any value");
    }

    // -------------------------------------------------------------------------
    // Return-type validation: an `int` value does NOT satisfy a `float`
    // return (strict int≠float; the shared validator owns the recursive
    // structural cases — see `bex_external_types::host_return`).
    // -------------------------------------------------------------------------
    #[test]
    fn validate_return_value_int_does_not_satisfy_float() {
        validate_return_value(&BexExternalValue::Int(3), &Ty::float())
            .expect_err("an Int value must not satisfy a declared `float` return type");
        validate_return_value(&BexExternalValue::Float(3.0), &Ty::float())
            .expect("a Float value satisfies a declared `float` return type");
    }
}
