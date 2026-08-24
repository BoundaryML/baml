// The provider-agnostic stream TERMINATION contract of `ai.stream.TurnStream`,
// tested with a miniature provider protocol instead of a real client, so the
// mechanism is pinned once for all four providers.
//
// The protocol: `data: {"delta":"…"}` for text, `data: {"stop":"complete"}` for
// metadata, `data: [DONE]` as the explicit end-of-turn event (decoded as
// `ai.stream.TurnDone`).
//
// What is under test (see the contract comment at the top of
// `baml_std/ai/ns_stream/stream.baml`):
//
//   * a stream that ends WITHOUT the terminal event, opened WITH
//     `require_terminal`, throws `ai.errors.NetworkFailure` instead of
//     reporting the truncated text as a completed turn;
//   * the same truncated stream opened WITHOUT `require_terminal` keeps the
//     historical behavior (a `StopReason.Complete` turn with the prefix) — the
//     opt-in must be backward compatible;
//   * a complete stream opened WITH `require_terminal` behaves normally.
//
// Authoring rules: each case is a top-level `function` with a thin `test`
// block (test-block locals are boxed by the VM), and cross-namespace helpers
// are `root.`-absolute. See `../ns_llm_mock/README.md`.

// ─── the miniature provider protocol ─────────────────────────────────────────

// One decoded `data:` payload of the toy protocol.
class ContractChunk {
    delta: string?,
    stop: string?,
}

// The decoder handed to `TurnStream.from_sse`: one raw SSE batch -> events.
// `[DONE]` is this protocol's terminal event, so it yields `TurnDone` — which
// is exactly what makes `require_terminal` meaningful for it.
function contract_decode(batch: string) -> ai.stream.StreamEvent[] throws ai.errors.InvokeError {
    let out: ai.stream.StreamEvent[] = [];
    let events = ai.stream.decode_sse_batch(batch) catch_all (e) {
        _ => throw ai.errors.normalize(e),
    };
    for (let raw in events) {
        let data = (raw.data ?? "").trim();
        if (data == "") {
            continue;
        }
        if (data == "[DONE]") {
            let _ = out.push(ai.stream.TurnDone {  });
            continue;
        }
        // A malformed chunk is wire corruption: classify like the real decoders.
        let chunk = baml.json.from_string<ContractChunk>(data) catch_all (e) {
            _ => throw ai.errors.normalize(e),
        };
        if let text: string = chunk.delta {
            let _ = out.push(ai.stream.TextDelta { text: text });
        }
        if let stop: string = chunk.stop {
            let _ = out.push(
                ai.stream.TurnMeta {
                    stop_reason: ai.content.StopReason.Complete,
                    input_tokens: 7,
                    output_tokens: 3,
                },
            );
        }
    }
    out
}

// ─── helpers ─────────────────────────────────────────────────────────────────

// Open the mock's SSE endpoint. The mock answers any method/path.
function contract_open(base_url: string) -> baml.http.SseStream {
    baml.http.fetch_sse(
        baml.http.Request {
            method: "GET",
            url: base_url + "/v1/stream",
            headers: { },
            body: "",
        },
    )
}

// A completed turn rendered as a comparable label.
function contract_turn_label(turn: ai.ModelTurn) -> string {
    [
        `stop=${turn.stop_reason.to_string()}`,
        `text=${turn.terminal_text() ?? ""}`,
    ].join("|")
}

// A thrown failure rendered as a comparable label: class, retry safety, detail.
function contract_error_label(error: unknown) -> string {
    match (error) {
        let network: ai.errors.NetworkFailure => {
            [
                "NetworkFailure",
                network.retry_safety().to_string(),
                network.provider,
                network.detail,
                `raw=${network.raw_body ?? ""}`,
            ].join("|")
        },
        let other: ai.errors.Failure => `other-failure:${other.to_string()}`,
        _ => `other:${error.to_string()}`,
    }
}

// Drain a strict stream (opted in) and label the outcome.
function contract_drain_strict(base_url: string) -> string {
    let stream = ai.stream.TurnStream.from_sse(
        contract_open(base_url),
        contract_decode,
        require_terminal = "mockprovider",
    );
    contract_turn_label(stream.final_turn())
}

// Drain a lenient stream (no opt-in) and label the outcome.
function contract_drain_lenient(base_url: string) -> string {
    let stream = ai.stream.TurnStream.from_sse(contract_open(base_url), contract_decode);
    contract_turn_label(stream.final_turn())
}

// The events of a stream cut off mid-generation: text arrived, the connection
// then closed with no `[DONE]`.
function contract_truncated_events() -> string[] {
    [
        root.llm_mock.sse_data(`{"delta":"par"}`),
        root.llm_mock.sse_data(`{"delta":"tial"}`),
    ]
}

