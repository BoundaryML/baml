use baml_compiler_emit::{LoweringError, compile_files};
use baml_tests::bytecode::setup_test_db;

#[test]
fn invalid_retry_policy_multiplier_returns_error() {
    let source = r#"
retry_policy Bad {
    max_retries 2
    strategy {
        type exponential_backoff
        delay_ms 100
        multiplier nope
        max_delay_ms 500
    }
}
"#;

    let db = setup_test_db(source);
    let project = db.get_project().expect("project should be initialized");
    let files = project.files(&db).clone();

    let err = compile_files(&db, &files).expect_err("invalid retry_policy should fail compile");

    match err {
        LoweringError::InvalidRetryPolicyValue {
            policy_name,
            field_name,
            value,
            ..
        } => {
            assert_eq!(policy_name, "Bad");
            assert_eq!(field_name, "multiplier");
            assert_eq!(value, "nope");
        }
        other => panic!("expected InvalidRetryPolicyValue, got {other:?}"),
    }
}
