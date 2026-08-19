//! Runs all BAML tests in `crates/baml_tests/baml_src/`.
//!
//! Also snapshots the entire project's bytecode grouped by namespace.

use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
};

use bex_vm::debug::{BytecodeFormat, display_program};
use bex_vm_types::{Function, FunctionOrigin, Object, Program};

const SNAPSHOT_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/snapshots/baml_src");

fn baml_src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("baml_src")
}

/// Read every `*.baml` file under `baml_src/`, returning `(relative_path, content)`
/// pairs sorted by path so the compiled program is deterministic.
fn read_baml_src_files() -> Vec<(String, String)> {
    let root = baml_src_dir();
    let mut files = Vec::new();
    collect_baml_files(&root, &root, &mut files);
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}

fn collect_baml_files(root: &Path, dir: &Path, out: &mut Vec<(String, String)>) {
    for entry in std::fs::read_dir(dir).expect("read_dir baml_src") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            // Skip hidden dirs (e.g. a stray `.baml/cache` a CLI run may have
            // left behind); the corpus is only the checked-in `.baml` sources.
            if path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with('.'))
            {
                continue;
            }
            collect_baml_files(root, &path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("baml") {
            let rel = path
                .strip_prefix(root)
                .expect("strip baml_src prefix")
                .to_string_lossy()
                .replace('\\', "/");
            // Normalize line endings so snapshots match across platforms.
            let content = std::fs::read_to_string(&path)
                .expect("read .baml file")
                .replace("\r\n", "\n");
            out.push((rel, content));
        }
    }
}

/// Compile the whole baml_src project. Panics (via `compile_multi_file`) if the
/// project has any diagnostic errors.
fn compile_baml_src() -> Program {
    let files = read_baml_src_files();
    assert!(!files.is_empty(), "no .baml files found in baml_src/");
    let refs: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, content)| (path.as_str(), content.as_str()))
        .collect();
    baml_db::testing::compile_multi_file(&refs)
}

#[test]
fn promptfiddle_demo_compiles() {
    // This cross-workspace include is intentionally cursed: Prompt Fiddle owns
    // the demo, while this existing test binary checks it without a second compiler build.
    let source =
        include_str!("../../../../typescript2/app-promptfiddle/src/playground/default.baml");
    baml_db::testing::compile_multi_file(&[("baml_src/main.baml", source)]);
}

/// Strip the `ns_` prefix from a directory segment if it names a valid namespace
/// (BAML identifier: starts with a letter or `_`, rest alphanumeric or `_`),
/// matching the compiler's `file_package` rule. Returns `None` otherwise.
fn extract_ns_name(component: &str) -> Option<&str> {
    let ns = component.strip_prefix("ns_")?;
    let mut chars = ns.chars();
    let valid = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    valid.then_some(ns)
}

/// Namespace key for a source file: the dotted chain of `ns_`-prefixed directory
/// segments, e.g. `ns_assignments/assignments.baml` -> `assignments` and
/// `ns_a/b/ns_c/x.baml` -> `a.c`. Functions with no source file (synthesized,
/// e.g. the global `$init_test`) or no namespace -> `_root`.
fn namespace_key(source_file: &str) -> String {
    if source_file.is_empty() {
        return "_root".to_string();
    }
    let parts: Vec<&str> = Path::new(source_file)
        .parent()
        .map(|parent| {
            parent
                .components()
                .filter_map(|c| match c {
                    Component::Normal(s) => s.to_str().and_then(extract_ns_name),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    if parts.is_empty() {
        "_root".to_string()
    } else {
        parts.join(".")
    }
}

/// Snapshot each namespace's bytecode separately so a change shows up in just
/// that namespace's `.snap`.
#[test]
fn bytecode() {
    let program = compile_baml_src();

    // Group user (non-stdlib, non-auto-derived) functions by their namespace.
    let mut by_namespace: BTreeMap<String, Vec<(String, &Function)>> = BTreeMap::new();
    for (name, idx) in &program.function_indices {
        // Stdlib functions are not this suite's subject — it snapshots the
        // BAML written in `baml_src/`. The package list comes from
        // `baml_builtins2::ALL`, so adding a builtin package (ai, openai,
        // anthropic, google, claude_code, ...) never floods these snapshots.
        let is_stdlib = baml_builtins2::stdlib_package_names().iter().any(|pkg| {
            let pkg: &str = pkg;
            name.len() > pkg.len() && name.as_bytes()[pkg.len()] == b'.' && name.starts_with(pkg)
        });
        if is_stdlib || name.starts_with("env.") {
            continue;
        }
        let Some(Object::Function(func)) = program.objects.get(*idx) else {
            continue;
        };
        if func.origin == FunctionOrigin::AutoDerive {
            continue;
        }
        // Strip the leading "user." package prefix for display.
        let display_name = name.strip_prefix("user.").unwrap_or(name).to_owned();
        by_namespace
            .entry(namespace_key(&func.source_file))
            .or_default()
            .push((display_name, &**func));
    }

    for (key, mut funcs) in by_namespace {
        // The `llm_*` provider-suite namespaces are large wire-shape/behavior
        // suites whose guarantees live in their own `test` blocks; their
        // bytecode dumps flooded these snapshots (thousands of lines each)
        // without adding signal. Codegen stability is still covered by the
        // remaining namespaces and the `compiles/` phase snapshots.
        if key.starts_with("llm_") {
            continue;
        }
        funcs.sort_by(|(a, _), (b, _)| a.cmp(b));
        let output = display_program(&funcs, BytecodeFormat::Textual);
        insta::with_settings!({
            snapshot_path => SNAPSHOT_PATH,
            omit_expression => true,
            prepend_module_to_snapshot => false,
        }, {
            insta::assert_snapshot!(key.as_str(), output);
        });
    }
}

/// Execute `baml test`
#[test]
fn baml_test() {
    // Isolate the CLI's bytecode cache and home per run. Without this, the CLI
    // writes `<project>/.baml/cache` straight into the source tree that the
    // `bytecode`/`emit_determinism`/`link_units_oracle` tests scan
    // concurrently, and successive runs share (and can corrupt) that cache.
    let tmp = tempfile::tempdir().expect("tempdir for corpus cache");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("config.toml"), "[update]\nauto_check = false\n").unwrap();
    let status = std::process::Command::new("cargo")
        .args([
            "run",
            "-p",
            "baml_cli",
            "--",
            "test",
            "--from",
            concat!(env!("CARGO_MANIFEST_DIR"), "/baml_src"),
        ])
        .env("BAML_CLI_ALLOW_DIRECT", "1")
        .env("BAML_HOME", &home)
        .env("BAML_CACHE_DIR", tmp.path().join("cache"))
        .status()
        .expect("baml_cli test should not fail");
    assert!(status.success());
}
