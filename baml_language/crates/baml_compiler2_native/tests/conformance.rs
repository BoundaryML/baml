//! Conformance of the standard library's natives: each file under
//! `tests/conformance` holds the cases of one native, as functions taking no
//! arguments. Every case runs on the interpreter, as a process of
//! `baml-cli run`, and as a native executable every way of `common::WAYS`,
//! and all must behave the same: the same exit code, the same bytes on
//! stdout and the same on stderr, but for what `comparable` leaves out.
//!
//! A file is a program of its own, with the files that declare what it
//! names: `support.baml`, where values come out of loops so that they are
//! built at run time, on the heap, and any other file whose helpers it
//! calls.
//!
//! What the interpreter does with a case is kept under `target/`, by a key
//! of the `baml-cli` binary, the text of the case's program and its name.
//! So a change to native code alone reruns nothing on the interpreter, a
//! change to a file reruns its cases and those of the files that name what
//! it declares, and a new `baml-cli` reruns all. The
//! cases of a `.baml.in` file talk to what lives for one run only, and are
//! never kept.
//!
//! A case native code is known to behave otherwise in is listed in
//! `tests/conformance/differences.txt`, with why and with what the native
//! program does. The interpreter must then do something else, so that the
//! entry goes when it stops being true. A native program a signal killed is
//! a failure whatever the list says.
//!
//! `BAML_CONFORMANCE_ONLY=a.baml,b.baml` runs the cases of those files
//! alone.

use std::{
    collections::BTreeMap,
    fmt::Write as _,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
};

use baml_db::testing::{assert_no_diagnostic_errors, setup_test_db};
use sha2::{Digest as _, Sha256};

mod common;
use common::{Built, WAYS, build, command, output};

/// What a program's caller sees of a run: how it exited, and what it wrote.
#[derive(Clone, PartialEq, Eq)]
struct Behavior {
    /// The exit code, or why there is none: the signal that killed the
    /// process, or why there was no process.
    exit: Result<i32, String>,
    stdout: String,
    stderr: String,
}

