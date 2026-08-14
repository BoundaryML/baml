use std::{
    io::Write as _,
    path::Path,
    process::{Command, Output, Stdio},
};

fn create_project(root: &Path, source: &str) {
    std::fs::create_dir_all(root.join("baml_src/ns_api")).unwrap();
    std::fs::write(
        root.join("baml.toml"),
        "[package]\nname = \"graphql-fixture\"\n\n[generator.typescript]\noutput_type = \"typescript\"\nnaming_convention = \"preserve-case\"\noutput_dir = \"../generated\"\n",
    )
    .unwrap();
    std::fs::write(root.join("baml_src/main.baml"), source).unwrap();
    std::fs::write(
        root.join("baml_src/ns_api/main.baml"),
        r#"/// Looks up one person.
function Lookup(id: string, tags: string[]) -> string {
  id
}

client Demo = openai.OpenAiClient.new(model = "gpt-4o-mini");

test LookupCase {
  functions [Lookup]
  args {
    id "person-1"
    tags ["one", "two"]
  }
}
"#,
    )
    .unwrap();
}

fn valid_source() -> &'static str {
    r#"/// A person record.
class Person {
  /// Name shown to users.
  name string @alias("full_name")
  tags string[]
}

enum Status {
  Active
  Done
}

type PersonId = string
"#
}

fn run(root: &Path, args: &[&str], stdin: Option<&str>) -> Output {
    let home = root.join(".baml-home");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("config.toml"), "[update]\nauto_check = false\n").unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_baml-cli"));
    command
        .args(args)
        .current_dir(root)
        .env("BAML_CLI_ALLOW_DIRECT", "1")
        .env("BAML_HOME", home)
        .env("BAML_OUTPUT_PRESET", "human")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if stdin.is_some() {
        command.stdin(Stdio::piped());
    }
    let mut child = command.spawn().unwrap();
    if let Some(stdin) = stdin {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
    }
    child.wait_with_output().unwrap()
}

fn json_stdout(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "stdout is not JSON: {error}\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        )
    })
}

#[test]
fn realistic_query_supports_fragments_variables_traversal_and_locations() {
    let project = tempfile::tempdir().unwrap();
    create_project(project.path(), valid_source());
    let query = r#"query Find($name: String!) {
  classes(name: $name) {
    ...ClassDetails
  }
  functions(name: "Lookup") {
    qualifiedName
    parameters { name type { kind display elementType { kind display } } }
    returnType { kind display }
  }
  clients { name qualifiedName }
  generators { name outputType outputDir }
  tests { name qualifiedName functions arguments { name valueJson } }
}
fragment ClassDetails on GraphClass {
  name
  qualifiedName
  documentation
  fields {
    name
    documentation
    attributes { name arguments { value } }
    type { kind display elementType { kind display } }
    location { path startLine startColumn endLine endColumn }
  }
  location { path startLine startColumn endLine endColumn }
}"#;
    let args = [
        "graphql",
        "--query",
        query,
        "--variables",
        r#"{"name":"Person"}"#,
    ];
    let first = run(project.path(), &args, None);
    let second = run(project.path(), &args, None);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert_eq!(
        first.stdout, second.stdout,
        "GraphQL JSON must be byte-stable"
    );
    assert!(
        first.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );

    let response = json_stdout(&first);
    assert_eq!(response["data"]["classes"][0]["name"], "Person");
    assert_eq!(
        response["data"]["classes"][0]["documentation"],
        "A person record."
    );
    assert_eq!(
        response["data"]["classes"][0]["fields"][0]["attributes"][0]["name"],
        "alias"
    );
    assert_eq!(
        response["data"]["classes"][0]["fields"][0]["location"]["path"],
        "baml_src/main.baml"
    );
    assert_eq!(
        response["data"]["classes"][0]["fields"][0]["location"]["startLine"],
        4
    );
    assert_eq!(
        response["data"]["functions"][0]["qualifiedName"],
        "api.Lookup"
    );
    assert_eq!(
        response["data"]["functions"][0]["parameters"][1]["type"]["kind"],
        "LIST"
    );
    assert_eq!(
        response["data"]["functions"][0]["parameters"][1]["type"]["elementType"]["kind"],
        "STRING"
    );
    assert_eq!(response["data"]["clients"][0]["qualifiedName"], "api.Demo");
    assert_eq!(response["data"]["generators"][0]["name"], "typescript");
    assert_eq!(
        response["data"]["tests"][0]["qualifiedName"],
        "api.LookupCase"
    );
}

