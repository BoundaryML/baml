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
//!    calls `complete_host_call(call_id, ...)`. On completion:
//!    * a successful return value is validated against the declared return
//!      type `T` (`type_arg_0`); mismatches surface as
//!      `baml.panics.HostContractViolation`.
//!    * a host throw is checked against the declared throws contract `E`
//!      (`type_arg_1`); on-contract throws propagate as catchable
//!      `baml.errors.HostCallable`, off-contract throws surface as
//!      `baml.panics.HostContractViolation`.
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

use baml_type::RuntimeTy;
use bex_external_types::validate_host_return;
use bex_heap::BexHeap;
use bridge_ctypes::CffiHandleTableOptions;
use prost::Message as _;
use sys_ops::io::{
    self, BexExternalValue, CallId, SysOpContext, SysOpOutput, VmBamlError, VmRustFnError,
};
use sys_types::{OpError, SysOp, SysOpResult, VmPanic};

use crate::{NativeSysOps, host_dispatch};

impl io::IoNamespaceHost for NativeSysOps {
    fn call_host_value(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        handle: BexExternalValue,
        args: Vec<BexExternalValue>,
        type_arg_0: RuntimeTy,
        // `type_arg_1` is the declared throws contract `E`. The contract
        // check itself lives engine-side (in `BexEngine::materialize_host_throw`)
        // because it needs heap access to convert the thrown
        // `BexExternalValue` into a `Value` and inject a
        // `HostContractViolation` panic on mismatch. This impl just
        // forwards the throw via `OpError.host_thrown` (set by
        // `host_dispatch::complete_with_throw` on the bridge side).
        _type_arg_1: RuntimeTy,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<BexExternalValue> {
        // Extract the HostValueArc from the incoming handle and confirm
        // it's a callable. Only `HostValueKind::Callable` is dispatchable —
        // the bridge's dispatcher invokes a host *function* through this
        // path. An `HostValueKind::Opaque` arc represents an opaque
        // host value (e.g. a host-thrown exception) and has no callable
        // identity; dispatching
        // it would either find no entry in the bridge's callable registry
        // (returning a confusing "no callable for key" error) or, worse,
        // collide with a callable that happens to share its key. Reject
        // up front with a clear `InvalidArgument`.
        let host_arc = match handle {
            BexExternalValue::HostValue(arc)
                if arc.kind == bex_external_types::HostValueKind::Callable =>
            {
                arc
            }
            BexExternalValue::HostValue(arc) => {
                return SysOpOutput::err(VmBamlError::InvalidArgument {
                    message: format!(
                        "expected a host callable, got a HostValue of kind {:?}",
                        arc.kind,
                    ),
                });
            }
            other => {
                return SysOpOutput::err(VmBamlError::InvalidArgument {
                    message: format!("expected HostValue, got {other:?}"),
                });
            }
        };

        // The VM split the call's args by the callable's declared params and
        // packed them as `[positional_array, optional_map]` (see
        // `host_closure_call_sysop`). Unpack and encode them into the
        // `BamlToHostCall`'s flat `args` list: required args first, then the
        // supplied optionals (tagged + keyed by name). Omitted optionals are
        // already absent — the host's own default applies.
        let options = CffiHandleTableOptions::for_wire();
        let mut pack = args.into_iter();
        let positional = match pack.next() {
            Some(BexExternalValue::Array { items, .. }) => items,
            other => {
                return SysOpOutput::err(sys_types::VmInternalError::BridgeFailure {
                    message: format!(
                        "host-call args pack[0] must be the positional array, got {other:?}"
                    ),
                });
            }
        };
        let optional = match pack.next() {
            Some(BexExternalValue::Map { entries, .. }) => entries,
            other => {
                return SysOpOutput::err(sys_types::VmInternalError::BridgeFailure {
                    message: format!(
                        "host-call args pack[1] must be the optional map, got {other:?}"
                    ),
                });
            }
        };
        let encoded: Vec<u8> =
            match bridge_ctypes::build_to_host_call(&positional, &optional, &options) {
                Ok(to_host_call) => to_host_call.encode_to_vec(),
                Err(e) => {
                    // Arg encoding is bridge-side serialization, not a
                    // host-language error. A failure here means the engine
                    // had a `BexExternalValue` it could not put on the wire
                    // — an engine/bridge bug. Surface as a fatal internal
                    // error rather than a catchable `VmBamlError`.
                    return SysOpOutput::err(sys_types::VmInternalError::BridgeFailure {
                        message: format!("failed to encode host-call arguments: {e}"),
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
            // synchronously so the engine does not hang waiting on the
            // oneshot. A missing dispatcher is an SDK / infrastructure
            // fault, not a user-callable error — route as `BridgeFailure`
            // so the host SDK surfaces it as `baml.panics.SdkPanic`
            // (fatal), not a catchable error class. (In practice the
            // working bridges register a dispatcher at module init *and*
            // fast-fail missing-callable cases before this point — but the
            // engine-side contract has to be self-enforcing for any new
            // bridge that doesn't pre-check.)
            if !host_dispatch::fire_dispatch(host_arc.key, call_id, &encoded) {
                if let Some(c) = host_dispatch::take(call_id) {
                    c.complete(Err(OpError::new(
                        SysOp::BamlHostCallHostValue,
                        sys_types::VmInternalError::BridgeFailure {
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
        // from the inbound payload by `complete_host_call`). The async-body
        // arm validates the host's returned value against `type_arg_0` (T)
        // — mismatch is a `HostContractViolation` panic — and the host
        // throw, if any, against `type_arg_1` (E) — off-contract throws
        // are also `HostContractViolation` panics; on-contract throws
        // propagate as catchable.
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
            // `Ready(Err)` is unreachable in practice because
            // `SysOpResult::pending` always yields `Async`. Surface a
            // VM-side payload conservatively — a host-throw can never
            // reach here, so collapsing to a generic Vm payload is safe.
            SysOpResult::Ready(Err(err)) => match err.payload {
                sys_types::OpErrorPayload::Vm(kind) => SysOpOutput::err(kind),
                sys_types::OpErrorPayload::HostThrown(_) => {
                    unreachable!("Ready(Err) is never produced for the host-callable sysop")
                }
            },
            // Use `async_op_with_throw` (not `async_op`) so the future
            // yields the full [`OpErrorBody`] (preserving a `HostThrown`
            // payload through to the engine).
            // host-throw path sets `host_thrown` to the decoded thrown
            // `BexExternalValue` (via `host_dispatch::complete_with_throw`);
            // the engine reads that field to run the unified contract
            // check + `Value` materialisation. Routing through `async_op`
            // (which threads through `OpErrorBody::from(VmRustFnError)`,
            // defaulting `host_thrown` to `None`) would drop the thrown
            // payload before the engine ever sees it.
            SysOpResult::Async(fut) => SysOpOutput::async_op_with_throw(async move {
                // Move the guard into the future so it is dropped — and the
                // in-flight entry evicted — if this future is cancelled. It is
                // `None` on the collision path (no entry of ours to evict); the
                // awaited `fut` then resolves to the collision error.
                let _guard = guard;
                // A host *throw* arrives as `Err(op_err)` with
                // `op_err.host_thrown = Some(...)`; `?` widens it to
                // `OpErrorBody` (preserving `host_thrown`) so
                // `materialize_host_throw` can run the throws-contract
                // check against `E`.
                let value = fut.await?;
                // Return-type validation stays at the FFI guard (class-name
                // identity + scalar discrimination); a mismatch is a
                // `HostContractViolation` panic.
                validate_return_value(&value, &type_arg_0)?;
                Ok(value)
            }),
        }
    }
}

/// Validate a host-returned value against the wrapper's declared return
/// type. A mismatch becomes a `baml.panics.HostContractViolation` panic —
/// the host has violated its typed contract, so the call cannot be
/// reasonably continued.
///
/// Delegates to the shared, strict [`validate_host_return`] guard (shared
/// with the WASM bridge) so the native and WASM bridges enforce an identical
/// shape contract: scalar discrimination (`int` ≠ `float`), container
/// recursion, enum identity, and class-name identity. Class *field types* are
/// validated engine-side at the result-push site, where the resolved class
/// schema is available.
fn validate_return_value(
    value: &BexExternalValue,
    expected: &RuntimeTy,
) -> Result<(), VmRustFnError> {
    validate_host_return(value, expected).map_err(|err| {
        VmPanic::HostContractViolation {
            message: format!(
                "host callable returned a value of the wrong type: {err} (expected {expected})"
            ),
            class_name: None,
            language: None,
        }
        .into()
    })
}

#[cfg(test)]
mod tests {
    use baml_type::{RuntimeTy, TyAttr};
    use sys_ops::io::{BexExternalValue, CallId, IoNamespaceHost as _, SysOpContext, SysOpOutput};
    use sys_types::{OpError, SysOp, SysOpResult, VmBamlError, VmRustFnError};

    use super::*;
    use crate::host_dispatch;

    fn make_heap() -> Arc<BexHeap> {
        BexHeap::new(vec![])
    }

    fn int_ty() -> RuntimeTy {
        RuntimeTy::Int {
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
            RuntimeTy::unknown(),
            &ctx,
        );
        let result = match output {
            SysOpOutput::Ready(r) => r,
            SysOpOutput::Async(fut) => fut.await.map_err(|body| match body.payload {
                sys_types::OpErrorPayload::Vm(kind) => kind,
                sys_types::OpErrorPayload::HostThrown(_) => {
                    panic!("expected a Vm payload, got a host-thrown value")
                }
            }),
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
                    handle: bex_resource_types::HostValueArc::new(
                        42,
                        bex_resource_types::HostValueKind::Opaque,
                    ),
                },
            ),
        );

        let err = match result {
            SysOpResult::Async(fut) => fut.await.expect_err("expected error"),
            SysOpResult::Ready(Err(e)) => e,
            SysOpResult::Ready(Ok(_)) => panic!("expected error"),
        };
        match &err.payload {
            sys_types::OpErrorPayload::Vm(VmRustFnError::BamlError(
                VmBamlError::HostCallable {
                    class_name,
                    language,
                    ..
                },
            )) => {
                assert_eq!(class_name, "RuntimeError");
                assert_eq!(language.as_deref(), Some("python"));
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
        // Wrong-type return is a contract violation, not a catchable
        // error — must panic with `HostContractViolation`.
        match err {
            VmRustFnError::Panic(VmPanic::HostContractViolation {
                class_name,
                language,
                ..
            }) => {
                // Return-type mismatches have no offending host exception,
                // so class_name / language stay `None`.
                assert!(
                    class_name.is_none(),
                    "expected no class_name, got {class_name:?}"
                );
                assert!(language.is_none(), "expected no language, got {language:?}");
            }
            other => panic!("expected Panic(HostContractViolation), got {other:?}"),
        }
    }

    // -------------------------------------------------------------------------
    // Return-type validation: `unknown` accepts anything
    // -------------------------------------------------------------------------
    #[test]
    fn validate_return_value_unknown_accepts_anything() {
        validate_return_value(
            &BexExternalValue::String("anything".to_string().into()),
            &RuntimeTy::unknown(),
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
        validate_return_value(&BexExternalValue::Int(3), &RuntimeTy::float())
            .expect_err("an Int value must not satisfy a declared `float` return type");
        validate_return_value(&BexExternalValue::Float(3.0), &RuntimeTy::float())
            .expect("a Float value satisfies a declared `float` return type");
    }
}
