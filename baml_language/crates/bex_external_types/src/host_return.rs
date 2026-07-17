//! Strict, schema-free validation of a host-callable's returned value against
//! its declared return type.
//!
//! When BAML code invokes a host callable (`f(x)` where `f: (A) -> R` was
//! supplied by the host language), the host's returned value flows back into
//! the engine and must be type-checked against the declared return type `R`
//! before it is materialized on the VM heap. Skipping this check lets a buggy
//! or malicious host inject a value that violates `R` — a string where an
//! `int` is declared, a `Variant` of the wrong enum, an `int` for a `float`,
//! and so on — corrupting later type-directed VM operations.
//!
//! # Layering
//!
//! This validator is the **shape-level** guard shared by the native and WASM
//! bridges (`sys_native::host_impls`, `bridge_wasm::host_value`). It enforces
//! everything that can be checked from the value tree + the declared [`RuntimeTy`]
//! alone:
//!
//! - scalar discrimination (`Int` does *not* satisfy `Float` and vice-versa);
//! - `String` / `Bool` / `Uint8Array` exact tags;
//! - `Literal` value equality;
//! - container recursion (`List` element types, `Map` value types);
//! - `Optional` (null or inner-valid) and `Union` (matches ≥ 1 member);
//! - **enum identity** — a `Variant` must name the *declared* enum;
//! - **class-name identity** — an `Instance` must name the *declared* class
//!   (a bare `Map` does *not* satisfy a class type: a class return must arrive
//!   as a class value, since a map materializes as `Object::Map` rather than an
//!   instance of the declared class).
//!
//! It deliberately does **not** validate class *field types*: a
//! [`RuntimeTy::Class`] carries only the class name + generic args, not its field
//! definitions, so per-field type checking requires the engine's resolved
//! class schema and is performed engine-side at the result-push site (see
//! `bex_engine::conversion`). This module is the first, schema-free line of
//! defense; the engine adds the schema-aware second line.
//!
//! `RuntimeTy::TypeVar`-style opaque positions and [`RuntimeTy::BuiltinUnknown`] accept any
//! value: at the FFI boundary the declared return type handed to a host call
//! is always concrete (generic functions are not host entry points), so these
//! arms are defensive rather than load-bearing.

use baml_type::{Literal, RuntimeTy, TypeName};

use crate::BexExternalValue;

/// A host callable returned a value whose runtime shape cannot inhabit the
/// declared return type.
///
/// Carries the offending value's type name and the declared type's display
/// string so the caller can build a `HostCallable` / `TypeError` message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostReturnTypeError {
    /// The runtime type name of the offending value (e.g. `"string"`).
    pub actual: String,
    /// The display form of the declared return type (e.g. `"int"`).
    pub expected: String,
}

impl std::fmt::Display for HostReturnTypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "host callable returned a value of type `{}` that does not match the declared return type `{}`",
            self.actual, self.expected
        )
    }
}

impl std::error::Error for HostReturnTypeError {}

/// Strictly validate `value` against the declared host-callable return type
/// `expected`.
///
/// Returns `Ok(())` when the value's shape can inhabit `expected`, or a
/// [`HostReturnTypeError`] describing the mismatch otherwise. See the module
/// docs for the exact per-`RuntimeTy` contract and the shape-vs-schema layering.
pub fn validate_host_return(
    value: &BexExternalValue,
    expected: &RuntimeTy,
) -> Result<(), HostReturnTypeError> {
    if value_satisfies_ty(value, expected) {
        Ok(())
    } else {
        Err(HostReturnTypeError {
            actual: value.type_name().to_string(),
            expected: expected.to_string(),
        })
    }
}