impl Behavior {
    fn of(output: &std::process::Output) -> Self {
        Self {
            exit: output.status.code().ok_or(output.status.to_string()),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
    }
}

/// The advice `baml-cli run` gives about the source it ran, which is not
/// the program's doing.
const FORMAT_HINT: &str = "warning: code is unformatted; run `baml fmt`";

/// The line that starts the frames of an error report.
const TRACEBACK: &str = "Traceback (most recent call last):";

/// `behavior` without what the interpreter and a native program are not
/// held to agree on. This is every such thing, and all of it is on stderr:
///
/// - the line of `FORMAT_HINT`, which only `baml-cli` writes;
/// - the frames of an error report, from the `TRACEBACK` line to the last
///   indented line after it: a native frame has no file and line;
/// - where `error: ` stands in a report: `baml-cli` writes it before the
///   traceback and a native program after, so what precedes `TRACEBACK` on
///   its line is put before the line that follows the frames.
fn comparable(behavior: &Behavior) -> Behavior {
    let mut stderr = String::new();
    let mut lines = behavior.stderr.split_inclusive('\n').peekable();
    while let Some(line) = lines.next() {
        let text = line.strip_suffix('\n').unwrap_or(line);
        if text == FORMAT_HINT {
            continue;
        }
        match text.strip_suffix(TRACEBACK) {
            Some(before) => {
                while lines.next_if(|frame| frame.starts_with("  ")).is_some() {}
                stderr.push_str(before);
            }
            None => stderr.push_str(line),
        }
    }
    Behavior {
        exit: behavior.exit.clone(),
        stdout: behavior.stdout.clone(),
        stderr,
    }
}

/// `text` on one line: a backslash doubled, and a line end or any other
/// control character as Rust writes it in a string. This is how
/// `differences.txt` holds what a program wrote, and how a report shows it.
fn escaped(text: &str) -> String {
    let mut out = String::new();
    for character in text.chars() {
        if character == '\\' || character.is_control() {
            out.extend(character.escape_default());
        } else {
            out.push(character);
        }
    }
    out
}

/// The `baml-cli` this tree builds, and where what it did is kept.
struct Interpreter {
    binary: PathBuf,
    /// The SHA-256 of the binary, in hex.
    hash: String,
    /// The directory of the kept behaviors, a file each.
    kept: PathBuf,
    /// The `BAML_HOME` its processes run with, so that nothing is read from
    /// or written to the user's own.
    home: PathBuf,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

/// Find the interpreter and check that it is this tree's: next to the test
/// binary's directory, where `cargo build -p baml_cli` with the same profile
/// puts it, or where `BAML_CONFORMANCE_CLI` says. Cargo writes beside it
/// the list of the files it was built from; one of them newer than the
/// binary means the binary is stale, and the test stops rather than compare
/// against it. The files under `.git` are left out of that: the binary
/// embeds the commit it was built at, which no behavior depends on.
fn interpreter() -> &'static Interpreter {
    static FOUND: OnceLock<Interpreter> = OnceLock::new();
    FOUND.get_or_init(|| {
        let how = "build it with `cargo build --profile <the profile of this test run> -p \
                   baml_cli --bin baml-cli`, or name one with BAML_CONFORMANCE_CLI";
        let binary = match std::env::var_os("BAML_CONFORMANCE_CLI") {
            Some(named) => PathBuf::from(named),
            None => std::env::current_exe()
                .expect("the test binary has a path")
                .parent()
                .and_then(Path::parent)
                .expect("the test binary is in `deps` under a profile directory")
                .join("baml-cli"),
        };
        let built = std::fs::metadata(&binary)
            .and_then(|metadata| metadata.modified())
            .unwrap_or_else(|error| panic!("no `{}` ({error}): {how}", binary.display()));
        let listed = binary.with_extension("d");
        let sources = std::fs::read_to_string(&listed).unwrap_or_else(|error| {
            panic!(
                "no `{}` ({error}) to check `{}` against its sources: {how}",
                listed.display(),
                binary.display()
            )
        });
        let (_, sources) = sources.split_once(": ").expect("a list of sources");
        for source in sources.split_whitespace() {
            if source.contains("/.git/") {
                continue;
            }
            let fresh = std::fs::metadata(source)
                .and_then(|metadata| metadata.modified())
                .is_ok_and(|changed| changed <= built);
            assert!(
                fresh,
                "`{}` is older than `{source}`, which it was built from: {how}",
                binary.display()
            );
        }
        let hash = hex(&Sha256::digest(
            std::fs::read(&binary).expect("a readable interpreter"),
        ));
        // One directory for each binary, and no other left: nothing kept
        // for a binary that is gone is ever read again.
        let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("conformance");
        let own = root.join(&hash[..16]);
        if !own.exists() {
            let _ = std::fs::remove_dir_all(&root);
        }
        let (kept, home) = (own.join("kept"), own.join("home"));
        for directory in [&kept, &home] {
            std::fs::create_dir_all(directory).expect("a directory under `target`");
        }
        Interpreter {
            binary,
            hash,
            kept,
            home,
        }
    })
}

/// What the interpreter does with `case` of the program in the directory
/// `project`: read from the file `key` names where there is a key and the
/// file, and otherwise run, and kept under the key where there is one.
fn interpreted(project: &Path, case: &str, key: Option<&str>) -> Behavior {
    let interpreter = interpreter();
    let file = key.map(|key| interpreter.kept.join(key));
    if let Some(kept) = file.as_ref().and_then(|file| std::fs::read(file).ok()) {
        let kept = String::from_utf8(kept).expect("a kept behavior is UTF-8");
        let mut parts = kept.splitn(3, '\n');
        let mut number = || parts.next().and_then(|part| part.parse::<i32>().ok());
        let (exit, length) = (number(), number());
        let (stdout, stderr) = parts
            .next()
            .expect("a kept behavior has three parts")
            .split_at(usize::try_from(length.expect("a length")).expect("a length"));
        return Behavior {
            exit: Ok(exit.expect("an exit code")),
            stdout: stdout.to_owned(),
            stderr: stderr.to_owned(),
        };
    }
    let ran = Command::new(&interpreter.binary)
        .args(["run", "--quiet", "--agent-skill-check", "off"])
        .args(["--output-preset", "human", "--color", "never"])
        .arg("--project-dir")
        .arg(project)
        .arg(case)
        .env("BAML_TELEMETRY", "off")
        .env("BAML_CLI_ALLOW_DIRECT", "1")
        .env("BAML_HOME", &interpreter.home)
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap_or_else(|error| {
            panic!("`{}` does not start: {error}", interpreter.binary.display())
        });
    let behavior = Behavior::of(&ran);
    assert!(
        !behavior.stderr.contains("error: cannot run: "),
        "the interpreter did not run `{case}`:\n{}",
        behavior.stderr
    );
    if let (Some(file), Ok(exit)) = (file, &behavior.exit) {
        // Whole or not at all: another run may be reading.
        let partial = file.with_extension(std::process::id().to_string());
        let kept = format!(
            "{exit}\n{}\n{}{}",
            behavior.stdout.len(),
            behavior.stdout,
            behavior.stderr
        );
        std::fs::write(&partial, kept).expect("a writable `target`");
        std::fs::rename(&partial, &file).expect("a writable `target`");
    }
    behavior
}

/// A case native code is known to behave otherwise in: why, and what the
/// native program does, its output as `escaped` writes it.
struct Difference {
    reason: String,
    exit: i32,
    stdout: String,
    stderr: String,
}

impl Difference {
    fn is(&self, behavior: &Behavior) -> bool {
        behavior.exit == Ok(self.exit)
            && escaped(&behavior.stdout) == self.stdout
            && escaped(&behavior.stderr) == self.stderr
    }
}

/// An entry of `differences.txt` for a case that behaves as `native`.
fn entry(case: &str, reason: &str, native: &Behavior) -> String {
    let mut entry = format!("{case}: {reason}\n");
    if let Ok(exit) = native.exit
        && exit != 0
    {
        let _ = writeln!(entry, "  exit {exit}");
    }
    for (stream, written) in [("stdout", &native.stdout), ("stderr", &native.stderr)] {
        if !written.is_empty() {
            let _ = writeln!(entry, "  {stream} {}", escaped(written));
        }
    }
    entry
}

/// The cases of `differences.txt`, by name. An entry is a line `<case>:
/// <why>` and then, each on a line of its own that two spaces indent, what
/// the native program does: `exit <code>` unless it is 0, `stdout <text>`
/// and `stderr <text>` unless the text is empty, after `comparable` and as
/// `escaped` writes it. A line that starts with `#` is a comment.
fn differences() -> BTreeMap<String, Difference> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/conformance/differences.txt");
    let text = std::fs::read_to_string(&path).expect("a readable `differences.txt`");
    let mut found: Vec<(String, Difference)> = Vec::new();
    for (number, line) in text.lines().enumerate() {
        let malformed =
            || -> ! { panic!("{}:{}: not an entry: {line}", path.display(), number + 1) };
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some(field) = line.strip_prefix("  ") else {
            let Some((case, reason)) = line.split_once(": ") else {
                malformed()
            };
            found.push((
                case.to_owned(),
                Difference {
                    reason: reason.to_owned(),
                    exit: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                },
            ));
            continue;
        };
        let (Some((_, entry)), Some((name, value))) = (found.last_mut(), field.split_once(' '))
        else {
            malformed()
        };
        match name {
            "exit" => entry.exit = value.parse().unwrap_or_else(|_| malformed()),
            "stdout" => value.clone_into(&mut entry.stdout),
            "stderr" => value.clone_into(&mut entry.stderr),
            _ => malformed(),
        }
    }
    let count = found.len();
    let found: BTreeMap<_, _> = found.into_iter().collect();
    assert_eq!(
        found.len(),
        count,
        "a case is listed twice in `{}`",
        path.display()
    );
    found
}

