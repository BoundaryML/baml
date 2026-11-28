//! A library that sends HTTP requests itself (`baml.http`), called from
//! Rust: the functions `http_get` and `http_post_json` of
//! `type_shapes/bridge_test.baml`, against a server on a loopback port that
//! this test starts. Calls park on I/O, and the library's I/O thread wakes
//! the host when a reply comes.
//!
//! A test binary of its own: a process loads one library.
#![cfg(all(target_arch = "aarch64", target_vendor = "apple"))]
#![allow(clippy::print_stderr, reason = "tests that say what they saw")]

use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::OnceLock,
    time::{Duration, Instant},
};

use baml_compiler2_native::{
    CompileOptions, compile_library, export_name, library_interface, link_library,
};
use baml_db::{
    ProjectDatabase, SourceRootKind, SourceRootSpec, baml_compiler2_hir::item_data::file_functions,
};
use baml_native_host::{
    BamlValue, Error, FromBaml, Library, SdkError, Session, ToBaml, TypeDefKind, futures,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct HttpReply {
    status: i64,
    body: String,
}

impl FromBaml for HttpReply {
    fn from_baml(value: BamlValue<'_>) -> Result<Self, SdkError> {
        value.expect_class(number("bridge_test.HttpReply"))?;
        Ok(Self {
            status: value.field(0)?.int()?,
            body: value.field(1)?.string()?,
        })
    }
}

struct Greeting {
    name: String,
    count: i64,
}

impl ToBaml for Greeting {
    fn to_baml<'s>(&self, session: &'s Session) -> Result<BamlValue<'s>, SdkError> {
        session.instance(
            number("bridge_test.Greeting"),
            vec![session.string(&self.name)?, session.int(self.count)],
        )
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Echo {
    name: String,
    count: i64,
    served_by: String,
}

impl FromBaml for Echo {
    fn from_baml(value: BamlValue<'_>) -> Result<Self, SdkError> {
        value.expect_class(number("bridge_test.Echo"))?;
        Ok(Self {
            name: value.field(0)?.string()?,
            count: value.field(1)?.int()?,
            served_by: value.field(2)?.string()?,
        })
    }
}

const EXPORTS: [&str; 2] = ["bridge_test.http_get", "bridge_test.http_post_json"];

/// The library: `bridge_test` alone, exporting the two HTTP functions.
fn library() -> &'static Library {
    static LIBRARY: OnceLock<&'static Library> = OnceLock::new();
    LIBRARY.get_or_init(|| {
        let root = tempfile::tempdir().expect("a directory").keep();
        let file = root.join("ns_bridge_test/types.baml");
        std::fs::create_dir_all(file.parent().expect("a parent")).expect("a directory");
        let text = include_str!("type_shapes/bridge_test.baml");
        std::fs::write(&file, text).expect("the source");
        let mut db = ProjectDatabase::new();
        db.ensure_stdlib_sources();
        let source_root = db
            .add_source_root(SourceRootSpec::new(&root, SourceRootKind::Workspace))
            .expect("a source root");
        db.add_or_update_file_in(source_root, &file, text);
        let functions: Vec<_> = db
            .workspace_files()
            .into_iter()
            .flat_map(|file| file_functions(&db, file).iter().copied())
            .collect();
        let exports: Vec<_> = EXPORTS
            .iter()
            .map(|name| {
                *functions
                    .iter()
                    .find(|function| export_name(&db, **function) == *name)
                    .unwrap_or_else(|| panic!("no function {name}"))
            })
            .collect();
        let compiled = compile_library(&db, &exports, CompileOptions::default())
            .unwrap_or_else(|error| panic!("the functions compile: {error}"));
        let hash = library_interface(&db, &exports).expect("an interface").hash;
        let path: PathBuf = root.join("libhttp.dylib");
        link_library(&compiled.object, &path).expect("the library links");
        futures::load(path.to_str().expect("UTF-8"), hash).expect("the library loads")
    })
}

/// A class's number in the library's interface.
fn number(name: &str) -> u32 {
    let interface = library().interface();
    let place = interface
        .types
        .iter()
        .position(|ty| ty.name == name && matches!(ty.kind, TypeDefKind::Class { .. }))
        .unwrap_or_else(|| panic!("no class {name}"));
    u32::try_from(place).expect("a small number")
}

async fn http_get(url: &str, timeout_ms: i64) -> Result<HttpReply, Error> {
    let url = url.to_owned();
    futures::call(library(), 0, move |session| {
        Ok(vec![session.string(&url)?, session.int(timeout_ms)])
    })
    .await
}

async fn http_post_json(url: &str, greeting: Greeting) -> Result<Echo, Error> {
    let url = url.to_owned();
    futures::call(library(), 1, move |session| {
        Ok(vec![session.string(&url)?, greeting.to_baml(session)?])
    })
    .await
}

