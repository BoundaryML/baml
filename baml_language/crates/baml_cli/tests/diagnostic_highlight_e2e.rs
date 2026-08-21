mod common;

use std::{path::Path, process::Command};

fn create_project(root: &Path) {
    std::fs::write(
        root.join("baml.toml"),
        "[package]\nname = \"diagnostic-highlight-e2e\"\n",
    )
    .unwrap();
    let source_dir = root.join("baml_src");
    std::fs::create_dir_all(&source_dir).unwrap();
    std::fs::write(
        source_dir.join("main.baml"),
        r#"
client TestClient = openai.ResponsesClient.new(
    model = "gpt-4.1",
    api_key = "test-key",
);

function QuotedPrompt(topic: string) -> string {
    client: TestClient
    prompt: "Interpolation stays literal: ${topic}"
}
"#,
    )
    .unwrap();
}

#[test]
fn forced_color_malformed_diagnostic_fragment_falls_back_and_reports() {
    let cli = common::baml_cli();
    let tmp = tempfile::tempdir().unwrap();
    create_project(tmp.path());

    let home = tmp.path().join(".baml-home");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("config.toml"), "[update]\nauto_check = false\n").unwrap();

    let output = Command::new(cli)
        .args(["check", "--from", "."])
        .current_dir(tmp.path())
        .env("BAML_CLI_ALLOW_DIRECT", "1")
        .env("BAML_COLOR", "always")
        .env("BAML_HOME", home)
        .env("BAML_NO_BYTECODE_CACHE", "1")
        .env("BAML_OUTPUT_PRESET", "human")
        .output()
        .expect("run baml check");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "forced-color check failed with {:?}:\n{stderr}",
        output.status.code()
    );
    assert!(
        stderr.contains("error rendering diagnostics:"),
        "fallback was not reported:\n{stderr}"
    );
    assert!(
        stderr.contains(r#"invalid fragment "${...}""#),
        "fallback did not identify the malformed fragment:\n{stderr}"
    );
    assert!(
        stderr.contains("in a quoted prompt is sent to the model as literal text"),
        "the original diagnostic was not rendered:\n{stderr}"
    );
    assert!(
        !stderr.contains("panicked at"),
        "renderer panicked:\n{stderr}"
    );

    let diagnostic_start = stderr.find("E0010").expect("rendered E0010 code");
    let diagram_end = stderr[diagnostic_start..]
        .find("╰────")
        .map(|offset| diagnostic_start + offset + "╰────".len())
        .expect("rendered E0010 diagram");
    let diagnostic = &stderr[diagnostic_start..diagram_end];
    assert!(
        diagnostic.contains("╭─[main.baml:9:43]"),
        "fallback lost the graphical source layout:\n{diagnostic}"
    );
    assert!(
        diagnostic.contains("literal text here, not an interpolation"),
        "fallback lost the graphical annotation:\n{diagnostic}"
    );
    assert!(
        !diagnostic.contains('\u{1b}'),
        "fallback diagnostic should be pretty but contain no ANSI color:\n{diagnostic:?}"
    );
}
