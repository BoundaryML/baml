// Transport pin for the streaming replay harness
// (thoughts/sam-projects/bridge-generics/streaming/02): serve an OpenAI-shaped
// SSE body from a BAML `http.Server` and consume it with the engine's own SSE
// client — `baml.http.fetch_sse`, the exact transport the LLM streaming path
// uses (baml_std/baml/ns_llm/llm_types.baml). This proves a BAML-implemented
// server can stand in for WireMock before any bridge/SDK machinery is involved,
// so a bridge-level (pytest/vitest) replay failure can only be a
// bridge/orchestration issue.
//
// Driven by the Rust test `tests/transport_pin.rs` in this crate: it compiles
// this file with the engine helpers and runs `replay_sse_roundtrip` directly
// (asserting the drained event sequence in Rust), so it needs no `baml test`
// CLI wiring.
//
// The handler is a *bound method* (`pin.handle`), not an inline closure, on
// purpose: a closure passed to the `throws never` `serve` callback types its
// captured variables as `unknown`, so a direct `captured_var.to_utf8()` is an
// `unknown`-method call that "may throw unknown" and is rejected with E0096
// (http_server.baml's closures dodge this only because they touch captures
// through an expression like `greeting + req.body`, which re-types to string).
// Field access on `self` is properly typed, so the bound-method form — also
// what the real ReplayHandler uses — sidesteps the bug. Same `server.serve`
// bound-method shape as ns_http_server/http_server.baml's
// `server_bound_method_handler`.
class ReplayPin {
  sse_body: string

  function handle(self, req: baml.http.Request) -> baml.http.Response {
    if (req.method == "POST") {
      baml.http.Response.new(200, { "content-type": "text/event-stream" }, self.sse_body.to_utf8())
    } else {
      baml.http.Response.new(404, { }, "not found".to_utf8())
    }
  }
}

function replay_sse_roundtrip() -> string {
  // OpenAI-format SSE body: three content chunks, a finish event, then [DONE]
  // (same chunk shape as crates/baml_tests/tests/streaming_parsing.rs).
  let pin = ReplayPin {
    sse_body: "data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\", world\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"!\"}}]}\n\ndata: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n",
  };

  let server = baml.http.Server.bind("127.0.0.1:0");
  let task = spawn { server.serve(pin.handle) };

  let sse = baml.http.fetch_sse(baml.http.Request {
    method: "POST",
    url: "http://" + server.addr + "/chat/completions",
    headers: { "content-type": "application/json" },
    body: "{}",
  });

  // Drain every batch the SSE client yields (a buffered body may arrive in one
  // or several `next()` calls) into one accumulation, preserving arrival order.
  let acc = "";
  while (true) {
    let batch = sse.next();
    match (batch) {
      null => { break; }
      let b: string => { acc = acc + b; }
    }
  }
  sse.close();
  task.cancel();
  acc
}
