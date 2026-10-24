// OpenAI Structured Outputs (`structured_output = true`) on `openai.ChatClient`
// and `openai.ResponsesClient`, against the shared mock provider in
// ../ns_llm_mock.
//
// Asserted: the strict JSON Schema each request carries, the strict reading of
// the answer (no SAP fallback, no re-ask), and the output types rejected
// before anything is sent.
//
// Corpus rules (see ../ns_llm_mock/README.md): every case body is a top-level
// `function`, and the `test` block only asserts. Cross-namespace references
// are `root.`-absolute.

// ─── fixtures ────────────────────────────────────────────────────────────────

enum StructuredMood {
    Happy @alias("happy")
    Sad
}

class StructuredPlace {
    city: string,
}

class StructuredAnswer {
    title: string @alias("headline"),
    mood: StructuredMood,
    note: string?,
    /// Where it happened.
    place: StructuredPlace,
}

function StructuredPick(question: string) -> StructuredAnswer {
    client: "openai/gpt-4o-mini"
    prompt: `${role("user")}${question}\n${ctx.output_format()}`
}

function StructuredWords(question: string) -> string[] {
    client: "openai/gpt-4o-mini"
    prompt: `${role("user")}${question}\n${ctx.output_format()}`
}

function StructuredCounts(question: string) -> map<string, int> {
    client: "openai/gpt-4o-mini"
    prompt: `${role("user")}${question}\n${ctx.output_format()}`
}

class StructuredPhoto {
    caption: string,
    photo: image,
}

function StructuredPhotoPick(question: string) -> StructuredPhoto {
    client: "openai/gpt-4o-mini"
    prompt: `${role("user")}${question}\n${ctx.output_format()}`
}

function _structured_answer_json() -> string {
    `{"headline": "Rain", "mood": "happy", "note": null, "place": {"city": "Oslo"}}`
}

function _structured_chat_body(content: string) -> string {
    let text = baml.json.stringify(baml.json.to_json(content));
    `{"id":"chatcmpl-1","model":"gpt-4o-mini","choices":[{"index":0,"message":{"role":"assistant","content":${text}},"finish_reason":"stop"}]}`
}

function _structured_responses_body(content: string) -> string {
    let text = baml.json.stringify(baml.json.to_json(content));
    `{"id":"resp-1","model":"gpt-4o-mini","status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":${text}}]}]}`
}

function _structured_chat_client(mock: root.llm_mock.MockJsonServer, structured: bool) -> openai.ChatClient {
    openai.ChatClient.new(
        model = "gpt-4o-mini",
        api_key = "test-key",
        base_url = mock.base_url(),
        structured_output = structured,
    )
}

function _structured_failure(error: unknown) -> string {
    match (error) {
        let f: ai.errors.InvalidRequest => `InvalidRequest: ${f.detail}`,
        let f: ai.errors.ParseFailed => `ParseFailed: ${f.message ?? "-"}`,
        let f: ai.errors.Refused => "Refused",
        _ => "other",
    }
}

// ─── chat: the request ───────────────────────────────────────────────────────

function structured_chat_request() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_structured_chat_body(_structured_answer_json()))],
        (mock: root.llm_mock.MockJsonServer) -> {
            let _ = ai.Agent.new(client = _structured_chat_client(mock, true)).run(StructuredPick@spec("q"));
            let body = mock.last_request().json();
            let format = baml.json.path<json>(body, ".response_format");
            let schema = baml.json.path<json>(format, ".json_schema.schema");
            [
                baml.json.path<string>(format, ".type"),
                baml.json.path<string>(format, ".json_schema.name"),
                `${baml.json.path<bool>(format, ".json_schema.strict")}`,
                baml.json.path<map<string, json>>(schema, ".properties").keys().join(","),
                baml.json.path<string[]>(schema, ".required").join(","),
                `${baml.json.path<bool>(schema, ".additionalProperties")}`,
                baml.json.stringify(baml.json.path<json>(schema, ".properties.note.type")),
                baml.json.path<string[]>(schema, ".properties.mood.enum").join(","),
                // a described `$ref` is wrapped: strict mode rejects keywords beside `$ref`
                baml.json.path<string>(schema, ".properties.place.anyOf[0].$ref"),
                baml.json.path<string>(schema, ".properties.place.description"),
                `${baml.json.path<bool>(schema, ".$defs[\"llm_openai_structured.StructuredPlace\"].additionalProperties")}`,
            ].join("|")
        },
    )
}

