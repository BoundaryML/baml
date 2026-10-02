//! A runtime mount alias equal to a host package's spelling binds the MOUNT:
//! the loader reaches a dependency slot through the artifact's pins, never
//! through the host image by name. Needs a host program with a dependency
//! root, which only the harness can build, so it lives here rather than in
//! the BAML corpus.

use std::sync::Arc;

use baml_base::{Dependency, Name, SourceRootKind};
use baml_compiler2_emit::OptLevel;
use baml_db::{ProjectDatabase, SourceRootSpec, compile_program};
use baml_tests::engine::TestDbExt;
use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use sys_native::SysOpsExt;

#[tokio::test]
async fn a_mount_alias_equal_to_a_host_package_spelling_binds_the_mount() {
    let mut db = ProjectDatabase::new();
    let workspace = db.workspace(std::path::Path::new("/mount-alias"));
    let host_app = db
        .add_source_root(SourceRootSpec::new(
            "/mount-alias-app",
            SourceRootKind::Dependency,
        ))
        .expect("a dependency root");
    db.file(
        "/mount-alias-app/app.baml",
        "function tag() -> string throws never { \"host\" }\n",
    );
    db.add_dependency(
        workspace,
        Dependency {
            name: Name::new("app"),
            root: host_app,
        },
    )
    .expect("edge from the workspace");
    db.file(
        "main.baml",
        r#"
function go() -> string throws never {
    let mounted = reflect.Package.compile({ "m.baml": `function tag() -> string throws never { "mount" }` });
    let consumer = reflect.Package.compile(
        { "main.baml": `function go() -> string throws never { app.tag() }` },
        packages = { "app": mounted },
    );
    let go = consumer.get_function<() -> string>("root.go") ?? throw "the consumer has no `go`";
    app.tag() + ":" + go()
}
"#,
    );
    let program =
        compile_program(&db, workspace, OptLevel::Two).expect("the host program compiles");
    let engine = Arc::new(
        BexEngine::new_with_runtime_compiler(
            program,
            Arc::new(sys_native::SysOps::native()),
            Vec::new(),
            bex_project::runtime_compiler(),
        )
        .expect("engine"),
    );
    let result = engine
        .call_function(
            "user.go",
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("`go` runs");
    let BexExternalValue::String(tags) = result else {
        panic!("unexpected result: {result:?}")
    };
    assert_eq!(tags.as_str(), "host:mount");
}
