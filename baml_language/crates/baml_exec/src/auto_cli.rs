// Type-driven coercion for individual auto-CLI parameter values.
//
// The argv-level parser lives in `clap_target`; this module owns the
// per-value coercion step (`"42" + RuntimeTy::Int` -> `BexExternalValue::Int(42)`).
// Split this way because the clap path and the (much) simpler "single
// raw token" path both need the same semantic conversion, but only the
// clap path needs full argv parsing.

use anyhow::{Context, Result};
use bex_engine::{BexExternalValue, RuntimeTy};

/// Whether a parameter of type `ty` can be passed directly as a
/// `--name value` flag via auto-CLI. Mirrors the accepted arms of
/// [`parse_cli_value`] — keep the two in sync. Anything that returns
/// `false` must arrive via `--json-args` (or in a mixed call, can come
/// through either, since `--json-args` is universal).
///
/// `Optional<T>` unwraps to the inner type — `string?` is still a
/// primitive (the user just passes `--name null` or the value).
pub fn is_auto_cli_primitive(ty: &RuntimeTy) -> bool {
    match ty {
        RuntimeTy::String { .. }
        | RuntimeTy::Int { .. }
        | RuntimeTy::Float { .. }
        | RuntimeTy::Bool { .. }
        | RuntimeTy::Null { .. }
        | RuntimeTy::Enum(..) => true,
        // `T?` is `T | null`: a nullable wrapper around a primitive is still
        // CLI-passable; a genuine multi-member union is not.
        RuntimeTy::Union(..) if ty.is_nullable_union() => is_auto_cli_primitive(&ty.strip_null()),
        _ => false,
    }
}

/// Convert a CLI string value to a typed [`BexExternalValue`] based on
/// the target type.
///
/// Structured types (`Class`, `List`, `Map`, `Union`) are rejected with a
/// pointer at `--json-args`, per BEP-027 §"Open questions" #5: class
/// parameters can't be passed through auto-CLI. `--json-args` is the
/// universal path (inline / `@file` / `-`) and the only one that
/// survives nontrivial shell quoting.
pub fn parse_cli_value(raw: &str, ty: &RuntimeTy) -> Result<BexExternalValue> {
    match ty {
        RuntimeTy::String { .. } => Ok(BexExternalValue::String(raw.into())),

        RuntimeTy::Int { .. } => {
            let v: i64 = raw
                .parse()
                .with_context(|| format!("expected integer, got `{raw}`"))?;
            Ok(BexExternalValue::Int(v))
        }

        RuntimeTy::Float { .. } => {
            let v: f64 = raw
                .parse()
                .with_context(|| format!("expected float, got `{raw}`"))?;
            Ok(BexExternalValue::Float(v))
        }

        RuntimeTy::Bool { .. } => match raw {
            "true" => Ok(BexExternalValue::Bool(true)),
            "false" => Ok(BexExternalValue::Bool(false)),
            _ => anyhow::bail!("expected `true` or `false`, got `{raw}`"),
        },

        RuntimeTy::Null { .. } => {
            if raw == "null" {
                Ok(BexExternalValue::Null)
            } else {
                anyhow::bail!("expected `null`, got `{raw}`")
            }
        }

        // `T?` is `T | null`: accept the literal `null`, else parse the value
        // against the non-null inner type.
        RuntimeTy::Union(..) if ty.is_nullable_union() => {
            if raw == "null" {
                Ok(BexExternalValue::Null)
            } else {
                parse_cli_value(raw, &ty.strip_null())
            }
        }

        RuntimeTy::Enum(type_name, _) => Ok(BexExternalValue::Variant {
            enum_name: type_name.display_name().to_string(),
            variant_name: raw.to_string(),
        }),

        // Per BEP-027 §"Open questions" #5: anything auto-CLI can't
        // faithfully represent must be delivered via `--json-args`.
        // Structured types (class/list/map/union) are the obvious case;
        // media, literals, type aliases, opaque types, and the engine-
        // internal types (function/void/future) fall in the same
        // bucket — they either can't survive shell quoting or aren't
        // valid CLI parameter types. The previous catchall silently
        // String-coerced everything that fell through; that hides
        // genuine "this param can't be passed this way" errors behind a
        // confusing downstream type mismatch.
        _ => anyhow::bail!(
            "type `{ty}` can't be passed through auto-CLI; \
             deliver it via `--json-args '{{ \"<param>\": ... }}'` \
             (or `--json-args @file` / `--json-args -` for stdin)"
        ),
    }
}

