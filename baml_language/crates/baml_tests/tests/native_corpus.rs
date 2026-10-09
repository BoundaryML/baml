//! The Rust backend over the whole `baml_src` conformance corpus.
//!
//! Admission is a per-function decision, so the corpus is the only place to
//! see what the subset covers and to check that every function it admits
//! turns into Rust that `rustc` accepts. The first test runs on every CI
//! build and asserts the backend's own consistency; the second shells out to
//! `cargo check` on the emitted crate and is run by hand:
//!
//! ```text
//! cargo test -p baml_tests --test native_corpus -- --ignored --nocapture
//! ```
#![allow(clippy::disallowed_methods, clippy::print_stdout)]

use std::path::{Path, PathBuf};

use baml_db::{
    ProjectDatabase,
    baml_compiler_diagnostics::Severity,
    baml_compiler2_hir::{item_data::file_functions, loc::FunctionLoc},
    baml_compiler2_rust as native,
};
use baml_test_support::setup_multi_file_db;

fn collect(root: &Path, dir: &Path, out: &mut Vec<(String, String)>) {
    for entry in std::fs::read_dir(dir).expect("corpus directory") {
        let path = entry.expect("directory entry").path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if path.is_dir() {
            if !name.starts_with('.') {
                collect(root, &path, out);
            }
        } else if name.ends_with(".baml") {
            let relative = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            out.push((relative, std::fs::read_to_string(&path).unwrap()));
        }
    }
}

/// Every `.baml` file of the corpus in one workspace, as the conformance
/// snapshot tests load it.
fn corpus_db() -> ProjectDatabase {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("baml_src");
    let mut files = Vec::new();
    collect(&root, &root, &mut files);
    files.sort();
    let refs: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let db = setup_multi_file_db(&refs);
    let errors = baml_test_support::check_user_files(&db)
        .iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .count();
    assert_eq!(errors, 0, "the corpus has user-error diagnostics");
    db
}

fn functions(db: &ProjectDatabase) -> Vec<FunctionLoc<'_>> {
    db.workspace_files()
        .into_iter()
        .flat_map(|file| file_functions(db, file).iter().copied())
        .collect()
}

struct Sweep<'db> {
    total: usize,
    roots: Vec<FunctionLoc<'db>>,
    module: native::NativeModule<'db>,
}

/// Admit every function with its callees and compile the admitted roots as
/// one module, as `baml __emit-rust --all` does.
fn sweep(db: &ProjectDatabase) -> Sweep<'_> {
    let all = functions(db);
    let mut roots = Vec::new();
    let mut invalid = Vec::new();
    for &loc in &all {
        match native::admit_closure(db, loc) {
            Ok(_) => roots.push(loc),
            Err(native::Rejection::Unsupported(_)) => {}
            Err(native::Rejection::Invalid(reason)) => invalid.push(reason),
        }
    }
    assert!(
        invalid.is_empty(),
        "{} function(s) report invalid MIR:\n{}",
        invalid.len(),
        invalid.join("\n")
    );
    let module = native::compile_many(db, &roots)
        .unwrap_or_else(|rejection| panic!("admitted roots do not compile: {rejection}"));
    Sweep {
        total: all.len(),
        roots,
        module,
    }
}

/// Admission with callees is what `compile` needs: every admitted root
/// compiles, together, into one module. The floor on the count catches a
/// change that silently shrinks the subset; the corpus grows, so the exact
/// number is printed rather than pinned.
#[test]
fn every_admitted_function_compiles() {
    let db = corpus_db();
    let sweep = sweep(&db);
    println!(
        "corpus: {} functions, {} admitted with their callees, {} compiled",
        sweep.total,
        sweep.roots.len(),
        sweep.module.functions.len()
    );
    assert!(
        sweep.roots.len() >= 1000,
        "only {} of {} corpus functions are admitted",
        sweep.roots.len(),
        sweep.total
    );
    assert!(
        sweep.module.functions.len() >= sweep.roots.len(),
        "every root is in the module"
    );
}

/// The emitted module, as a crate over `bex_aot`, passes `cargo check`:
/// `rustc` agrees with the backend about initialization, borrows and types.
/// Writes the crate under `NATIVE_CORPUS_OUT` when set, else a temporary
/// directory.
#[test]
#[ignore = "runs cargo check on the whole corpus; about a minute"]
fn emitted_corpus_passes_rustc() {
    let db = corpus_db();
    let sweep = sweep(&db);
    let out = match std::env::var_os("NATIVE_CORPUS_OUT") {
        Some(dir) => PathBuf::from(dir),
        None => std::env::temp_dir().join("baml_native_corpus"),
    };
    let runtime = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../bex_aot")
        .canonicalize()
        .expect("bex_aot next to baml_tests");
    std::fs::create_dir_all(&out).unwrap();
    native::write_project(
        &sweep.module,
        &out,
        &native::ProjectOptions {
            crate_name: "baml_native_corpus",
            runtime_path: &runtime,
            release_profile: false,
        },
    )
    .expect("write the corpus crate");
    println!(
        "checking {} functions in {}",
        sweep.module.functions.len(),
        out.display()
    );
    let status = std::process::Command::new(env!("CARGO"))
        .args(["check", "--lib", "--manifest-path"])
        .arg(out.join("Cargo.toml"))
        .status()
        .expect("run cargo check");
    assert!(status.success(), "cargo check failed on {}", out.display());
}
