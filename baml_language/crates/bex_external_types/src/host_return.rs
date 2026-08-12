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

/// Canonical dotted path of the builtin JSON alias — test fixtures build the
/// builtin `TypeName` from it; runtime identity checks go through
/// [`is_canonical_json_alias`], never a rendered string.
#[cfg(test)]
const BAML_JSON_JSON: &str = "baml.json.json";

/// Whether a type name is the canonical builtin `baml.json.json` alias.
///
/// Compares the fully qualified identity — package `baml`, namespace
/// `json`, name `json` — never a rendered display string:
/// `TypeName::display_name()` elides the implicit `user` package for local
/// types, so a user alias declared at namespace path `baml.json` with name
/// `json` would render identically and must NOT receive builtin JSON
/// behavior.
pub fn is_canonical_json_alias(name: &TypeName) -> bool {
    name.package().as_str() == "baml"
        && name.namespace().len() == 1
        && name.namespace()[0].as_str() == "json"
        && name.name().as_str() == "json"
}

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
    // A sparse inbound `value_type` is represented transiently by a
    // `BexExternalValue::Union`, but it is not an actual union value. Its
    // selected type is the authoritative node type; the inner payload may be
    // deliberately structural at this layer (an anonymous class payload or a
    // class-shaped media transport shell). Check the annotation against the
    // host callable's declared return type here. The engine then coerces the
    // payload with that exact type and performs schema-aware validation.
    if let BexExternalValue::Union { metadata, .. } = value
        && metadata.is_inbound_type_annotation
    {
        if inbound_annotation_satisfies_ty(&metadata.selected_option, expected) {
            return Ok(());
        }
        // The canonical `baml.json.json` alias is nominal-opaque to the
        // context-free subtype check above (`NoFacts` cannot expand it), so a
        // bridge that annotates a container-valued json return — the C++
        // codec annotates the selected variant alternative, e.g.
        // `map<string, baml.json.json>` — would be rejected here. Admit the
        // annotation structurally instead: the declared type must admit the
        // json alias, the annotation must stay within the JSON algebra, and
        // the payload (peeled by `value_satisfies_json`) must inhabit it.
        if expected_admits_json_alias(expected, 0)
            && runtime_ty_within_json_algebra(&metadata.selected_option, 0)
            && value_satisfies_json(value)
        {
            return Ok(());
        }
        return Err(HostReturnTypeError {
            actual: metadata.selected_option.to_string(),
            expected: expected.to_string(),
        });
    }

    if value_satisfies_ty(value, expected) {
        Ok(())
    } else {
        Err(HostReturnTypeError {
            actual: value.type_name().to_string(),
            expected: expected.to_string(),
        })
    }
}

fn inbound_annotation_satisfies_ty(actual: &RuntimeTy, expected: &RuntimeTy) -> bool {
    #[expect(
        deprecated,
        reason = "the host boundary has RuntimeTy values but no VM-backed type facts"
    )]
    baml_type::normalize::is_subtype(
        actual.as_ty(),
        expected.as_ty(),
        &baml_type::normalize::NoFacts,
    )
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

        RuntimeTy::TypeAlias(name, _) if is_canonical_json_alias(name) => {
            value_satisfies_json(value)
        }

        // A `Union`-wrapped value against a non-union declared type: validate
        // the inner value against the declared type.
        _ if matches!(value, BexExternalValue::Union { .. }) => {
            let BexExternalValue::Union { value: inner, .. } = value else {
                unreachable!("guarded by the matches! above")
            };
            value_satisfies_ty(inner, ty)
        }

        // A host bridge represents a completed `void` callback as Null on the
        // wire.
        RuntimeTy::Void { .. } | RuntimeTy::Null { .. } => {
            matches!(value, BexExternalValue::Null)
        }
        RuntimeTy::Bool { .. } => matches!(value, BexExternalValue::Bool(_)),
        // `Int` and `Float` are distinct: an `Int` value does NOT satisfy
        // `Float`, nor a `Float` value `Int`. A host-returned wire tag must match
        // the declared representation exactly — never silently reinterpreted (the
        // int→float/bigint conversions are boundary coercions, not subtyping).
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
            // `Literal::Float` stores the literal as a string for precision;
            // match by tag (any float), mirroring
            // `bex_engine::conversion::value_matches_type`.
            (Literal::Float(_), BexExternalValue::Float(_)) => true,
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
        RuntimeTy::EnumVariant(tn, expected_variant, _) => match value {
            BexExternalValue::Variant {
                enum_name,
                variant_name,
            } => {
                type_name_matches_external_name(enum_name, tn)
                    && variant_name == expected_variant.as_str()
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
        RuntimeTy::Function { .. } => {
            matches!(
                value,
                BexExternalValue::HostValue(host)
                    if host.kind == crate::HostValueKind::Callable
            ) || matches!(value, BexExternalValue::FunctionRef { .. })
        }

        // Opaque / compiler-only / otherwise-unhandled `RuntimeTy` shapes (e.g.
        // `Opaque`, `Future`): accept rather than risk a false rejection of a
        // value the engine's typed conversion will handle. These should not
        // appear as concrete host-callable return types in practice.
        _ => true,
    }
}