/// Every file under `directory` whose name ends with `ending`, in a stable
/// order.
fn sources(directory: &Path, ending: &str, found: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("cannot read `{}`: {error}", directory.display()))
        .map(|entry| entry.expect("a directory entry").path())
        .collect();
    entries.sort();
    for entry in entries {
        if entry.is_dir() {
            sources(&entry, ending, found);
        } else if entry.to_string_lossy().ends_with(ending) {
            found.push(entry);
        }
    }
}

/// The functions `source` declares without parameters: its cases.
fn cases(source: &str) -> Vec<&str> {
    source
        .lines()
        .filter_map(|line| Some(line.strip_prefix("function ")?.split_once("()")?.0))
        .collect()
}

/// The names `source` declares at its top level.
fn declared(source: &str) -> impl Iterator<Item = &str> {
    source.lines().filter_map(|line| {
        let rest = ["function ", "class ", "enum ", "interface "]
            .iter()
            .find_map(|keyword| line.strip_prefix(keyword))?;
        rest.split(|character: char| !(character.is_alphanumeric() || character == '_'))
            .next()
    })
}

/// The files of `all` that `file` needs to be a program: each that declares
/// a name `file` has as a word, and each those need. A word in a comment
/// counts, which only makes a program larger than it has to be.
fn needed<'a>(
    file: &'a (String, String),
    all: &'a [(String, String)],
) -> Vec<&'a (String, String)> {
    let mut found = vec![file];
    let mut at = 0;
    while let Some((_, text)) = found.get(at) {
        let words: std::collections::HashSet<&str> = text
            .split(|character: char| !(character.is_alphanumeric() || character == '_'))
            .collect();
        for other in all {
            if !found.iter().any(|known| std::ptr::eq(*known, other))
                && declared(&other.1).any(|name| words.contains(name))
            {
                found.push(other);
            }
        }
        at += 1;
    }
    found.remove(0);
    found
}

/// The cases of one file, which with `shared` are a program.
struct Unit<'a> {
    /// The file's name, as a report gives it and as the interpreter sees it.
    file: &'a str,
    text: &'a str,
    /// The files that declare what this one names.
    shared: Vec<&'a (String, String)>,
    /// Whether what the interpreter does with a case may be kept.
    kept: bool,
}

