//! Proof-of-concept tests for the `baml.ToString` interface and the
//! `string.from<T>` driver that replaces the hardcoded to_string magic.
//!
//! `string.from(value)` resolves `to_string` on `value`'s *runtime* class:
//!   - if that class has an `implements baml.ToString` override, dispatch to it;
//!   - otherwise render structurally via `baml._to_string_default`, which is
//!     also the interface's default `to_string` body.
//!
//! Runtime-class dispatch (`baml._to_string_shim`) is used instead of a static
//! `match (value) { baml.ToString => ... }` because an interface match in the
//! stdlib package can't see downstream user implementors — see `string.from`.

/// Compile errors raised in the user file, as `[CODE] message`.
fn compile_errors(source: &str) -> Vec<String> {
    use baml_compiler_diagnostics::Severity;
    use baml_project::{collect_diagnostics, testing::setup_test_db};
    let db = setup_test_db(source);
    collect_diagnostics(&db)
        .into_iter()
        .filter(|d| matches!(d.severity, Severity::Error))
        .map(|d| format!("[{}] {}", d.code(), d.message_with_primary_label()))
        .collect()
}

#[test]
fn direct_to_string_method_is_banned() {
    let errors = compile_errors(
        r#"
        class Point {
            x int
            function to_string(self) -> string throws never { "p" }
        }
    "#,
    );
    assert!(
        errors
            .iter()
            .any(|e| e.contains("to_string") && e.contains("baml.ToString")),
        "expected a to_string ban error recommending baml.ToString; got:\n  {}",
        errors.join("\n  ")
    );
}

#[test]
fn to_string_via_interface_is_allowed() {
    // The same method inside an `implements baml.ToString` block is fine.
    let errors = compile_errors(
        r#"
        class Point {
            x int
            implements baml.ToString {
                function to_string(self) -> string throws never { "p" }
            }
        }
    "#,
    );
    assert!(
        !errors.iter().any(|e| e.contains("to_string")),
        "implementing baml.ToString must not be flagged; got:\n  {}",
        errors.join("\n  ")
    );
}