/// Whether an external value is exactly in the recursive `baml.json.json`
/// algebra. BAML extensions such as bigint, bytes, classes, enums, media,
/// handles, and non-finite floats are intentionally rejected.
///
/// A sparse inbound `value_type` annotation (a transient
/// `BexExternalValue::Union` with `is_inbound_type_annotation`, e.g. the
/// Swift bridge annotates every json scalar leaf) is peeled — but only when
/// the annotation itself stays within the JSON algebra, so a payload
/// annotated as `bigint` or a class is still rejected. A genuine union
/// carrier (a value produced from a declared union) is never JSON.
pub fn value_satisfies_json(value: &BexExternalValue) -> bool {
    fn recurse(value: &BexExternalValue, depth: usize) -> bool {
        if depth > 256 {
            return false;
        }
        match value {
            BexExternalValue::Null
            | BexExternalValue::Int(_)
            | BexExternalValue::Bool(_)
            | BexExternalValue::String(_) => true,
            BexExternalValue::Float(value) => value.is_finite(),
            BexExternalValue::Array { items, .. } => {
                items.iter().all(|item| recurse(item, depth + 1))
            }
            BexExternalValue::Map { entries, .. } => {
                entries.values().all(|item| recurse(item, depth + 1))
            }
            BexExternalValue::Union { value, metadata } => {
                metadata.is_inbound_type_annotation
                    && runtime_ty_within_json_algebra(&metadata.selected_option, 0)
                    && recurse(value, depth + 1)
            }
            _ => false,
        }
    }

    recurse(value, 0)
}

/// Whether a declared type admits the canonical `baml.json.json` alias: the
/// alias itself, or a union with the alias among its members.
fn expected_admits_json_alias(ty: &RuntimeTy, depth: usize) -> bool {
    if depth > 64 {
        return false;
    }
    match ty {
        RuntimeTy::TypeAlias(name, _) => is_canonical_json_alias(name),
        RuntimeTy::Union(members, _) => members
            .iter()
            .any(|member| expected_admits_json_alias(member, depth + 1)),
        _ => false,
    }
}