/// Strict, recursive shape match of a `BexExternalValue` against a `RuntimeTy`.
///
/// Unlike a conservative "reject only the obviously-wrong" check, this returns
/// `false` whenever the value's runtime shape is not a member of `ty`. It
/// stops short only where the type is genuinely "any" (`BuiltinUnknown`) or
/// where the necessary schema (class field types) is unavailable in this
/// crate — class checking is limited to name identity here and completed
/// engine-side.
fn value_satisfies_ty(value: &BexExternalValue, ty: &RuntimeTy) -> bool {
    match ty {
        // `unknown` / `any`: accept anything. At the FFI boundary the declared
        // return type is concrete, so this is defensive.
        RuntimeTy::BuiltinUnknown { .. } => true,

        // Union: matches at least one member. A `Union`-wrapped value is
        // unwrapped and checked against the arms.
        RuntimeTy::Union(members, _) => match value {
            BexExternalValue::Union { value: inner, .. } => {
                members.iter().any(|m| value_satisfies_ty(inner, m))
            }
            _ => members.iter().any(|m| value_satisfies_ty(value, m)),
        },

        // A `Union`-wrapped value against a non-union declared type: validate
        // the inner value against the declared type.
        _ if matches!(value, BexExternalValue::Union { .. }) => {
            let BexExternalValue::Union { value: inner, .. } = value else {
                unreachable!("guarded by the matches! above")
            };
            value_satisfies_ty(inner, ty)
        }

        RuntimeTy::Null { .. } => matches!(value, BexExternalValue::Null),
        RuntimeTy::Bool { .. } => matches!(value, BexExternalValue::Bool(_)),
        // `Int` and `Float` are distinct: an `Int` value does NOT satisfy
        // `Float`, nor a `Float` value `Int`. The numeric-widening that
        // `RuntimeTy::is_subtype_of` allows for *static* typing must not silently
        // reinterpret a host's returned tag.
        RuntimeTy::Int { .. } => matches!(value, BexExternalValue::Int(_)),
        RuntimeTy::Float { .. } => matches!(value, BexExternalValue::Float(_)),
        RuntimeTy::Bigint { .. } => matches!(value, BexExternalValue::Bigint(_)),
        RuntimeTy::String { .. } => matches!(value, BexExternalValue::String(_)),
        RuntimeTy::Uint8Array { .. } => matches!(value, BexExternalValue::Uint8Array(_)),

        RuntimeTy::Literal(lit, _, _) => match (lit, value) {
            (Literal::Bool(b), BexExternalValue::Bool(v)) => b == v,
            (Literal::Int(i), BexExternalValue::Int(v)) => i == v,
            (Literal::Bigint(b), BexExternalValue::Bigint(v)) => b == v,
            (Literal::String(s), BexExternalValue::String(v)) => s == v,
            // `Literal::Float` stores source text; compare its parsed semantic
            // value so host-return validation and union arm selection cannot
            // accept contradictory same-tag literal metadata.
            (Literal::Float(literal), BexExternalValue::Float(value)) => {
                float_matches_literal(*value, literal)
            }
            _ => false,
        },

        RuntimeTy::List(inner, _) => match value {
            BexExternalValue::Array { items, .. } => {
                items.iter().all(|item| value_satisfies_ty(item, inner))
            }
            _ => false,
        },

        RuntimeTy::Map { value: v_ty, .. } => match value {
            BexExternalValue::Map { entries, .. } => {
                entries.values().all(|v| value_satisfies_ty(v, v_ty))
            }
            _ => false,
        },

        // Class identity: an `Instance` must name the declared class. A bare
        // `Map` does NOT satisfy a class type — a class return must arrive as a
        // class value (→ `Instance`); a plain map materializes as `Object::Map`
        // engine-side, never an instance of the declared class. Field *types*
        // are not checked here (the declared `RuntimeTy::Class` carries no field
        // defs); the engine completes per-field validation against its resolved
        // schema.
        RuntimeTy::Class(tn, _, _) => match value {
            BexExternalValue::Instance { class_name, .. } => {
                type_name_matches_external_name(class_name, tn)
            }
            _ => false,
        },

        // Enum identity: a `Variant` must name the declared enum.
        RuntimeTy::Enum(tn, _) => match value {
            BexExternalValue::Variant { enum_name, .. } => {
                type_name_matches_external_name(enum_name, tn)
            }
            _ => false,
        },

        RuntimeTy::Media(..) => matches!(
            value,
            BexExternalValue::Adt(crate::BexExternalAdt::Media(_))
        ),

        // A function-typed return is satisfied only by a callable value: a host
        // callable (`HostValue`) or a BAML function reference (`FunctionRef`).
        // No other value can inhabit a function type, so reject it rather than
        // let it fall through to the accept-anything opaque tail below.
        RuntimeTy::Function { .. } => matches!(
            value,
            BexExternalValue::HostValue(_) | BexExternalValue::FunctionRef { .. }
        ),

        // Opaque / compiler-only / otherwise-unhandled `RuntimeTy` shapes (e.g.
        // `Opaque`, `Future`): accept rather than risk a false rejection of a
        // value the engine's typed conversion will handle. These should not
        // appear as concrete host-callable return types in practice.
        _ => true,
    }
}

