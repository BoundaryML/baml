// Tests for `baml.http.fetch` client behavior.
//
// Each case uses a top-level helper `function` plus a `test` block that
// asserts on its result. (Helper functions work around a test-block
// local-boxing VM limitation.)
//
// To simulate a peer that accepts a TCP connection but never answers,
// use `baml.http.Server.bind("127.0.0.1:0")` (bound but never `serve`d):
// its socket queues connections from bind time, so the client connects and
// then hangs waiting for a response that never comes.
//
// Tests requiring bytecode snapshots cannot be expressed in BAML.

// ─── fetch total timeout ──────────────────────────────────────────────────────

// A listener that accepts (bind-time backlog) but never writes an HTTP
// response, so the request hangs after connecting. A short total timeout must
// surface as baml.errors.Timeout.
function http_fetch_timeout_fires_fn() -> string {
    let server = baml.http.Server.bind("127.0.0.1:0");
    let response = baml.http.fetch(
        "http://" + server.addr + "/",
        timeout = baml.time.Duration.from_milliseconds(100n),
    );
    response.text()
}

test "http_fetch_timeout_fires" {
    {
        http_fetch_timeout_fires_fn();
        baml.sys.panic("fetch with a 100ms timeout against a silent server should time out")
    } catch (e) {
        baml.errors.Timeout => null
    }
}
