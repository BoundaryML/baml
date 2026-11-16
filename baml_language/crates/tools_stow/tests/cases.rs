//! Runs every `tests/cases/*.case` file against a stow binary.
//!
//! The case format is described in `tests/cases/README.md`. It is plain text
//! so that any implementation can be driven by the same files: set `STOW_BIN`
//! to an executable that accepts stow's arguments to test it instead of this
//! crate's binary. Set `STOW_BLESS=1` to rewrite the expectations of every
//! case from what the binary does.

use std::{
    collections::BTreeMap,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Mutex,
};

const HEADER: &str = "--- ";

#[derive(Default)]
struct Case {
    args: Vec<String>,
    cwd: String,
    files: BTreeMap<String, String>,
    exit: Option<i32>,
    stdout: Option<String>,
    expect: BTreeMap<String, String>,
}

struct Run {
    exit: Option<i32>,
    stdout: String,
    stderr: String,
    /// Every file that differs from the case's inputs, by relative path.
    changed: BTreeMap<String, String>,
}

fn parse(text: &str) -> Result<Case, String> {
    let mut case = Case::default();
    let mut sections: Vec<(&str, String)> = Vec::new();
    for line in text.split_inclusive('\n') {
        match line.strip_prefix(HEADER) {
            Some(header) => sections.push((header.trim_end(), String::new())),
            None => match sections.last_mut() {
                Some((_, body)) => body.push_str(line),
                None if line.trim().is_empty() => {}
                None => return Err(format!("text before the first section: {line:?}")),
            },
        }
    }
    for (header, body) in sections {
        let (kind, path) = header.split_once(' ').unwrap_or((header, ""));
        match (kind, path.is_empty()) {
            ("args", true) => case.args = body.lines().map(str::to_owned).collect(),
            ("cwd", true) => body.trim().clone_into(&mut case.cwd),
            ("exit", true) => {
                case.exit = Some(
                    body.trim()
                        .parse()
                        .map_err(|_| "bad exit code".to_owned())?,
                );
            }
            ("stdout", true) => case.stdout = Some(body),
            ("file", false) => drop(case.files.insert(path.to_owned(), body)),
            ("expect", false) => drop(case.expect.insert(path.to_owned(), body)),
            _ => return Err(format!("unknown section `{header}`")),
        }
    }
    Ok(case)
}

fn serialize(case: &Case) -> String {
    let mut out = String::new();
    if !case.args.is_empty() {
        let _ = writeln!(out, "{HEADER}args\n{}", case.args.join("\n"));
    }
    if !case.cwd.is_empty() {
        let _ = writeln!(out, "{HEADER}cwd\n{}", case.cwd);
    }
    for (path, body) in &case.files {
        let _ = write!(out, "{HEADER}file {path}\n{body}");
    }
    if let Some(exit) = case.exit {
        let _ = writeln!(out, "{HEADER}exit\n{exit}");
    }
    if let Some(stdout) = &case.stdout {
        let _ = write!(out, "{HEADER}stdout\n{stdout}");
    }
    for (path, body) in &case.expect {
        let _ = write!(out, "{HEADER}expect {path}\n{body}");
    }
    out
}

/// A package manifest needs a target to be valid, so each one gets an empty
/// `src/lib.rs` unless the case provides its own.
fn stubs(case: &Case) -> Vec<String> {
    case.files
        .iter()
        .filter(|(path, body)| {
            Path::new(path)
                .file_name()
                .is_some_and(|name| name == "Cargo.toml")
                && body.lines().any(|line| line.trim() == "[package]")
        })
        .map(|(path, _)| match path.rsplit_once('/') {
            Some((dir, _)) => format!("{dir}/src/lib.rs"),
            None => "src/lib.rs".to_owned(),
        })
        .filter(|stub| !case.files.contains_key(stub))
        .collect()
}

fn files_under(root: &Path, dir: &Path, found: &mut BTreeMap<String, String>) {
    for entry in fs::read_dir(dir).expect("readable directory").flatten() {
        let path = entry.path();
        if path.is_dir() {
            files_under(root, &path, found);
        } else {
            let relative = path.strip_prefix(root).expect("path under root");
            let relative = relative.to_string_lossy().replace('\\', "/");
            let body =
                String::from_utf8_lossy(&fs::read(&path).expect("readable file")).into_owned();
            found.insert(relative, body);
        }
    }
}