#[cfg(test)]
mod tests {
    use baml_type::{MediaKind, TyAttr, TypeName};

    use super::*;

    fn ty_string() -> RuntimeTy {
        RuntimeTy::String {
            attr: TyAttr::default(),
        }
    }
    fn ty_int() -> RuntimeTy {
        RuntimeTy::Int {
            attr: TyAttr::default(),
        }
    }
    fn ty_bool() -> RuntimeTy {
        RuntimeTy::Bool {
            attr: TyAttr::default(),
        }
    }
    fn ty_float() -> RuntimeTy {
        RuntimeTy::Float {
            attr: TyAttr::default(),
        }
    }
    fn ty_null() -> RuntimeTy {
        RuntimeTy::Null {
            attr: TyAttr::default(),
        }
    }
    fn ty_optional(inner: RuntimeTy) -> RuntimeTy {
        RuntimeTy::optional(inner)
    }
    fn ty_enum(name: &str) -> RuntimeTy {
        RuntimeTy::Enum(TypeName::local(name.into()), TyAttr::default())
    }
    fn ty_list(elem: RuntimeTy) -> RuntimeTy {
        RuntimeTy::List(Box::new(elem), TyAttr::default())
    }
    fn ty_class(name: &str) -> RuntimeTy {
        RuntimeTy::Class(
            TypeName::local(name.into()),
            Box::new([]),
            TyAttr::default(),
        )
    }

    fn assert_string(raw: &BexExternalValue, expected: &str) {
        match raw {
            BexExternalValue::String(s) => assert_eq!(s, expected),
            other => panic!("expected String, got {other:?}"),
        }
    }

    // ── parse_cli_value: primitives ─────────────────────────────────────

    #[test]
    fn parse_cli_value_string_is_primitive() {
        assert_string(&parse_cli_value("hello", &ty_string()).unwrap(), "hello");
    }

