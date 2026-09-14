//! Coverage for handle-backed stdlib types returned from BAML to Rust.
//!
//! The non-media cases are encode-back tests: Rust receives a generated
//! class instance with an embedded handle, calls generated stdlib methods
//! with that same instance, and the engine must see the original handle
//! state. No external dependency: the HTTP test binds an ephemeral localhost
//! server and the FS test uses a temp file.

// PROVISIONAL: handle-backed stdlib types have no Rust SDK design yet. This
// port assumes opaque generated structs whose engine-backed methods take
// `&self` and return `Result`, with plain data fields (`status_code`)
// public, and that literal-union-typed params (`open`'s mode, `seek_from`'s
// whence) surface as plain `String`.
use baml_bridge::OptionalArg::Unset;
use baml_sdk::baml::fs::{File, open as baml_open};
use baml_sdk::baml::http::fetch;
use baml_sdk::baml::media::Image;

// 1x1 transparent PNG.
const PNG_B64: &str = concat!(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk",
    "+M8AAAQEAQB9eIv5AAAAAElFTkSuQmCC"
);

// --- media: Image.from_base64 ---------------------------------------------

#[test]
fn test_handles_image_from_base64_roundtrips_payload() {
    let img = Image::from_base64(PNG_B64.to_string(), Some("image/png".to_string())).unwrap();
    // `mime_type()` returns `string?`, so python's bare-string comparison
    // becomes a comparison against `Some`.
    assert_eq!(img.mime_type().unwrap(), Some("image/png".to_string()));
    assert_eq!(img.base64().unwrap(), PNG_B64);
}

// --- baml.http.Response ---------------------------------------------------

const HTTP_BODY: &[u8] = b"hello from localhost";

/// Binds an ephemeral localhost HTTP server that answers every request with
/// a 200 `text/plain` `HTTP_BODY`, and returns its base URL. Stands in for
/// the python `http_server` fixture (whose `_Handler` class collapses into
/// the accept loop); the server thread is detached instead of torn down —
/// it dies with the test process.
fn http_server() -> String {
    use std::io::{Read, Write};

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            // Read the request head; its contents don't matter.
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                HTTP_BODY.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(HTTP_BODY);
        }
    });
    format!("http://127.0.0.1:{port}/")
}

#[test]
fn test_handles_http_get_response_fields_and_methods() {
    let http_server = http_server();
    let resp = fetch(http_server, Unset).unwrap();
    assert_eq!(resp.status_code, 200);
    assert!(resp.ok().unwrap());
    assert_eq!(resp.text().unwrap(), str::from_utf8(HTTP_BODY).unwrap());
}

// --- baml.fs.File: cursor state preserved across calls --------------------

/// Creates a fresh temp file containing `"0123456789"` and returns its
/// path. Stands in for the python `temp_file` fixture; there is no teardown
/// — the per-test directory is left for the OS temp cleaner.
fn temp_file() -> std::path::PathBuf {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("baml_sdk_handles_{}_{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("digits.txt");
    std::fs::write(&path, "0123456789").unwrap();
    path
}

#[test]
fn test_handles_open_file_returns_file_handle() {
    let temp_file = temp_file();
    // DIVERGENCE(rust): python asserts `type(f).__name__ == "File"`; here
    // the annotated binding pins the static type instead.
    let f: File = baml_open(temp_file.to_str().unwrap().to_string(), "r".to_string()).unwrap();
    // `close()` returns null — the successful unwrap is python's `is None`.
    f.close().unwrap();
}

#[test]
fn test_handles_file_cursor_state_persists_across_calls() {
    let temp_file = temp_file();
    let f = baml_open(temp_file.to_str().unwrap().to_string(), "r".to_string()).unwrap();

    // Relative seeks verify that separate calls share one engine-side handle.
    assert_eq!(f.seek_from("current".to_string(), 3).unwrap(), 3);
    assert_eq!(f.seek_from("current".to_string(), 3).unwrap(), 6);

    // Seek back to the start and confirm the cursor actually moved.
    assert_eq!(f.seek_from("start".to_string(), 0).unwrap(), 0);
    assert_eq!(f.seek_from("current".to_string(), 2).unwrap(), 2);

    // text() reads from the current cursor (now at 2) to EOF.
    assert_eq!(f.text().unwrap(), "23456789");

    f.close().unwrap();
}