/// The report of a case that does not behave as expected: as the
/// interpreter does, or as `listed` says where the case is listed, which
/// the interpreter then must not. Every behavior here is `comparable`.
fn disagreement(
    interpreter: &Behavior,
    native: &[(&'static str, Behavior)],
    listed: Option<&Difference>,
) -> Option<String> {
    let dead = native.iter().any(|(_, behavior)| behavior.exit.is_err());
    let expected = |behavior: &Behavior| match listed {
        Some(listed) => listed.is(behavior),
        None => behavior == interpreter,
    };
    let stale = listed.is_some_and(|listed| listed.is(interpreter));
    if !dead && !stale && native.iter().all(|(_, behavior)| expected(behavior)) {
        return None;
    }
    let mut report = String::new();
    if dead {
        report.push_str("  a native program did not run to an exit, which nothing can list\n");
    }
    if let Some(listed) = listed.filter(|_| stale) {
        let _ = writeln!(
            report,
            "  listed in `differences.txt` ({}), and the interpreter does as listed",
            listed.reason
        );
    }
    // The exit, the stdout and the stderr of a run, each on a line.
    let fields = |behavior: &Behavior| {
        let exit = match &behavior.exit {
            Ok(code) => code.to_string(),
            Err(why) => why.clone(),
        };
        [exit, escaped(&behavior.stdout), escaped(&behavior.stderr)]
    };
    let interpreted = fields(interpreter);
    let wanted = listed.map(|listed| {
        [
            listed.exit.to_string(),
            listed.stdout.clone(),
            listed.stderr.clone(),
        ]
    });
    let native: Vec<_> = native
        .iter()
        .map(|(way, behavior)| (way, fields(behavior)))
        .collect();
    for (field, name) in ["exit", "stdout", "stderr"].iter().enumerate() {
        let interpreted = &interpreted[field];
        let wanted = wanted.as_ref().map(|wanted| &wanted[field]);
        // A field every run agrees on says nothing.
        if native
            .iter()
            .all(|(_, fields)| fields[field] == *interpreted)
            && wanted.is_none_or(|wanted| wanted == interpreted)
        {
            continue;
        }
        let _ = writeln!(report, "  {name}");
        let _ = writeln!(report, "    {:<26}{interpreted}", "interpreter");
        if let Some(wanted) = wanted {
            let _ = writeln!(report, "    {:<26}{wanted}", "listed");
        }
        let (_, first) = &native[0];
        if native
            .iter()
            .all(|(_, fields)| fields[field] == first[field])
        {
            let _ = writeln!(report, "    {:<26}{}", "native, every way", first[field]);
            continue;
        }
        for (way, fields) in &native {
            let _ = writeln!(report, "    {way:<26}{}", fields[field]);
        }
    }
    Some(report)
}

/// Run every case of `unit` on the interpreter and every way a native
/// program runs. Returns how many cases ran and a report for each that
/// does not behave as expected.
fn check(unit: &Unit, differences: &BTreeMap<String, Difference>) -> (usize, Vec<String>) {
    let program: String = unit
        .shared
        .iter()
        .map(|(_, text)| text.as_str())
        .chain([unit.text])
        .collect();
    let db = setup_test_db(&program);
    assert_no_diagnostic_errors(&db);
    let executables = tempfile::tempdir().expect("a temporary directory");

    // The program as the interpreter loads it: its files in a directory.
    let project = tempfile::tempdir().expect("a temporary directory");
    let file = Path::new(unit.file)
        .file_name()
        .expect("a file has a name")
        .to_string_lossy();
    let written = unit
        .shared
        .iter()
        .map(|(name, text)| (name.replace('/', "."), text.as_str()))
        .chain([(file.trim_end_matches(".in").to_owned(), unit.text)]);
    for (name, text) in written {
        std::fs::write(project.path().join(name), text).expect("a writable directory");
    }

    let mut failures = Vec::new();
    let found = cases(unit.text);
    for case in &found {
        let key = unit.kept.then(|| {
            let mut hasher = Sha256::new();
            let parts = [interpreter().hash.as_str(), &program, case];
            for part in parts {
                hasher.update(part.len().to_le_bytes());
                hasher.update(part);
            }
            hex(&hasher.finalize())
        });
        let interpreter = comparable(&interpreted(project.path(), case, key.as_deref()));
        let built: Result<Built, String> = build(&db, case, executables.path());
        let native: Vec<(&'static str, Behavior)> = WAYS
            .iter()
            .map(|way| {
                let behavior = match &built {
                    Ok(built) => comparable(&Behavior::of(&output(built, way))),
                    Err(error) => Behavior {
                        exit: Err(error.clone()),
                        stdout: String::new(),
                        stderr: String::new(),
                    },
                };
                (way.name, behavior)
            })
            .collect();
        let listed = differences.get(*case);
        if let Some(mut report) = disagreement(&interpreter, &native, listed) {
            let (_, first) = &native[0];
            if listed.is_none() && native.iter().all(|(_, behavior)| behavior == first) {
                let _ = write!(report, "  as an entry:\n{}", entry(case, "<why>", first));
            }
            failures.push(format!("{}: {case}\n{report}", unit.file));
        }
    }
    (found.len(), failures)
}

/// The files under `tests/conformance` whose names end with `ending`, each
/// with its name from there and its text.
fn texts(ending: &str) -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/conformance");
    let mut files = Vec::new();
    sources(&root, ending, &mut files);
    files
        .iter()
        .map(|file| {
            let name = file.strip_prefix(&root).expect("under the root");
            let text = std::fs::read_to_string(file).expect("a readable source");
            (name.display().to_string(), text)
        })
        .collect()
}

#[test]
fn natives_behave_as_the_interpreter_does() {
    let mut all = texts(".baml");
    // The longest first, so that no worker is left alone with one at the
    // end.
    all.sort_by_key(|(_, text)| std::cmp::Reverse(cases(text).len()));
    let only = std::env::var("BAML_CONFORMANCE_ONLY").ok();
    let units: Vec<Unit> = all
        .iter()
        .filter(|(file, _)| {
            only.as_ref()
                .is_none_or(|only| only.split(',').any(|name| file.contains(name)))
        })
        .map(|file| Unit {
            file: &file.0,
            text: &file.1,
            shared: needed(file, &all),
            kept: true,
        })
        .collect();
    let differences = differences();
    // Checked before the workers start, so that a stale interpreter is one
    // failure and not one a worker.
    interpreter();

    let next = AtomicUsize::new(0);
    let results = Mutex::new((0, Vec::new()));
    let workers = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                while let Some(unit) = units.get(next.fetch_add(1, Ordering::Relaxed)) {
                    let (ran, failures) = check(unit, &differences);
                    let mut results = results.lock().expect("no worker panicked with the lock");
                    results.0 += ran;
                    results.1.extend(failures);
                }
            });
        }
    });
    let (ran, mut failures) = results
        .into_inner()
        .expect("no worker panicked with the lock");
    failures.sort();
    assert!(ran > 0, "no conformance cases");
    assert!(
        failures.is_empty(),
        "{} of {ran} cases:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// An entry of `differences.txt` is checked when its case runs, so one
/// that names no case would never be.
#[test]
fn every_listed_difference_names_a_case() {
    let all = [texts(".baml"), texts(".baml.in")].concat();
    let named: Vec<&str> = all.iter().flat_map(|(_, text)| cases(text)).collect();
    for case in differences().keys() {
        assert!(
            named.contains(&case.as_str()),
            "`differences.txt` lists `{case}`, which is no case"
        );
    }
}

/// Answer one request on `stream`, and close it.
fn answer(mut stream: std::net::TcpStream) {
    use std::io::{Read as _, Write as _};

    let mut request = Vec::new();
    let mut chunk = [0; 1024];
    let head_end = loop {
        if let Some(at) = request.windows(4).position(|window| window == b"\r\n\r\n") {
            break at + 4;
        }
        match stream.read(&mut chunk) {
            Ok(read) if read > 0 => request.extend_from_slice(&chunk[..read]),
            _ => return,
        }
    };
    let head = String::from_utf8_lossy(&request[..head_end]).into_owned();
    let header = |name: &str| {
        head.lines().skip(1).find_map(|line| {
            let (found, value) = line.split_once(':')?;
            found.eq_ignore_ascii_case(name).then(|| value.trim())
        })
    };
    let length = header("content-length")
        .and_then(|length| length.parse().ok())
        .unwrap_or(0);
    while request.len() < head_end + length {
        match stream.read(&mut chunk) {
            Ok(read) if read > 0 => request.extend_from_slice(&chunk[..read]),
            _ => return,
        }
    }
    let mut words = head.split(' ');
    let (method, path) = (words.next().unwrap_or(""), words.next().unwrap_or(""));
    let (echoed, large);
    let (status, kind, body): (&str, &str, &[u8]) = match path {
        "/hello" => (
            "200 OK",
            "text/plain",
            "hello from loopback, caf\u{e9}".as_bytes(),
        ),
        "/bytes" => ("200 OK", "text/plain", b"not \xFF utf-8"),
        "/slow" => {
            std::thread::sleep(std::time::Duration::from_secs(3));
            ("200 OK", "text/plain", b"late")
        }
        "/echo" => {
            echoed = format!(
                "{method} x-custom={} content-type={} body={}",
                header("x-custom").unwrap_or("none"),
                header("content-type").unwrap_or("none"),
                String::from_utf8_lossy(&request[head_end..])
            );
            ("200 OK", "text/plain", echoed.as_bytes())
        }
        // Events whose lines the chunks split, the last without the blank
        // line that ends an event.
        "/events" => {
            let _ = stream.write_all(
                b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n",
            );
            for piece in [
                "event: greet\ndata: hello\nid: 1\n\n: a comment\nda",
                "ta: split\r\ndata: lines\r\n\r",
                "\ndata: last",
            ] {
                let _ = stream.write_all(piece.as_bytes());
                let _ = stream.flush();
                std::thread::sleep(std::time::Duration::from_millis(60));
            }
            return;
        }
        // A body that ends before the length the head promised.
        "/cut" => {
            let _ = stream.write_all(
                b"HTTP/1.1 200 OK\r\ncontent-type: text/plain\r\ncontent-length: 1000\r\n\
                  connection: close\r\n\r\nonly this much",
            );
            return;
        }
        "/large" => {
            large = large_body();
            ("200 OK", "text/plain", large.as_slice())
        }
        // An event whose character, whose line end and whose blank line
        // the chunks split.
        "/events-split" => {
            let pieces: [&[u8]; 4] = [
                b"id: 7\nevent: spl",
                b"it\ndata: caf\xc3",
                b"\xa9 au lait\r",
                b"\n\r\ndata: whole\n\n",
            ];
            in_pieces(&mut stream, "text/event-stream", &pieces);
            return;
        }
        // One event, and then a chunk the connection closes in the middle
        // of.
        "/events-cut" => {
            let _ = stream.write_all(
                b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\n\
                  transfer-encoding: chunked\r\n\r\nd\r\ndata: first\n\n\r\n",
            );
            let _ = stream.flush();
            std::thread::sleep(std::time::Duration::from_millis(60));
            let _ = stream.write_all(b"40\r\ndata: never whole");
            return;
        }
        // Three messages of an AWS event stream, in pieces that end inside
        // a message.
        "/aws-events" => {
            let mut frames = aws_frame(
                &[(":message-type", "event"), (":event-type", "chunk")],
                b"{\"bytes\":\"aGVsbG8=\"}",
            );
            frames.extend(aws_frame(
                &[
                    (":message-type", "exception"),
                    (":exception-type", "throttlingException"),
                ],
                b"{\"message\":\"slow down\"}",
            ));
            frames.extend(aws_frame(
                &[
                    (":message-type", "error"),
                    (":error-code", "InternalError"),
                    (":error-message", "it broke"),
                ],
                b"",
            ));
            let pieces = [
                &frames[..7],
                &frames[7..60],
                &frames[60..150],
                &frames[150..],
            ];
            in_pieces(&mut stream, "application/vnd.amazon.eventstream", &pieces);
            return;
        }
        // A message, and then one whose checksum is wrong.
        "/aws-events-bad" => {
            let headers = [(":message-type", "event"), (":event-type", "chunk")];
            let good = aws_frame(&headers, b"good");
            let mut bad = aws_frame(&headers, b"bad");
            *bad.last_mut().expect("a frame has a checksum") ^= 0xFF;
            in_pieces(
                &mut stream,
                "application/vnd.amazon.eventstream",
                &[&good, &bad],
            );
            return;
        }
        // A message the connection closes one byte short of.
        "/aws-events-short" => {
            let frame = aws_frame(&[(":message-type", "event")], b"cut");
            in_pieces(
                &mut stream,
                "application/vnd.amazon.eventstream",
                &[&frame[..frame.len() - 1]],
            );
            return;
        }
        "/aws-events-odd" => {
            let frame = aws_frame(&[(":message-type", "weird")], b"");
            in_pieces(&mut stream, "application/vnd.amazon.eventstream", &[&frame]);
            return;
        }
        _ => ("404 Not Found", "text/plain", b"nothing here"),
    };
    let head = format!(
        "HTTP/1.1 {status}\r\ncontent-type: {kind}\r\nx-asked: {path}\r\ncontent-length: \
         {}\r\nconnection: close\r\n\r\n",
        body.len()
    );
    // The client may have given up: a late answer has no reader.
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
}

/// Answer a request with a body of `kind` that arrives as `pieces`, each
/// some time after the one before it, and ends when the connection closes.
fn in_pieces(stream: &mut std::net::TcpStream, kind: &str, pieces: &[&[u8]]) {
    use std::io::Write as _;

    let head = format!("HTTP/1.1 200 OK\r\ncontent-type: {kind}\r\nconnection: close\r\n\r\n");
    let _ = stream.write_all(head.as_bytes());
    for piece in pieces {
        let _ = stream.write_all(piece);
        let _ = stream.flush();
        std::thread::sleep(std::time::Duration::from_millis(60));
    }
}

/// A body of several mebibytes of text, no two of its lines alike.
fn large_body() -> Vec<u8> {
    let mut body = Vec::new();
    let mut line = 0_u32;
    while body.len() < 5 * 1024 * 1024 {
        body.extend_from_slice(format!("line {line} of a large body\n").as_bytes());
        line += 1;
    }
    body
}

/// The checksum an AWS event stream carries: CRC-32 as zlib computes it.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & (crc & 1).wrapping_neg());
        }
    }
    !crc
}

