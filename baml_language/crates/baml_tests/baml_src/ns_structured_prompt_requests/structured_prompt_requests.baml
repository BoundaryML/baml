// Provider request builders consume the structural `ai.Prompt` produced by an
// LLM spec. These tests stay offline and inspect only the serialized request:
// every assertion runs against `@build_request` / `render` previews, never an
// invocation. Ported from the Rust `structured_prompt_requests.rs` suite; the
// one env-isolation test (Vertex late-bound project id) remains in Rust.

client PreviewResponses = openai.ResponsesClient.new(
    model = "gpt-test",
    api_key = "test-key",
    base_url = "http://localhost:1234/v1",
);

client SecondResponses = openai.ResponsesClient.new(
    model = "second",
    api_key = "test-key",
    base_url = "http://localhost:5678/v1",
);

client PreviewAnthropic = anthropic.Client.new(
    model = "claude-test",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

client PreviewGemini = google.GeminiClient.new(
    model = "gemini-test",
    api_key = "test-key",
    base_url = "http://localhost:1234/v1",
);

client PreviewBedrock = aws.BedrockClient.new(
    model = "amazon.test",
    endpoint_url = "http://localhost:1234",
);

client PreviewChat = openai.ChatClient.new(
    model = "chat-test",
    api_key = "test-key",
    base_url = "http://localhost:1234/v1",
);

client PreviewImages = openai.ImageClient.new(
    model = "gpt-image-1",
    api_key = "test-key",
    base_url = "http://localhost:1234/v1",
);

// Port 9 (discard) makes any accidental network attempt fail loudly.
client UnroutableChat = openai.ChatClient.new(
    model = "gpt-test",
    api_key = "test-key",
    base_url = "http://127.0.0.1:9/v1",
);

client UnroutableOllama = openai.OllamaClient.new(
    model = "local-test",
    base_url = "http://127.0.0.1:9/v1",
);

client UnroutableResponses = openai.ResponsesClient.new(
    model = "gpt-test",
    api_key = "test-key",
    base_url = "http://127.0.0.1:9/v1",
);

function _input_for(spec: ai.FunctionSpec<string>) -> ai.ModelTurnInput {
    ai.ModelTurnInput {
        prompt: spec.prompt_template,
        journal: ai.Journal { log: [] },
        toolbox: ai.tools.Toolbox.new([]),
        output_type: reflect.Type.of<string>(),
    }
}

// ── build_request companion basics ───────────────────────────────────────────

function RequestShape(input: string) -> string {
    client: "openai/gpt-4o-mini"
    prompt: `${input} ${ctx.output_format()}`
}

test "generated build_request companion is network-free and overridable" {
    assert.equal(
        RequestShape@build_request("hello", client = PreviewResponses).url,
        "http://localhost:1234/v1/responses",
    )
}

function Greet(name: string, suffix: string = "!") -> string {
    client: PreviewResponses
    prompt: `Hello ${name}${suffix}`
}

test "generated companions apply defaulted llm arguments" {
    let prompt = Greet@render_prompt("Ada").text();
    assert.is_true(prompt.starts_with("[system]\nHello Ada!"));
    assert.is_true(Greet@build_request("Ada").body.includes("Hello Ada!"))
}

test "generated build_request companion is reflection-visible" {
    let f: reflect.AnyFunction<
        Returns = baml.http.Request,
        Throws = ai.errors.Failure
            | baml.errors.InvalidArgument
            | baml.errors.Io
            | baml.errors.ParseError
            | baml.errors.Timeout
            | baml.errors.UnknownError
            | baml.panics.Cancelled
            | reflect.errors.CompilationError
    > = RequestShape@build_request;
    let request = reflect.call_any(f, { "input": "hello", "client": PreviewResponses });
    assert.equal(request.url, "http://localhost:1234/v1/responses")
}

// ── generic output-format specialization ─────────────────────────────────────

enum Choice {
    Empty,
}

class Inner<T> {
    values: T[],
}

class Outer<T> {
    inner: Inner<T>,
}

test "generic nested output format uses concrete class arguments" {
    let rendered = ai.wire.render_output_format(reflect.Type.of<Outer<Choice>>());
    assert.is_true(rendered.includes("'Empty'"));
    assert.is_true(!rendered.includes("TypeVar"));
    assert.is_true(!rendered.includes("T[]"))
}

function GenericShape<T>() -> Outer<T> {
    client: PreviewResponses
    prompt: `${ctx.output_format()}`
}

test "generic build_request specializes nested output format" {
    let body = GenericShape@build_request<Choice>().body;
    assert.is_true(body.includes("'Empty'"));
    assert.is_true(!body.includes("TypeVar"));
    assert.is_true(!body.includes("T[]"))
}

// ── composite-client rendering is pure ───────────────────────────────────────

function FirstShape() -> string {
    client: PreviewResponses
    prompt: `hello ${ctx.output_format()}`
}

test "client wrapper rendering is pure and deterministic" {
    let input = _input_for(FirstShape@spec());
    let retry = ai.clients.Retry.new(PreviewResponses);
    let fallback = ai.clients.Fallback { members: [PreviewResponses, SecondResponses] };
    let round_robin = ai.clients.RoundRobin.new([PreviewResponses, SecondResponses]);
    assert.equal(retry.render(input).url, "http://localhost:1234/v1/responses");
    assert.equal(fallback.render(input).url, "http://localhost:1234/v1/responses");
    // Rendering previews member 0 and never advances the rotation counter.
    assert.equal(round_robin.render(input).url, "http://localhost:1234/v1/responses");
    assert.equal(round_robin.render(input).url, "http://localhost:1234/v1/responses")
}

test "client render smoke covers non-responses providers" {
    let input = _input_for(FirstShape@spec());
    let a: ai.Client = PreviewAnthropic;
    let g: ai.Client = PreviewGemini;
    let b: ai.Client = PreviewBedrock;
    let c: ai.Client = PreviewChat;
    let i: ai.Client = PreviewImages;
    assert.equal(a.render(input).url, "http://localhost:1234/v1/messages");
    assert.equal(g.render(input).url, "http://localhost:1234/v1/models/gemini-test:generateContent");
    assert.equal(b.render(input).url, "http://localhost:1234/model/amazon.test/converse");
    assert.equal(c.render(input).url, "http://localhost:1234/v1/chat/completions");
    assert.equal(i.render(input).url, "http://localhost:1234/v1/images/generations")
}

// ── preview media purity ──────────────────────────────────────────────────────

function MediaShape(photo: image) -> string {
    client: UnroutableResponses
    prompt: `${role("user")}Inspect:${photo}`
}

test "preview media never reads files or fetches urls" {
    assert.equal(
        MediaShape@build_request(image.from_file("/definitely/not-a-real-preview-file.png", "image/png")).body catch_all (e) {
            ai.errors.PreviewUnsupported => "preview",
            _ => `wrong:${e.to_string()}`,
        },
        "preview",
    );
    assert.is_true(
        (MediaShape@build_request(image.from_url("http://127.0.0.1:9/image.png", "image/png")).body catch_all (e) {
            _ => `wrong:${e.to_string()}`,
        }).includes("http://127.0.0.1:9/image.png"),
    );
    assert.is_true(
        !(MediaShape@build_request(image.from_url("http://127.0.0.1:9/image.png", "image/png")).body catch_all (e) {
            _ => `wrong:${e.to_string()}`,
        }).includes("wrong:"),
    )
}

function ChatMediaShape(photo: image) -> string {
    client: UnroutableChat
    prompt: `${role("user")}Inspect:${photo}`
}

test "chat preview preserves urls unless invoke would inline them" {
    let photo = image.from_url("http://127.0.0.1:9/image.png", "image/png");
    let chat = ChatMediaShape@build_request(photo).body;
    assert.is_true(chat.includes("http://127.0.0.1:9/image.png"));
    // Ollama inlines image URLs at invoke time, so its preview must refuse the
    // URL rather than silently building a request invoke would not send.
    assert.equal(
        ChatMediaShape@build_request(photo, client = UnroutableOllama).body catch_all (e) {
            ai.errors.PreviewUnsupported => "unsupported",
            _ => `wrong:${e.to_string()}`,
        },
        "unsupported",
    )
}

// ── provider prompt lowering: roles ──────────────────────────────────────────

function RoledShape() -> string {
    client: "openai/gpt-4o-mini"
    prompt: `${role("system")}Follow the rules.${role("user")}Answer this.${ctx.output_format()}`
}

function _roled_body(request: baml.http.Request) -> baml.json.json {
    baml.json.parse(request.body)
}

test "openai preserves prompt message roles" {
    let input = _input_for(RoledShape@spec());
    let body = _roled_body(
        openai.internal.render(
            openai.ResponsesClient.new(model = "gpt-test", api_key = "test-key"),
            input,
        ),
    );
    assert.equal(baml.json.path<string>(body, ".input[0].role"), "system");
    assert.equal(baml.json.path<string>(body, ".input[0].content[0].type"), "input_text");
    assert.equal(baml.json.path<string>(body, ".input[0].content[0].text"), "Follow the rules.");
    assert.equal(baml.json.path<string>(body, ".input[1].role"), "user");
    assert.is_true(
        baml.json.path<string>(body, ".input[1].content[0].text").starts_with("Answer this."),
    );
    assert.equal(baml.json.path_or<string>(body, ".instructions", "<absent>"), "<absent>")
}

test "anthropic splits system from prompt messages" {
    let input = _input_for(RoledShape@spec());
    let body = _roled_body(
        anthropic.internal._request(
            anthropic.Client.new(model = "claude-test", api_key = "test-key"),
            input,
            false,
        ),
    );
    assert.equal(baml.json.path<string>(body, ".system[0].type"), "text");
    assert.equal(baml.json.path<string>(body, ".system[0].text"), "Follow the rules.");
    assert.equal(baml.json.path<string>(body, ".messages[0].role"), "user");
    assert.is_true(
        baml.json.path<string>(body, ".messages[0].content[0].text").starts_with("Answer this."),
    )
}

test "google splits system and maps user prompt" {
    let input = _input_for(RoledShape@spec());
    let body = _roled_body(
        google.internal.render(
            google.GeminiClient.new(model = "gemini-test", api_key = "test-key"),
            input,
        ),
    );
    assert.equal(
        baml.json.path<string>(body, ".systemInstruction.parts[0].text"),
        "Follow the rules.",
    );
    assert.equal(baml.json.path<string>(body, ".contents[0].role"), "user");
    assert.is_true(
        baml.json.path<string>(body, ".contents[0].parts[0].text").starts_with("Answer this."),
    )
}

// ── the compiler-generated `prompt:` role defaults (assemble_llm_prompt) ─────

function RolelessShape() -> string {
    client: PreviewResponses
    prompt: `Answer briefly.`
}

test "openai build_request defaults roleless llm prompt to system" {
    let body = baml.json.parse(RolelessShape@build_request().body);
    assert.equal(baml.json.path<string>(body, ".input[0].role"), "system");
    assert.equal(baml.json.path<string>(body, ".input[0].content[0].text"), "Answer briefly.")
}

function RolelessImageShape(photo: image) -> string {
    client: PreviewResponses
    prompt: `Inspect the image: ${photo}`
}

test "openai build_request allows image in roleless system prompt" {
    let body = baml.json.parse(
        RolelessImageShape@build_request(image.from_base64("image-data", "image/png")).body,
    );
    assert.equal(baml.json.path<string>(body, ".input[0].role"), "system");
    assert.equal(baml.json.path<string>(body, ".input[0].content[0].type"), "input_text");
    assert.equal(baml.json.path<string>(body, ".input[0].content[1].type"), "input_image");
    assert.equal(
        baml.json.path<string>(body, ".input[0].content[1].image_url"),
        "data:image/png;base64,image-data",
    )
}

function ExplicitRolesShape() -> string {
    client: PreviewResponses
    prompt: `${role("user")}Answer this.${role("system")}Follow the rules.`
}

test "openai build_request preserves explicit llm prompt roles" {
    let body = baml.json.parse(ExplicitRolesShape@build_request().body);
    assert.equal(baml.json.path<string>(body, ".input[0].role"), "user");
    assert.equal(baml.json.path<string>(body, ".input[0].content[0].text"), "Answer this.");
    assert.equal(baml.json.path<string>(body, ".input[1].role"), "system");
    assert.equal(baml.json.path<string>(body, ".input[1].content[0].text"), "Follow the rules.")
}

function PreludeShape() -> string {
    client: PreviewResponses
    prompt: `You are a differential assistant.\n${role("user")}Answer this.`
}

test "openai build_request defaults leading llm prelude to system" {
    let body = baml.json.parse(PreludeShape@build_request().body);
    assert.equal(baml.json.path<string>(body, ".input[0].role"), "system");
    assert.equal(
        baml.json.path<string>(body, ".input[0].content[0].text"),
        "You are a differential assistant.",
    );
    assert.equal(baml.json.path<string>(body, ".input[1].role"), "user");
    assert.equal(baml.json.path<string>(body, ".input[1].content[0].text"), "Answer this.")
}

function WhitespaceShape() -> string {
    client: PreviewResponses
    prompt: `
        ${role("user")}Answer this.
    `
}

test "openai build_request ignores formatting whitespace before first role" {
    let body = baml.json.parse(WhitespaceShape@build_request().body);
    assert.equal(baml.json.path<baml.json.json[]>(body, ".input").length(), 1);
    assert.equal(baml.json.path<string>(body, ".input[0].role"), "user")
}

// ── provider media lowering ──────────────────────────────────────────────────

function MediaAll(photo: image, sound: audio, document: pdf, movie: video) -> string {
    client: "google/gemini-2.5-flash"
    tools: []
    prompt: `${role("user")}Inspect:${photo}:${sound}:${document}:${movie}`
}

function MediaImageAudioPdf(photo: image, sound: audio, document: pdf) -> string {
    client: "google/gemini-2.5-flash"
    tools: []
    prompt: `${role("user")}Inspect:${photo}:${sound}:${document}:`
}

function MediaImagePdf(photo: image, document: pdf) -> string {
    client: "google/gemini-2.5-flash"
    tools: []
    prompt: `${role("user")}Inspect:${photo}:${document}:`
}

test "openai lowers image audio and pdf parts" {
    let spec = MediaImageAudioPdf@spec(
        image.from_base64("image-data", "image/png"),
        audio.from_base64("audio-data", "audio/mpeg"),
        pdf.from_base64("pdf-data", "application/pdf"),
    );
    let body = baml.json.parse(
        openai.internal
            .render(
                openai.ResponsesClient.new(model = "gpt-test", api_key = "test-key"),
                _input_for(spec),
            )
            .body,
    );
    let parts = baml.json.path<baml.json.json[]>(body, ".input[0].content");
    let by_type = (kind: string) -> baml.json.json {
        for (let part in parts) {
            if (baml.json.path_or<string>(part, ".type", "") == kind) {
                return part;
            }
        }
        baml.sys.panic(`missing OpenAI ${kind} part`)
    };
    assert.equal(
        baml.json.path<string>(by_type("input_image"), ".image_url"),
        "data:image/png;base64,image-data",
    );
    assert.equal(baml.json.path<string>(by_type("input_audio"), ".input_audio.data"), "audio-data");
    assert.equal(baml.json.path<string>(by_type("input_audio"), ".input_audio.format"), "mp3");
    assert.equal(
        baml.json.path<string>(by_type("input_file"), ".file_data"),
        "data:application/pdf;base64,pdf-data",
    )
}

test "anthropic lowers image and pdf parts" {
    let spec = MediaImagePdf@spec(image.from_base64("image-data", "image/png"), pdf.from_base64("pdf-data", "application/pdf"));
    let body = baml.json.parse(
        anthropic.internal
            ._request(
                anthropic.Client.new(model = "claude-test", api_key = "test-key"),
                _input_for(spec),
                false,
            )
            .body,
    );
    let parts = baml.json.path<baml.json.json[]>(body, ".messages[0].content");
    let by_type = (kind: string) -> baml.json.json {
        for (let part in parts) {
            if (baml.json.path_or<string>(part, ".type", "") == kind) {
                return part;
            }
        }
        baml.sys.panic(`missing Anthropic ${kind} part`)
    };
    assert.equal(baml.json.path<string>(by_type("image"), ".source.type"), "base64");
    assert.equal(baml.json.path<string>(by_type("image"), ".source.data"), "image-data");
    assert.equal(baml.json.path<string>(by_type("document"), ".source.data"), "pdf-data")
}

// The Messages API has no audio input content block; the lowering rejects it
// with a typed error instead of inventing a wire shape.
function AudioShape(sound: audio) -> string {
    client: "google/gemini-2.5-flash"
    tools: []
    prompt: `${role("user")}Listen:${sound}`
}

test "anthropic rejects audio parts" {
    let spec = AudioShape@spec(audio.from_base64("audio-data", "audio/mpeg"));
    let message = anthropic.internal
        ._request(
            anthropic.Client.new(model = "claude-test", api_key = "test-key"),
            _input_for(spec),
            false,
        )
        .body catch_all (e) {
        _ => e.to_string(),
    };
    assert.is_true(message.includes("audio"))
}

test "google lowers every supported media part" {
    let spec = MediaAll@spec(
        image.from_base64("image-data", "image/png"),
        audio.from_base64("audio-data", "audio/mpeg"),
        pdf.from_base64("pdf-data", "application/pdf"),
        video.from_base64("video-data", "video/mp4"),
    );
    let body = baml.json.parse(
        google.internal
            .render(
                google.GeminiClient.new(model = "gemini-test", api_key = "test-key"),
                _input_for(spec),
            )
            .body,
    );
    let parts = baml.json.path<baml.json.json[]>(body, ".contents[0].parts");
    let inline: baml.json.json[] = [];
    for (let part in parts) {
        if (baml.json.path_or<string>(part, ".inlineData.mimeType", "") != "") {
            inline.push(part);
        }
    }
    assert.equal(inline.length(), 4);
    assert.equal(baml.json.path<string>(inline[0], ".inlineData.mimeType"), "image/png");
    assert.equal(baml.json.path<string>(inline[0], ".inlineData.data"), "image-data");
    assert.equal(baml.json.path<string>(inline[1], ".inlineData.mimeType"), "audio/mpeg");
    assert.equal(baml.json.path<string>(inline[1], ".inlineData.data"), "audio-data");
    assert.equal(baml.json.path<string>(inline[2], ".inlineData.mimeType"), "application/pdf");
    assert.equal(baml.json.path<string>(inline[2], ".inlineData.data"), "pdf-data");
    assert.equal(baml.json.path<string>(inline[3], ".inlineData.mimeType"), "video/mp4");
    assert.equal(baml.json.path<string>(inline[3], ".inlineData.data"), "video-data")
}

// ── cache delimiters ─────────────────────────────────────────────────────────
//
// `${cache(args)}` marks a position. A client with no cache marker for it sends
// exactly the request it would send for the same prompt without the delimiter:
// same text, same parts, same string-versus-array content shape.

client PreviewGeneric = openai.GenericClient.new(
    model = "generic-test",
    base_url = "http://localhost:1234/v1",
);

client PreviewOpenRouter = openai.OpenRouterClient.new(
    model = "router-test",
    api_key = "test-key",
    base_url = "http://localhost:1234/v1",
);

function CacheMarked(name: string, picture: image) -> string {
    client: PreviewResponses
    prompt: `${role("system")}You greet people.${cache({ "type": "default" })}${role("user")}Long stable document. ${cache({ "type": "default", "ttl": "1h" })}Greet ${name}.${role("user")}Look at ${picture} closely.${cache({ "type": "default" })}`
}

function CachePlain(name: string, picture: image) -> string {
    client: PreviewResponses
    prompt: `${role("system")}You greet people.${role("user")}Long stable document. Greet ${name}.${role("user")}Look at ${picture} closely.`
}

function _cache_delimiter_is_dropped_by(target: ai.Client) -> bool {
    let picture = image.from_base64("image-data", "image/png");
    let marked = CacheMarked@build_request("Ada", picture, client = target).body;
    let plain = CachePlain@build_request("Ada", picture, client = target).body;
    marked == plain
}

test "clients without a cache marker send the prompt as if the delimiter were absent" {
    assert.is_true(_cache_delimiter_is_dropped_by(PreviewGemini));
    assert.is_true(_cache_delimiter_is_dropped_by(PreviewGeneric));
    assert.is_true(_cache_delimiter_is_dropped_by(PreviewOpenRouter));
    assert.is_true(_cache_delimiter_is_dropped_by(UnroutableOllama))
}

// The clients whose wire has a cache marker send one, so their two bodies
// differ. `args` is written for one wire; here it is Bedrock's.
test "clients with a cache marker send one for a cache delimiter" {
    assert.is_true(_cache_delimiter_is_dropped_by(PreviewAnthropic) == false);
    assert.is_true(_cache_delimiter_is_dropped_by(PreviewChat) == false);
    assert.is_true(_cache_delimiter_is_dropped_by(PreviewResponses) == false);
    assert.is_true(_cache_delimiter_is_dropped_by(PreviewBedrock) == false)
}

test "bedrock sends a cachePoint for a cache delimiter" {
    let picture = image.from_base64("image-data", "image/png");
    let body = baml.json.parse(CacheMarked@build_request("Ada", picture, client = PreviewBedrock).body);
    assert.equal(
        baml.json.stringify(baml.json.path<json>(body, ".system")),
        `[{"text":"You greet people."},{"cachePoint":{"type":"default"}}]`,
    );
    assert.equal(
        baml.json.stringify(baml.json.path<json>(body, ".messages[0].content[1]")),
        `{"cachePoint":{"type":"default","ttl":"1h"}}`,
    )
}

// ── `${cache()}`: the client's default ───────────────────────────────────────
//
// With no args (or null), the delimiter carries no provider object. The client
// that sends the request fills in its own `default_cache_args()`, or sends
// nothing when it has none.

client DefaultBedrockClaude = aws.BedrockClient.new(
    model = "us.anthropic.claude-haiku-4-5-20251001-v1:0",
    endpoint_url = "http://localhost:1234",
);

client DefaultResponses = openai.ResponsesClient.new(
    model = "gpt-5.6-luna",
    api_key = "test-key",
    base_url = "http://localhost:1234/v1",
);

client DefaultChat = openai.ChatClient.new(
    model = "gpt-5.6-luna",
    api_key = "test-key",
    base_url = "http://localhost:1234/v1",
);

client OldResponses = openai.ResponsesClient.new(
    model = "gpt-5.5",
    api_key = "test-key",
    base_url = "http://localhost:1234/v1",
);

client OldChat = openai.ChatClient.new(
    model = "gpt-5-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234/v1",
);

client PreviewAzure = openai.AzureClient.new(
    model = "my-deployment",
    api_key = "test-key",
    base_url = "http://localhost:1234/openai/deployments/my-deployment",
);

function CacheDefault(name: string) -> string {
    client: PreviewResponses
    prompt: `${role("system")}You greet people.${cache()}${role("user")}Long stable document. ${cache()}Greet ${name}.`
}

function CacheNull(name: string) -> string {
    client: PreviewResponses
    prompt: `${role("system")}You greet people.${cache(null)}${role("user")}Long stable document. ${cache(null)}Greet ${name}.`
}

function CacheUnmarked(name: string) -> string {
    client: PreviewResponses
    prompt: `${role("system")}You greet people.${role("user")}Long stable document. Greet ${name}.`
}

function _default_cache_body(target: ai.Client) -> string {
    CacheDefault@build_request("Ada", client = target).body
}

function _default_cache_sends_nothing(target: ai.Client) -> bool {
    _default_cache_body(target) == CacheUnmarked@build_request("Ada", client = target).body
}

test "cache() sends each client's own default marker" {
    assert.equal(
        baml.json.stringify(
            baml.json.path<json>(baml.json.parse(_default_cache_body(PreviewAnthropic)), ".system"),
        ),
        `[{"type":"text","text":"You greet people.","cache_control":{"type":"ephemeral"}}]`,
    );
    assert.equal(
        baml.json.stringify(
            baml.json.path<json>(baml.json.parse(_default_cache_body(PreviewAnthropic)), ".messages[0].content"),
        ),
        `[{"type":"text","text":"Long stable document.","cache_control":{"type":"ephemeral"}},{"type":"text","text":"Greet Ada."}]`,
    );
    assert.equal(
        baml.json.stringify(
            baml.json.path<json>(baml.json.parse(_default_cache_body(DefaultBedrockClaude)), ".system"),
        ),
        `[{"text":"You greet people."},{"cachePoint":{"type":"default"}}]`,
    );
    assert.equal(
        baml.json.stringify(
            baml.json.path<json>(baml.json.parse(_default_cache_body(DefaultResponses)), ".input[0].content"),
        ),
        `[{"type":"input_text","text":"You greet people.","prompt_cache_breakpoint":{"mode":"explicit"}}]`,
    );
    assert.equal(
        baml.json.stringify(
            baml.json.path<json>(baml.json.parse(_default_cache_body(DefaultChat)), ".messages[0].content"),
        ),
        `[{"type":"text","text":"You greet people.","prompt_cache_breakpoint":{"mode":"explicit"}}]`,
    )
}

// No default: no cache marker on the wire (Gemini, the generic chat hosts), a
// model the marker would be rejected by (GPT-5.5 and earlier, a Bedrock model
// that is not Claude or Nova), or a model that cannot be told (an Azure
// deployment). The request is the one the prompt produces without `${cache()}`.
test "cache() sends nothing where the client has no default" {
    assert.is_true(_default_cache_sends_nothing(PreviewGemini));
    assert.is_true(_default_cache_sends_nothing(PreviewGeneric));
    assert.is_true(_default_cache_sends_nothing(PreviewOpenRouter));
    assert.is_true(_default_cache_sends_nothing(UnroutableOllama));
    assert.is_true(_default_cache_sends_nothing(OldResponses));
    assert.is_true(_default_cache_sends_nothing(OldChat));
    assert.is_true(_default_cache_sends_nothing(PreviewBedrock));
    assert.is_true(_default_cache_sends_nothing(PreviewAzure));
    // ...and where it has one, the bodies differ
    assert.is_true(_default_cache_sends_nothing(PreviewAnthropic) == false);
    assert.is_true(_default_cache_sends_nothing(DefaultBedrockClaude) == false);
    assert.is_true(_default_cache_sends_nothing(DefaultResponses) == false);
    assert.is_true(_default_cache_sends_nothing(DefaultChat) == false)
}

function _null_is_default(target: ai.Client) -> bool {
    CacheNull@build_request("Ada", client = target).body == _default_cache_body(target)
}

test "cache(null) is cache()" {
    assert.is_true(_null_is_default(PreviewAnthropic));
    assert.is_true(_null_is_default(DefaultBedrockClaude));
    assert.is_true(_null_is_default(DefaultResponses));
    assert.is_true(_null_is_default(DefaultChat));
    assert.is_true(_null_is_default(OldResponses));
    assert.is_true(_null_is_default(PreviewGemini))
}

// Explicit args are never gated: they are sent as written to a model the
// default would have skipped.
test "cache(args) is sent where the default is withheld" {
    let picture = image.from_base64("image-data", "image/png");
    assert.is_true(_cache_delimiter_is_dropped_by(OldResponses) == false);
    assert.is_true(_cache_delimiter_is_dropped_by(OldChat) == false);
    assert.is_true(_cache_delimiter_is_dropped_by(PreviewAzure) == false);
    assert.contains(
        CacheMarked@build_request("Ada", picture, client = OldResponses).body,
        `"prompt_cache_breakpoint":{"type":"default"}`,
    )
}

test "default_cache_args per client" {
    let vertex_claude: ai.Client = google.VertexClient.new(
        model = "claude-sonnet-4-5",
        project_id = "p",
        location = "us-east5",
    );
    let vertex_gemini: ai.Client = google.VertexClient.new(
        model = "gemini-2.5-flash",
        project_id = "p",
        location = "us-east5",
    );
    let clients: ai.Client[] = [
        PreviewAnthropic,
        DefaultBedrockClaude,
        PreviewBedrock,
        DefaultResponses,
        OldResponses,
        DefaultChat,
        OldChat,
        PreviewAzure,
        vertex_claude,
        vertex_gemini,
        PreviewGemini,
        PreviewGeneric,
        PreviewOpenRouter,
        UnroutableOllama,
    ];
    let got: string[] = [];
    for (let c in clients) {
        got.push(baml.json.stringify(c.default_cache_args()));
    }
    assert.equal(
        got.join(" "),
        `{"type":"ephemeral"} {"type":"default"} null {"mode":"explicit"} null {"mode":"explicit"} null null {"type":"ephemeral"} null null null null null`,
    )
}

// A client that does not say otherwise has no default, so the interface
// method costs an existing implementer nothing.
class NoCacheClient {
    implements ai.Client {
        function id(self) -> string {
            "no-cache"
        }

        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            PreviewGeneric.render(input)
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            PreviewGeneric.invoke(input)
        }
    }
}

