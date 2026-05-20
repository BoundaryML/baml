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
//!    (`type_arg_0`), raising `OpErrorKind::HostCallable` on mismatch.

use std::sync::Arc;

use baml_type::{Literal, Ty};
use bex_external_types::BexExternalAdt;
use bex_heap::BexHeap;
use bridge_ctypes::{
    CffiHandleTableOptions, baml_core::cffi::HostCallableErrorCategory, external_to_outbound,
};
use prost::Message as _;
use sys_ops::io::{self, BexExternalValue, CallId, OpErrorKind, SysOpContext, SysOpOutput};
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
        _ctx: &SysOpContext,
    ) -> SysOpOutput<BexExternalValue> {
        // Extract the HostValueArc from the incoming handle.
        let host_arc = match handle {
            BexExternalValue::HostValue(arc) => arc,
            other => {
                return SysOpOutput::err(OpErrorKind::TypeError {
                    expected: "HostValue",
                    actual: format!("{other:?}"),
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
                return SysOpOutput::err(OpErrorKind::HostCallable {
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
        host_dispatch::insert(call_id, completion);

        // Fire the dispatch callback. If no bridge has registered, fail
        // synchronously so the engine does not hang waiting on the oneshot.
        if !host_dispatch::fire_dispatch(host_arc.key, call_id, &encoded) {
            if let Some(c) = host_dispatch::take(call_id) {
                c.complete(Err(OpError::new(
                    SysOp::BamlHostCallHostValue,
                    OpErrorKind::NotImplemented {
                        message: "no host bridge registered for host-value dispatch".to_string(),
                    },
                )));
            }
        }

        // The completion already yields a typed `BexExternalValue` (decoded
        // from the inbound payload by `complete_host_call`). Validate the
        // host's returned value against the expected return type `type_arg_0`
        // before handing it back to the VM; on mismatch surface a catchable
        // `root.errors.HostCallable`.
        match result {
            SysOpResult::Ready(Ok(value)) => match validate_return_value(&value, &type_arg_0) {
                Ok(()) => SysOpOutput::ok(value),
                Err(err) => SysOpOutput::err(err),
            },
            SysOpResult::Ready(Err(err)) => SysOpOutput::err(err.kind),
            SysOpResult::Async(fut) => SysOpOutput::async_op(async move {
                let value = fut.await.map_err(|op_err| op_err.kind)?;
                validate_return_value(&value, &type_arg_0)?;
                Ok(value)
            }),
        }
    }
}

/// Validate a host-returned value against the wrapper's declared return
/// type, producing an `OpErrorKind::HostCallable` on mismatch.
///
/// This is a conservative structural check: it rejects values whose runtime
/// shape definitely cannot inhabit `expected`, and accepts anything it
/// cannot conclusively reject. The host bridge re-validates inbound values
/// against the declared signature, so this is a defense-in-depth guard
/// rather than the sole validation point.
fn validate_return_value(value: &BexExternalValue, expected: &Ty) -> Result<(), OpErrorKind> {
    if value_matches_ty(value, expected) {
        Ok(())
    } else {
        Err(OpErrorKind::HostCallable {
            class_name: "TypeError".to_string(),
            message: format!(
                "host callable returned a value of type `{}` that does not match the \
                 declared return type",
                value.type_name(),
            ),
            traceback: None,
            language: None,
            category: HostCallableErrorCategory::HostCallableInvalidArgument as i32,
        })
    }
}

/// Conservative structural match of a `BexExternalValue` against a `Ty`.
///
/// Returns `false` only when the value definitely cannot inhabit `ty`.
/// Ambiguous or opaque cases (`Opaque`, `RustData`, `Handle`, function
/// references, host values, compiler-only `Ty` variants) return `true`.
fn value_matches_ty(value: &BexExternalValue, ty: &Ty) -> bool {
    match ty {
        // `unknown` / `any` accept everything.
        Ty::BuiltinUnknown { .. } => true,

        // Optional accepts null or a value matching the inner type.
        Ty::Optional(inner, _) => {
            matches!(value, BexExternalValue::Null) || value_matches_ty(value, inner)
        }

        // Union accepts a value matching any member. A `Union`-wrapped value
        // is unwrapped and checked against the union arms.
        Ty::Union(members, _) => match value {
            BexExternalValue::Union { value: inner, .. } => {
                members.iter().any(|m| value_matches_ty(inner, m))
            }
            _ => members.iter().any(|m| value_matches_ty(value, m)),
        },

        Ty::Null { .. } => matches!(value, BexExternalValue::Null),
        Ty::Bool { .. } => matches!(value, BexExternalValue::Bool(_)),
        Ty::Int { .. } => matches!(value, BexExternalValue::Int(_)),
        // Int widens to Float (numeric widening, mirrors `Ty::is_subtype_of`).
        Ty::Float { .. } => matches!(value, BexExternalValue::Float(_) | BexExternalValue::Int(_)),
        Ty::String { .. } => matches!(value, BexExternalValue::String(_)),
        Ty::Uint8Array { .. } => matches!(value, BexExternalValue::Uint8Array(_)),

        Ty::Literal(lit, _) => match (lit, value) {
            (Literal::Bool(b), BexExternalValue::Bool(v)) => b == v,
            (Literal::Int(i), BexExternalValue::Int(v)) => i == v,
            (Literal::String(s), BexExternalValue::String(v)) => s == v,
            _ => false,
        },

        Ty::List(inner, _) => match value {
            BexExternalValue::Array { items, .. } => {
                items.iter().all(|item| value_matches_ty(item, inner))
            }
            _ => false,
        },

        Ty::Map { value: v_ty, .. } => match value {
            BexExternalValue::Map { entries, .. } => {
                entries.values().all(|v| value_matches_ty(v, v_ty))
            }
            _ => false,
        },

        // A class is satisfied by an `Instance`; a host encoder may also
        // deliver a bare `Map` for a class shape (see
        // `bex_project::coerce_arg_to_declared_type`).
        Ty::Class(..) => matches!(
            value,
            BexExternalValue::Instance { .. } | BexExternalValue::Map { .. }
        ),

        Ty::Enum(..) => matches!(value, BexExternalValue::Variant { .. }),

        Ty::Media(..) => matches!(value, BexExternalValue::Adt(BexExternalAdt::Media(_))),

        // Opaque / compiler-only / unhandled `Ty` shapes: accept rather than
        // risk a false rejection (the host bridge re-validates).
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use baml_type::{Literal, Ty, TyAttr};
    use sys_ops::io::{BexExternalValue, CallId, IoNamespaceHost as _, SysOpContext, SysOpOutput};
    use sys_types::{OpError, OpErrorKind, SysOp, SysOpResult};

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
            BexExternalValue::String("not a host value".to_string()),
            vec![],
            int_ty(),
            &ctx,
        );
        let result = match output {
            SysOpOutput::Ready(r) => r,
            SysOpOutput::Async(fut) => fut.await,
        };
        let err = result.expect_err("expected type error");
        assert!(
            matches!(err, OpErrorKind::TypeError { .. }),
            "expected TypeError, got {err:?}"
        );
    }

    // -------------------------------------------------------------------------
    // Table round-trip: insert + complete_with_value
    // -------------------------------------------------------------------------
    #[tokio::test]
    async fn completion_table_success_round_trip() {
        let (result, completion) = SysOpResult::pending(SysOp::BamlHostCallHostValue);
        let call_id = host_dispatch::next_call_id();
        host_dispatch::insert(call_id, completion);

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
        host_dispatch::insert(call_id, completion);

        // Simulate the host calling back with a HostCallable error.
        host_dispatch::complete_with_error(
            call_id,
            OpError::new(
                SysOp::BamlHostCallHostValue,
                OpErrorKind::HostCallable {
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
            OpErrorKind::HostCallable {
                class_name,
                language,
                category,
                ..
            } => {
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
                OpErrorKind::NotImplemented {
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
        let err = validate_return_value(&BexExternalValue::String("oops".to_string()), &int_ty())
            .expect_err("a String should not match the `int` return type");
        match err {
            OpErrorKind::HostCallable { category, .. } => {
                assert_eq!(
                    category,
                    HostCallableErrorCategory::HostCallableInvalidArgument as i32
                );
            }
            other => panic!("expected HostCallable, got {other:?}"),
        }
    }

    // -------------------------------------------------------------------------
    // Return-type validation: `unknown` accepts anything
    // -------------------------------------------------------------------------
    #[test]
    fn validate_return_value_unknown_accepts_anything() {
        validate_return_value(
            &BexExternalValue::String("anything".to_string()),
            &Ty::unknown(),
        )
        .expect("`unknown` return type should accept any value");
    }

    // -------------------------------------------------------------------------
    // value_matches_ty: optional, union, literal, list
    // -------------------------------------------------------------------------
    #[test]
    fn value_matches_ty_structural_cases() {
        let opt_int = Ty::Optional(Box::new(int_ty()), TyAttr::default());
        assert!(value_matches_ty(&BexExternalValue::Null, &opt_int));
        assert!(value_matches_ty(&BexExternalValue::Int(1), &opt_int));
        assert!(!value_matches_ty(
            &BexExternalValue::String("x".to_string()),
            &opt_int
        ));

        let union = Ty::Union(
            vec![
                int_ty(),
                Ty::String {
                    attr: TyAttr::default(),
                },
            ],
            TyAttr::default(),
        );
        assert!(value_matches_ty(&BexExternalValue::Int(1), &union));
        assert!(value_matches_ty(
            &BexExternalValue::String("x".to_string()),
            &union
        ));
        assert!(!value_matches_ty(&BexExternalValue::Bool(true), &union));

        let lit = Ty::Literal(Literal::Int(5), TyAttr::default());
        assert!(value_matches_ty(&BexExternalValue::Int(5), &lit));
        assert!(!value_matches_ty(&BexExternalValue::Int(6), &lit));

        let list_int = Ty::List(Box::new(int_ty()), TyAttr::default());
        assert!(value_matches_ty(
            &BexExternalValue::Array {
                element_type: int_ty(),
                items: vec![BexExternalValue::Int(1), BexExternalValue::Int(2)],
            },
            &list_int
        ));
        assert!(!value_matches_ty(
            &BexExternalValue::Array {
                element_type: int_ty(),
                items: vec![BexExternalValue::String("x".to_string())],
            },
            &list_int
        ));
    }
}
