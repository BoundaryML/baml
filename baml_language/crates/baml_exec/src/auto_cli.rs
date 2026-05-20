// Type-driven coercion for individual auto-CLI parameter values.
//
// The argv-level parser lives in `clap_target`; this module owns the
// per-value coercion step (`"42" + Ty::Int` -> `BexExternalValue::Int(42)`).
// Split this way because the clap path and the (much) simpler "single
// raw token" path both need the same semantic conversion, but only the
// clap path needs full argv parsing.

use anyhow::{Context, Result};
use bex_engine::{BexExternalValue, Ty};

/// Whether a parameter of type `ty` can be passed directly as a
/// `--name value` flag via auto-CLI. Mirrors the accepted arms of
/// [`parse_cli_value`] — keep the two in sync. Anything that returns
/// `false` must arrive via `--json-args` (or in a mixed call, can come
/// through either, since `--json-args` is universal).
///
/// `Optional<T>` unwraps to the inner type — `string?` is still a
/// primitive (the user just passes `--name null` or the value).
pub fn is_auto_cli_primitive(ty: &Ty) -> bool {
    match ty {
        Ty::String { .. }
        | Ty::Int { .. }
        | Ty::Float { .. }
        | Ty::Bool { .. }
        | Ty::Null { .. }
        | Ty::Enum(..) => true,
        Ty::Optional(inner, _) => is_auto_cli_primitive(inner),
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
pub fn parse_cli_value(raw: &str, ty: &Ty) -> Result<BexExternalValue> {
    match ty {
        Ty::String { .. } => Ok(BexExternalValue::String(raw.to_string())),

        Ty::Int { .. } => {
            let v: i64 = raw
                .parse()
                .with_context(|| format!("Expected integer, got `{raw}`"))?;
            Ok(BexExternalValue::Int(v))
        }

        Ty::Float { .. } => {
            let v: f64 = raw
                .parse()
                .with_context(|| format!("Expected float, got `{raw}`"))?;
            Ok(BexExternalValue::Float(v))
        }

        Ty::Bool { .. } => match raw {
            "true" => Ok(BexExternalValue::Bool(true)),
            "false" => Ok(BexExternalValue::Bool(false)),
            _ => anyhow::bail!("Expected `true` or `false`, got `{raw}`"),
        },

        Ty::Null { .. } => {
            if raw == "null" {
                Ok(BexExternalValue::Null)
            } else {
                anyhow::bail!("Expected `null`, got `{raw}`")
            }
        }

        Ty::Optional(inner, _) => {
            if raw == "null" {
                Ok(BexExternalValue::Null)
            } else {
                parse_cli_value(raw, inner)
            }
        }

        Ty::Enum(type_name, _) => Ok(BexExternalValue::Variant {
            enum_name: type_name.display_name.to_string(),
            variant_name: raw.to_string(),
        }),

        // Per BEP-027 §"Open questions" #5: anything auto-CLI can't
        // faithfully represent must be delivered via `--json-args`.
        // Structured types (class/list/map/union) are the obvious case;
        // media, literals, type aliases, opaque types, and the engine-
        // internal types (function/void/watch/future) fall in the same
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

    fn ty_string() -> Ty {
        Ty::String {
            attr: TyAttr::default(),
        }
    }
    fn ty_int() -> Ty {
        Ty::Int {
            attr: TyAttr::default(),
        }
    }
    fn ty_bool() -> Ty {
        Ty::Bool {
            attr: TyAttr::default(),
        }
    }
    fn ty_float() -> Ty {
        Ty::Float {
            attr: TyAttr::default(),
        }
    }
    fn ty_null() -> Ty {
        Ty::Null {
            attr: TyAttr::default(),
        }
    }
    fn ty_optional(inner: Ty) -> Ty {
        Ty::Optional(Box::new(inner), TyAttr::default())
    }
    fn ty_enum(name: &str) -> Ty {
        Ty::Enum(
            TypeName {
                name: name.into(),
                module_path: vec![],
                display_name: name.into(),
            },
            TyAttr::default(),
        )
    }
    fn ty_list(elem: Ty) -> Ty {
        Ty::List(Box::new(elem), TyAttr::default())
    }
    fn ty_class(name: &str) -> Ty {
        Ty::Class(
            TypeName {
                name: name.into(),
                module_path: vec![],
                display_name: name.into(),
            },
            vec![],
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
        let ty = Ty::Media(MediaKind::Image, TyAttr::default());
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
        let ty = Ty::Function {
            params: vec![],
            ret: Box::new(Ty::Int {
                attr: TyAttr::default(),
            }),
            throws: Box::new(Ty::Void {
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
