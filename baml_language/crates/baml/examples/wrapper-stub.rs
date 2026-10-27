#![allow(clippy::print_stderr)]

//! Configurable process fixture; never linked into or distributed with the wrapper.
use std::{
    env, fs,
    io::{self, Read, Write},
    path::Path,
    process,
};

use serde_json::{Value, json};

fn main() {
    let root = env::var("WRAPPER_FIXTURE_ROOT").expect("fixture root");
    let exe = env::current_exe().unwrap().canonicalize().unwrap();
    let identity = fs::read_to_string(exe.with_extension("identity"))
        .unwrap_or_else(|_| {
            fs::read_to_string(exe.parent().unwrap().parent().unwrap().join("VERSION"))
                .unwrap_or_else(|_| "replacement".into())
        })
        .trim()
        .to_owned();
    let behavior: Value = serde_json::from_str(
        &fs::read_to_string(
            Path::new(&root)
                .join("behaviors")
                .join(format!("{identity}.json")),
        )
        .expect("unexpected stub identity"),
    )
    .unwrap();
    let args: Vec<_> = env::args().skip(1).collect();
    let record = json!({"identity":identity,"args":args,"cwd":env::current_dir().unwrap(),"env":{"BAML_HOME":env::var("BAML_HOME").ok(),"BAML_TOOLCHAIN":env::var("BAML_TOOLCHAIN").ok(),"BAML_CLI_ALLOW_DIRECT":env::var("BAML_CLI_ALLOW_DIRECT").ok()}});
    let mut log = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(Path::new(&root).join("invocations.jsonl"))
        .unwrap();
    writeln!(log, "{record}").unwrap();
    let allowed = behavior["args"]
        .as_array()
        .expect("allowed argument arrays");
    if !allowed.iter().any(|expected| expected == &json!(args)) {
        eprintln!("unexpected invocation: {record}");
        process::exit(97);
    }
    io::stdout()
        .write_all(behavior["stdout"].as_str().unwrap_or("").as_bytes())
        .unwrap();
    io::stdout().flush().unwrap();
    io::stderr()
        .write_all(behavior["stderr"].as_str().unwrap_or("").as_bytes())
        .unwrap();
    io::stderr().flush().unwrap();
    if behavior["echo_stdin"].as_bool().unwrap_or(false) {
        let mut input = Vec::new();
        io::stdin().read_to_end(&mut input).unwrap();
        io::stdout().write_all(&input).unwrap();
    }
    process::exit(
        i32::try_from(behavior["exit_code"].as_i64().unwrap_or(0)).expect("fixture exit code"),
    );
}