fn run(case: &Case) -> Run {
    let dir = tempfile::tempdir().expect("temporary directory");
    let root = dir.path().canonicalize().expect("canonical path");
    let stubs = stubs(case);
    let inputs = case
        .files
        .iter()
        .map(|(path, body)| (path.as_str(), body.as_str()))
        .chain(stubs.iter().map(|stub| (stub.as_str(), "")));
    for (path, body) in inputs {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("parent directory")).expect("created directory");
        fs::write(path, body).expect("written file");
    }
    let cwd = root.join(&case.cwd);
    fs::create_dir_all(&cwd).expect("created directory");

    let mut command = match std::env::var_os("STOW_BIN") {
        Some(bin) => Command::new(bin),
        None => {
            let mut command = Command::new(env!("CARGO_BIN_EXE_cargo-stow"));
            command.arg("stow");
            command
        }
    };
    let output = command
        .args(&case.args)
        .current_dir(&cwd)
        .env("CARGO_NET_OFFLINE", "true")
        .output()
        .expect("stow binary runs");

    let mut after = BTreeMap::new();
    files_under(&root, &root, &mut after);
    let mut changed: BTreeMap<String, String> = after
        .iter()
        .filter(|(path, body)| match case.files.get(*path) {
            Some(input) => input != *body,
            None => !stubs.contains(path),
        })
        .map(|(path, body)| (path.clone(), body.clone()))
        .collect();
    for path in case.files.keys().filter(|path| !after.contains_key(*path)) {
        changed.insert(path.clone(), "<deleted>\n".to_owned());
    }

    Run {
        exit: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        changed,
    }
}

fn mismatches(case: &Case, run: &Run) -> Vec<String> {
    let mut problems = Vec::new();
    if case.exit.is_none() {
        problems.push("case has no `exit` section".to_owned());
    }
    if run.exit != case.exit {
        problems.push(format!(
            "exit: expected {:?}, got {:?}",
            case.exit, run.exit
        ));
    }
    if let Some(stdout) = &case.stdout {
        if *stdout != run.stdout {
            problems.push(format!("stdout: expected\n{stdout}got\n{}", run.stdout));
        }
    }
    for (path, expected) in &case.expect {
        match run.changed.get(path) {
            Some(actual) if actual == expected => {}
            Some(actual) => problems.push(format!("{path}: expected\n{expected}got\n{actual}")),
            None => problems.push(format!(
                "{path}: expected a change, file is untouched or missing"
            )),
        }
    }
    for (path, actual) in &run.changed {
        if !case.expect.contains_key(path) {
            problems.push(format!("{path}: unexpected change\n{actual}"));
        }
    }
    if !problems.is_empty() && !run.stderr.is_empty() {
        problems.push(format!("stderr:\n{}", run.stderr));
    }
    problems
}

#[test]
fn cases() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/cases");
    let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("tests/cases exists")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "case")
        })
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "no cases found in {}", dir.display());

    let bless = std::env::var_os("STOW_BLESS").is_some();
    let next = Mutex::new(paths.iter());
    let failures = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..std::thread::available_parallelism().map_or(4, usize::from) {
            scope.spawn(|| {
                loop {
                    let Some(path) = next.lock().expect("lock").next() else {
                        break;
                    };
                    let name = path
                        .file_name()
                        .expect("file name")
                        .to_string_lossy()
                        .into_owned();
                    let text = fs::read_to_string(path).expect("readable case");
                    let mut case = match parse(&text) {
                        Ok(case) => case,
                        Err(problem) => {
                            failures
                                .lock()
                                .expect("lock")
                                .push(format!("{name}: {problem}"));
                            continue;
                        }
                    };
                    let run = run(&case);
                    if bless {
                        case.exit = run.exit;
                        case.stdout = Some(run.stdout);
                        case.expect = run.changed;
                        fs::write(path, serialize(&case)).expect("written case");
                    } else {
                        let problems = mismatches(&case, &run);
                        if !problems.is_empty() {
                            let report = format!("{name}:\n  {}", problems.join("\n  "));
                            failures.lock().expect("lock").push(report);
                        }
                    }
                }
            });
        }
    });

    let mut failures = failures.into_inner().expect("lock");
    failures.sort();
    assert!(
        failures.is_empty(),
        "{} of {} cases failed:\n\n{}",
        failures.len(),
        paths.len(),
        failures.join("\n\n")
    );
}
