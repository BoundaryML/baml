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
use common::{Built, Ending, WAYS, build, command, engine, interpret, listed, output, run};

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

/// The report of a case whose native runs do not all end as expected: as
/// the interpreter does, or as `diverging` says where the case declares
/// that native code ends otherwise, which the interpreter then must not.
fn disagreement(
    case: &str,
    interpreted: &Ending,
    diverging: Option<&Ending>,
    native: &[(&'static str, Ending)],
) -> Option<String> {
    let expected = diverging.unwrap_or(interpreted);
    let failed = native.iter().any(|(_, ending)| ending != expected)
        || (interpreted == expected) != diverging.is_none();
    failed.then(|| {
        format!(
            "{case}:\n  {:<24} {interpreted:?}{}{}",
            "interpreter",
            match diverging {
                Some(expected) => format!("\n  {:<24} {expected:?}", "native expected"),
                None => String::new(),
            },
            report(native),
        )
    })
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
            if let Some(failure) = disagreement(case, &interpreted, diverging.as_ref(), &native) {
                failures.push(format!("{file}: {failure}"));
            }
        }
    }
    assert!(ran > 0, "no conformance cases under `{}`", root.display());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
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

/// Run every case of `source`, a program of its own, on the interpreter
/// and every way a native program runs, and fail when two end otherwise.
async fn cases_agree(source: &str) {
    let db = setup_test_db(source);
    assert_no_diagnostic_errors(&db);
    let engine = engine(&db);
    let directory = tempfile::tempdir().expect("a temporary directory");

    let mut failures = Vec::new();
    for (case, diverging) in cases(source) {
        let interpreted = interpret(&engine, case).await;
        let native = run_every_way(&build(&db, case, directory.path()));
        if let Some(failure) = disagreement(case, &interpreted, diverging.as_ref(), &native) {
            failures.push(failure);
        }
    }
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
    cases_agree(&source).await;
}

/// `baml.fs` in a directory of the test's own, which every case leaves as
/// it found it.
#[tokio::test(flavor = "multi_thread")]
async fn files_end_as_the_interpreter_does() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let path = directory
        .path()
        .canonicalize()
        .expect("the directory exists");
    let source = include_str!("conformance/fs.baml.in")
        .replace("{dir}", path.to_str().expect("a path that is UTF-8"));
    cases_agree(&source).await;
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
    cases_agree(&source).await;
}

/// `baml.io` writes to the program's own streams, which no case of the
/// suites above can show: the interpreter there prints into the test's
/// output. So this checks the native program alone: what it prints
/// before its result, in order, on each stream, and a line read from its
/// input with what was left of the input for the next read.
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