// The events of a well-formed turn: text, metadata, then the terminal event.
function contract_complete_events() -> string[] {
    [
        root.llm_mock.sse_data(`{"delta":"all"}`),
        root.llm_mock.sse_data(`{"delta":" good"}`),
        root.llm_mock.sse_data(`{"stop":"complete"}`),
        root.llm_mock.sse_data("[DONE]"),
    ]
}

// ─── cases ───────────────────────────────────────────────────────────────────

// Opted in + truncated: the disconnect surfaces as a classified failure.
function stream_contract_truncated_strict_throws() -> string {
    root.llm_mock.mock_sse_serve(
        contract_truncated_events(),
        (mock: root.llm_mock.MockSseServer) -> {
            contract_drain_strict(mock.base_url()) catch_all (e) {
                _ => contract_error_label(e),
            }
        },
    )
}

// The three things the strict label must say, as one comparable string: the
// classification, the reason, and the preserved prefix. Asserted from a
// top-level function so the value is never a test-block local (the corpus rule
// at the top of this file).
function stream_contract_truncated_strict_label() -> string {
    let got = stream_contract_truncated_strict_throws();
    [
        // classified as a transport failure, safely retryable, named provider
        `classified=${got.includes("NetworkFailure|Safe|mockprovider|")}`,
        `reason=${got.includes("stream ended before the provider's terminal event")}`,
        // the prefix that did arrive is preserved for debugging, not returned
        // as the answer
        `prefix=${got.includes("raw=partial")}`,
    ].join("|")
}

test "stream_contract_truncated_strict_throws" {
    assert.equal(
        stream_contract_truncated_strict_label(),
        "classified=true|reason=true|prefix=true",
    )
}

// NOT opted in + truncated: unchanged legacy behavior. This is the guard that
// keeps the mechanism additive — every existing `from_sse` call site behaves
// exactly as it did before `require_terminal` existed.
function stream_contract_truncated_lenient_completes() -> string {
    root.llm_mock.mock_sse_serve(
        contract_truncated_events(),
        (mock: root.llm_mock.MockSseServer) -> {
            contract_drain_lenient(mock.base_url()) catch_all (e) {
                _ => contract_error_label(e),
            }
        },
    )
}

test "stream_contract_truncated_lenient_completes" {
    assert.equal(stream_contract_truncated_lenient_completes(), "stop=Complete|text=partial")
}

// Opted in + well-formed: the terminal event was decoded, so nothing changes.
function stream_contract_complete_strict_returns_turn() -> string {
    root.llm_mock.mock_sse_serve(
        contract_complete_events(),
        (mock: root.llm_mock.MockSseServer) -> {
            contract_drain_strict(mock.base_url()) catch_all (e) {
                _ => contract_error_label(e),
            }
        },
    )
}

test "stream_contract_complete_strict_returns_turn" {
    assert.equal(stream_contract_complete_strict_returns_turn(), "stop=Complete|text=all good")
}

// The delta path is unaffected by strictness: a strict stream still yields the
// same chunks from `next()` before it decides how the stream ended.
function contract_deltas_then_outcome(base_url: string) -> string {
    let stream = ai.stream.TurnStream.from_sse(
        contract_open(base_url),
        contract_decode,
        require_terminal = "mockprovider",
    );
    let chunks: string[] = [];
    while (true) {
        match (stream.next()) {
            let d: ai.stream.Done => {
                break;
            },
            let s: string => {
                let _ = chunks.push(s);
                null
            },
        };
    }
    chunks.join(",")
}

function stream_contract_truncated_strict_next_throws() -> string {
    root.llm_mock.mock_sse_serve(
        contract_truncated_events(),
        (mock: root.llm_mock.MockSseServer) -> {
            contract_deltas_then_outcome(mock.base_url()) catch_all (e) {
                _ => contract_error_label(e),
            }
        },
    )
}

test "stream_contract_truncated_strict_next_throws" {
    // next() itself refuses to report Done for a stream that was cut off
    assert.contains(stream_contract_truncated_strict_next_throws(), "NetworkFailure|Safe|mockprovider|")
}

function stream_contract_complete_strict_yields_deltas() -> string {
    root.llm_mock.mock_sse_serve(
        contract_complete_events(),
        (mock: root.llm_mock.MockSseServer) -> {
            contract_deltas_then_outcome(mock.base_url()) catch_all (e) {
                _ => contract_error_label(e),
            }
        },
    )
}

test "stream_contract_complete_strict_yields_deltas" {
    assert.equal(stream_contract_complete_strict_yields_deltas(), "all, good")
}

// Scripted streams have no wire to cut, so they are never strict.
function stream_contract_from_chunks_unchanged() -> string {
    contract_turn_label(ai.stream.TurnStream.from_chunks(["a", "b"]).final_turn())
}

test "stream_contract_from_chunks_unchanged" {
    assert.equal(stream_contract_from_chunks_unchanged(), "stop=Complete|text=ab")
}