/// Serve HTTP on a loopback port until the process ends. `GET /hello`
/// answers `hello`; `GET /slow/<ms>` answers after that many
/// milliseconds; `POST /echo` answers the JSON it was sent, with
/// `served_by` added.
fn serve() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("an address").port();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            std::thread::spawn(move || answer(&stream));
        }
    });
    port
}

fn answer(stream: &TcpStream) {
    // Read and write through references: a clone per connection would take
    // a file descriptor more each, and macOS allows a process 256.
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return;
    }
    let mut parts = line.split_whitespace();
    let (method, path) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    let mut length = 0;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).is_err() || header.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0; length];
    if reader.read_exact(&mut body).is_err() {
        return;
    }
    let (status, reply) = match (method, path) {
        ("GET", "/hello") => ("200 OK", "hello".to_owned()),
        ("GET", slow) if slow.starts_with("/slow/") => {
            let ms: u64 = slow["/slow/".len()..].parse().unwrap_or(0);
            std::thread::sleep(Duration::from_millis(ms));
            ("200 OK", "slow".to_owned())
        }
        ("POST", "/echo") => {
            let mut json: serde_json::Value =
                serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
            json["served_by"] = "rust-test".into();
            ("200 OK", json.to_string())
        }
        _ => ("404 Not Found", "no".to_owned()),
    };
    let mut writer = stream;
    let _ = write!(
        writer,
        "HTTP/1.1 {status}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{reply}",
        reply.len()
    );
}

/// A loopback port nothing listens on.
fn closed_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    listener.local_addr().expect("an address").port()
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_time()
        .build()
        .expect("a runtime")
}

#[test]
fn one_get_and_one_post() {
    let port = serve();
    runtime().block_on(async {
        let reply = http_get(&format!("http://127.0.0.1:{port}/hello"), 5000)
            .await
            .expect("a reply");
        assert_eq!(
            reply,
            HttpReply {
                status: 200,
                body: "hello".to_owned()
            }
        );
        let echo = http_post_json(
            &format!("http://127.0.0.1:{port}/echo"),
            Greeting {
                name: "Ada".to_owned(),
                count: 3,
            },
        )
        .await
        .expect("an echo");
        assert_eq!(
            echo,
            Echo {
                name: "Ada".to_owned(),
                count: 3,
                served_by: "rust-test".to_owned()
            }
        );
    });
}

#[test]
fn a_hundred_requests_at_once_overlap() {
    let port = serve();
    let delay = 300;
    let started = Instant::now();
    let replies = runtime().block_on(async {
        let calls: Vec<_> = (0..100)
            .map(|_| {
                let url = format!("http://127.0.0.1:{port}/slow/{delay}");
                tokio::spawn(async move { http_get(&url, 10_000).await })
            })
            .collect();
        let mut replies = Vec::new();
        for call in calls {
            replies.push(call.await.expect("joined").expect("a reply"));
        }
        replies
    });
    let elapsed = started.elapsed();
    assert!(
        replies
            .iter()
            .all(|reply| reply.status == 200 && reply.body == "slow")
    );
    // One after another they would take 30 s.
    assert!(elapsed < Duration::from_secs(5), "took {elapsed:?}");
    eprintln!("100 requests of {delay} ms each: {elapsed:?}");
}

#[test]
fn a_refused_connection_and_a_timeout_are_errors() {
    let port = serve();
    runtime().block_on(async {
        match http_get(&format!("http://127.0.0.1:{}/hello", closed_port()), 5000).await {
            Err(Error::Runtime {
                class_name,
                message,
            }) => {
                eprintln!("refused: {class_name:?} {message}");
                assert_eq!(class_name.as_deref(), Some("baml.errors.Io"), "{message}");
            }
            other => panic!("expected an Io error, got {other:?}"),
        }
        let started = Instant::now();
        match http_get(&format!("http://127.0.0.1:{port}/slow/3000"), 200).await {
            Err(Error::Runtime {
                class_name,
                message,
            }) => {
                eprintln!("timed out: {class_name:?} {message}");
                assert_eq!(
                    class_name.as_deref(),
                    Some("baml.errors.Timeout"),
                    "{message}"
                );
            }
            other => panic!("expected a Timeout error, got {other:?}"),
        }
        assert!(started.elapsed() < Duration::from_secs(2));
    });
}

/// One real HTTPS request, run by hand (`cargo test --test http --
/// --ignored`): TLS and certificate roots inside the loaded library. It
/// leaves the machine, so it is not part of the suite.
#[test]
#[ignore = "sends a request to https://example.com/"]
fn one_real_https_request() {
    let reply = runtime()
        .block_on(http_get("https://example.com/", 20_000))
        .expect("a reply from example.com");
    eprintln!(
        "https://example.com/: status {}, {} bytes, title {:?}",
        reply.status,
        reply.body.len(),
        reply
            .body
            .split("<title>")
            .nth(1)
            .and_then(|rest| rest.split("</title>").next())
    );
    assert_eq!(reply.status, 200);
}
