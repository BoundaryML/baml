use std::{path::Path, process::Command};

use baml_project::{ProjectDatabase, collect_compiler2_diagnostics};

const CHILD_TEST_NAME: &str = "incremental_function_typing_repro_child";

fn run_incremental_repro_sequence() {
    let mut db = ProjectDatabase::new();
    let root = Path::new("/repro");
    let file = Path::new("/repro/repro.baml");
    db.set_project_root(root);

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
        db.add_or_update_file(file, &current);

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
