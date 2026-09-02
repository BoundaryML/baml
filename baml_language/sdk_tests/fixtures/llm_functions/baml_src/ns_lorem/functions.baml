// Client must be in-namespace; ns_lorem can't reach root-level clients.
// Provider construction is pure: api_key resolves from OPENAI_API_KEY at
// request time, so declaring the client never touches env.
client Stub = openai.ResponsesClient.new(model = "gpt-4o-mini");

// The streaming client. A `client Name = ...` declaration is evaluated during
// `$init`, which cannot run io sysops — so `env.NAME` names the variables
// instead of reading them, and the provider resolves them at request time. ONE
// client therefore serves the keyless replay harness (the vars point at the
// in-process replay server) and the key-gated live e2e (real endpoint + key)
// — see customizable/conftest.py.
client StreamStub = openai.ResponsesClient.new(
    model = "gpt-5.4-nano",
    api_key = env.BAML_REPLAY_API_KEY,
    base_url = env.BAML_REPLAY_BASE_URL,
);

// `extract_resume() -> Resume` from 09a. Prompt rendering, parsing, and request
// preview are exposed through the function's first-class `@spec` projection.
function ExtractResume(text: string) -> Resume {
  client: "openai/gpt-4o-mini"
  prompt: `
    Extract a resume from the following text.

    Respond in JSON with this exact schema:
    {"name": "string", "email": "string or null"}

    Text:
    ${text}

    JSON:
  `
}

// Paired with `StreamingDoc` to exercise the always-different
// `Class$stream` type-companion branch (previously covered by the standalone
// `python_llm_functions` crate). The former `openai-responses` shorthand is
// gone (only openai | anthropic | google | claude-code prefixes remain).
function StreamingExtract(text: string) -> StreamingDoc {
  client: "openai/gpt-4o-mini"
  prompt: `
    Extract a streaming doc from the following text.

    Respond in JSON with this exact schema:
    {"title": "string", "body": "string or null", "word_count": "integer"}

    Text:
    ${text}

    JSON:
  `
}

// Portable Prompt/Media boundary coverage: the spec is rendered and reused
// repeatedly by the Python host before its provider request is previewed.
function InspectMedia(photo: image) -> string {
  client: Stub
  prompt: `${role("user")}Describe this image: ${photo}`
}

// === Streaming coverage ===
//
// `Fn@stream` is available for every tool-less LLM function and returns
// `ai.stream.Stream<PARTIAL, FINAL>`, driven by `invoke_stream`
// (`ai.stream.StreamingClient`). The 8-bullet plain-text format forces
// multi-chunk SSE so `next()` yields a series of partials before Done.
function stream_e2e_extract(text: string) -> string {
  client: StreamStub
  prompt: `
    Read the resume below and write a short summary as plain text.
    Begin with one sentence summarizing the candidate, then list
    exactly 8 bullet points (one per line, prefixed with "- ")
    highlighting different aspects: role, years of experience,
    primary skills, secondary interests, location, working style,
    notable strengths, and one concrete achievement.

    Output plain text only (no JSON).

    Resume:
    ${text}

    Summary:
  `
}

// Class-typed streaming: the partial type is `StreamingDoc$stream` /
// `Resume$stream` (multi-field T) rather than a bare scalar.
function stream_e2e_extract_doc(text: string) -> StreamingDoc {
  client: StreamStub
  prompt: `
    Read the resume below and produce a structured doc.

    Respond in JSON with this exact schema:
    {"title": "string", "body": "string", "word_count": integer}

    The title is a one-line headline for the candidate, the body is a
    2-3 sentence summary, and word_count is the number of words in body.

    Resume:
    ${text}

    JSON:
  `
}

function stream_e2e_extract_resume(text: string) -> Resume {
  client: StreamStub
  prompt: `
    Extract a resume from the following text.

    Respond in JSON with this exact schema:
    {"name": "string", "email": "string or null"}

    Text:
    ${text}

    JSON:
  `
}

// HS-7: a generic LLM function whose realized output may be a runtime-created
// nominal. The replay server ignores the request and returns the checked-in
// StreamingDoc-shaped JSON, so this stays deterministic and keyless.
function hs7_dynamic_extract<T>(text: string) -> T {
  client: StreamStub
  prompt: `
    Produce a document matching this runtime output schema.

    Input:
    ${text}

    ${ctx.output_format()}
  `
}

// Deliberately repeats the display name of the compiled `Hs7Collision` class
// above while carrying unrelated fields. Identity, never this name, decides
// whether results become generated Pydantic models or live RuntimeValues.
function hs7_collision_type() -> reflect.Type {
  reflect.class.new("Hs7Collision", {
    "title": reflect.Type.of<string?>(),
    "body": reflect.Type.of<string?>(),
    "word_count": reflect.Type.of<int?>(),
  }).as_type()
}