/// One message of an AWS event stream: its string headers and its payload,
/// between the lengths and the checksums of the framing.
fn aws_frame(headers: &[(&str, &str)], payload: &[u8]) -> Vec<u8> {
    let be =
        |length: usize, width: usize| length.to_be_bytes()[size_of::<usize>() - width..].to_vec();
    let mut encoded = Vec::new();
    for (name, value) in headers {
        encoded.extend(be(name.len(), 1));
        encoded.extend_from_slice(name.as_bytes());
        // The type of a string header.
        encoded.push(7);
        encoded.extend(be(value.len(), 2));
        encoded.extend_from_slice(value.as_bytes());
    }
    let mut frame = be(16 + encoded.len() + payload.len(), 4);
    frame.extend(be(encoded.len(), 4));
    frame.extend(crc32(&frame).to_be_bytes());
    frame.extend(encoded);
    frame.extend_from_slice(payload);
    frame.extend(crc32(&frame).to_be_bytes());
    frame
}

/// Serve HTTP on a loopback port until the test process ends, and return
/// the port. Every connection is answered once and closed.
fn serve_http() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("a bound address").port();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            std::thread::spawn(move || answer(stream));
        }
    });
    port
}

/// A loopback port nothing listens on.
fn closed_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    listener.local_addr().expect("a bound address").port()
}

