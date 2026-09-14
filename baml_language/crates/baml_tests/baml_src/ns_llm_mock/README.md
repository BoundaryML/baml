# `ns_llm_mock` — mock provider servers for the native LLM clients

Shared, pure-BAML fake provider endpoints. Every per-provider test namespace
(`ns_openai_chat`, `ns_anthropic`, `ns_bedrock`, …) drives its client against one
of these instead of a real API, so the whole corpus stays offline and runs in CI
with no secrets. Real provider calls live in exactly one place —
`../ns_llm_live_smoke/`, gated behind the `live` test profile.

Files:

| File | What it is |
|---|---|
| `mock_provider.baml` | the helpers (this document's subject) + their self-tests |
| `example_openai_client.baml` | the copy-me template: a real client driven against both mocks |
| `README.md` | this file |

---

## 1. Running the suite

From the repo root (`baml_language/`). Always `target/debug/baml-cli`, **never**
`target/debug/baml` (stale wrapper that fabricates phantom failures).

```bash
# offline — the default profile, what CI runs. No keys, no network.
target/debug/baml-cli test --from crates/baml_tests/baml_src

# one namespace / one case
target/debug/baml-cli test --from crates/baml_tests/baml_src -i "llm_mock"
target/debug/baml-cli test --from crates/baml_tests/baml_src -i "mock_sse_streams_events"

# what would run, without running it
target/debug/baml-cli test --from crates/baml_tests/baml_src --list
```

Live tests (real providers, keys injected by Infisical — run from the monorepo
root or anywhere below it):

```bash
cd /Users/aaron/projects/baml
infisical run -- baml_language/target/debug/baml-cli test \
    --from baml_language/crates/baml_tests/baml_src --profile live

# one provider only
infisical run -- baml_language/target/debug/baml-cli test \
    --from baml_language/crates/baml_tests/baml_src --profile live -i "::live::anthropic"
```

Profiles are defined in `../baml.toml`: `offline` (the default) is
`-x "::live::"`, `live` is `-i "::live::"`. `--no-profile` ignores both.
A selector without `*` is a case-sensitive substring match on the full canonical
id (`root.<ns>::<testset>::<test>`); with `*` it is an anchored glob. Excludes
always beat includes, and a CLI `-i` **narrows** the profile's selection rather
than OR-ing with it.

Set `BAML_CACHE_DIR` / `BAML_HOME` to a temp dir when scripting, so the CLI does
not write `.baml/cache` into the source tree that the snapshot tests scan.

---

## 2. The mock API

Everything is referenced `root.`-absolutely from another namespace:
`root.llm_mock.mock_json_serve(...)`, `root.llm_mock.MockResponse.ok(...)`.

### 2.1 JSON mock — request shape, response parse, error taxonomy

```baml
function mock_json_serve<T, E>(
    responses: root.llm_mock.MockResponse[],
    body: (mock: root.llm_mock.MockJsonServer) -> T throws E,
) -> T throws E | baml.errors.Io
```

`baml.errors.Io` is in the throws set because `baml.http.Server.bind` can fail;
a caller that catches only `E` does not handle a bind failure.

Binds `127.0.0.1:0`, serves `responses` to **any** method/path (so the same
helper answers `/responses`, `/v1/messages`, `/v1beta/models/x:generateContent`,
…), runs `body`, and cancels the serve task on the way out — including when
`body` throws, so a failing assertion never leaks a task.

Responses are answered in order and **the last one repeats**, so one canned
response covers any number of requests and an agent/tool loop can be scripted
turn by turn.

```baml
class MockResponse {
    status: int, body: string, headers: map<string, string>,

    function ok(body: string) -> MockResponse                 // 200 + JSON body
    function error(status: int, body: string) -> MockResponse // 4xx/5xx + JSON body
    function with_headers(status: int, body: string, headers: map<string, string>) -> MockResponse
}
```

`content-type: application/json` is sent by default; `headers` overrides it.

```baml
class MockJsonServer {
    server: baml.http.Server,
    responses: MockResponse[],
    requests: MockRequest[],          // every captured request, in order

    function base_url(self) -> string        // "http://127.0.0.1:54321"
    function request_count(self) -> int
    function last_request(self) -> MockRequest   // panics if nothing was sent
}
```

```baml
class MockRequest {
    method: string,
    url: string,                       // origin-form: "/v1/messages?key=abc"
    headers: map<string, string>,      // names lowercased by the HTTP layer
    body: string,

    function json(self) -> json               // parsed body, for baml.json.path<T>
    function header(self, name: string) -> string?   // case-insensitive
    function path(self) -> string             // url without the query string
    function query(self) -> string?           // query string without the "?"
}
```

Typical request-shape assertion:

```baml
let sent = mock.last_request();
assert.equal(baml.json.path<string>(sent.json(), ".model"), "claude-haiku-4-5");
assert.equal(sent.header("x-api-key"), "test-key");
assert.equal(baml.json.path_or<int>(sent.json(), ".max_tokens", 0), 8192)
```

### 2.2 SSE mock — streaming decode

```baml
function mock_sse_serve<T, E>(
    events: string[],
    body: (mock: root.llm_mock.MockSseServer) -> T throws E,
    chunk_delay_ms: int = 10,
) -> T throws E | baml.errors.Io
```

`baml.errors.Io` comes from `bind`, exactly as in §2.1.

Each element of `events` is one SSE event **without** the trailing blank line;
the mock appends the delimiter and flushes each event as its own socket write,
sleeping `chunk_delay_ms` in between. Do not set the delay to 0: a buffered
single-write body arrives in one socket read, the partial parser drains straight
to done, and host-driven streaming bugs go unseen (the reason is spelled out at
`sdk_tests/fixtures/llm_functions/baml_src/ns_replay/replay_server.baml:21-28`).

Event constructors:

```baml
function sse_data(data: string) -> string                  // "data: {…}"          (OpenAI)
function sse_event(name: string, data: string) -> string   // "event: x\ndata: {…}" (Anthropic)
function sse_events(body: string) -> string[]              // split a recorded body
```

`MockSseServer` exposes the same `base_url()` / `requests` / `request_count()` /
`last_request()` surface as the JSON mock, so a streaming test can still assert
on the request that opened the stream.

Reading raw SSE in a test: `baml.http.fetch_sse(req)` then `stream.next()`, which
returns **a JSON array string of `{"event","data","id"}` objects per batch** (not
one event), or null at end of stream — see
`../ns_streaming_sse_primitives/streaming_sse_primitives.baml:9-11`.

---

## 3. Driving a real client against a mock

The mock's port is ephemeral and only known at runtime, and BAML has no
`env.set`, so a `client Foo = openai.ResponsesClient.new(base_url = …)`
*declaration* (evaluated during `$init`) can never point at it. Construct the
client inside the test function and pass it to the runner explicitly — which is
exactly what a direct `Fn(args)` call desugars to
(`crates/baml_compiler2_ast/src/lower_expr_body.rs:729`, `:819`):

```baml
// non-streaming
let cl = openai.ResponsesClient.new(model = "gpt-4o-mini", api_key = "test-key",
                                    base_url = mock.base_url());
let result = ai.Agent<string>.new(client = cl).run(MockEcho@spec("Say pong."));
result.value      // also: result.journal, result.usage

// streaming (from_spec cannot infer its two type params — write them out)
let stream = ai.stream.from_spec<string | null, string>(MockEcho@spec("hi"), client = cl);
stream.final()
```

`example_openai_client.baml` is the runnable version of both. To test a client
*below* the runner (no prompt/SAP involved), call the interface directly:
`cl.invoke(input)` / `cl.invoke_stream(input)` with an `ai.ModelTurnInput`, or
the provider's internal render function (e.g.
`openai.internal.openai_render(cl, input)`) to assert on a built request without
any server at all.

