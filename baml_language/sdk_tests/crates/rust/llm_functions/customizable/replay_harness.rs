//! Keyless replay harness shared by the streaming tests.
//!
//! [`replay_server`] runs the wrapped test (sync; [`replay_server_async`] is
//! the `async` sibling) against an in-process BAML server replaying a
//! checked-in SSE recording, with the env-driven `StreamStub` client pointed
//! at it — so the test exercises the full streaming path with **no
//! `OPENAI_API_KEY`**. Inside a wrapped test the server address is available
//! as `std::env::var("BAML_REPLAY_BASE_URL")`.
//!
//! ADAPTATION(rust): python applies this as the `@replay_server(...)` test
//! decorator; Rust has no test decorators, so the ported tests wrap their
//! bodies in `replay_server(recording, || { … })` /
//! `replay_server_async(recording, async { … })`. Python's `_running_server`
//! context manager becomes [`_running_server`] returning an RAII guard that
//! tears the server down (and unsets the env) on drop.

use std::{
    io::{Read as _, Write as _},
    path::PathBuf,
    time::{Duration, Instant},
};

use baml_sdk::replay::replay_serve_until_shutdown;

/// Absolute path to a checked-in SSE recording under `sdk_tests/fixtures`.
///
/// Python walks up from `__file__`; the compiled analogue walks up from the
/// crate manifest dir (the generated crate sits under `sdk_tests/`).
pub fn recording_path(name: &str) -> PathBuf {
    for parent in std::path::Path::new(env!("CARGO_MANIFEST_DIR")).ancestors() {
        if parent.file_name().is_some_and(|dir| dir == "sdk_tests") {
            let rec = parent
                .join("fixtures")
                .join("llm_functions")
                .join("recordings")
                .join(format!("{name}.snap.sse"));
            assert!(rec.exists(), "missing recording {}", rec.display());
            return rec;
        }
    }
    panic!("could not locate the sdk_tests/ ancestor directory");
}

/// Guard for a replay server bound on a background thread: the yielded value
/// of python's `_running_server` context manager, with the `finally` block
/// as [`Drop`].
struct RunningServer {
    /// The bound `host:port`.
    addr: String,
    /// The serve thread; `None` once joined.
    thread: Option<std::thread::JoinHandle<Result<String, String>>>,
    addr_file: PathBuf,
}

/// Serve `recording` on a background thread, set the replay client env, and
/// tear it down when the returned guard drops. The bound `host:port` is on
/// the guard.
fn _running_server(recording: &str) -> RunningServer {
    let rec = recording_path(recording);
    // `{:?}` is ThreadId's only stable rendering (parity with python's
    // `threading.get_ident()` — tests share one process but not one thread).
    let addr_file = std::env::temp_dir().join(format!(
        "baml_replay_{}_{:?}_{}",
        std::process::id(),
        std::thread::current().id(),
        recording
    ));
    let _ = std::fs::remove_file(&addr_file);

    // Surface bridge/engine failures to the poller: python collects them in
    // an `error` list; here the JoinHandle carries the thread's Result
    // (stringified so the generated error type never crosses this module).
    let rec_arg = rec.to_string_lossy().into_owned();
    let addr_file_arg = addr_file.to_string_lossy().into_owned();
    let mut thread = Some(std::thread::spawn(move || {
        replay_serve_until_shutdown(rec_arg, addr_file_arg).map_err(|e| format!("{e:?}"))
    }));

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut addr = None;
    while Instant::now() < deadline {
        if thread.as_ref().is_some_and(|t| t.is_finished()) {
            match thread.take().expect("just checked is_finished").join() {
                // Bound, served, and already shut down — the addr file is on
                // disk, so keep polling for it.
                Ok(Ok(_)) => {}
                Ok(Err(e)) => panic!("replay server thread failed: {e}"),
                Err(panic) => std::panic::resume_unwind(panic),
            }
        }
        if let Ok(text) = std::fs::read_to_string(&addr_file)
            && !text.trim().is_empty()
        {
            addr = Some(text.trim().to_string());
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let Some(addr) = addr else {
        panic!("replay server did not bind within 10s");
    };

    // SAFETY: mutating the process environment is unsynchronized global
    // state (why edition 2024 marks it `unsafe`); this harness owns these
    // two variables, mirroring what python writes into `os.environ`.
    unsafe {
        std::env::set_var("BAML_REPLAY_BASE_URL", format!("http://{addr}"));
        std::env::set_var("BAML_REPLAY_API_KEY", "replay-test-key");
    }
    RunningServer {
        addr,
        thread,
        addr_file,
    }
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        // Best-effort cooperative shutdown via the control route.
        let _ = post_shutdown(&self.addr);
        if let Some(thread) = self.thread.take() {
            // python joins with a 10s timeout; `JoinHandle::join` has no
            // timeout, so a serve thread that ignores shutdown hangs the
            // test instead of leaking.
            let _ = thread.join();
        }
        let _ = std::fs::remove_file(&self.addr_file);
        // SAFETY: same process-global env reasoning as the writes.
        unsafe {
            std::env::remove_var("BAML_REPLAY_BASE_URL");
            std::env::remove_var("BAML_REPLAY_API_KEY");
        }
    }
}

/// POST to the replay server's shutdown route: a hand-rolled HTTP/1.1
/// request over a plain `TcpStream` (python reaches for `urllib.request`;
/// std has no HTTP client and the harness should not pull one in).
fn post_shutdown(addr: &str) -> std::io::Result<()> {
    let mut stream = std::net::TcpStream::connect(addr)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    write!(
        stream,
        "POST /__replay__/shutdown HTTP/1.1\r\n\
         Host: {addr}\r\n\
         Content-Length: 0\r\n\
         Connection: close\r\n\
         \r\n"
    )?;
    let mut response = Vec::new();
    let _ = stream.read_to_end(&mut response);
    Ok(())
}

/// Run `test` against the keyless replay server for the `recording_path`
/// recording (a name under `recordings/`, resolved by [`recording_path`]).
/// Python's decorator works on both sync and `async` tests; the `async`
/// sibling here is [`replay_server_async`].
pub fn replay_server<T>(recording_path: &str, test: impl FnOnce() -> T) -> T {
    let _server = _running_server(recording_path);
    test()
}

/// `async` sibling of [`replay_server`]. The server outlives the awaited
/// test body via the same RAII guard (its setup polls with blocking sleeps,
/// exactly as python's harness blocks inside the event loop).
pub async fn replay_server_async<T>(recording_path: &str, test: impl Future<Output = T>) -> T {
    let _server = _running_server(recording_path);
    test.await
}