/// Whether a declared or annotated `RuntimeTy` lies entirely within the
/// recursive `baml.json.json` algebra: the alias itself, the JSON scalar
/// primitives, their literals, and string-keyed containers thereof.
fn runtime_ty_within_json_algebra(ty: &RuntimeTy, depth: usize) -> bool {
    if depth > 64 {
        return false;
    }
    match ty {
        RuntimeTy::Null { .. }
        | RuntimeTy::Bool { .. }
        | RuntimeTy::Int { .. }
        | RuntimeTy::Float { .. }
        | RuntimeTy::String { .. } => true,
        RuntimeTy::TypeAlias(name, _) => is_canonical_json_alias(name),
        RuntimeTy::Literal(literal, _, _) => matches!(
            literal,
            Literal::Bool(_) | Literal::Int(_) | Literal::String(_) | Literal::Float(_)
        ),
        RuntimeTy::List(inner, _) => runtime_ty_within_json_algebra(inner, depth + 1),
        RuntimeTy::Map { key, value, .. } => {
            matches!(key.as_ref(), RuntimeTy::String { .. })
                && runtime_ty_within_json_algebra(value, depth + 1)
        }
        RuntimeTy::Union(members, _) => {
            !members.is_empty()
                && members
                    .iter()
                    .all(|member| runtime_ty_within_json_algebra(member, depth + 1))
        }
        _ => false,
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

    fn json_ty() -> RuntimeTy {
        RuntimeTy::TypeAlias(
            TypeName::from_dotted_path(BAML_JSON_JSON),
            TyAttr::default(),
        )
    }

    #[test]
    fn canonical_json_alias_accepts_only_the_json_value_algebra() {
        let mut nested = IndexMap::new();
        nested.insert(
            "items".to_string(),
            BexExternalValue::Array {
                element_type: RuntimeTy::unknown(),
                items: vec![
                    BexExternalValue::Null,
                    BexExternalValue::Bool(true),
                    BexExternalValue::Int(7),
                    BexExternalValue::Float(1.5),
                    BexExternalValue::String("ok".into()),
                ],
            },
        );
        let valid = BexExternalValue::Map {
            key_type: RuntimeTy::string(),
            value_type: RuntimeTy::unknown(),
            entries: nested,
        };
        assert!(validate_host_return(&valid, &json_ty()).is_ok());
        assert!(validate_host_return(&BexExternalValue::Float(f64::NAN), &json_ty()).is_err());
        assert!(validate_host_return(&BexExternalValue::Bigint(1.into()), &json_ty()).is_err());
        assert!(validate_host_return(&BexExternalValue::Uint8Array(vec![1]), &json_ty()).is_err());
        assert!(
            validate_host_return(
                &BexExternalValue::Instance {
                    class_name: "JsonLooking".to_string(),
                    type_args: vec![],
                    fields: IndexMap::new(),
                },
                &json_ty(),
            )
            .is_err()
        );

        let forged = BexExternalValue::union(
            BexExternalValue::String("json-shaped payload".into()),
            [RuntimeTy::bigint(), RuntimeTy::string()],
            RuntimeTy::bigint(),
        );
        assert!(validate_host_return(&forged, &json_ty()).is_err());
    }

    #[test]
    fn user_alias_shadowing_json_display_name_is_not_canonical() {
        // `display_name()` elides the implicit `user` package, so a user alias
        // declared at namespace path `baml.json` with name `json` renders
        // identically to the builtin. Identity must be package-qualified.
        let builtin = TypeName::from_dotted_path(BAML_JSON_JSON);
        let shadow = TypeName::from_dotted_path("user.baml.json.json");
        assert_eq!(
            builtin.display_name().as_str(),
            shadow.display_name().as_str()
        );
        assert!(is_canonical_json_alias(&builtin));
        assert!(!is_canonical_json_alias(&shadow));

        // The shadow alias must not inherit builtin JSON strictness: a bigint
        // is rejected by the json algebra but passes the shadow alias through
        // this layer's defensive accept-any tail (its body is validated
        // engine-side, where alias definitions are available).
        let shadow_ty = RuntimeTy::TypeAlias(shadow, TyAttr::default());
        let bigint = BexExternalValue::Bigint(1.into());
        assert!(validate_host_return(&bigint, &json_ty()).is_err());
        assert!(validate_host_return(&bigint, &shadow_ty).is_ok());
    }

    #[test]
    fn canonical_json_alias_accepts_algebra_scoped_inbound_annotations() {
        // The C++ codec annotates a json return's selected variant alternative
        // (`map<string, baml.json.json>`); Swift annotates scalar leaves. Both
        // are sparse inbound annotations inside the JSON algebra and must
        // validate against a declared json return — including nested in a
        // container — while annotations outside the algebra stay rejected.
        let mut entries = IndexMap::new();
        entries.insert(
            "type".to_string(),
            BexExternalValue::typed(BexExternalValue::String("ok".into()), RuntimeTy::string()),
        );
        let annotated_map = BexExternalValue::typed(
            BexExternalValue::Map {
                key_type: RuntimeTy::string(),
                value_type: RuntimeTy::unknown(),
                entries,
            },
            RuntimeTy::Map {
                key: Box::new(RuntimeTy::string()),
                value: Box::new(json_ty()),
                attr: TyAttr::default(),
            },
        );
        assert!(validate_host_return(&annotated_map, &json_ty()).is_ok());

        let annotated_bigint =
            BexExternalValue::typed(BexExternalValue::Bigint(1.into()), RuntimeTy::bigint());
        assert!(validate_host_return(&annotated_bigint, &json_ty()).is_err());
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
        assert!(
            validate_host_return(
                &BexExternalValue::Null,
                &RuntimeTy::Void {
                    attr: TyAttr::default()
                }
            )
            .is_ok()
        );
        assert!(
            validate_host_return(
                &BexExternalValue::Int(1),
                &RuntimeTy::Void {
                    attr: TyAttr::default()
                }
            )
            .is_err()
        );
        // Cross-tag rejections.
        assert!(
            validate_host_return(&BexExternalValue::String("x".into()), &RuntimeTy::int()).is_err()
        );
        assert!(validate_host_return(&BexExternalValue::Bool(true), &RuntimeTy::string()).is_err());
    }

    #[test]
    fn void_requires_the_null_boundary_value() {
        let void = RuntimeTy::Void {
            attr: TyAttr::default(),
        };
        assert!(validate_host_return(&BexExternalValue::Null, &void).is_ok());
        assert!(validate_host_return(&BexExternalValue::Int(1), &void).is_err());
    }

    #[test]
    fn literal_value_equality() {
        let lit5 = RuntimeTy::Literal(Literal::Int(5), Freshness::Regular, TyAttr::default());
        assert!(validate_host_return(&BexExternalValue::Int(5), &lit5).is_ok());
        assert!(validate_host_return(&BexExternalValue::Int(6), &lit5).is_err());
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
    fn enum_variant_requires_exact_enum_and_variant() {
        let mood = TypeName::from_dotted_path("user.callbacks.Mood");
        let happy = RuntimeTy::EnumVariant(mood.clone(), Name::new("HAPPY"), TyAttr::default());
        let value = BexExternalValue::Variant {
            enum_name: mood.to_string(),
            variant_name: "HAPPY".to_string(),
        };
        assert!(validate_host_return(&value, &happy).is_ok());

        let wrong_variant = BexExternalValue::Variant {
            enum_name: mood.to_string(),
            variant_name: "SAD".to_string(),
        };
        assert!(validate_host_return(&wrong_variant, &happy).is_err());

        let wrong_enum = BexExternalValue::Variant {
            enum_name: "user.callbacks.OtherMood".to_string(),
            variant_name: "HAPPY".to_string(),
        };
        assert!(validate_host_return(&wrong_enum, &happy).is_err());

        let nested = RuntimeTy::list(happy.clone());
        let nested_valid = BexExternalValue::Array {
            element_type: happy.clone(),
            items: vec![value],
        };
        assert!(validate_host_return(&nested_valid, &nested).is_ok());
        let nested_invalid = BexExternalValue::Array {
            element_type: happy,
            items: vec![wrong_variant],
        };
        assert!(validate_host_return(&nested_invalid, &nested).is_err());

        let nested_union = RuntimeTy::list(RuntimeTy::union([
            RuntimeTy::EnumVariant(mood.clone(), Name::new("HAPPY"), TyAttr::default()),
            RuntimeTy::int(),
        ]));
        let nested_union_valid = BexExternalValue::Array {
            element_type: RuntimeTy::unknown(),
            items: vec![BexExternalValue::Variant {
                enum_name: mood.to_string(),
                variant_name: "HAPPY".to_string(),
            }],
        };
        assert!(validate_host_return(&nested_union_valid, &nested_union).is_ok());
        let nested_union_invalid = BexExternalValue::Array {
            element_type: RuntimeTy::unknown(),
            items: vec![BexExternalValue::Variant {
                enum_name: mood.to_string(),
                variant_name: "SAD".to_string(),
            }],
        };
        assert!(validate_host_return(&nested_union_invalid, &nested_union).is_err());
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
    fn sparse_class_annotation_is_checked_before_anonymous_payload_shape() {
        let user = RuntimeTy::Class(
            TypeName::from_dotted_path("user.callbacks.User"),
            vec![RuntimeTy::int()],
            TyAttr::default(),
        );
        let anonymous_payload = BexExternalValue::Instance {
            class_name: String::new(),
            type_args: vec![],
            fields: IndexMap::new(),
        };

        assert!(
            validate_host_return(
                &BexExternalValue::typed(anonymous_payload.clone(), user.clone()),
                &user,
            )
            .is_ok()
        );

        let other = RuntimeTy::Class(
            TypeName::from_dotted_path("user.callbacks.Other"),
            vec![RuntimeTy::int()],
            TyAttr::default(),
        );
        let error = validate_host_return(&BexExternalValue::typed(anonymous_payload, other), &user)
            .expect_err("an annotation for another class must not satisfy User<int>");
        assert_eq!(error.expected, user.to_string());
        assert!(error.actual.contains("Other"));
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
        // An opaque host value has the same wire carrier but is not callable.
        let opaque =
            BexExternalValue::HostValue(crate::HostValueArc::new(2, crate::HostValueKind::Opaque));
        assert!(validate_host_return(&opaque, &fn_ty).is_err());
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