---

## 4. Authoring gotchas (all of these have bitten someone)

1. **Test bodies must be top-level functions.** Test-block locals are boxed by
   the VM; a `test { }` block with locals miscompiles. Write the logic as a
   top-level `function`, keep the `test` block to `assert.*` on its return value.
   Same rule as `../ns_http_server/http_server.baml:3-5`.

2. **~~The word `client` (or `prompt`) inside a string literal can turn your
   function into an LLM function.~~ FIXED — kept as history.** The parser
   decides "is this an LLM function" with a raw token pre-scan of the body
   (`looks_like_llm_function_body_from` in
   `crates/baml_compiler_parser/src/parser.rs`), and the lexer emits no string
   token, so string *contents* used to arrive in that scan as ordinary tokens
   (`client` even lexes as `KW_CLIENT` inside quotes). A body whose first
   `client`/`prompt` token was not followed by `=`, `,`, `)`, `.` or `(`
   classified as an LLM function, producing a cascade of
   `Only 'client', 'tools' and 'prompt' allowed in LLM function`:

   ```baml
   function f() -> string { "the client sent nothing" }   // used to ERROR
   ```

   The pre-scan is now string-aware: it steps over `"…"`, `` `…` `` (including
   `${…}` interpolations and nested literals), `#"…"#` and `b"…"` spans via
   `skip_string_literal_from`, so prose is never read as syntax and a literal
   containing an unbalanced `{`/`}` no longer desynchronizes brace depth. Real
   LLM bodies still classify on their `client:`/`prompt:` fields. The full
   matrix is pinned in `../ns_llm_classifier_strings/` plus the parser unit
   tests `string_contents_never_classify_a_body_as_llm` and
   `real_llm_bodies_still_classify_with_string_aware_scan`.

   The old workarounds still read fine and stay valid, but are no longer
   required:

   ```baml
   function f() -> string { let msg = "the client sent nothing"; msg }   // ok
   function f() -> string { "no request was captured" }                  // ok
   ```

