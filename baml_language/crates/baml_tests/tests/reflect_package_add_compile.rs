//! Phase 5.2c sanity test: `reflect.Package.add_compile` accepts a map of
//! files and stores each as a runtime source file under `<runtime>/{pkg}/…`
//! in the engine's `Compiler2RuntimeFiles` Salsa input.
//!
//! Re-emit + item extraction land in later commits; this test just verifies
//! the file-insertion plumbing.

use std::sync::Arc;

use baml_base::SourceFile;
use baml_compiler2_hir::Db;
use baml_project::testing::{OptLevel, compile_source_with_opt_returning_db};
use bex_engine::{BexEngine, FunctionCallContextBuilder};
use sys_native::SysOpsExt;

#[tokio::test]
async fn add_compile_inserts_files_into_runtime_input() {
    let source = r#"
        function main() -> bool {
            let pkg = reflect.Package.new();
            let _ = pkg.add_compile({
                "lib.baml": "function hello() -> int { 1 }",
                "more.baml": "function bye() -> int { 2 }"
            });
            true
        }
    "#;

    let (program, db) = compile_source_with_opt_returning_db(source, OptLevel::One);
    let db_handle = Arc::new(parking_lot::Mutex::new(db));

    let mut engine = BexEngine::new(
        program,
        Arc::new(sys_ops::SysOps::native()),
        None,
        Vec::new(),
    )
    .expect("BexEngine::new");
    engine.set_project_db(Arc::clone(&db_handle));
    let engine = Arc::new(engine);

    let result = engine
        .call_function_bound_args(
            "user.main",
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;
    assert!(result.is_ok(), "main() returned: {result:?}");

    // The package mint counter starts at 0, so `Package.new` returned
    // `_pkg_0`. Both files should now live under `<runtime>/_pkg_0/…`.
    let db = db_handle.lock();
    let runtime_files = db
        .compiler2_runtime_files()
        .expect("runtime files input should exist after set_project_root");
    let files = runtime_files.files(&*db);
    let paths: Vec<String> = files
        .iter()
        .map(|f: &SourceFile| f.path(&*db).to_string_lossy().into_owned())
        .collect();
    assert!(
        paths.contains(&"<runtime>/_pkg_0/lib.baml".to_string()),
        "expected `<runtime>/_pkg_0/lib.baml` in {paths:?}"
    );
    assert!(
        paths.contains(&"<runtime>/_pkg_0/more.baml".to_string()),
        "expected `<runtime>/_pkg_0/more.baml` in {paths:?}"
    );
}
