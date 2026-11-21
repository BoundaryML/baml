//! Conformance of the standard library's natives: each file under
//! `tests/conformance` holds the cases of one native, as functions taking no
//! arguments. Every case runs on the interpreter and as a native executable
//! every way of `common::WAYS`, and all must end the same way: the same
//! printed result or the same uncaught throw.
//!
//! Native `int` is 64 bits wide and wraps, and the interpreter's is still 63
//! bits wide and panics on overflow. A case whose answer depends on that says
//! how native code ends in a comment on the line before it, `// native
//! prints: <text>` or `// native throws: <text>`, and the interpreter must
//! then end some other way, so that the comment goes when it stops being
//! true.
//!
//! `support.baml` holds what the cases share. Values come out of loops there
//! so that they are built at run time, on the heap.

use std::path::{Path, PathBuf};

use baml_db::testing::{assert_no_diagnostic_errors, setup_test_db};

mod common;
use common::{Built, Ending, WAYS, build, engine, interpret, listed, output, run};

/// Run a program every way of `WAYS`, or report why there is none.
fn run_every_way(built: &Result<Built, String>) -> Vec<(&'static str, Ending)> {
    WAYS.iter()
        .map(|way| {
            let ending = match built {
                Ok(built) => run(built, way),
                Err(error) => Ok(error.clone()),
            };
            (way.name, ending)
        })
        .collect()
}

/// A line for each way of `endings`, for the report of a case that failed.
fn report(endings: &[(&'static str, Ending)]) -> String {
    let endings: Vec<(&str, String)> = endings
        .iter()
        .map(|(way, ending)| (*way, format!("{ending:?}")))
        .collect();
    listed(&endings)
}

/// Every `.baml` file under `directory`, in a stable order.
fn sources(directory: &Path, found: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("cannot read `{}`: {error}", directory.display()))
        .map(|entry| entry.expect("a directory entry").path())
        .collect();
    entries.sort();
    for entry in entries {
        if entry.is_dir() {
            sources(&entry, found);
        } else if entry
            .extension()
            .is_some_and(|extension| extension == "baml")
        {
            found.push(entry);
        }
    }
}

/// The functions `source` declares without parameters: its cases, each with
/// how native code ends where the interpreter is expected to end otherwise.
fn cases(source: &str) -> Vec<(&str, Option<Ending>)> {
    let mut native = None;
    let mut found = Vec::new();
    for line in source.lines() {
        let declared = native.take();
        if let Some(text) = line.strip_prefix("// native prints: ") {
            native = Some(Ok(text.to_owned()));
        } else if let Some(text) = line.strip_prefix("// native throws: ") {
            native = Some(Err(text.to_owned()));
        } else if let Some((name, _)) = line
            .strip_prefix("function ")
            .and_then(|rest| rest.split_once("()"))
        {
            found.push((name, declared));
        }
    }
    found
}

#[tokio::test(flavor = "multi_thread")]
async fn natives_end_as_the_interpreter_does() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/conformance");
    let mut files = Vec::new();
    sources(&root, &mut files);
    let texts: Vec<(String, String)> = files
        .iter()
        .map(|file| {
            let name = file.strip_prefix(&root).expect("under the root");
            let text = std::fs::read_to_string(file).expect("a readable source");
            (name.display().to_string(), text)
        })
        .collect();
    // One program for every file: a case names its native, so no two
    // collide.
    let program: String = texts.iter().map(|(_, text)| text.as_str()).collect();

    let db = setup_test_db(&program);
    assert_no_diagnostic_errors(&db);
    let engine = engine(&db);
    let directory = tempfile::tempdir().expect("a temporary directory");

    // While a file is being worked on, its name here runs its cases alone,
    // or those of each file of a list with commas between. The program is
    // still every file.
    let only = std::env::var("BAML_CONFORMANCE_ONLY").ok();
    let mut ran = 0;
    let mut failures = Vec::new();
    for (file, text) in &texts {
        if only
            .as_ref()
            .is_some_and(|only| !only.split(',').any(|name| file.contains(name)))
        {
            continue;
        }
        for (case, diverging) in cases(text) {
            ran += 1;
            let interpreted = interpret(&engine, case).await;
            let native = run_every_way(&build(&db, case, directory.path()));
            let agrees = diverging.is_none();
            let expected = diverging.as_ref().unwrap_or(&interpreted);
            if native.iter().any(|(_, ending)| ending != expected)
                || (interpreted == *expected) != agrees
            {
                failures.push(format!(
                    "{file}: {case}:\n  {:<24} {interpreted:?}{}{}",
                    "interpreter",
                    match &diverging {
                        Some(expected) => format!("\n  {:<24} {expected:?}", "native expected"),
                        None => String::new(),
                    },
                    report(&native),
                ));
            }
        }
    }
    assert!(ran > 0, "no conformance cases under `{}`", root.display());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Serve HTTP on a loopback port until the test process ends, and return
/// the port. Every connection is answered once and closed.
fn serve_http() -> u16 {
    use std::io::{Read as _, Write as _};

    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("a bound address").port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            std::thread::spawn(move || {
                let mut request = Vec::new();
                let mut chunk = [0; 1024];
                while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                    match stream.read(&mut chunk) {
                        Ok(read) if read > 0 => request.extend_from_slice(&chunk[..read]),
                        _ => return,
                    }
                }
                let request = String::from_utf8_lossy(&request);
                let path = request.split(' ').nth(1).unwrap_or("");
                let (status, body): (&str, &[u8]) = match path {
                    "/hello" => ("200 OK", "hello from loopback, caf\u{e9}".as_bytes()),
                    "/bytes" => ("200 OK", b"not \xFF utf-8"),
                    "/slow" => {
                        std::thread::sleep(std::time::Duration::from_secs(3));
                        ("200 OK", b"late")
                    }
                    _ => ("404 Not Found", b"nothing here"),
                };
                let head = format!(
                    "HTTP/1.1 {status}\r\ncontent-type: text/plain\r\nx-asked: {path}\r\n\
                     content-length: {}\r\nconnection: close\r\n\r\n",
                    body.len()
                );
                // The client may have given up: a late answer has no reader.
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(body);
            });
        }
    });
    port
}

/// A loopback port nothing listens on.
fn closed_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    listener.local_addr().expect("a bound address").port()
}

/// `baml.http.fetch` against a server in this process: the request is sent
/// by the runtime's own client, and the `Response` it returns is built in
/// Rust and owns its unread body.
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
        .replace("{closed}", &format!("http://127.0.0.1:{}", closed_port()));
    let db = setup_test_db(&source);
    assert_no_diagnostic_errors(&db);
    let engine = engine(&db);
    let directory = tempfile::tempdir().expect("a temporary directory");

    let mut failures = Vec::new();
    for (case, _) in cases(&source) {
        let interpreted = interpret(&engine, case).await;
        let native = run_every_way(&build(&db, case, directory.path()));
        if native.iter().any(|(_, ending)| *ending != interpreted) {
            failures.push(format!(
                "{case}:\n  {:<24} {interpreted:?}{}",
                "interpreter",
                report(&native),
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
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
/// The interpreter as this suite runs it only logs such an error, so this
/// cannot be one of the cases above. The report is `baml run`'s but for the
/// lines of the traceback, which name no file and line here, and for where
/// they are: before the error, as for the entry function's own. Tasks are
/// reported in the order they were spawned in.
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
    assert!(can_start_a_thread(
        "function main() -> string { baml.fs.read(\"no such file\") }\n"
    ));
}
