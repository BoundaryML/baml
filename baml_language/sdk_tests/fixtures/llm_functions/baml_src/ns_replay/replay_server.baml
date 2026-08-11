// A fake LLM endpoint implemented in BAML: serves a recorded SSE body for
// any POST to the LLM route, plus control routes. Type-agnostic — it never
// parses the recording, so one server implementation covers every function.
// See thoughts/sam-projects/bridge-generics/streaming/02 for the harness
// design; the proven server primitives are in
// crates/baml_tests/baml_src/ns_http_server/http_server.baml.
class ReplayHandler {
  sse_body: string
  stop: baml.spawn.CancelToken

  function handle(self, req: baml.http.Request) -> baml.http.Response {
    if (req.url == "/__replay__/health") {
      baml.http.Response.new(200, { }, "ok".to_utf8())
    } else if (req.url == "/__replay__/shutdown") {
      self.stop.cancel();
      baml.http.Response.new(200, { }, "shutting down".to_utf8())
    } else if (req.method == "POST") {
      // The openai provider posts to {base_url}/responses; serve the
      // recording for any POST so other providers' paths also work.
      //
      // Stream the body one SSE event at a time with a small inter-event delay,
      // exactly as a real provider does. A buffered single-write response (the
      // earlier `Response.new` form) is delivered to the client in one socket
      // read, so the SAP partial parser drains straight to `ai.stream.Done`
      // without a partial ever crossing the FFI boundary — masking host-driven
      // streaming bugs. Real-time chunked delivery makes those partials cross
      // deterministically. The writes run in a `spawn` because hyper only reads
      // the response body after the handler returns (see `new_streaming`).
      let resp = baml.http.Response.new_streaming(
        200,
        { "content-type": "text/event-stream" },
      );
      let body = self.sse_body;
      spawn {
        // `split("\n\n")` yields one SSE event per element (events are
        // blank-line delimited); re-append the delimiter and flush each as its
        // own chunk. `write` backpressures until the prior chunk is on the wire.
        for (let event in body.split("\n\n")) {
          if (event.length() > 0) {
            resp.write((event + "\n\n").to_utf8());
            baml.sys.sleep(baml.time.Duration.from_milliseconds(10n));
          }
        }
        resp.end();
        null
      };
      resp
    } else {
      baml.http.Response.new(404, { }, "not found".to_utf8())
    }
  }
}

// Blocking mode: binds an ephemeral port, writes the bound address to
// `addr_file` (the host polls for it), serves until a shutdown request,
// then returns the address. Invoked by the host on a background thread or
// as an asyncio task (the `_async` variant).
function replay_serve_until_shutdown(recording_path: string, addr_file: string) -> string {
  let handler = ReplayHandler {
    sse_body: baml.fs.read(recording_path),
    stop: baml.spawn.CancelToken.new(),
  };
  let server = baml.http.Server.bind("127.0.0.1:0");
  let task = spawn { server.serve(handler.handle) };
  baml.fs.write(addr_file, server.addr);
  while (!handler.stop.is_cancelled()) {
    baml.sys.sleep(baml.time.Duration.from_milliseconds(25n));
  }
  baml.sys.sleep(baml.time.Duration.from_milliseconds(50n));  // let the shutdown response flush before killing serve
  task.cancel();
  server.addr
}

// Detached mode (Phase 5): returns the address immediately; the serve task
// outlives this bridge call via detach. Shutdown still via the control route.
function replay_serve_detached(recording_path: string) -> string {
  let handler = ReplayHandler {
    sse_body: baml.fs.read(recording_path),
    stop: baml.spawn.CancelToken.new(),
  };
  let server = baml.http.Server.bind("127.0.0.1:0");
  let task = spawn with baml.spawn.options(detach = true) { server.serve(handler.handle) };
  // Reaper: also detached; tears the serve down once shutdown fires.
  spawn with baml.spawn.options(detach = true) {
    while (!handler.stop.is_cancelled()) {
      baml.sys.sleep(baml.time.Duration.from_milliseconds(25n));
    }
    baml.sys.sleep(baml.time.Duration.from_milliseconds(50n));
    task.cancel();
    null
  };
  server.addr
}
