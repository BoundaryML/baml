//! The native platform resolves a program's relative paths against the
//! working directory its host chose. A command-line run keeps the process's
//! own directory; an engine hosted next to other projects (the playground)
//! is given its project root, and the process never changes directory.
//!
//! A BAML program cannot observe which directory its host picked, so this
//! lives here rather than in the corpus.

use std::sync::Arc;

use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use sys_native::SysOpsExt;

const SOURCE: &str = r#"
function read_note() -> string throws baml.errors.Io | baml.errors.ParseError {
    baml.fs.read("note.txt")
}

function write_result() -> int throws baml.errors.Io {
    baml.fs.write("out/result.txt", "written")
}

function note_exists() -> bool throws baml.errors.Io {
    baml.fs.exists("note.txt")
}

function scan_notes() -> string[] throws baml.errors.Io | baml.errors.ParseError {
    baml.glob.new("*.txt").scan(".")
}

function shell_dir() -> string throws baml.errors.Io | baml.errors.Timeout {
    baml.sys.shell("pwd", null).stdout.to_string()
}

function run_local_script() -> string throws baml.errors.Io | baml.errors.Timeout {
    baml.sys.exec("./hello.sh", null, null).stdout.to_string()
}
"#;

async fn call(engine: &Arc<BexEngine>, name: &str) -> BexExternalValue {
    engine
        .call_function(
            name,
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .unwrap_or_else(|e| panic!("{name} runs: {e:?}"))
}

fn string(value: BexExternalValue) -> String {
    let BexExternalValue::String(s) = value else {
        panic!("expected a string, got {value:?}");
    };
    s.to_string()
}

#[tokio::test]
async fn relative_paths_resolve_against_the_hosts_working_directory() {
    let project = tempfile::tempdir().expect("temp project");
    let root = project
        .path()
        .canonicalize()
        .expect("temp project canonicalizes");
    std::fs::write(root.join("note.txt"), "from the project").expect("note written");
    let process_dir = std::env::current_dir().expect("cwd should be available");
    assert_ne!(
        process_dir, root,
        "the fixture must not already be the process directory"
    );

    let program = baml_db::testing::compile_source(SOURCE);
    let engine = Arc::new(
        BexEngine::new_with_runtime_compiler(
            program,
            Arc::new(sys_native::SysOps::native_in(root.clone())),
            Vec::new(),
            bex_project::runtime_compiler(),
        )
        .expect("engine constructs"),
    );

    assert_eq!(
        string(call(&engine, "user.read_note").await),
        "from the project"
    );
    assert_eq!(
        call(&engine, "user.note_exists").await,
        BexExternalValue::Bool(true)
    );

    call(&engine, "user.write_result").await;
    assert_eq!(
        std::fs::read_to_string(root.join("out").join("result.txt"))
            .expect("written in the project"),
        "written"
    );
    assert!(
        !process_dir.join("out").exists(),
        "nothing lands in the process's own directory"
    );

    let BexExternalValue::Array { items, .. } = call(&engine, "user.scan_notes").await else {
        panic!("scan returns an array");
    };
    let scanned: Vec<String> = items.into_iter().map(string).collect();
    assert_eq!(scanned, vec!["note.txt".to_string()]);

    #[cfg(unix)]
    {
        let reported = string(call(&engine, "user.shell_dir").await);
        let reported = std::path::PathBuf::from(reported.trim())
            .canonicalize()
            .expect("the child's directory exists");
        assert_eq!(
            reported, root,
            "a child process starts in the working directory"
        );

        // A program named by a relative path is found in the working
        // directory too, not in the process's.
        use std::os::unix::fs::PermissionsExt as _;
        let script = root.join("hello.sh");
        std::fs::write(&script, "#!/bin/sh\necho hello from the project\n")
            .expect("script written");
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
            .expect("script executable");
        assert_eq!(
            string(call(&engine, "user.run_local_script").await).trim(),
            "hello from the project"
        );
    }

    assert_eq!(
        std::env::current_dir().expect("cwd should be available"),
        process_dir,
        "the host never changes the process's directory"
    );
}