#[test]
fn query_file_operation_selection_and_empty_results_are_standard_json() {
    let project = tempfile::tempdir().unwrap();
    create_project(project.path(), valid_source());
    let document = project.path().join("query.graphql");
    std::fs::write(
        &document,
        "query Ignored { classes(name: \"Missing\") { name } }\nquery Selected($name: String!) { classes(name: $name) { name } }\n",
    )
    .unwrap();
    let output = run(
        project.path(),
        &[
            "graphql",
            "--query-file",
            document.to_str().unwrap(),
            "--operation",
            "Selected",
            "--variables",
            r#"{"name":"Missing"}"#,
        ],
        None,
    );
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "{\"data\":{\"classes\":[]}}\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn stdin_queries_and_introspection_work_without_a_server() {
    let project = tempfile::tempdir().unwrap();
    create_project(project.path(), valid_source());
    let stdin_output = run(
        project.path(),
        &["graphql"],
        Some("{ definitions(kind: [CLASS, ENUM]) { kind name } }"),
    );
    assert!(stdin_output.status.success());
    let stdin_json = json_stdout(&stdin_output);
    assert_eq!(
        stdin_json["data"]["definitions"].as_array().unwrap().len(),
        2
    );

    let introspection = run(project.path(), &["graphql", "--introspect"], None);
    assert!(introspection.status.success());
    assert_eq!(
        json_stdout(&introspection)["data"]["__schema"]["queryType"]["name"],
        "QueryRoot"
    );

    let schema = run(project.path(), &["graphql", "--schema"], None);
    assert!(schema.status.success());
    let schema = String::from_utf8(schema.stdout).unwrap();
    assert!(schema.contains("type QueryRoot"), "{schema}");
    assert!(schema.contains("type GraphTypeRef"), "{schema}");
}

#[test]
fn invalid_graphql_is_deterministic_standard_error_json() {
    let project = tempfile::tempdir().unwrap();
    create_project(project.path(), valid_source());
    let args = ["graphql", "--query", "{ classes { notAField } }"];
    let first = run(project.path(), &args, None);
    let second = run(project.path(), &args, None);
    assert!(!first.status.success());
    assert_eq!(first.stdout, second.stdout);
    assert!(first.stderr.is_empty());
    let response = json_stdout(&first);
    assert!(response.get("data").is_none());
    assert!(
        response["errors"][0]["message"]
            .as_str()
            .unwrap()
            .contains("notAField")
    );
}

#[test]
fn invalid_baml_is_structured_and_deterministic() {
    let project = tempfile::tempdir().unwrap();
    create_project(project.path(), "class Broken {\n  value MissingType\n}\n");
    let args = ["graphql", "--query", "{ classes { name } }"];
    let first = run(project.path(), &args, None);
    let second = run(project.path(), &args, None);
    assert!(!first.status.success());
    assert_eq!(first.stdout, second.stdout);
    assert!(first.stderr.is_empty());
    let response = json_stdout(&first);
    assert_eq!(
        response["errors"][0]["extensions"]["code"],
        "BAML_VALIDATION_FAILED"
    );
    assert_eq!(
        response["errors"][0]["extensions"]["diagnostics"][0]["location"]["path"],
        "baml_src/main.baml"
    );
    assert_eq!(
        response["errors"][0]["extensions"]["diagnostics"][0]["location"]["startLine"],
        1
    );
}
