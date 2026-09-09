//! The book's foreign webhook must be checked with a distinct package identity.
use baml_tests::{
    engine::TestDbExt,
    stdlib_prefix::{assert_no_user_diagnostic_errors, check_user_files, setup_test_db},
};

const WEBHOOK: &str = r#"
class WebhookNotifier { url: string, }
"#;

fn check_fixture(source: &str) -> baml_db::ProjectDatabase {
    let mut db = setup_test_db("");
    db.dependency("webhooks");
    db.file("<builtin>/webhooks/main.baml", WEBHOOK);
    db.file("main.baml", source);
    db
}

#[tokio::test]
async fn local_notifier_can_be_implemented_for_foreign_webhook() {
    let db = check_fixture(
        r#"
interface Notifier { function send(self, message: string) -> string throws never }
implements Notifier for webhooks.WebhookNotifier {
    function send(self, message: string) -> string {
        `webhook to ${self.url}: ${message}`
    }
}
function notify(notifier: Notifier, message: string) -> string throws never {
    notifier.send(message)
}
function main() -> string throws never {
    notify(webhooks.WebhookNotifier { url: "https://example.com/hooks/deploy" }, "Build passed")
}
"#,
    );
    assert_no_user_diagnostic_errors(&db);
    let program = baml_compiler2_emit::generate_project_bytecode_with_opt(
        &db,
        baml_compiler2_emit::OptLevel::One,
    )
    .expect("cross-package notifier compiles");
    let output = baml_tests::engine::run_compiled(program, "main", Default::default(), false).await;
    assert_eq!(
        output.result.expect("foreign interface dispatch succeeds"),
        bex_engine::BexExternalValue::String(
            "webhook to https://example.com/hooks/deploy: Build passed".into()
        )
    );
}

#[test]
fn foreign_field_mapping_requires_the_class_owner() {
    let db = check_fixture(
        r#"
interface Destination { destination: string }
implements Destination for webhooks.WebhookNotifier { destination as url }
"#,
    );
    let errors = check_user_files(&db);
    assert!(
        errors
            .iter()
            .any(|d| d.message_with_primary_label().contains("field")),
        "{errors:?}"
    );
    assert!(errors.iter().any(|d| d.code() == "E0126"), "{errors:?}");
}

#[test]
fn foreign_interface_and_foreign_receiver_violate_local_ownership() {
    let db = check_fixture(
        r#"
implements baml.ToString for webhooks.WebhookNotifier {
    function to_string(self) -> string { self.url }
}
"#,
    );
    let errors = check_user_files(&db);
    assert!(errors.iter().any(|d| d.code() == "E0139"), "{errors:?}");
}