/// Whether the value-tree class/enum name string matches the declared
/// [`TypeName`].
///
/// The value carries a flat string (`class_name` / `enum_name`); the declared
/// type carries a structured [`TypeName`]. A match is any of: the display
/// name, the bare short name (for local types), or the dotted module-qualified
/// name. Mirrors `bex_engine::conversion::type_name_matches_external_name`.
fn type_name_matches_external_name(external_name: &str, type_name: &TypeName) -> bool {
    external_name == type_name.display_name().as_str()
        || external_name == type_name.render_dotted(false)
}

#[allow(
    clippy::float_cmp,
    reason = "BAML literal membership requires exact semantic equality, not an approximate numeric comparison"
)]
fn float_matches_literal(value: f64, literal: &str) -> bool {
    literal.parse::<f64>().is_ok_and(|literal| value == literal)
}

#[cfg(test)]
mod tests {
    use baml_type::{
        Freshness, Literal, Name, RuntimeFunctionParamTy, RuntimeTy, TyAttr, TypeName,
    };
    use indexmap::IndexMap;

    use super::*;
    use crate::BexExternalValue;

    fn int_ty() -> RuntimeTy {
        RuntimeTy::int()
    }

    #[test]
    fn scalar_int_does_not_satisfy_float_and_vice_versa() {
        // The core int≠float distinction.
        assert!(validate_host_return(&BexExternalValue::Int(1), &RuntimeTy::int()).is_ok());
        assert!(validate_host_return(&BexExternalValue::Int(1), &RuntimeTy::float()).is_err());
        assert!(validate_host_return(&BexExternalValue::Float(1.0), &RuntimeTy::float()).is_ok());
        assert!(validate_host_return(&BexExternalValue::Float(1.0), &RuntimeTy::int()).is_err());
    }

    #[test]
    fn scalar_exact_tags() {
        assert!(validate_host_return(&BexExternalValue::Bool(true), &RuntimeTy::bool()).is_ok());
        assert!(
            validate_host_return(&BexExternalValue::String("x".into()), &RuntimeTy::string())
                .is_ok()
        );
        assert!(
            validate_host_return(
                &BexExternalValue::Uint8Array(vec![1]),
                &RuntimeTy::uint8array()
            )
            .is_ok()
        );
        assert!(validate_host_return(&BexExternalValue::Null, &RuntimeTy::null()).is_ok());
        // Cross-tag rejections.
        assert!(
            validate_host_return(&BexExternalValue::String("x".into()), &RuntimeTy::int()).is_err()
        );
        assert!(validate_host_return(&BexExternalValue::Bool(true), &RuntimeTy::string()).is_err());
    }

    #[test]
    fn literal_value_equality() {
        let cases = [
            (
                RuntimeTy::Literal(Literal::Int(5), Freshness::Regular, TyAttr::default()),
                BexExternalValue::Int(5),
                BexExternalValue::Int(6),
            ),
            (
                RuntimeTy::Literal(
                    Literal::Float("2.5".to_string()),
                    Freshness::Regular,
                    TyAttr::default(),
                ),
                BexExternalValue::Float(2.5),
                BexExternalValue::Float(2.25),
            ),
            (
                RuntimeTy::Literal(
                    Literal::String("crlf".to_string()),
                    Freshness::Regular,
                    TyAttr::default(),
                ),
                BexExternalValue::String("crlf".into()),
                BexExternalValue::String("lf".into()),
            ),
            (
                RuntimeTy::Literal(Literal::Bool(false), Freshness::Regular, TyAttr::default()),
                BexExternalValue::Bool(false),
                BexExternalValue::Bool(true),
            ),
        ];

        for (literal, equal, unequal) in cases {
            assert!(validate_host_return(&equal, &literal).is_ok());
            assert!(validate_host_return(&unequal, &literal).is_err());
        }
    }

    #[test]
    fn optional_accepts_null_or_inner() {
        let opt_int = RuntimeTy::optional(int_ty());
        assert!(validate_host_return(&BexExternalValue::Null, &opt_int).is_ok());
        assert!(validate_host_return(&BexExternalValue::Int(1), &opt_int).is_ok());
        assert!(validate_host_return(&BexExternalValue::String("x".into()), &opt_int).is_err());
    }

    #[test]
    fn union_matches_one_member() {
        let union = RuntimeTy::union([int_ty(), RuntimeTy::string()]);
        assert!(validate_host_return(&BexExternalValue::Int(1), &union).is_ok());
        assert!(validate_host_return(&BexExternalValue::String("x".into()), &union).is_ok());
        assert!(validate_host_return(&BexExternalValue::Bool(true), &union).is_err());
    }