    #[test]
    fn parse_cli_value_int_parses_signed() {
        let raw = parse_cli_value("-42", &ty_int()).unwrap();
        match raw {
            BexExternalValue::Int(v) => assert_eq!(v, -42),
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn parse_cli_value_int_rejects_non_integer() {
        assert!(parse_cli_value("3.14", &ty_int()).is_err());
        assert!(parse_cli_value("abc", &ty_int()).is_err());
    }

    #[test]
    fn parse_cli_value_float_parses() {
        let raw = parse_cli_value("2.5", &ty_float()).unwrap();
        match raw {
            BexExternalValue::Float(v) => {
                assert!((v - 2.5).abs() < 1e-9);
            }
            other => panic!("got {other:?}"),
        }
    }

    /// BEP-027 §"Auto-CLI conventions": booleans use `--flag=true` /
    /// `--flag=false`, not `--flag` / `--no-flag`.
    #[test]
    fn parse_cli_value_bool_requires_true_or_false() {
        assert!(matches!(
            parse_cli_value("true", &ty_bool()).unwrap(),
            BexExternalValue::Bool(true)
        ));
        assert!(matches!(
            parse_cli_value("false", &ty_bool()).unwrap(),
            BexExternalValue::Bool(false)
        ));
        assert!(parse_cli_value("yes", &ty_bool()).is_err());
        assert!(parse_cli_value("1", &ty_bool()).is_err());
    }

    #[test]
    fn parse_cli_value_null_accepts_literal_null_only() {
        assert!(matches!(
            parse_cli_value("null", &ty_null()).unwrap(),
            BexExternalValue::Null
        ));
        assert!(parse_cli_value("None", &ty_null()).is_err());
    }

    #[test]
    fn parse_cli_value_optional_null_is_null() {
        let raw = parse_cli_value("null", &ty_optional(ty_int())).unwrap();
        assert!(matches!(raw, BexExternalValue::Null));
    }

    #[test]
    fn parse_cli_value_optional_unwraps_inner() {
        let raw = parse_cli_value("7", &ty_optional(ty_int())).unwrap();
        assert!(matches!(raw, BexExternalValue::Int(7)));
    }

    /// BEP-027 §"Auto-CLI conventions": *"Enum values match the declared
    /// variant name exactly (case-sensitive)."* The parser just constructs
    /// the variant verbatim; runtime SAP rejects unknown variants.
    #[test]
    fn parse_cli_value_enum_constructs_variant_verbatim() {
        let raw = parse_cli_value("Concise", &ty_enum("Style")).unwrap();
        match raw {
            BexExternalValue::Variant {
                enum_name,
                variant_name,
            } => {
                assert_eq!(enum_name, "Style");
                assert_eq!(variant_name, "Concise");
            }
            other => panic!("got {other:?}"),
        }
    }

    // ── parse_cli_value: complex types rejected (BEP-027 Open Q #5) ────

    /// Lists must be delivered via `--json-args`, not inline in auto-CLI.
    /// The error names `--json-args` so the user knows where to look.
    #[test]
    fn parse_cli_value_list_rejected_with_json_args_hint() {
        let err = parse_cli_value("[1, 2, 3]", &ty_list(ty_int())).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("can't be passed through auto-CLI"),
            "got: {msg}"
        );
        assert!(msg.contains("--json-args"), "got: {msg}");
    }

    /// Classes are the canonical case from BEP-027 Open Q #5.
    #[test]
    fn parse_cli_value_class_rejected_with_json_args_hint() {
        let err = parse_cli_value(r#"{"x": 1}"#, &ty_class("Foo")).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("can't be passed through auto-CLI"),
            "got: {msg}"
        );
        assert!(msg.contains("--json-args"), "got: {msg}");
    }

    /// Media (image / audio / video / pdf) can't survive shell quoting —
    /// previously the `_ =>` catchall silently String-coerced these,
    /// passing a raw filename through to the engine and producing a
    /// confusing downstream type mismatch. Reject up-front with the same
    /// `--json-args` pointer as structured types (BEP-027 Open Q #5).
    #[test]
    fn parse_cli_value_media_rejected_with_json_args_hint() {
        let ty = RuntimeTy::Media(MediaKind::Image, TyAttr::default());
        let err = parse_cli_value("/tmp/cat.png", &ty).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("can't be passed through auto-CLI"),
            "got: {msg}"
        );
        assert!(msg.contains("--json-args"), "got: {msg}");
    }

    /// Engine-internal types (`Function`) aren't valid auto-CLI inputs.
    /// The pre-fix catchall would silently produce `String(raw)` and let
    /// the engine fail later with a type mismatch; we now reject at the
    /// CLI boundary.
    #[test]
    fn parse_cli_value_engine_internal_type_rejected() {
        let ty = RuntimeTy::Function {
            params: Box::new([]),
            ret: Box::new(RuntimeTy::Int {
                attr: TyAttr::default(),
            }),
            throws: Box::new(RuntimeTy::Void {
                attr: TyAttr::default(),
            }),
            attr: TyAttr::default(),
        };
        let err = parse_cli_value("anything", &ty).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("can't be passed through auto-CLI"),
            "got: {msg}"
        );
    }

    // ── is_auto_cli_primitive ────────────────────────────────────────────

    #[test]
    fn is_auto_cli_primitive_recognizes_scalars() {
        assert!(is_auto_cli_primitive(&ty_string()));
        assert!(is_auto_cli_primitive(&ty_int()));
        assert!(is_auto_cli_primitive(&ty_float()));
        assert!(is_auto_cli_primitive(&ty_bool()));
        assert!(is_auto_cli_primitive(&ty_null()));
        assert!(is_auto_cli_primitive(&ty_enum("Color")));
    }

    #[test]
    fn is_auto_cli_primitive_rejects_structural() {
        assert!(!is_auto_cli_primitive(&ty_class("User")));
        assert!(!is_auto_cli_primitive(&ty_list(ty_int())));
    }

    /// `int?` is still a primitive in CLI terms — `--name null` or
    /// `--name 7` both work. `User?` is not, since `User` isn't.
    #[test]
    fn is_auto_cli_primitive_unwraps_optional() {
        assert!(is_auto_cli_primitive(&ty_optional(ty_string())));
        assert!(!is_auto_cli_primitive(&ty_optional(ty_class("User"))));
    }
}