// Mirror the original VetRec wrapper: create and bind the runtime type inside
// BAML, then erase only the public Stream slots to `unknown`. The live stream
// object must retain its realized dynamic type arguments for next/final.
function hs7_open_collision_stream(text: string) -> ai.stream.Stream<unknown, unknown> {
  let output_type = hs7_collision_type();
  type OutputType = unreflect(output_type);
  hs7_dynamic_extract@stream<OutputType>(text)
}

// Exercise the ordinary direct-result boundary with the same BAML-created
// declaration, without involving an LLM transport. The host sees only
// `unknown`; the live nominal must not be decoded through the compiled
// same-name class.
function hs7_open_collision_value(json: string) -> unknown {
  let output_type = hs7_collision_type();
  type OutputType = unreflect(output_type);
  baml.json.from_string<OutputType>(json)
}

// Return the same live spec through `unknown` so Python exercises the handle's
// realized runtime type without constructing or reflecting any type itself.
function hs7_open_collision_spec(text: string) -> unknown {
  let output_type = hs7_collision_type();
  type OutputType = unreflect(output_type);
  hs7_dynamic_extract@spec<OutputType>(text)
}

// A pass-back pin independent of `.to_data()`: the original rooted value must
// survive repeated host -> engine -> host crossings as a RuntimeValue.
function hs7_echo_runtime_value(value: unknown) -> unknown {
  value
}

// Recorder-only request builders. They use the same native-BAML provider
// lowering as `Fn@stream`, but stop before opening the SSE connection so the
// Rust recording harness can send the exact request with curl and retain the
// raw response bytes.
function _stream_recording_request<T>(spec: ai.FunctionSpec<T>) -> baml.http.Request {
  openai.internal._openai_request(
    StreamStub,
    ai.ModelTurnInput {
      prompt: spec.prompt_template,
      journal: ai.Journal.new(spec),
      toolbox: spec.tools(),
      output_type: spec.output_type(),
    },
    true,
  )
}

function stream_e2e_extract_recording_request(text: string) -> baml.http.Request {
  _stream_recording_request(stream_e2e_extract@spec(text))
}

function stream_e2e_extract_doc_recording_request(text: string) -> baml.http.Request {
  _stream_recording_request(stream_e2e_extract_doc@spec(text))
}

// BAML-driven collectors: they drive `Fn@stream` inside BAML so the
// partial values never cross the FFI boundary as `T | ai.stream.Done`. The
// two values that do cross are typed concretely. Useful for isolating a host
// bridge problem from an engine one — if these pass while the host-driven
// stream fails, the break is in the bridge.
function stream_e2e_collect_doc(text: string) -> StreamingDoc {
  let s = stream_e2e_extract_doc@stream(text);
  while (true) {
    match (s.next()) {
      ai.stream.Done => {
        break;
      }
      _ => {}
    }
  }
  s.final()
}

function stream_e2e_collect(text: string) -> StreamE2ECollectResult {
  let s = stream_e2e_extract@stream(text);
  let next_calls: (null | string)[] = [];
  while (true) {
    match (s.next()) {
      ai.stream.Done => {
        break;
      }
      null => {
        next_calls.push(null);
      }
      let item: string => {
        next_calls.push(item);
      }
    }
  }
  let final_call = s.final();
  StreamE2ECollectResult { next_calls: next_calls, final_call: final_call }
}

// === Unified on_event coverage ===
//
// A scripted client with a canned parseable reply and fixed usage: the
// keyless, network-free backing for the host-side `on_event` tests on the
// plain (Agent) call path. The stream path reuses StreamStub + the replay
// server instead (see customizable/test_on_event.py).
class OnEventScriptedClient {
  implements ai.Client {
    function id(self) -> string {
      "scripted"
    }

    function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
      let _ = input;
      baml.http.Request { method: "POST", url: "https://example.invalid/x", headers: {}, body: "{}" }
    }

    function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
      let _ = input;
      ai.ModelTurn {
        // One hand-made wire record so hosts can assert the generated
        // ai.events.LLMCall model arrives through on_event.
        calls: [
          ai.events.LLMCall {
            client_name: "scripted",
            provider: "scripted",
            timing: ai.events.Timing { start_time_utc_ms: 1000n, duration_ms: 42n, ttft_ms: null },
            http_request: null,
            http_response: ai.events.HttpResponse { status: 200, headers: {}, body: null },
            usage: null,
            selected: true,
            sse_responses: null,
          },
        ],
        content: [ai.content.Text { text: "{\"name\": \"Ada\", \"email\": null}" }],
        stop_reason: ai.content.StopReason.Complete,
        usage: ai.events.Usage { input_tokens: 3, output_tokens: 5, cached_input_tokens: null, reasoning_tokens: null },
      }
    }
  }
}

client OnEventScripted = OnEventScriptedClient {  };

function on_event_probe(text: string) -> Resume {
  client: OnEventScripted
  prompt: `
    Extract a resume from the following text.

    Respond in JSON with this exact schema:
    {"name": "string", "email": "string or null"}

    Text:
    ${text}

    JSON:
  `
}
