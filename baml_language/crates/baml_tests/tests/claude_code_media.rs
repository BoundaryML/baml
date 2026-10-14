//! The Claude Code client's prompt transport, observed at the process
//! boundary: every prompt, text-only or not, goes to the CLI as one
//! stream-json message on stdin. BAML cannot see a child's argv or stdin, so a
//! fake `claude` records both and the assertions run here.
//!
//! The fake fills stdout past any pipe buffer before it reads stdin, so a
//! client that wrote all of stdin before draining stdout would deadlock until
//! its timeout instead of passing.
#![cfg(unix)]

use std::{os::unix::fs::PermissionsExt as _, path::Path};

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

const FAKE_CLAUDE: &str = r#"#!/bin/sh
dir=$(dirname "$0")
for arg in "$@"; do printf '%s\0' "$arg"; done > "$dir/argv"
pad=$(head -c 70000 /dev/zero | tr '\0' x)
for _ in 1 2 3 4; do printf '{"type":"system","subtype":"pad","pad":"%s"}\n' "$pad"; done
cat > "$dir/stdin"
printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"structured_output":"seen","usage":{"input_tokens":1,"output_tokens":1}}'
"#;

/// Answers, closes stdout, and lingers without ever reading stdin.
const ANSWERS_WITHOUT_READING: &str = r#"#!/bin/sh
printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"structured_output":"seen","usage":{"input_tokens":1,"output_tokens":1}}'
exec 1>&-
sleep 1
"#;

const CLIENT: &str = r#"
    function invoke_with(executable: string, input: ai.ModelTurnInput) -> string {
        let cl = claude_code.ClaudeCodeClient.new(
            model = "offline-transport-probe",
            executable = executable,
            timeout_ms = 30000,
        );
        let turn = cl.invoke(input) catch_all (e) {
            _ => { return `unexpected: ${e.to_string()}`; },
        };
        match (turn.content[0]) {
            let t: ai.content.Text => t.text,
            _ => "non-text",
        }
    }

    function Look(picture: image) -> string {
        client: "claude-code/haiku"
        prompt: `
          ${role("user")}
          Look at ${picture}
        `
    }

    function input_of(spec: ai.FunctionSpec<string>) -> ai.ModelTurnInput {
        ai.ModelTurnInput {
            prompt: spec.prompt_template,
            journal: ai.Journal.new(spec),
            toolbox: spec.tools(),
            output_type: spec.output_type(),
        }
    }
"#;

fn install_fake_claude(dir: &Path, body: &str) -> String {
    let script = dir.join("claude");
    std::fs::write(&script, body).expect("write fake claude");
    let mut permissions = std::fs::metadata(&script)
        .expect("stat fake claude")
        .permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&script, permissions).expect("make fake claude executable");
    script.to_string_lossy().into_owned()
}

fn recorded_argv(dir: &Path) -> Vec<String> {
    let raw = std::fs::read(dir.join("argv")).expect("fake claude recorded argv");
    raw.split(|byte| *byte == 0)
        .filter(|arg| !arg.is_empty())
        .map(|arg| String::from_utf8(arg.to_vec()).expect("argv is UTF-8"))
        .collect()
}

fn recorded_stdin(dir: &Path) -> String {
    std::fs::read_to_string(dir.join("stdin")).expect("fake claude recorded stdin")
}

/// Asserts argv selects stream-json input and carries no positional prompt:
/// the schema value is the last argument.
fn assert_prompt_left_argv(argv: &[String]) {
    assert!(
        argv.windows(2)
            .any(|pair| pair == ["--input-format", "stream-json"]),
        "argv selects stream-json input: {argv:?}"
    );
    assert_eq!(
        argv.get(argv.len().wrapping_sub(2)).map(String::as_str),
        Some("--json-schema"),
        "no prompt trails argv: {argv:?}"
    );
}

/// The one stream-json line the fake recorded on stdin, parsed.
fn recorded_message(dir: &Path) -> serde_json::Value {
    let stdin = recorded_stdin(dir);
    assert_eq!(stdin.matches('\n').count(), 1, "one stream-json line");
    let message: serde_json::Value = serde_json::from_str(stdin.trim()).expect("stdin is JSON");
    assert_eq!(message["type"], "user");
    assert_eq!(message["message"]["role"], "user");
    message
}

