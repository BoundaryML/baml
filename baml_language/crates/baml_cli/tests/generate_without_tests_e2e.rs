//! Production generation must drop test subtrees before serialization.
mod common;

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::Arc,
};

use bex_engine::{BexExternalValue, CallId, CancellationToken, FunctionCallContextBuilder};
use bex_vm_types::Program;
use sys_native::SysOpsExt as _;

const FUNCTIONS: &str = r#"
function Halve(n: int) -> int {
    let outer = (x: int) -> int {
        let inner = (y: int) -> int { y / 2 }
        inner(x)
    };
    outer(n)
}

function Recover() -> string {
    (throw "production throw") catch (e) {
        _ => "production recovery"
    }
}
"#;

const TESTS: &str = r#"
test "ONLY_TEST_5246_inline" {
    let outer = () -> string {
        let inner = () -> string { "ONLY_TEST_5246_nested_lambda" }
        inner()
    };
    assert.equal(outer(), "ONLY_TEST_5246_nested_lambda");
    assert.equal(Halve(4), 2);
    let caught = (throw "ONLY_TEST_5246_throw") catch (e) {
        _ => "ONLY_TEST_5246_catch_lambda"
    };
    assert.equal(caught, "ONLY_TEST_5246_catch_lambda");
}

testset "ONLY_TEST_5246_suite" {
    let setup = "ONLY_TEST_5246_setup";
    test "leaf" { assert.equal(setup, "ONLY_TEST_5246_setup") }
    testset "nested" {
        test "deep" {
            let nested = () -> int { Halve(8) };
            let caught = (throw "ONLY_TEST_5246_deep_throw") catch (e) {
                _ => nested()
            };
            assert.equal(caught, 4);
        }
    }
}
"#;

/// Run a fixture command without telemetry or passive agent checks.
fn cli(project: &Path, args: &[&str]) -> Output {
    Command::new(common::baml_cli())
        .args(["--agent-skill-check", "off"])
        .args(args)
        .current_dir(project)
        .env("BAML_CLI_ALLOW_DIRECT", "1")
        .env("BAML_HOME", common::shared_baml_home())
        .env("BAML_TELEMETRY", "off")
        .env("DEV_BAML_CLI_DISABLE_AGENT_DETECTION", "1")
        .env("DO_NOT_TRACK", "1")
        .env_remove("BOUNDARY_API_KEY")
        .env_remove("BOUNDARY_PROJECT")
        .output()
        .expect("spawn baml-cli")
}

