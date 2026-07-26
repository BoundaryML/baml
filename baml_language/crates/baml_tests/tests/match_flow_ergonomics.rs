use baml_compiler_diagnostics::{DiagnosticId, Severity};
use baml_project::{collect_diagnostics, testing::setup_test_db};

fn errors(source: &str) -> Vec<baml_compiler_diagnostics::Diagnostic> {
    let db = setup_test_db(source);
    collect_diagnostics(&db)
        .into_iter()
        .filter(|diag| matches!(diag.severity, Severity::Error))
        .collect()
}

#[test]
fn wildcard_match_arm_uses_residual_scrutinee_type() {
    let source = r#"
        function main() -> int {
            let v: int | null = 5;
            match (v) {
                null => 0,
                _ => v + 1,
            }
        }
    "#;
    assert!(errors(source).is_empty());
}

#[test]
fn guarded_match_arm_does_not_consume_residual_type() {
    let source = r#"
        function main(flag: bool) -> int {
            let v: int | null = 5;
            match (v) {
                null if flag => 0,
                _ => v + 1,
            }
        }
    "#;
    assert!(
        errors(source)
            .iter()
            .any(|diag| diag.id == DiagnosticId::InvalidOperator),
        "a guarded null arm must not remove null from the wildcard residual"
    );
}

#[test]
fn structural_match_arm_does_not_consume_its_whole_type_member() {
    let source = r#"
        class Box {
            flag bool
        }

        function main(value: Box | null) -> int {
            match (value) {
                Box { flag: true } => 0,
                _ => match (value) {
                    Box {} => 1,
                    null => 2,
                },
            }
        }
    "#;
    assert!(
        errors(source).is_empty(),
        "a field-constrained class arm must leave other class values in the residual"
    );
}

#[test]
fn logical_and_narrows_rhs_and_then_branch() {
    let source = r#"
        function main() -> int {
            let f: int? = 1;
            if (f != null && f > 0) {
                f + 1
            } else {
                0
            }
        }
    "#;
    assert!(errors(source).is_empty());
}

#[test]
fn logical_or_narrows_rhs_and_else_branch() {
    let source = r#"
        function main() -> int {
            let f: int? = 1;
            if (f == null || f > 0) {
                0
            } else {
                f + 1
            }
        }
    "#;
    assert!(errors(source).is_empty());
}

#[test]
fn logical_and_false_branch_stays_conservative() {
    let source = r#"
        function main() -> int {
            let f: int? = 1;
            if (f != null && f > 0) {
                0
            } else {
                f + 1
            }
        }
    "#;
    assert!(
        errors(source)
            .iter()
            .any(|diag| diag.id == DiagnosticId::InvalidOperator),
        "a false `&&` result does not prove that the optional is non-null"
    );
}

#[test]
fn logical_or_true_branch_stays_conservative() {
    let source = r#"
        function main() -> int {
            let f: int? = 1;
            if (f == null || f > 0) {
                f + 1
            } else {
                0
            }
        }
    "#;
    assert!(
        errors(source)
            .iter()
            .any(|diag| diag.id == DiagnosticId::InvalidOperator),
        "a true `||` result does not prove that the optional is non-null"
    );
}

#[test]
fn typed_wildcard_binding_discards_the_name() {
    let source = r#"
        function main() -> int {
            let x: int = 5;
            match (x) {
                let _: int => 0,
            }
        }
    "#;
    assert!(errors(source).is_empty());
}

#[test]
fn bare_identifier_match_pattern_suggests_binding_syntax() {
    let source = r#"
        function main() -> int {
            let v: int = 5;
            match (v) {
                x => x,
            }
        }
    "#;
    let diagnostics = errors(source);
    let unresolved_type = diagnostics
        .iter()
        .find(|diag| diag.id == DiagnosticId::UnknownType)
        .expect("expected an unresolved-type diagnostic");
    assert!(
        unresolved_type.message.contains("let x: T =>"),
        "expected binding syntax hint, got: {}",
        unresolved_type.message
    );
}

#[test]
fn unresolved_is_pattern_does_not_suggest_a_match_binding() {
    let source = r#"
        function main() -> bool {
            let v: int = 5;
            v is Missing
        }
    "#;
    let diagnostics = errors(source);
    let unresolved_type = diagnostics
        .iter()
        .find(|diag| diag.id == DiagnosticId::UnknownType)
        .expect("expected an unresolved-type diagnostic");
    assert!(
        !unresolved_type.message.contains("let Missing: T =>"),
        "the binding hint is specific to match arms: {}",
        unresolved_type.message
    );
}

#[test]
fn braceless_return_match_arm_is_accepted() {
    let source = r#"
        function main() -> int {
            let x: int = 5;
            match (x) {
                5 => return 1,
                _ => 0,
            }
        }
    "#;
    assert!(errors(source).is_empty());
}
