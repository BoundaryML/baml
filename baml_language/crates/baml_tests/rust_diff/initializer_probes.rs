//! Artifact variants and assertions for initializer differential probes.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::{Command, Output},
};

#[path = "collect_tests.rs"]
mod collect_tests;

pub fn build(
    build: &Path,
    workspace: &Path,
    manifest: &Path,
    only: Option<&str>,
) -> BTreeMap<&'static str, PathBuf> {
    let selected = |probe| only.is_none_or(|only| only.split(',').any(|name| name == probe));
    let mut binaries = BTreeMap::new();
    if selected("InitializerTests") || selected("InitializerTestErrors") {
        let executable = build_test_initializer_probe(build, workspace);
        binaries.insert("InitializerTests", executable);
    }
    let template = std::fs::read(build.join("src/program.bexa")).unwrap();
    let template: baml_exec::PackEnvelope =
        baml_artifact::decode(baml_artifact::ArtifactKind::PackedProgram, &template).unwrap();
    for (probe, mode) in [
        ("InitializerUncaught", "throw"),
        ("InitializerPanic", "panic"),
        ("InitializerPending", "pending"),
        ("GlobalCallPending", "pending-call"),
        ("InitializerTestErrors", "registration"),
    ] {
        if !selected(probe) {
            continue;
        }
        let mut executable = build_probe_variant(build, workspace, manifest, &template, mode);
        if collects_tests(probe) {
            executable = build_binary(build, workspace, "initializer-tests", probe);
        }
        binaries.insert(probe, executable);
    }
    binaries
}

pub fn collects_tests(probe: &str) -> bool {
    matches!(probe, "InitializerTests" | "InitializerTestErrors")
}

fn build_test_initializer_probe(build: &Path, workspace: &Path) -> PathBuf {
    // The included module is also compiled as part of this test target.
    let _ = collect_tests::main;
    std::fs::write(
        build.join("src/collect_tests.rs"),
        include_str!("collect_tests.rs"),
    )
    .unwrap();
    std::fs::write(
        build.join("src/initializer_tests.rs"),
        r#"mod collect_tests;

fn main() -> std::process::ExitCode {
    collect_tests::main(
        baml_rust_program::PROGRAM_ARTIFACT,
        &baml_rust_program::COMPILED_BODIES,
    )
}

"#,
    )
    .unwrap();
    let manifest_path = build.join("Cargo.toml");
    let mut manifest = std::fs::read_to_string(&manifest_path).unwrap();
    for dependency in ["baml_artifact", "baml_exec", "bex_engine", "sys_native"] {
        manifest.push_str(&format!(
            "\n[dependencies.{dependency}]\npath = {:?}\n",
            workspace
                .join("crates")
                .join(dependency)
                .display()
                .to_string()
        ));
    }
    manifest.push_str(
        "\n[dependencies.tokio]\nversion = \"1.36\"\nfeatures = [\"rt-multi-thread\"]\n\
         \n[[bin]]\nname = \"initializer-tests\"\npath = \"src/initializer_tests.rs\"\n",
    );
    std::fs::write(&manifest_path, manifest).unwrap();
    build_binary(build, workspace, "initializer-tests", "InitializerTests")
}

fn build_binary(build: &Path, workspace: &Path, binary: &str, probe: &str) -> PathBuf {
    let status = Command::new("cargo")
        .args(["build", "--release", "--bin", binary])
        .arg("--manifest-path")
        .arg(build.join("Cargo.toml"))
        .arg("--target-dir")
        .arg(build.join("target"))
        .current_dir(workspace)
        .status()
        .expect("build differential probe");
    assert!(status.success(), "{probe} must compile");
    let executable = build.join(format!("probes-{probe}{}", std::env::consts::EXE_SUFFIX));
    std::fs::copy(
        build
            .join("target/release")
            .join(format!("{binary}{}", std::env::consts::EXE_SUFFIX)),
        &executable,
    )
    .unwrap();
    executable
}

fn build_probe_variant(
    build: &Path,
    workspace: &Path,
    manifest: &Path,
    template: &baml_exec::PackEnvelope,
    mode: &str,
) -> PathBuf {
    // `baml pack` creates an engine for signature resolution. Emit failing
    // startup and registration variants directly, using a literal selector.
    let opt = baml_compiler2_emit::OptLevel::Two;
    let mut sources: Vec<_> = std::fs::read_dir(manifest.join("rust_diff/baml_src"))
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            (
                entry.file_name().into_string().unwrap(),
                std::fs::read_to_string(entry.path()).unwrap().replace(
                    "const INIT_FAILURE = \"none\";",
                    &format!("const INIT_FAILURE = {mode:?};"),
                ),
            )
        })
        .collect();
    sources.sort_by(|a, b| a.0.cmp(&b.0));
    let files: Vec<_> = sources
        .iter()
        .map(|(name, source)| (name.as_str(), source.as_str()))
        .collect();
    let db = baml_test_support::setup_multi_file_db(&files);
    let root = db.workspace_root().unwrap();
    let mut envelope = template.clone();
    envelope.program = baml_db::compile_program(&db, root, opt).unwrap();
    let package = &envelope.program.packages[envelope.program.root as usize].name;
    for target in &mut envelope.targets {
        let name = target.qualified_name.rsplit('.').next().unwrap();
        target.qualified_name = format!("{package}.{name}");
    }
    let callables = envelope.program.rendered_callables();
    let entries: Vec<_> = envelope
        .targets
        .iter()
        .map(|target| callables[&target.qualified_name].object.raw())
        .collect();
    let (artifact, fingerprint) =
        baml_artifact::encode_with_digest(baml_artifact::ArtifactKind::PackedProgram, &envelope)
            .unwrap();
    let module = baml_db::rust_backend::compile_program(
        &db,
        root,
        &envelope.program,
        opt,
        "program.bexa",
        fingerprint,
        Some(&entries),
    )
    .unwrap();
    for name in ["GlobalCalls", "GlobalCallPending"] {
        let object = callables[&format!("{package}.{name}")].object.raw();
        assert!(
            module.functions.iter().any(|f| f.object == object),
            "{name} at {opt:?}: {:?}",
            module.rejected
        );
    }
    std::fs::write(build.join("src/program.bexa"), artifact).unwrap();
    std::fs::write(build.join("src/lib.rs"), module.source).unwrap();
    build_binary(build, workspace, "baml_rust_program", mode)
}

pub fn assert_outputs(probe: &str, outputs: &[Option<Output>]) {
    for output in outputs.iter().flatten() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        match probe {
            "GlobalCalls" => {
                assert!(output.status.success(), "{stderr}");
                for expected in ["6:9", "8:10:15", "global-call:9"] {
                    assert!(stdout.contains(expected), "{probe}: {stdout}");
                }
            }
            "InitializerUncaught"
            | "InitializerPanic"
            | "InitializerPending"
            | "InitializerTestErrors"
            | "GlobalCallPending" => {
                assert!(
                    !output.status.success(),
                    "{probe} must fail during initialization"
                );
                let evidence = match probe {
                    "InitializerPending" => "INIT_LATE",
                    "GlobalCallPending" => "INIT_LATE_CALLBACK",
                    "InitializerPanic" => "INIT_GATE",
                    _ => "initializer-error",
                };
                assert!(stderr.contains(evidence), "{probe}: {stderr}");
                assert!(!stdout.contains("unreachable:"));
            }
            _ => {}
        }
    }
}
