//! `obj.to_string()` sugar: a 0-arg `to_string` call on a value whose type has no
//! real `to_string` method lowers to `string.from(obj)`, so any value can be
//! rendered with `.to_string()` regardless of whether its type implements
//! `baml.ToString`.

/// Compile errors raised in the user file, as `[CODE] message`.
fn compile_errors(source: &str) -> Vec<String> {
    use baml_compiler_diagnostics::Severity;
    use baml_db::{collect_diagnostics, testing::setup_test_db};
    let db = setup_test_db(source);
    collect_diagnostics(&db)
        .into_iter()
        .filter(|d| matches!(d.severity, Severity::Error))
        .map(|d| format!("[{}] {}", d.code(), d.message_with_primary_label()))
        .collect()
}

#[test]
fn receiver_error_is_not_swallowed() {
    // The fallback rolls back only its own `to_string` member error; a real error
    // in the receiver expression (here: a bad argument) must still be reported.
    let errors = compile_errors(
        r#"
        function takes_int(n: int) -> int { n }
        function main() -> string {
            return takes_int("not an int").to_string()
        }
    "#,
    );
    assert!(
        errors.iter().any(|e| e.contains("not an int")
            || e.to_lowercase().contains("type mismatch")
            || e.contains("int")),
        "expected the bad-argument error to survive the to_string fallback; got:\n  {}",
        errors.join("\n  ")
    );
}
