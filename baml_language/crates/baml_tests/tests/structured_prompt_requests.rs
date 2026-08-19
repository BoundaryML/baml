//! Provider request builders consume the structural `ai.Prompt` produced by an
//! LLM spec. The offline request-shape suite lives in
//! `baml_src/ns_structured_prompt_requests/` as native BAML tests; this file
//! keeps only the test that must control the host environment (a late-bound
//! `env.NAME` reference resolved during preview), which needs a re-exec'd
//! child process for isolation.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn vertex_project_id_accepts_late_bound_env_ref_in_preview() {
    const CHILD_MARKER: &str = "BAML_VERTEX_PROJECT_PREVIEW_CHILD";
    if std::env::var(CHILD_MARKER).as_deref() != Ok("1") {
        let status = std::process::Command::new(
            std::env::current_exe().expect("test binary path should be available"),
        )
        .arg("--exact")
        .arg("vertex_project_id_accepts_late_bound_env_ref_in_preview")
        .arg("--nocapture")
        .env(CHILD_MARKER, "1")
        .env("VERTEX_PROJECT_ID", "test-project")
        .status()
        .expect("isolated preview test process should start");
        assert!(status.success(), "isolated preview test failed: {status}");
        return;
    }

    let source = r#"
client Vertex = google.VertexClient.new(
  model = "gemini-test",
  project_id = env.VERTEX_PROJECT_ID,
  location = "us-central1",
  api_key = "test-key",
  headers = { "x-preview": "yes" },
  query_params = { "trace": "enabled" },
  request_body = baml.json.parse(`{"preview_marker":"kept"}`),
)

function Shape() -> string {
  client: Vertex
  prompt: `hello`
}

function main() -> string {
  let expected = env.VERTEX_PROJECT_ID.get_or_panic()
  let input = ai.ModelTurnInput {
    prompt: Shape$spec().prompt_template,
    journal: ai.Journal { log: [] },
    toolbox: ai.tools.Toolbox.new([]),
    output_type: type.of<string>(),
  }
  let c: ai.Client = Vertex
  let request = c.render(input)
  let body = baml.json.parse(request.body)
  let base_client = google.VertexClient.new(
    model = "gemini-override",
    base_url = "https://preview.example/v1/models",
    api_key = "override-key",
    headers = { "x-base": "yes" },
    query_params = { "q": "one" },
    request_body = baml.json.parse(`{"base_marker":"kept"}`),
  )
  let base_request: ai.Client = base_client
  let override = base_request.render(input)
  let override_body = baml.json.parse(override.body)
  if (
    request.url.includes(expected)
    && request.url.includes("/locations/us-central1/")
    && request.url.includes("trace=enabled")
    && request.headers.get("x-preview") == "yes"
    && baml.json.path<string>(body, ".preview_marker") == "kept"
    && override.url.includes("https://preview.example/v1/models/gemini-override")
    && override.url.includes("q=one")
    && override.headers.get("x-base") == "yes"
    && baml.json.path<string>(override_body, ".base_marker") == "kept"
  ) { "ok" } else { "wrong" }
}
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("ok".to_string().into()))
    );
}