test "a chat request carries the strict schema of the output type" {
    assert.equal(
        structured_chat_request(),
        "json_schema|output|true|headline,mood,note,place|headline,mood,note,place|false|[\"string\",\"null\"]|happy,Sad|#/$defs/llm_openai_structured.StructuredPlace|Where it happened.|false",
    )
}

function structured_chat_request_off() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_structured_chat_body(_structured_answer_json()))],
        (mock: root.llm_mock.MockJsonServer) -> {
            let _ = ai.Agent.new(client = _structured_chat_client(mock, false)).run(StructuredPick@spec("q"));
            baml.json.path_or<string>(mock.last_request().json(), ".response_format.type", "absent")
        },
    )
}

test "without the option a chat request carries no response_format" {
    assert.equal(structured_chat_request_off(), "absent")
}

// ─── chat: the answer ────────────────────────────────────────────────────────

function structured_chat_answer() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_structured_chat_body(_structured_answer_json()))],
        (mock: root.llm_mock.MockJsonServer) -> {
            let answer = ai.Agent.new(client = _structured_chat_client(mock, true)).run(StructuredPick@spec("q")).value;
            `${answer.title}|${answer.mood}|${answer.note ?? "null"}|${answer.place.city}`
        },
    )
}

test "a chat answer is read strictly under aliased names" {
    assert.equal(structured_chat_answer(), "Rain|Happy|null|Oslo")
}

// SAP would read this fenced answer; the strict reading does not, and the
// runner does not re-ask: one request, then `ParseFailed`.
function structured_chat_lenient_answer() -> string {
    root.llm_mock.mock_json_serve(
        [
            root.llm_mock.MockResponse.ok(_structured_chat_body("```json\n" + _structured_answer_json() + "\n```")),
            root.llm_mock.MockResponse.ok(_structured_chat_body(_structured_answer_json())),
        ],
        (mock: root.llm_mock.MockJsonServer) -> {
            let outcome = {
                ai.Agent.new(client = _structured_chat_client(mock, true)).run(StructuredPick@spec("q"));
                "answered"
            } catch_all (e) {
                _ => _structured_failure(e),
            };
            `${mock.request_count()}|${outcome.starts_with("ParseFailed: the structured output does not fit")}`
        },
    )
}

test "a chat answer that is not the schema's JSON fails without a re-ask" {
    assert.equal(structured_chat_lenient_answer(), "1|true")
}

function structured_chat_refusal() -> string {
    let refusal = `{"id":"chatcmpl-1","model":"gpt-4o-mini","choices":[{"index":0,"message":{"role":"assistant","content":null,"refusal":"I can't help with that."},"finish_reason":"stop"}]}`;
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(refusal)],
        (mock: root.llm_mock.MockJsonServer) -> {
            {
                ai.Agent.new(client = _structured_chat_client(mock, true)).run(StructuredPick@spec("q"));
                "answered"
            } catch_all (e) {
                _ => _structured_failure(e),
            }
        },
    )
}

test "a chat refusal is a refusal, not a parse failure" {
    assert.equal(structured_chat_refusal(), "Refused")
}

// ─── chat: an output type that is not an object ──────────────────────────────

function structured_chat_wrapped() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_structured_chat_body(`{"value": ["a", "b"]}`))],
        (mock: root.llm_mock.MockJsonServer) -> {
            let words = ai.Agent.new(client = _structured_chat_client(mock, true)).run(StructuredWords@spec("q")).value;
            let schema = baml.json.path<json>(mock.last_request().json(), ".response_format.json_schema.schema");
            [
                baml.json.path<string[]>(schema, ".required").join(","),
                baml.json.path<string>(schema, ".properties.value.type"),
                words.join("+"),
            ].join("|")
        },
    )
}

test "a list output is sent as the value of an object, and read back out of it" {
    assert.equal(structured_chat_wrapped(), "value|array|a+b")
}

// ─── chat: rejected before sending ───────────────────────────────────────────

function structured_chat_rejects_map() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_structured_chat_body(`{"value": {}}`))],
        (mock: root.llm_mock.MockJsonServer) -> {
            let outcome = {
                ai.Agent.new(client = _structured_chat_client(mock, true)).run(StructuredCounts@spec("q"));
                "answered"
            } catch_all (e) {
                _ => _structured_failure(e),
            };
            `${mock.request_count()}|${outcome}`
        },
    )
}