    #[test]
    fn list_and_map_recurse() {
        let list_int = RuntimeTy::list(int_ty());
        assert!(
            validate_host_return(
                &BexExternalValue::Array {
                    element_type: int_ty(),
                    items: vec![BexExternalValue::Int(1), BexExternalValue::Int(2)],
                },
                &list_int,
            )
            .is_ok()
        );
        assert!(
            validate_host_return(
                &BexExternalValue::Array {
                    element_type: int_ty(),
                    items: vec![BexExternalValue::String("x".into())],
                },
                &list_int,
            )
            .is_err()
        );

        let map_int = RuntimeTy::Map {
            key: Box::new(RuntimeTy::string()),
            value: Box::new(int_ty()),
            attr: TyAttr::default(),
        };
        let mut ok_entries = IndexMap::new();
        ok_entries.insert("a".to_string(), BexExternalValue::Int(1));
        assert!(
            validate_host_return(
                &BexExternalValue::Map {
                    key_type: RuntimeTy::string(),
                    value_type: int_ty(),
                    entries: ok_entries,
                },
                &map_int,
            )
            .is_ok()
        );
        let mut bad_entries = IndexMap::new();
        bad_entries.insert("a".to_string(), BexExternalValue::String("x".into()));
        assert!(
            validate_host_return(
                &BexExternalValue::Map {
                    key_type: RuntimeTy::string(),
                    value_type: int_ty(),
                    entries: bad_entries,
                },
                &map_int,
            )
            .is_err()
        );
    }

    #[test]
    fn enum_identity_is_enforced() {
        let status = RuntimeTy::Enum(TypeName::local(Name::new("Status")), TyAttr::default());
        assert!(validate_host_return(&BexExternalValue::variant("Status", "Ok"), &status,).is_ok());
        // Wrong enum name → reject.
        assert!(
            validate_host_return(&BexExternalValue::variant("Color", "Red"), &status,).is_err()
        );
        // A non-variant value → reject.
        assert!(validate_host_return(&BexExternalValue::Int(1), &status).is_err());
    }

    #[test]
    fn class_name_identity_is_enforced() {
        let user = RuntimeTy::Class(
            TypeName::local(Name::new("User")),
            Vec::new(),
            TyAttr::default(),
        );
        assert!(
            validate_host_return(
                &BexExternalValue::Instance {
                    class_name: "User".to_string(),
                    type_args: vec![],
                    fields: IndexMap::new(),
                },
                &user,
            )
            .is_ok()
        );
        // Wrong class name → reject.
        assert!(
            validate_host_return(
                &BexExternalValue::Instance {
                    class_name: "Other".to_string(),
                    type_args: vec![],
                    fields: IndexMap::new(),
                },
                &user,
            )
            .is_err()
        );
        // A bare map does NOT satisfy a class type — a class return must arrive
        // as a class value (an `Instance`), never a plain map.
        assert!(
            validate_host_return(
                &BexExternalValue::Map {
                    key_type: RuntimeTy::string(),
                    value_type: RuntimeTy::unknown(),
                    entries: IndexMap::new(),
                },
                &user,
            )
            .is_err()
        );
    }

    #[test]
    fn function_type_accepts_only_callables() {
        let fn_ty = RuntimeTy::Function {
            params: vec![RuntimeFunctionParamTy::required(None, RuntimeTy::int())],
            ret: Box::new(RuntimeTy::string()),
            throws: Box::new(RuntimeTy::null()),
            attr: TyAttr::default(),
        };
        // A host callable satisfies a function-typed return.
        let host = BexExternalValue::HostValue(crate::HostValueArc::new(
            1,
            crate::HostValueKind::Callable,
        ));
        assert!(validate_host_return(&host, &fn_ty).is_ok());
        // A BAML function reference satisfies it too.
        assert!(
            validate_host_return(&BexExternalValue::FunctionRef { global_index: 0 }, &fn_ty)
                .is_ok()
        );
        // A non-callable value cannot inhabit a function type.
        assert!(validate_host_return(&BexExternalValue::String("x".into()), &fn_ty).is_err());
        assert!(validate_host_return(&BexExternalValue::Int(1), &fn_ty).is_err());
    }

    #[test]
    fn builtin_unknown_accepts_anything() {
        assert!(
            validate_host_return(
                &BexExternalValue::String("anything".into()),
                &RuntimeTy::unknown()
            )
            .is_ok()
        );
        assert!(validate_host_return(&BexExternalValue::Int(1), &RuntimeTy::unknown()).is_ok());
    }
}
