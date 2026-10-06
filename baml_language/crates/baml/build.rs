use std::{env, fs, path::PathBuf};

fn main() {
    let sources = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest directory"))
        .join("baml_src");
    println!("cargo:rerun-if-changed={}", sources.display());
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_SELF_UPDATE");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_NO_SELF_UPDATE");
    let main = fs::read_to_string(sources.join("main.baml")).expect("wrapper source");
    let version = format!(
        "function WrapperVersion() -> string throws never {{ {:?} }}",
        env::var("CARGO_PKG_VERSION").expect("wrapper version")
    );
    let mut db = baml_db::ProjectDatabase::new();
    db.ensure_stdlib_sources();
    let root = db
        .add_source_root(
            baml_db::SourceRootSpec::new(&sources, baml_db::SourceRootKind::Workspace)
                .named(baml_db::Name::new("baml_wrapper")),
        )
        .expect("wrapper source root");
    for (path, text) in [("main.baml", &main), ("version.baml", &version)] {
        db.add_or_update_file_in(root, &sources.join(path), text);
    }
    for diagnostic in db
        .workspace_files()
        .iter()
        .flat_map(|file| db.check_file(*file))
    {
        if diagnostic.severity == baml_db::baml_compiler_diagnostics::Severity::Error {
            println!("cargo:warning={}", diagnostic.message);
        }
    }
    let program = db.get_bytecode(root).expect("compile BAML wrapper");
    let target = if env::var_os("CARGO_FEATURE_SELF_UPDATE").is_some()
        && env::var_os("CARGO_FEATURE_NO_SELF_UPDATE").is_none()
    {
        "Main"
    } else {
        "NoSelfUpdateMain"
    };
    let qualified_name = program
        .rendered_callables()
        .keys()
        .find(|name| name.as_str() == target || name.ends_with(&format!(".{target}")))
        .unwrap_or_else(|| panic!("missing wrapper entry point {target}"))
        .clone();
    let program = baml_db::compile_program_selected_with(
        &db,
        root,
        baml_db::OptLevel::Two,
        &baml_db::NoCache,
        &baml_db::LinkRoots::EntryPoints(vec![qualified_name.clone()]),
    )
    .expect("link wrapper entry point");
    let envelope = baml_exec::PackEnvelope {
        program,
        mode: baml_exec::PackMode::Single,
        targets: vec![baml_exec::TargetEntry {
            qualified_name,
            display_name: target.into(),
            subcommand_name: target.into(),
        }],
        output_format: baml_exec::OutputFormat::Json,
        telemetry: btel_settings::artifact::ArtifactTelemetry::default(),
    };
    let bytes = baml_artifact::encode(baml_artifact::ArtifactKind::PackedProgram, &envelope)
        .expect("encode wrapper program");
    fs::write(
        PathBuf::from(env::var_os("OUT_DIR").expect("build directory")).join("wrapper.bamlpack"),
        bytes,
    )
    .expect("write wrapper program");
}
