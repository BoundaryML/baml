use std::{path::Path, process::Command};

use baml_db::{ProjectDatabase, collect_compiler2_diagnostics};
use baml_tests::engine::TestDbExt;

const CHILD_TEST_NAME: &str = "incremental_function_typing_repro_child";
const INCOMPLETE_LOG_CHILD_TEST_NAME: &str = "incremental_incomplete_log_repro_child";

fn run_incremental_repro_sequence() {
    let mut db = ProjectDatabase::new();
    let root = Path::new("/repro");
    let file = Path::new("/repro/repro.baml");
    db.workspace(root);

    let prefix = r#"
function Existing() -> string {
  "ok"
}

"#;
    let typed = "function display";
    let suffix = "\n";

    for i in 0..=typed.len() {
        let current = format!("{prefix}{}{}", &typed[..i], suffix);
        eprintln!("repro step {i}: `{}`", &typed[..i]);
        db.file(file, &current);

        // This is the narrowest known path that reproduces the crash from
        // `textDocument/didChange`: mutate the same DB incrementally and then
        // force compiler2 diagnostics to re-run.
        let _ = collect_compiler2_diagnostics(&db);
    }
}

#[test]
fn incremental_function_typing_repro_stays_alive() {
    // Keep the actual repro sequence in a subprocess so future abort
    // regressions don't take down the main test runner during `cargo test`.
    let output = Command::new(std::env::current_exe().expect("current test binary"))
        .arg("--ignored")
        .arg("--exact")
        .arg(CHILD_TEST_NAME)
        .arg("--nocapture")
        .output()
        .expect("spawn child test process");

    assert!(
        output.status.success(),
        "expected child repro to complete successfully\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
#[ignore = "Executed by incremental_function_typing_repro_stays_alive in a subprocess"]
fn incremental_function_typing_repro_child() {
    run_incremental_repro_sequence();
}

fn run_incomplete_log_repro_sequence() {
    let mut db = ProjectDatabase::new();
    let root = Path::new("/repro-incomplete-log");
    let file = Path::new("/repro-incomplete-log/main.baml");
    db.workspace(root);

    let source = r##"
client GPT4o {
  provider openai
  options {
    model "gpt-5"
    api_key env.OPENAI_API_KEY
  }
}

class GuessResponse {
  game_won bool
  text string
}

function GenerateFamousPersonName(previous_names: string[]) -> string {
  client: GPT4o
  prompt: `
    ${previous_names}
  `
}

function SimulateHumanGuess(history: string[]) -> string {
  client: GPT4o
  prompt: `
    ${history}
  `
}

function TakeGuess(user_guess: string, famous_person_name: string, history: string[]) -> GuessResponse {
  client: GPT4o
  prompt: `
    ${user_guess}
    ${famous_person_name}
    ${history}
    ${ctx.output_format()}
  `
}

function GuessGameAgent() -> GuessResponse {
  let history: string[] = []
  let famous_person_name = GenerateFamousPersonName([])
  log.info({"famous_person_name":
  let user_input = SimulateHumanGuess(history)
  let guess_response = TakeGuess(user_input, famous_person_name, history)
  guess_response
}
"##;

    db.file(file, source);
    let _ = collect_compiler2_diagnostics(&db);
}

#[test]
fn incremental_incomplete_log_repro_stays_alive() {
    let output = Command::new(std::env::current_exe().expect("current test binary"))
        .arg("--ignored")
        .arg("--exact")
        .arg(INCOMPLETE_LOG_CHILD_TEST_NAME)
        .arg("--nocapture")
        .output()
        .expect("spawn child test process");

    assert!(
        output.status.success(),
        "expected child repro to complete successfully\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
#[ignore = "Executed by incremental_incomplete_log_repro_stays_alive in a subprocess"]
fn incremental_incomplete_log_repro_child() {
    run_incomplete_log_repro_sequence();
}

#[test]
fn incremental_map_key_expression_change_invalidates_inference() {
    let mut db = ProjectDatabase::new();
    let root = Path::new("/map-key-expression");
    let file = Path::new("/map-key-expression/main.baml");
    db.workspace(root);

    db.file(
        file,
        r#"
function build() -> map<string, string> {
  { key: "value" }
}
"#,
    );
    let expression_messages: Vec<_> = collect_compiler2_diagnostics(&db)
        .into_iter()
        .filter(|diagnostic| diagnostic.severity == baml_compiler_diagnostics::Severity::Error)
        .map(|diagnostic| diagnostic.message)
        .collect();
    assert!(
        expression_messages
            .iter()
            .any(|message| message.contains("unresolved name: `key`")),
        "expected unresolved key diagnostic, got: {expression_messages:#?}"
    );

    db.file(
        file,
        r#"
function build() -> map<string, string> {
  { "key": "value" }
}
"#,
    );
    let quoted_messages: Vec<_> = collect_compiler2_diagnostics(&db)
        .into_iter()
        .filter(|diagnostic| diagnostic.severity == baml_compiler_diagnostics::Severity::Error)
        .map(|diagnostic| diagnostic.message)
        .collect();
    assert!(
        quoted_messages.is_empty(),
        "quoted key reused stale expression inference: {quoted_messages:#?}"
    );

    db.file(
        file,
        r#"
function build() -> map<string, string> {
  { key: "value" }
}
"#,
    );
    let restored_messages: Vec<_> = collect_compiler2_diagnostics(&db)
        .into_iter()
        .filter(|diagnostic| diagnostic.severity == baml_compiler_diagnostics::Severity::Error)
        .map(|diagnostic| diagnostic.message)
        .collect();
    assert!(
        restored_messages
            .iter()
            .any(|message| message.contains("unresolved name: `key`")),
        "expression key reused stale quoted inference: {restored_messages:#?}"
    );
}