3. **`Array.push` and `Map.set` return a value**; consume it (`let _ = xs.push(x)`)
   or the statement is a type error.

4. **Cross-namespace references need `root.`-absolute paths.** No imports, no
   aliases: `root.llm_mock.MockResponse.ok(...)`. Within `ns_llm_mock` itself the
   short names work.

5. **Empty selections exit 5.** `crates/baml_cli/src/lib.rs:93-94`. If a whole
   group can be gated off (missing key), keep one always-registered leaf in it —
   see `preconditions` in `../ns_llm_live_smoke/live_smoke.baml`.

6. **Testsets run children in parallel by default** (`registry.baml:483-492`).
   Anything that can trip a rate limit or a port collision wants
   `testset "…" with testing.Sequential()`. Per-leaf runners exist too:
   `testing.Retry(n)`, `testing.Quorum(n, m)`, `testing.PassRate(p)`.

7. **There is no "skipped" outcome** — `Outcome = pass | fail | error`. A test is
   skipped by not being registered (env gate) or not being selected (profile).
   Use `log.warn` in the `else` branch; it needs `--logs warn` to be visible.

8. **Never put `test` blocks in `crates/baml_builtins2/baml_std/`.** `baml test`
   collects only the `user` package (`test_command.rs:584`,
   `bex_engine/src/lib.rs:3897-3906`), so stdlib tests compile and never run. All
   native provider tests belong here.

9. **New namespace directories add bytecode snapshots.** `tests/baml_src.rs`
   snapshots each `ns_*` dir separately; a new namespace needs
   `cargo insta test -p baml_tests --accept` once, and CI fails on any
   uncommitted `.snap`/`.snap.new`.

10. **Live tests must stay under a `::live::` testset** and be registered only
    when their key is present. Anything else breaks CI, which runs
    `-p baml_tests` with no Infisical step and no provider keys.

---

## 5. Adding a provider test namespace

1. `mkdir crates/baml_tests/baml_src/ns_<provider>/` and add one `.baml` file.
2. Write each case as a top-level `function` that calls
   `root.llm_mock.mock_json_serve` / `mock_sse_serve`, plus a thin `test` block.
3. Put real-endpoint coverage in `../ns_llm_live_smoke/live_smoke.baml`'s
   `testset "live"` (env-gated leaf, current model id), not in your namespace.
4. `target/debug/baml-cli test --from crates/baml_tests/baml_src -i "<provider>"`.
5. Regenerate snapshots once: `cargo insta test -p baml_tests --accept`.