/// A loopback port where connecting neither succeeds nor is refused: a
/// listener that accepts nothing, whose queue of connections this fills.
/// The system drops what asks to connect after that, so a connect waits
/// until its limit passes. The listener and the connections that fill its
/// queue stay open until the test process ends.
fn full_port() -> u16 {
    static PORT: std::sync::OnceLock<u16> = std::sync::OnceLock::new();
    *PORT.get_or_init(|| {
        let socket = tokio::net::TcpSocket::new_v4().expect("a socket");
        socket
            .bind("127.0.0.1:0".parse().expect("an address"))
            .expect("a loopback port");
        let listener = socket.listen(1).expect("a listener");
        let address = listener.local_addr().expect("a bound address");
        std::mem::forget(listener);
        for _ in 0..64 {
            let limit = std::time::Duration::from_millis(300);
            match std::net::TcpStream::connect_timeout(&address, limit) {
                Ok(queued) => std::mem::forget(queued),
                Err(error) if error.kind() == std::io::ErrorKind::TimedOut => {
                    return address.port();
                }
                Err(error) => panic!("cannot fill the queue of `{address}`: {error}"),
            }
        }
        panic!("the queue of `{address}` never filled");
    })
}

/// Run every case of `source`, the text of `file` and a program of its
/// own, on the interpreter and every way a native program runs, and fail
/// when one does not behave as expected. What it talks to lives for this
/// run only, so nothing the interpreter does is kept.
fn cases_agree(file: &str, source: &str) {
    let unit = Unit {
        file,
        text: source,
        shared: Vec::new(),
        kept: false,
    };
    let (_, failures) = check(&unit, &differences());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// `baml.http` against a server in this process: a request is sent by the
/// runtime's own client, and the `Response` or the `SseStream` it returns
/// is built in Rust and owns what of the answer has not been read.
#[tokio::test(flavor = "multi_thread")]
async fn http_requests_end_as_the_interpreter_does() {
    // A proxy named by the environment would be sent these requests.
    for variable in ["ALL_PROXY", "all_proxy", "HTTP_PROXY", "http_proxy"] {
        assert!(
            std::env::var_os(variable).is_none(),
            "unset `{variable}` to run this test: it must not leave the machine"
        );
    }
    let source = include_str!("conformance/http.baml.in")
        .replace("{open}", &format!("http://127.0.0.1:{}", serve_http()))
        .replace("{closed}", &format!("http://127.0.0.1:{}", closed_port()))
        .replace("{full}", &format!("http://127.0.0.1:{}", full_port()));
    cases_agree("http.baml.in", &source);
}

/// `baml.fs` in a directory of the test's own, which every case leaves as
/// it found it.
#[test]
fn files_end_as_the_interpreter_does() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let path = directory
        .path()
        .canonicalize()
        .expect("the directory exists");
    let source = include_str!("conformance/fs.baml.in")
        .replace("{dir}", path.to_str().expect("a path that is UTF-8"));
    cases_agree("fs.baml.in", &source);
    let left: Vec<_> = std::fs::read_dir(&path)
        .expect("the directory is still there")
        .map(|entry| entry.expect("an entry").file_name())
        .collect();
    assert!(left.is_empty(), "a case left {left:?} behind");
}

/// A loopback UDP port nothing is bound to.
fn free_udp_port() -> u16 {
    let socket = std::net::UdpSocket::bind("127.0.0.1:0").expect("a loopback port");
    socket.local_addr().expect("a bound address").port()
}

/// `baml.net` between the tasks of one program, on loopback ports that are
/// free when the test starts.
#[tokio::test(flavor = "multi_thread")]
async fn sockets_end_as_the_interpreter_does() {
    let source = include_str!("conformance/net.baml.in")
        .replace("{tcp}", &closed_port().to_string())
        .replace("{closed}", &closed_port().to_string())
        .replace("{full}", &full_port().to_string())
        .replace("{udp_a}", &free_udp_port().to_string())
        .replace("{udp_b}", &free_udp_port().to_string());
    cases_agree("net.baml.in", &source);
}