test "a map output is rejected before anything is sent" {
    assert.equal(
        structured_chat_rejects_map(),
        "0|InvalidRequest: structured outputs cannot describe `map<string, int>`: it holds a map, which strict mode has no form for",
    )
}

function structured_chat_rejects_media() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_structured_chat_body(`{}`))],
        (mock: root.llm_mock.MockJsonServer) -> {
            let outcome = {
                ai.Agent.new(client = _structured_chat_client(mock, true)).run(StructuredPhotoPick@spec("q"));
                "answered"
            } catch_all (e) {
                _ => _structured_failure(e),
            };
            `${mock.request_count()}|${outcome}`
        },
    )
}

test "an output that can hold media is rejected before anything is sent" {
    assert.contains(structured_chat_rejects_media(), "0|InvalidRequest: structured outputs cannot hold")
    assert.contains(structured_chat_rejects_media(), "it can hold media")
}

function structured_chat_rejects_stream() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_structured_chat_body(_structured_answer_json()))],
        (mock: root.llm_mock.MockJsonServer) -> {
            let outcome = {
                ai.stream.from_spec<StructuredAnswer>(
                    StructuredPick@spec("q"),
                    client = _structured_chat_client(mock, true),
                ).final();
                "streamed"
            } catch_all (e) {
                _ => _structured_failure(e),
            };
            `${mock.request_count()}|${outcome}`
        },
    )
}

test "streaming with structured outputs is rejected before anything is sent" {
    assert.contains(structured_chat_rejects_stream(), "0|InvalidRequest: structured outputs do not stream yet")
}

// A union member with no JSON form is not in the schema, so media in it does
// not reject the type.
class StructuredBlob {
    bytes: uint8array,
    photo: image,
}

test "media in a union member with no JSON form does not reach the answer" {
    assert.is_true(!ai.internal.reaches_media(reflect.Type.of<StructuredBlob | string>()));
    assert.is_true(ai.internal.reaches_media(reflect.Type.of<StructuredPhoto | string>()))
}

// A `@skip` field is not in the JSON the model answers with, so media in one
// does not reject the type: the field holds its empty value.
class StructuredSkipsPhoto {
    caption: string,
    thumb: image? @skip,
}

function StructuredSkipsPhotoPick(question: string) -> StructuredSkipsPhoto {
    client: "openai/gpt-4o-mini"
    prompt: `${role("user")}${question}\n${ctx.output_format()}`
}

function structured_chat_skipped_media() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_structured_chat_body(`{"caption": "Sunset"}`))],
        (mock: root.llm_mock.MockJsonServer) -> {
            let answer = ai.Agent.new(client = _structured_chat_client(mock, true)).run(StructuredSkipsPhotoPick@spec("q")).value;
            let schema = baml.json.path<json>(mock.last_request().json(), ".response_format.json_schema.schema");
            [
                baml.json.path<map<string, json>>(schema, ".properties").keys().join(","),
                answer.caption,
                if (answer.thumb == null) {
                    "null"
                } else {
                    "media"
                },
            ].join("|")
        },
    )
}

test "media in a skipped field does not reject the type" {
    assert.equal(structured_chat_skipped_media(), "caption|Sunset|null")
}

// ─── responses ───────────────────────────────────────────────────────────────

function structured_responses_round_trip() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_structured_responses_body(_structured_answer_json()))],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.ResponsesClient.new(
                model = "gpt-4o-mini",
                api_key = "test-key",
                base_url = mock.base_url(),
                structured_output = true,
            );
            let answer = ai.Agent.new(client = cl).run(StructuredPick@spec("q")).value;
            let format = baml.json.path<json>(mock.last_request().json(), ".text.format");
            [
                baml.json.path<string>(format, ".type"),
                baml.json.path<string>(format, ".name"),
                `${baml.json.path<bool>(format, ".strict")}`,
                baml.json.path<string[]>(format, ".schema.required").join(","),
                `${answer.title}|${answer.mood}|${answer.place.city}`,
            ].join("|")
        },
    )
}

test "a responses request carries text.format, and its answer is read strictly" {
    assert.equal(
        structured_responses_round_trip(),
        "json_schema|output|true|headline,mood,note,place|Rain|Happy|Oslo",
    )
}
