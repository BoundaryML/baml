//! Compile-error regressions for invalid BAML string literals.

#[cfg(test)]
mod tests {
    #[test]
    fn invalid_strings_still_produce_errors() {
        use baml_compiler_diagnostics::Severity;
        use baml_project::{collect_diagnostics, testing::setup_test_db};

        let cases = [
            (
                r#"function f() -> string { "hello }"#,
                "unterminated string",
            ),
            (r#"function f() -> string { "hello\"#, "backslash at EOF"),
            (
                r#"function f() -> string { "hello\"}"#,
                "escaped quote eats closing quote",
            ),
        ];

        for (source, label) in &cases {
            let db = setup_test_db(source);
            let diagnostics = collect_diagnostics(&db);
            let has_error = diagnostics
                .iter()
                .any(|d| matches!(d.severity, Severity::Error));
            assert!(
                has_error,
                "Expected compilation error for case '{}', but got none.\nDiagnostics: {:?}",
                label,
                diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
            );
        }
    }
}
