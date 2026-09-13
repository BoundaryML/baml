//! The language-observable mounted-package call matrix lives in the native
//! corpus under `root.mounted_package_calls`. This Rust residue constructs a
//! privileged builtin interface artifact that public reflection APIs cannot
//! produce and verifies that an ordinary mount cannot claim builtin trust.

use baml_base::Name;
use baml_compiler2_hir_ty::package_interface::export_interface;
use baml_db::{ProjectDatabase, collect_diagnostics, testing::assert_no_diagnostic_errors};
use baml_tests::engine::TestDbExt;

/// A mounted compiler/VM builtin has no ordinary bytecode-unit symbol. This
/// E0158 residue must stay in Rust because the fixture deliberately constructs
/// a privileged package-interface artifact without a native implementation.
#[test]
fn mounted_builtin_call_stays_reserved() {
    let mut lib_db = ProjectDatabase::new();
    lib_db.workspace(std::path::Path::new("/mounted-calls"));
    lib_db.dependency("app");
    lib_db.file(
        "<builtin>/app/native.baml",
        r#"
function native_value() -> int throws never {
    $rust_function
}

function intrinsic_type<T>() -> reflect.Type throws never {
    $compiler_intrinsic
}
"#,
    );
    assert_no_diagnostic_errors(&lib_db);
    let iface = export_interface(
        &lib_db,
        baml_compiler2_hir::package::spelling(&lib_db)
            .root(&Name::new("app"))
            .unwrap(),
    );
    let blob = baml_artifact::encode(baml_artifact::ArtifactKind::PackageInterface, &iface)
        .expect("serialize builtin app interface");

    let mut db = ProjectDatabase::new();
    db.workspace(std::path::Path::new("/mounted-calls"));
    db.mount("app", blob);
    db.file(
        "main.baml",
        r#"
function main() -> int throws never {
    let reference = app.native_value
    let intrinsic = app.intrinsic_type<int>()
    app.native_value()
}
"#,
    );
    let errors: Vec<String> = collect_diagnostics(&db)
        .iter()
        .filter(|d| matches!(d.severity, baml_compiler_diagnostics::Severity::Error))
        .map(|d| format!("[{}] {}", d.code(), d.message))
        .collect();
    let reserved = errors
        .iter()
        .filter(|error| error.contains("E0158") && error.contains("mounted"))
        .count();
    assert_eq!(
        reserved, 2,
        "ordinary mounts cannot claim native or compiler-intrinsic trust; got:\n{errors:#?}"
    );
}