#[tokio::test]
async fn image_prompt_goes_to_stdin_as_stream_json() {
    let temp = tempfile::tempdir().expect("tempdir for fake claude");
    let executable = install_fake_claude(temp.path(), FAKE_CLAUDE);
    // 1.5 MB of base64: past macOS's 1 MiB argv limit and every pipe buffer.
    let data = "QUJD".repeat(384 * 1024);

    let output = baml_test! {
        baml: &format!(r#"
            {CLIENT}

            function main(executable: string, data: string) -> string {{
                invoke_with(
                    executable,
                    input_of(Look@spec(baml.media.Image.from_base64(data, "image/png"))),
                )
            }}
        "#),
        args: {
            "executable" => BexExternalValue::String(executable.into()),
            "data" => BexExternalValue::String(data.clone().into()),
        },
    };
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("\"seen\"".into()))
    );

    assert_prompt_left_argv(&recorded_argv(temp.path()));
    let message = recorded_message(temp.path());
    let content = &message["message"]["content"];
    assert_eq!(content[0]["type"], "text");
    assert_eq!(content[1]["type"], "image");
    assert_eq!(content[1]["source"]["type"], "base64");
    assert_eq!(content[1]["source"]["media_type"], "image/png");
    assert_eq!(content[1]["source"]["data"], data.as_str());
}

/// Text-only prompts take the same path: argv would cap them (128 KiB per
/// argument on Linux, 1 MiB in total on macOS).
#[tokio::test]
async fn text_prompt_goes_to_stdin_as_stream_json() {
    let temp = tempfile::tempdir().expect("tempdir for fake claude");
    let executable = install_fake_claude(temp.path(), FAKE_CLAUDE);
    // 1.5 MB of text: past both argv limits and every pipe buffer.
    let name = "Ada".repeat(512 * 1024);

    let output = baml_test! {
        baml: &format!(r#"
            {CLIENT}

            function Greet(name: string) -> string {{
                client: "claude-code/haiku"
                prompt: `
                  ${{role("user")}}
                  Greet ${{name}}
                `
            }}

            function main(executable: string, name: string) -> string {{
                invoke_with(executable, input_of(Greet@spec(name)))
            }}
        "#),
        args: {
            "executable" => BexExternalValue::String(executable.into()),
            "name" => BexExternalValue::String(name.clone().into()),
        },
    };
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("\"seen\"".into()))
    );

    assert_prompt_left_argv(&recorded_argv(temp.path()));
    let message = recorded_message(temp.path());
    let content = message["message"]["content"]
        .as_array()
        .expect("content is an array");
    assert_eq!(content.len(), 1, "one text block");
    assert_eq!(content[0]["type"], "text");
    assert_eq!(
        content[0]["text"].as_str(),
        Some(format!("[user]\nGreet {name}").as_str())
    );
}

/// A CLI that answers without reading its stdin never saw the image, so its
/// answer must not be accepted.
#[tokio::test]
async fn image_prompt_fails_when_the_cli_never_reads_it() {
    let temp = tempfile::tempdir().expect("tempdir for fake claude");
    let executable = install_fake_claude(temp.path(), ANSWERS_WITHOUT_READING);
    let data = "QUJD".repeat(384 * 1024);

    let output = baml_test! {
        baml: &format!(r#"
            {CLIENT}

            function main(executable: string, data: string) -> string {{
                invoke_with(
                    executable,
                    input_of(Look@spec(baml.media.Image.from_base64(data, "image/png"))),
                )
            }}
        "#),
        args: {
            "executable" => BexExternalValue::String(executable.into()),
            "data" => BexExternalValue::String(data.into()),
        },
    };
    let Ok(BexExternalValue::String(result)) = output.result else {
        panic!("expected a string result, got {:?}", output.result);
    };
    assert!(result.contains("could not receive the prompt"), "{result}");
}
