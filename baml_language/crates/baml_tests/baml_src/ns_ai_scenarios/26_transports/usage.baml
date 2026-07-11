// Task sketch: s26_Ask(instruction) -> string.

// Manual expansion reference for task `s26_Ask`:
// - `s26_Ask_request(...)` models `s26_Ask.request(...)`.
// - `s26_Ask_manual(...)` models the plain `s26_Ask(...)` call, lowered
//   through `root.ai.run`.
// - `s26_Ask_stream_manual(...)` models `s26_Ask.stream(...)`, lowered through
//   `root.ai.stream` over the same request.
// - `s26_Ask_live_manual(...)` models the custom realtime consumer
//   `root.ai.open_live(s26_Ask.request(...), channel)`; `.live` is not a
//   compiler task modifier.
// These helper names are hand-written reference/test scaffolding, not literal
// compiler-generated names.
function s26_Ask_request(
  instruction: string,
  provider: root.ai.Provider = root.ai.FakeGenerateProvider { name: "s26-default", reply: `"unused"`, attributes: {} },
) -> root.ai.Request<string> {
  root.ai.task_request<string>(
    provider, "s26_Ask", "ns_ai_scenarios", { "instruction": instruction },
    [], root.ai.RequestOptions {}, {}, root.ai.prompt_template(instruction, false),
  )
}

function s26_Ask_manual(
  instruction: string,
  provider: root.ai.Provider,
) -> string throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run<string>(s26_Ask_request(instruction, provider = provider))
}

function s26_Ask_stream_manual(
  instruction: string,
  provider: root.ai.Provider,
) -> baml.llm.Stream<string, string>
    throws baml.errors.StreamError | baml.errors.UnknownError {
  root.ai.stream<string, string>(s26_Ask_request(instruction, provider = provider))
}

function s26_Ask_live_manual(
  instruction: string,
  channel: root.ai.Channel,
  provider: root.ai.Provider,
) -> root.ai.Live throws baml.errors.UnknownError {
  root.ai.open_live<string>(s26_Ask_request(instruction, provider = provider), channel)
}

function s26_describe_transport(provider: root.ai.Provider) -> string throws never {
  match (provider) {
    let info: root.ai.TransportInfo => {
      let wire = match (info.transport()) {
        root.ai.TransportKind.Http => "http",
        root.ai.TransportKind.HttpSse => "http+sse",
        root.ai.TransportKind.WebSocket => "websocket",
        root.ai.TransportKind.WebRtc => "webrtc",
      };
      let state = match (info.state_ownership()) {
        root.ai.StateOwnership.ClientOwned => "client-owned",
        root.ai.StateOwnership.ServerStoredById => "server-stored-by-id",
        root.ai.StateOwnership.ServerSession => "server-session",
      };
      wire + " / " + state
    },
    _ => "unknown transport",
  }
}

function s26_capability_of(provider: root.ai.Provider) -> string throws never {
  match (provider) {
    let realtime: root.ai.Realtime => "realtime",
    let streaming: root.ai.Streaming => "streaming",
    let generate: root.ai.Generate => "generate",
    _ => "none",
  }
}

function s26_collect_stream(instruction: string, provider: root.ai.Provider) -> string throws never {
  let stream = s26_Ask_stream_manual(instruction, provider) catch (e) { _ => return "stream-open-error" };
  while (true) {
    match (stream.next()) {
      let delta: string => {},
      let done: baml.stream.StreamFinished => { break; },
    } catch (e) { _ => return "stream-read-error" }
  }
  stream.final() catch (e) { _ => "stream-final-error" }
}

test "26: one request rides HTTP request-response" {
  assert.equal(s26_Ask_manual("q", s26_HttpTransport { reply: "pong" }), "pong")
}

test "26: the same request opens a WebSocket realtime resource" {
  let channel = root.ai.RecordingChannel { frames: [] };
  let live = s26_Ask_live_manual("q", channel, s26_RealtimeTransport { reply: "pong" });
  assert.equal(root.ai.collect_live_text(live), "pong")
  assert.is_true(channel.frames.length() > 0)
}

test "26: streaming is a capability independent of the HTTP+SSE wire label" {
  let provider: root.ai.Provider = s26_StreamTransport { reply: "pong" };
  assert.equal(s26_capability_of(provider), "streaming")
  assert.equal(s26_describe_transport(provider), "http+sse / client-owned")
}

test "26: the stream companion rejects a Generate-only provider" {
  let label = s26_Ask_stream_manual("q", s26_HttpTransport { reply: "x" }) catch (e) {
    let u: baml.errors.Unsupported => "unsupported",
    _ => "other",
  };
  assert.equal(label, "unsupported")
}

test "26: a realtime provider carries no Generate capability" {
  assert.equal(s26_capability_of(s26_RealtimeTransport { reply: "x" }), "realtime")
}

test "26: the same WebSocket wire can expose opposite capabilities and state" {
  let warm: root.ai.Provider = s26_WarmSocketTransport { reply: "x" };
  let live: root.ai.Provider = s26_RealtimeTransport { reply: "x" };
  assert.equal(s26_describe_transport(warm), "websocket / server-stored-by-id")
  assert.equal(s26_describe_transport(live), "websocket / server-session")
  assert.equal(s26_capability_of(warm), "generate")
  assert.equal(s26_capability_of(live), "realtime")
}

testset "integ-test-26-transports" {
  test "26 live: the one function calls over HTTP (transport 1/3)" {
    let answer = s26_Ask_manual(
      "Reply with exactly the lowercase word: pong",
      root.ai.OpenAi {
        model: "gpt-5.6-luna", api_key: baml.env.get_or_panic("OPENAI_API_KEY"), base_url: null,
      },
    );
    assert.contains(answer.to_lower_case(), "pong")
  }

  test "26 live: the SAME function streams over OpenAI SSE (transport 2/3)" {
    let answer = s26_collect_stream(
      "the number one, in one short line",
      root.ai.OpenAi {
        model: "gpt-5.6-luna", api_key: baml.env.get_or_panic("OPENAI_API_KEY"), base_url: null,
      },
    );
    assert.is_true(answer.length() > 0)
  }

  test "26 live: the SAME function runs a WebSocket duplex session (transport 3/3)" {
    let channel = root.ai.RecordingChannel { frames: [] };
    let live = s26_Ask_live_manual(
      "Reply with exactly the lowercase word: pong", channel,
      root.ai.OpenAiRealtime {
        model: "gpt-realtime", voice: "alloy",
        api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
      },
    );
    assert.contains(root.ai.collect_live_text(live).to_lower_case(), "pong")
    assert.is_true(channel.frames.length() > 0)
    live.cleanup();
  }
}
