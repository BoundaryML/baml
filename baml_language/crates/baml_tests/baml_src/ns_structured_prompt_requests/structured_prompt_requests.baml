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