/// Report CLI output when a fixture command fails.
fn succeeded(output: &Output) {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

/// Write identical offline Rust-client fixtures at arbitrary checkout paths.
fn project(path: &Path, tests: &str) {
    std::fs::create_dir_all(path).unwrap();
    common::write_project(path, &format!("{FUNCTIONS}{tests}"));
    std::fs::write(
        path.join("baml.toml"),
        r#"[package]
name = "generate_without_tests"
[generator.rust]
output_type = "rust"
output_dir = "generated"
naming_convention = "preserve-case"
"#,
    )
    .unwrap();
}

/// Read complete output trees, including manifests, for byte comparisons.
fn generated_files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    /// Collect each file under its output-relative path.
    fn visit(root: &Path, dir: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(root, &path, files);
            } else {
                files.insert(
                    path.strip_prefix(root).unwrap().into(),
                    std::fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut files = BTreeMap::new();
    visit(root, root, &mut files);
    files
}

/// Reject test symbols, constants, or checkout paths in persisted bytes.
fn assert_absent(bytes: &[u8], needle: &str) {
    assert!(
        !bytes
            .windows(needle.len())
            .any(|window| window == needle.as_bytes()),
        "persisted program contains {needle:?}",
    );
}

/// Compare artifacts across checkouts and exercise persisted and source execution.
#[tokio::test]
async fn generation_excludes_tests_and_preserves_production_closures() {
    let temp = tempfile::tempdir().unwrap();
    let a = temp.path().join("checkout-a");
    let b = temp.path().join("different/checkout-b");
    project(&a, TESTS);
    project(&b, TESTS);

    succeeded(&cli(&a, &["generate"]));
    succeeded(&cli(&b, &["generate"]));
    let generated_a = generated_files(&a.join("generated/baml_sdk"));
    let generated_b = generated_files(&b.join("generated/baml_sdk"));
    // This also compares every sha256 in .baml-generator-output.json.
    assert!(generated_a.contains_key(Path::new(".baml-generator-output.json")));
    assert_eq!(
        generated_a.keys().collect::<Vec<_>>(),
        generated_b.keys().collect::<Vec<_>>()
    );
    for (path, bytes) in &generated_a {
        assert_eq!(
            bytes,
            &generated_b[path],
            "{} differs across checkout directories",
            path.display()
        );
        assert_absent(bytes, &a.canonicalize().unwrap().to_string_lossy());
        assert_absent(bytes, &b.canonicalize().unwrap().to_string_lossy());
    }

    let bytecode = &generated_a[Path::new("src/_inlinedbaml.bin")];
    let program: Program =
        baml_artifact::decode(baml_artifact::ArtifactKind::Program, bytecode).unwrap();
    program.validate().unwrap();
    // Inspect the decoded payload too, so compression cannot hide test data.
    let payload = borsh::to_vec(&program).unwrap();
    assert_absent(&payload, "ONLY_TEST_5246");
    assert_absent(&payload, "$init_test");
    assert_absent(&payload, &a.canonicalize().unwrap().to_string_lossy());
    for package in &program.packages {
        assert!(package.test_init.is_none());
        if !package.interface_blob.is_empty() {
            let interface: baml_db::baml_compiler2_hir_ty::package_interface::WireInterface =
                baml_artifact::decode(
                    baml_artifact::ArtifactKind::PackageInterface,
                    &package.interface_blob,
                )
                .unwrap();
            assert_absent(&borsh::to_vec(&interface).unwrap(), "$init_test");
        }
    }

    let engine = Arc::new(
        bex_engine::BexEngine::new(program, Arc::new(sys_native::SysOps::native()), Vec::new())
            .unwrap(),
    );
    for (function, args, expected) in [
        (
            "Halve",
            vec![BexExternalValue::Int(8)],
            BexExternalValue::Int(4),
        ),
        (
            "Recover",
            vec![],
            BexExternalValue::String("production recovery".into()),
        ),
    ] {
        let result = engine
            .call_function(
                function,
                args,
                FunctionCallContextBuilder::new(CallId::next()).build(),
                true,
            )
            .await
            .unwrap();
        assert_eq!(result, expected);
    }
    assert!(matches!(
        engine
            .collect_tests(CallId::next(), CancellationToken::default())
            .await
            .unwrap(),
        BexExternalValue::Null
    ));

    // The complete source project still discovers and executes every test.
    let listed = cli(&a, &["test", "--list"]);
    succeeded(&listed);
    let listed = String::from_utf8_lossy(&listed.stdout);
    for name in [
        "ONLY_TEST_5246_inline",
        "ONLY_TEST_5246_suite::leaf",
        "ONLY_TEST_5246_suite::nested::deep",
    ] {
        assert!(listed.contains(name), "missing {name}: {listed}");
    }
    succeeded(&cli(&a, &["test"]));
    // A populated test/bytecode cache must not reintroduce tests into generation.
    succeeded(&cli(&a, &["generate"]));
    assert_eq!(
        bytecode,
        &std::fs::read(a.join("generated/baml_sdk/src/_inlinedbaml.bin")).unwrap()
    );

    // Removing the appended test blocks changes no persisted bytecode.
    std::fs::write(b.join("baml_src/main.baml"), FUNCTIONS).unwrap();
    succeeded(&cli(&b, &["generate"]));
    assert_eq!(
        bytecode,
        &std::fs::read(b.join("generated/baml_sdk/src/_inlinedbaml.bin")).unwrap()
    );
}

/// Keep test diagnostics visible before production compilation strips tests.
#[test]
fn generation_still_reports_errors_in_tests() {
    let temp = tempfile::tempdir().unwrap();
    project(
        temp.path(),
        "test \"invalid\" { assert.equal(MissingFunction(), 2) }",
    );
    let output = cli(temp.path(), &["generate"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("MissingFunction"));
    assert!(
        !temp
            .path()
            .join("generated/baml_sdk/src/_inlinedbaml.bin")
            .exists()
    );
}
