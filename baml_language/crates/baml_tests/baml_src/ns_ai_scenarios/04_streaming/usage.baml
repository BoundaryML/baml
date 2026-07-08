// scenario 04 — streaming: ../../../../../llm-provider/ideas/scenarios/04-streaming/README.md
// Streaming negotiation offline (SSE itself is the Rust mock/live tier:
// openai_stream_via_mock / openai_stream_live / structured_streaming_live).

client<llm> DeadS04 {
  provider "openai"
  options { model "gpt-4o" api_key "unused" base_url "http://127.0.0.1:9" }
}

function StreamStory(topic: string) -> string {
  client DeadS04
  prompt #"A one-liner about {{ topic }}"#
}

function s04_try_stream(p: baml.ai.Provider) -> string throws never {
    let st = StreamStory$stream("owls", client = p) catch (e) {
        let u: baml.errors.Unsupported => return "unsupported",
        _ => return "wrong error",
    };
    "streamed"
}

test "04: Foo$stream on a non-streaming client is a typed Unsupported" {
    assert.equal(s04_try_stream(EchoProvider { reply: "x" }), "unsupported")
}

test "04: drive_call degrades stream-capable-only clients honestly (absent both -> typed)" {
    let r = AskViaCall(NoCapProvider {});
    assert.equal(r, "neither")
}

function AskViaCall(p: baml.ai.Provider) -> string throws never {
    match (p) {
        let h: baml.ai.HttpProvider => "http",
        let s: baml.ai.Streaming => "stream-only",
        _ => "neither",
    }
}