/// `baml.io` reads the program's own input, which no case of the suites
/// above has any of. So this checks the native program alone: what it
/// prints before its result, in order, on each stream, and a line read from
/// its input with what was left of the input for the next read.
#[test]
fn a_program_prints_and_reads_its_own_streams() {
    use std::io::Write as _;

    let db = setup_test_db(
        "function main() -> string {
  baml.io.print(\"a\");
  baml.io.println(\"b\");
  baml.io.eprint(\"c\");
  baml.io.eprintln(\"d\");
  let first = baml.io.input(\"name? \");
  let second = baml.io.input(null);
  let third = baml.io.input(null);
  `${first}|${second}|${third}`
}
",
    );
    assert_no_diagnostic_errors(&db);
    let directory = tempfile::tempdir().expect("a temporary directory");
    let built = build(&db, "main", directory.path()).expect("the program compiles");
    let mut child = std::process::Command::new(&built.lean)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("the program starts");
    child
        .stdin
        .take()
        .expect("a piped input")
        .write_all(b"Ada\r\nLovelace\n")
        .expect("the program reads its input");
    let output = child.wait_with_output().expect("the program ends");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "ab\nname? \"Ada|Lovelace|\"\n"
    );
    assert_eq!(String::from_utf8_lossy(&output.stderr), "cd\n");
}

/// Every process there is, as `ps` lists it: its id, its parent's, and
/// its state with its command.
fn processes() -> Vec<(u32, u32, String)> {
    let listed = std::process::Command::new("/bin/ps")
        .args(["-axo", "pid=,ppid=,stat=,command="])
        .output()
        .expect("`ps` runs");
    String::from_utf8_lossy(&listed.stdout)
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let pid = fields.next()?.parse().ok()?;
            let parent = fields.next()?.parse().ok()?;
            Some((pid, parent, fields.collect::<Vec<_>>().join(" ")))
        })
        .collect()
}

/// A program `baml.sys.run` stops at its limit is gone when the error is
/// thrown: killed, and waited for, so that it is neither still running nor
/// left for the system as a process that has ended and that nothing asked
/// about. No case of the suites above can see another process, so this
/// looks at the native program's children while it waits for a line of
/// input after the error, and for the child once the program has ended.
#[test]
fn a_program_stopped_at_its_limit_is_killed_and_waited_for() {
    use std::io::{BufRead as _, Write as _};

    // Nothing else sleeps for this long.
    let nap = format!("97.{}", std::process::id());
    let db = setup_test_db(&format!(
        "function limited() -> string {{
  let ran = baml.sys.run(\"/bin/sleep\", args = [\"{nap}\"], timeout = baml.time.Duration.from_milliseconds(200)) catch (e) {{
    baml.errors.Io => {{ return \"io\"; }},
    baml.errors.Timeout => {{ return \"timed out\"; }},
    baml.errors.InvalidArgument => {{ return \"invalid\"; }},
  }};
  \"ran\"
}}
function main() -> string {{
  baml.io.println(limited());
  baml.io.input(null)
}}
"
    ));
    assert_no_diagnostic_errors(&db);
    let directory = tempfile::tempdir().expect("a temporary directory");
    let built = build(&db, "main", directory.path()).expect("the program compiles");
    for way in &WAYS {
        let mut program = command(&built, way)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("the program starts");
        let mut printed = std::io::BufReader::new(program.stdout.take().expect("a piped output"));
        let mut line = String::new();
        printed.read_line(&mut line).expect("a line");
        let children: Vec<_> = processes()
            .into_iter()
            .filter(|(_, parent, _)| *parent == program.id())
            .collect();
        program
            .stdin
            .take()
            .expect("a piped input")
            .write_all(b"\n")
            .expect("the program reads its input");
        let status = program.wait().expect("the program ends");
        let left: Vec<_> = processes()
            .into_iter()
            .filter(|(_, _, command)| command.contains(&nap))
            .collect();
        for (pid, _, _) in &left {
            let _ = std::process::Command::new("/bin/kill")
                .arg(pid.to_string())
                .status();
        }
        let way = way.name;
        assert_eq!(line, "timed out\n", "{way}");
        assert_eq!(status.code(), Some(0), "{way}");
        assert!(
            children.is_empty() && left.is_empty(),
            "{way}: while the program waited it had the children {children:?}, and once it had \
             ended {left:?} was left"
        );
    }
}

/// Build `source`'s `main` and run it every way: what it prints, what it
/// reports and the status it exits with.
fn ends(source: &str, stdout: &str, stderr: &str) {
    let db = setup_test_db(source);
    assert_no_diagnostic_errors(&db);
    let directory = tempfile::tempdir().expect("a temporary directory");
    let built = build(&db, "main", directory.path()).expect("the program compiles");
    for way in &WAYS {
        let output = output(&built, way);
        let way = way.name;
        assert_eq!(output.status.code(), Some(1), "{way}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), stdout, "{way}");
        assert_eq!(String::from_utf8_lossy(&output.stderr), stderr, "{way}");
    }
}

/// What the two programs below share.
const FAILING: &str = r"
class Failure { code: int }

function count(n: int) -> int {
  let x = 0;
  for (let i = 0; i < n; i += 1) { x += 1; };
  x
}

function fail(depth: int, code: int) -> int throws Failure {
  if depth > 0 {
    return fail(depth - 1, code) + 1;
  }
  if code > 0 { throw Failure { code: code } }
  0
}
";

/// A program whose spawned task fails with nothing awaiting it: `baml run`
/// prints the entry function's result, waits for every task, reports the
/// error no `await` took and exits with a failure, and so does native code.
/// The report is `baml run`'s but for the lines of the traceback, which
/// name no file and line here, and for where they are: before the error, as
/// for the entry function's own. Tasks are reported in the order they were
/// spawned in.
#[test]
fn an_error_no_await_took_fails_the_program_after_its_result() {
    let source = format!(
        "{FAILING}
function main() -> int {{
  let ignored = spawn {{ 10 / count(0) }};
  let thrown = spawn {{ fail(count(1), 4) }};
  let awaited = spawn {{ count(7) }};
  await awaited
}}
"
    );
    ends(
        &source,
        "7\n",
        "Traceback (most recent call last):\n  in .<lambda(main, 0)>\n\
         error: unhandled spawned task failed: uncaught throw: \
         baml.panics.DivisionByZero {dividend: 10}\n\
         Traceback (most recent call last):\n  in .<lambda(main, 1)>\n  in user.fail\n  \
         in user.fail\nerror: unhandled spawned task failed: uncaught throw: \
         user.Failure {code: 4}\n",
    );
}

/// A program whose entry function ends with an error it awaited, while
/// another task's error was never awaited: the entry function's error is
/// reported first, with the functions the task that threw it was in, then
/// the error no `await` took, and nothing is printed.
#[test]
fn an_uncaught_error_of_the_entry_function_is_reported_before_one_no_await_took() {
    let source = format!(
        "{FAILING}
function main() -> int throws Failure {{
  let ignored = spawn {{ 10 / count(0) }};
  let awaited = spawn {{ fail(count(2), 3) }};
  await awaited
}}
"
    );
    ends(
        &source,
        "",
        "Traceback (most recent call last):\n  in .<lambda(main, 1)>\n  in user.fail\n  \
         in user.fail\n  in user.fail\nerror: uncaught throw: user.Failure {code: 3}\n\
         Traceback (most recent call last):\n  in .<lambda(main, 0)>\n\
         error: unhandled spawned task failed: uncaught throw: \
         baml.panics.DivisionByZero {dividend: 10}\n",
    );
}

/// Why `source` is not compiled, as the entry `main`.
fn refused(source: &str) -> String {
    let db = setup_test_db(source);
    assert_no_diagnostic_errors(&db);
    let directory = tempfile::tempdir().expect("a temporary directory");
    match build(&db, "main", directory.path()) {
        Ok(_) => "compiled".to_owned(),
        Err(error) => error,
    }
}

/// The runtime's walk of two values calls the `eq` of a class by its type
/// id. What it has no entry for it would compare by variant or by field,
/// so a program where it could meet one is refused, and the refusal names
/// the type: an enum with an `eq` held in a class, in an array or in a
/// union, and a class with type parameters.
#[test]
fn comparing_values_with_an_eq_the_runtime_cannot_call_is_refused() {
    let refusal = "comparing values in a program with a type whose own eq the runtime cannot call";
    let shade = "enum Shade { Light Pale }
implement baml.ops.Equals for Shade {
  function eq(self, other: Self) -> bool throws never { true }
}
function shade(n: int) -> Shade { if (n == 0) { Shade.Light } else { Shade.Pale } }
";
    for (held, compared) in [
        (
            "a class",
            "class Holder { shade Shade }
function main() -> bool { Holder { shade: shade(0) } == Holder { shade: shade(1) } }",
        ),
        (
            "an array",
            "function main() -> bool { [shade(0), shade(1)] == [shade(1), shade(0)] }",
        ),
        (
            "a union",
            "function either(n: int) -> int | Shade { if (n > 5) { n } else { shade(n) } }
function main() -> string { `${either(0) == either(1)} ${either(7) == either(1)}` }",
        ),
    ] {
        let of_an_enum = refused(&format!("{shade}{compared}\n"));
        assert!(
            of_an_enum.contains(refusal) && of_an_enum.contains("Shade"),
            "in {held}: {of_an_enum}"
        );
    }
    let of_a_generic_class = refused(
        "class Wrapped<T> {
  value T
  implements baml.ops.Equals {
    function eq(self, other: Self) -> bool throws never { true }
  }
}
function main() -> bool { Wrapped<int> { value: 1 } == Wrapped<int> { value: 2 } }
",
    );
    assert!(
        of_a_generic_class.contains(refusal) && of_a_generic_class.contains("Wrapped"),
        "{of_a_generic_class}"
    );
    // The enum's `eq` is called where two of its values are compared, and
    // nothing else in the program is: that compiles.
    assert_eq!(
        refused(&format!(
            "{shade}function main() -> bool {{ shade(0) == shade(1) }}\n"
        )),
        "compiled"
    );
}

/// A program that calls what of the standard library native code does not
/// run is refused, and the refusal names the function and says why.
#[test]
fn a_refused_standard_library_call_names_the_function_and_says_why() {
    for (source, why, named) in [
        (
            "function main() -> int { baml.archive.entries(b\"x\", \"zip\").length() }",
            "standard library system operation that is not ported",
            "baml.archive.entries",
        ),
        (
            "function main() -> string { baml.csv.reader(\"a,b\"); \"read\" }",
            "standard library native that is not ported",
            "baml.csv._reader",
        ),
        (
            "class Point { x int }
function main() -> string { [Point { x: 1 }].join(\",\") }",
            "baml.Array.join of elements with their own to_string, which the runtime cannot call",
            "elements are class",
        ),
    ] {
        let refusal = refused(&format!("{source}\n"));
        assert!(
            refusal.contains(why) && refusal.contains(named),
            "{refusal}"
        );
    }
}

/// Whether the executable built from `source`, as it ships, names
/// `symbol`: an import is by name, and so is every function of an
/// executable that is not stripped.
fn names(source: &str, symbol: &[u8]) -> bool {
    let db = setup_test_db(source);
    assert_no_diagnostic_errors(&db);
    let directory = tempfile::tempdir().expect("a temporary directory");
    let built = build(&db, "main", directory.path()).expect("the program compiles");
    let bytes = std::fs::read(built.lean).expect("a readable executable");
    bytes.windows(symbol.len()).any(|window| window == symbol)
}

/// Whether the executable built from `source` imports what starts a
/// thread, which the runtime itself never does: only Tokio, for the jobs of
/// `scope.io`.
fn can_start_a_thread(source: &str) -> bool {
    names(source, b"_pthread_create")
}

/// A TLS library sets itself up when a program that links it starts, and
/// the linker cannot drop that. So a program is linked against the runtime
/// that has `baml.http` only when it calls a native of it.
#[test]
fn a_program_without_http_links_no_tls() {
    assert!(!names(
        "function main() -> string { baml.fs.read(\"no such file\") }\n",
        b"rustls"
    ));
    assert!(names(
        "function main() -> int { baml.http.fetch(\"http://127.0.0.1:9/\").status_code }\n",
        b"rustls"
    ));
}

/// Only a native that calls `scope.io` names Tokio, so a program with
/// `async` natives and no I/O links none of it: the linker has nothing to
/// keep it for. Linking it costs every start of the program the binding of
/// the system symbols it imports, whether or not a job ever runs.
#[test]
fn a_program_without_io_links_no_tokio() {
    assert!(!can_start_a_thread(
        "class Temp {
  celsius: float
  implements baml.ToJson {
    function to_json(self) -> baml.json.json throws baml.json.SerializationError {
      { \"c\": baml.json.from(self.celsius) }
    }
  }
}
function main() -> string { baml.json.stringify(Temp { celsius: 21.5 }) }
"
    ));
    assert!(!can_start_a_thread(
        "function main() -> int { baml.io.println(\"printed where it is called\"); 42 }\n"
    ));
    assert!(can_start_a_thread(
        "function main() -> string { baml.fs.read(\"no such file\") }\n"
    ));
}