test "a client that does not override default_cache_args has none" {
    let own: ai.Client = NoCacheClient {};
    assert.equal(own.default_cache_args(), null);
    assert.is_true(_default_cache_sends_nothing(own))
}

// Reliability wrappers hand the same input to each member, and the member
// that sends the request resolves the default. So one `${cache()}` is a
// `cachePoint` on a Bedrock member and a `cache_control` on an Anthropic one.
// (`Fallback.render` previews only its first member, so the members are
// rendered directly.)
test "each fallback member resolves its own default" {
    let fallback = ai.clients.Fallback { members: [DefaultBedrockClaude, PreviewAnthropic] };
    let whole: ai.Client = fallback;
    assert.equal(whole.default_cache_args(), null);
    let bodies: string[] = [];
    for (let member in fallback.members) {
        bodies.push(_default_cache_body(member));
    }
    assert.contains(bodies[0], `{"cachePoint":{"type":"default"}}`);
    assert.is_true(!bodies[0].includes("cache_control"));
    assert.contains(bodies[1], `"cache_control":{"type":"ephemeral"}`);
    assert.is_true(!bodies[1].includes("cachePoint"))
}

// `ai.wire.prompt_parts` is where a null `args` takes the default: the caller
// sees the resolved delimiter, or none at all when there is no default.
function _resolved_parts(default_cache_args: json) -> string {
    let message = CacheDefault@spec("Ada").prompt().messages()[1];
    let out: string[] = [];
    for (let part in ai.wire.prompt_parts(message, default_cache_args = default_cache_args)) {
        match (part) {
            let text: string => out.push(text),
            let delimiter: ai.CacheDelimiter => out.push(`<${baml.json.stringify(delimiter.args)}>`),
            _ => out.push("media"),
        };
    }
    out.join("|")
}

test "prompt_parts gives a null-args delimiter the default" {
    assert.equal(_resolved_parts({ "type": "x" }), `Long stable document.|<{"type":"x"}>|Greet Ada.`);
    assert.equal(_resolved_parts(null), "Long stable document. Greet Ada.");
    // the message itself still carries the unresolved delimiter
    let raw = CacheDefault@spec("Ada").prompt().messages()[1].parts[1];
    assert.is_true(raw is ai.CacheDelimiter)
}
