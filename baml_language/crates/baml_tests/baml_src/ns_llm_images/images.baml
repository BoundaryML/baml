// Image-generation clients: openai.ImageClient (POST /images/generations) and
// vercel.AiGatewayImageClient (POST /image-model), driven against the shared
// mock servers in ../ns_llm_mock/.
//
// These are the only two clients whose turns carry media instead of text, so
// this namespace is also the end-to-end proof of the runner's media path
// (ai/runner.baml): an `-> image` LLM function against a canned base64 payload
// must hand user code a real image value whose bytes round-trip.
//
// Corpus rules that shape every case below (../ns_llm_mock/README.md §4):
//   * each case is a top-level `function`; the `test` block only asserts
//     (test-block locals are boxed by the VM);
//   * cross-namespace names are `root.`-absolute;
//   * the mock's port is only known at runtime, so the client is constructed
//     inside the case and handed to `ai.Agent.new(client = …)` — which is
//     exactly what a direct `Fn(args)` call desugars to.
//
// The two endpoints differ on media INPUT: /images/generations is text-only
// (editing lives behind the multipart /images/edits), while the gateway's
// image-model protocol takes image inputs as `files[]` — so
// `openai_images_media_input_rejected` and `gateway_images_image_input_files`
// pin opposite behaviors on purpose.
//
// There is deliberately NO SSE test here: neither endpoint streams. The
// OpenAI Images API's `stream`/`partial_images` mode emits partially rendered
// previews, which `ai.stream.Event` (text deltas + turn metadata) cannot
// carry, and the gateway answers once with finished images. Both clients
// therefore omit `ai.stream.StreamingClient`, and
// `openai_images_streaming_unsupported` / `gateway_images_streaming_unsupported`
// pin that as the observable behavior.

// ─── functions under test ────────────────────────────────────────────────────

function ImagesDraw(subject: string) -> image {
    client: "openai-images/gpt-image-1"
    prompt: `
        A studio photograph of ${subject}.
    `
}

function ImagesDrawSet(subject: string) -> image[] {
    client: "openai-images/gpt-image-1"
    prompt: `
        Two studio photographs of ${subject}.
    `
}

function ImagesDrawOrSay(subject: string) -> string | image {
    client: "openai-images/gpt-image-1"
    prompt: `
        A studio photograph of ${subject}, or a sentence explaining why not.
    `
}

function ImagesRedraw(source: image, subject: string) -> image {
    client: "openai-images/gpt-image-1"
    prompt: `
        Redraw ${source} as ${subject}.
    `
}

function GatewayRedraw(source: image, subject: string) -> image {
    client: "ai-gateway-images/openai/gpt-image-1"
    prompt: `
        Redraw ${source} as ${subject}.
    `
}

function GatewayRedrawAudio(clip: audio) -> image {
    client: "ai-gateway-images/openai/gpt-image-1"
    prompt: `
        Draw what you hear in ${clip}.
    `
}

function GatewayDraw(subject: string) -> image {
    client: "ai-gateway-images/bfl/flux-2-pro"
    prompt: `
        A studio photograph of ${subject}.
    `
}

function GatewayDrawSet(subject: string) -> image[] {
    client: "ai-gateway-images/bfl/flux-2-pro"
    prompt: `
        Two studio photographs of ${subject}.
    `
}

// ─── canned provider payloads ────────────────────────────────────────────────

// One gpt-image-1 answer: base64 "hello", png, with usage. `revised_prompt` is
// a dall-e-3 field but is harmless (and ignored) here.
function images_one_png() -> string {
    let body = `{"created":1713833628,"data":[{"b64_json":"aGVsbG8="}],"background":"opaque","output_format":"png","size":"1024x1024","quality":"low","usage":{"input_tokens":11,"output_tokens":22,"total_tokens":33,"input_tokens_details":{"text_tokens":11,"image_tokens":0}}}`;
    body
}

function images_two_png() -> string {
    let body = `{"created":1713833628,"data":[{"b64_json":"aGVsbG8="},{"b64_json":"d29ybGQ="}],"output_format":"png"}`;
    body
}

// ─── openai.ImageClient: request shape ───────────────────────────────────────

// The typed request on the wire, plus the media value the runner produced from
// the canned answer. `response_format` must be ABSENT for a GPT image model
// (the API rejects it there), and unset options must not appear as nulls.
function openai_images_request_shape() -> string {
    let canned = images_one_png();
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(canned)],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.ImageClient.new(
                model = "gpt-image-1",
                api_key = "test-key",
                base_url = mock.base_url(),
                n = 1,
                size = "1024x1024",
                quality = "low",
                output_format = "png",
                background = "opaque",
            );
            let result = ai.Agent.new(client = cl).run(ImagesDraw@spec("a brass desk lamp"));
            let sent = mock.last_request();
            let body = sent.json();
            let text = baml.json.path<string>(body, ".prompt");
            `${sent.method} ${sent.path()}`
                + `|auth=${sent.header("authorization") ?? "?"}`
                + `|ct=${sent.header("content-type") ?? "?"}`
                + `|model=${baml.json.path<string>(body, ".model")}`
                + `|n=${baml.json.path_or<int>(body, ".n", 0)}`
                + `|size=${baml.json.path<string>(body, ".size")}`
                + `|quality=${baml.json.path<string>(body, ".quality")}`
                + `|output_format=${baml.json.path<string>(body, ".output_format")}`
                + `|background=${baml.json.path<string>(body, ".background")}`
                + `|response_format=${baml.json.path_or<string>(body, ".response_format", "absent")}`
                + `|style=${baml.json.path_or<string>(body, ".style", "absent")}`
                + `|user=${baml.json.path_or<string>(body, ".user", "absent")}`
                + `|moderation=${baml.json.path_or<string>(body, ".moderation", "absent")}`
                + `|prompt_has_subject=${text.includes("a brass desk lamp")}`
                + `|prompt_has_schema=${text.includes("Return an image output")}`
                + `|b64=${result.value.base64()}`
                + `|mime=${result.value.mime_type() ?? "?"}`
                + `|usage=${result.usage.input_tokens}/${result.usage.output_tokens}`
        },
    )
}

test "openai_images_request_shape" {
    assert.equal(
        openai_images_request_shape(),
        "POST /images/generations|auth=Bearer test-key|ct=application/json|model=gpt-image-1|n=1"
            + "|size=1024x1024|quality=low|output_format=png|background=opaque"
            + "|response_format=absent|style=absent|user=absent|moderation=absent"
            + "|prompt_has_subject=true|prompt_has_schema=false"
            + "|b64=aGVsbG8=|mime=image/png|usage=11/22",
    )
}

// dall-e models return URLs by default, which a media output type cannot use,
// so the client asks for base64 explicitly. An explicit setting still wins.
function openai_images_dalle_response_format() -> string {
    let canned = `{"created":1,"data":[{"b64_json":"aGVsbG8=","revised_prompt":"a nicer prompt"}]}`;
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(canned)],
        (mock: root.llm_mock.MockJsonServer) -> {
            let defaulted = openai.ImageClient.new(
                model = "dall-e-3",
                api_key = "test-key",
                base_url = mock.base_url(),
                style = "vivid",
            );
            let first = ai.Agent.new(client = defaulted).run(ImagesDraw@spec("a lamp"));
            let defaulted_body = mock.last_request().json();
            let overridden = openai.ImageClient.new(
                model = "dall-e-3",
                api_key = "test-key",
                base_url = mock.base_url(),
                response_format = "url",
            );
            let second = ai.Agent.new(client = overridden).run(ImagesDraw@spec("a lamp"));
            let overridden_body = mock.last_request().json();
            `${baml.json.path<string>(defaulted_body, ".response_format")}`
                + `|${baml.json.path<string>(defaulted_body, ".style")}`
                + `|${baml.json.path_or<string>(defaulted_body, ".output_format", "absent")}`
                + `|${baml.json.path<string>(overridden_body, ".response_format")}`
                + `|${first.value.mime_type() ?? "?"}`
                + `|${second.value.base64()}`
        },
    )
}

test "openai_images_dalle_response_format" {
    assert.equal(
        openai_images_dalle_response_format(),
        "b64_json|vivid|absent|url|image/png|aGVsbG8=",
    )
}

// A dall-e `response_format: "url"` answer must still hand user code real
// bytes: the link expires in 60 minutes, and an image whose `base64()` is empty
// is silent data loss. The client fetches it once and inlines it, taking the
// MIME type from the fetch's own `content-type`.
//
// Two nested mocks: the OUTER one serves the image bytes (its base_url is only
// known at runtime, so the canned API answer that points at it has to be built
// inside its closure), the INNER one is the Images API.
function openai_images_url_mode_inlines() -> string {
    root.llm_mock.mock_json_serve(
        [
            root.llm_mock.MockResponse.with_headers(
                200,
                "hello",
                { "content-type": "image/jpeg" },
            ),
        ],
        (assets: root.llm_mock.MockJsonServer) -> {
            let canned = `{"created":1,"data":[{"url":"${assets.base_url()}/img-1.png"}]}`;
            root.llm_mock.mock_json_serve(
                [root.llm_mock.MockResponse.ok(canned)],
                (mock: root.llm_mock.MockJsonServer) -> {
                    let cl = openai.ImageClient.new(
                        model = "dall-e-3",
                        api_key = "test-key",
                        base_url = mock.base_url(),
                        response_format = "url",
                    );
                    let picture = ai.Agent.new(client = cl)
                        .run(ImagesDraw@spec("a lamp"))
                        .value;
                    `${picture.base64()}`
                        + `|${picture.mime_type() ?? "?"}`
                        + `|fetches=${assets.request_count()}`
                },
            )
        },
    )
}

test "openai_images_url_mode_inlines" {
    assert.equal(openai_images_url_mode_inlines(), "aGVsbG8=|image/jpeg|fetches=1")
}

// …and a link that does not resolve is a typed, retry-safe transport failure,
// not an empty image.
function openai_images_url_fetch_failure() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.error(404, `{"error":"expired"}`)],
        (assets: root.llm_mock.MockJsonServer) -> {
            let canned = `{"created":1,"data":[{"url":"${assets.base_url()}/img-1.png"}]}`;
            root.llm_mock.mock_json_serve(
                [root.llm_mock.MockResponse.ok(canned)],
                (mock: root.llm_mock.MockJsonServer) -> {
                    let cl = openai.ImageClient.new(
                        model = "dall-e-3",
                        api_key = "test-key",
                        base_url = mock.base_url(),
                        response_format = "url",
                    );
                    ai.Agent.new(client = cl)
                        .run(ImagesDraw@spec("a lamp"))
                        .value
                        .base64() catch_all (e) {
                            let down: ai.errors.NetworkFailure => {
                                `NetworkFailure ${down.provider}`
                                    + `|${down.detail.ends_with("returned HTTP 404")}`
                                    + `|${down.retry_safety()}`
                            },
                            _ => `unexpected error class: ${e.to_string()}`,
                        }
                },
            )
        },
    )
}

test "openai_images_url_fetch_failure" {
    assert.equal(
        openai_images_url_fetch_failure(),
        "NetworkFailure openai-images|true|Safe",
    )
}

// `request_body` is merged last: it adds keys, overrides first-class params,
// and a null value deletes one the client injected.
function openai_images_passthrough() -> string {
    let canned = images_one_png();
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(canned)],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.ImageClient.new(
                model = "gpt-image-1",
                api_key = "test-key",
                base_url = mock.base_url(),
                n = 2,
                quality = "low",
                request_body = baml.json.parse(`{"n":3,"quality":null,"partial_images":2}`),
                headers = { "X-Trace-Id": "abc", "Content-Type": "application/json" },
                query_params = { "beta": "images v2" },
            );
            let _ = ai.Agent.new(client = cl).run(ImagesDraw@spec("a lamp"));
            let sent = mock.last_request();
            let body = sent.json();
            `n=${baml.json.path_or<int>(body, ".n", 0)}`
                + `|quality=${baml.json.path_or<string>(body, ".quality", "absent")}`
                + `|partial_images=${baml.json.path_or<int>(body, ".partial_images", 0)}`
                + `|trace=${sent.header("x-trace-id") ?? "?"}`
                + `|auth=${sent.header("authorization") ?? "?"}`
                + `|query=${sent.query() ?? "?"}`
        },
    )
}

test "openai_images_passthrough" {
    assert.equal(
        openai_images_passthrough(),
        "n=3|quality=absent|partial_images=2|trace=abc|auth=Bearer test-key|query=beta=images%20v2",
    )
}

// ─── openai.ImageClient: the runner's media output shapes ────────────────────

// `-> image[]`: every data[] entry becomes one media block, in order.
function openai_images_list_output() -> string {
    let canned = images_two_png();
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(canned)],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.ImageClient.new(
                model = "gpt-image-1",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            let pictures = ai.Agent.new(client = cl)
                .run(ImagesDrawSet@spec("a brass desk lamp"))
                .value;
            let encoded: string[] = [];
            for (let picture in pictures) {
                encoded.push(picture.base64());
            }
            `${pictures.length()}|${encoded.join(",")}`
        },
    )
}

test "openai_images_list_output" {
    assert.equal(openai_images_list_output(), "2|aGVsbG8=,d29ybGQ=")
}

// `-> string | image`: a single image answer narrows to the image member.
function openai_images_union_output() -> string {
    let canned = images_one_png();
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(canned)],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.ImageClient.new(
                model = "gpt-image-1",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            let answer = ai.Agent.new(client = cl)
                .run(ImagesDrawOrSay@spec("a brass desk lamp"))
                .value;
            // The image member narrows the ordinary way — `match` on the
            // primitive media type. (Kind-precise narrowing of the media
            // primitives is covered exhaustively in `ns_media_union_narrowing`.)
            match (answer) {
                let text: string => `text:${text}`,
                let picture: image => `image:${picture.base64()}|${picture.mime_type() ?? "?"}`,
            }
        },
    )
}

test "openai_images_union_output" {
    assert.equal(openai_images_union_output(), "image:aGVsbG8=|image/png")
}

// `-> string | image` with two images is the ported sys_llm mixed-shape error,
// which renders the DECLARED output type (sys_llm's `{target}`) rather than a
// hardcoded spelling of it.
function openai_images_mixed_shape_error() -> string {
    let canned = images_two_png();
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(canned)],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.ImageClient.new(
                model = "gpt-image-1",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            ai.Agent.new(client = cl)
                .run(ImagesDrawOrSay@spec("a lamp"))
                .value
                .to_string() catch_all (e) {
                    let failed: ai.errors.ParseFailed => failed.raw_output,
                    _ => `unexpected error class: ${e.to_string()}`,
                }
        },
    )
}

test "openai_images_mixed_shape_error" {
    assert.equal(
        openai_images_mixed_shape_error(),
        "Expected one text or image output for string | image, got 2 parts."
            + " Use (image | string)[] to preserve mixed outputs.",
    )
}

// `-> image` with two images is the ported sys_llm error, verbatim.
function openai_images_exactly_one_error() -> string {
    let canned = images_two_png();
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(canned)],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.ImageClient.new(
                model = "gpt-image-1",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            ai.Agent.new(client = cl).run(ImagesDraw@spec("a lamp")).value.base64() catch_all (e) {
                let failed: ai.errors.ParseFailed => failed.raw_output,
                _ => `unexpected error class: ${e.to_string()}`,
            }
        },
    )
}

test "openai_images_exactly_one_error" {
    assert.equal(
        openai_images_exactly_one_error(),
        "Expected exactly one image output, got 2. Use image[] for multiple outputs.",
    )
}

// An empty data[] is a parse failure, not an empty turn.
function openai_images_no_data_error() -> string {
    let canned = `{"created":1,"data":[]}`;
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(canned)],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.ImageClient.new(
                model = "gpt-image-1",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            ai.Agent.new(client = cl).run(ImagesDraw@spec("a lamp")).value.base64() catch_all (e) {
                let failed: ai.errors.ParseFailed => failed.raw_output,
                _ => `unexpected error class: ${e.to_string()}`,
            }
        },
    )
}

test "openai_images_no_data_error" {
    assert.equal(
        openai_images_no_data_error(),
        "the images response contained no image data in data[]",
    )
}

// ─── the runner's media path when the turn carries NO media ──────────────────
//
// A media-shaped `Out` whose turn has no media block does NOT fail: it falls
// through to the ordinary text parse, which is where absence is expressed.
// `image?` becomes null and `image[]` becomes `[]` — sys_llm's `Ok(None)` ->
// SAP path (`lib.rs:594-645`), and the documented contract for those shapes
// ("0 or 1", "all image parts"). Only a required `image` genuinely fails.
//
// The client here is an ordinary chat client so the turn is text and nothing
// else: the image endpoints never produce a media-less success (an empty
// `data[]` is `openai_images_no_data_error` above), so they cannot exercise
// this branch at all. `ai.Agent.new(client = …)` overrides the spec's
// default client, which is what makes one spec usable with either.

function ImagesMaybeDraw(subject: string) -> image? {
    client: "openai-chat/gpt-4o-mini"
    prompt: `
        A studio photograph of ${subject}, if you can make one.
    `
}

// A plain chat-completions answer with no media anywhere in it.
function _images_text_answer() -> string {
    let body = `{"id":"chatcmpl-1","object":"chat.completion","created":1,"model":"gpt-4o-mini","choices":[{"index":0,"message":{"role":"assistant","content":"I cannot draw that.","refusal":null},"finish_reason":"stop"}],"usage":{"prompt_tokens":9,"completion_tokens":5,"total_tokens":14}}`;
    body
}

// `-> image?`: no media block is `null`, not a re-ask and a ParseFailed.
function images_optional_accepts_absence() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_images_text_answer())],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.ChatClient.new(api_key = "test-key", base_url = mock.base_url());
            let answer = ai.Agent.new(client = cl)
                .run(ImagesMaybeDraw@spec("a brass desk lamp"))
                .value;
            match (answer) {
                null => "null",
                let picture: image => `image:${picture.base64()}`,
            } catch_all (e) {
                _ => `unexpected: ${e.to_string()}`,
            }
        },
    )
}

test "images_optional_accepts_absence" {
    assert.equal(images_optional_accepts_absence(), "null")
}

// `-> image[]`: no media block is the empty list.
function images_list_accepts_absence() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_images_text_answer())],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.ChatClient.new(api_key = "test-key", base_url = mock.base_url());
            let pictures = ai.Agent.new(client = cl)
                .run(ImagesDrawSet@spec("a brass desk lamp"))
                .value;
            `count=${pictures.length()}` catch_all (e) {
                _ => `unexpected: ${e.to_string()}`,
            }
        },
    )
}

test "images_list_accepts_absence" {
    assert.equal(images_list_accepts_absence(), "count=0")
}

// The guard on the other side: a REQUIRED `image` is not satisfied by absence.
// SAP rejects the media type outright, so the runner re-asks and then reports
// ParseFailed with the text it was given.
function images_required_rejects_absence() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_images_text_answer())],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.ChatClient.new(api_key = "test-key", base_url = mock.base_url());
            ai.Agent.new(client = cl).run(ImagesDraw@spec("a lamp")).value.base64() catch_all (e) {
                let failed: ai.errors.ParseFailed => `ParseFailed|${failed.raw_output}`,
                _ => `unexpected error class: ${e.to_string()}`,
            }
        },
    )
}

test "images_required_rejects_absence" {
    assert.equal(images_required_rejects_absence(), "ParseFailed|I cannot draw that.")
}

// Media INPUT is rejected before anything is sent: /images/generations takes
// text only (editing is a different, multipart endpoint).
function openai_images_media_input_rejected() -> string {
    let canned = images_one_png();
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(canned)],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.ImageClient.new(
                model = "gpt-image-1",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            let source = baml.media.Image.from_base64("aGVsbG8=", "image/png");
            let detail = ai.Agent.new(client = cl)
                .run(ImagesRedraw@spec(source, "a lamp"))
                .value
                .base64() catch_all (e) {
                    let invalid: ai.errors.InvalidRequest => invalid.detail,
                    _ => `unexpected error class: ${e.to_string()}`,
                };
            `${detail}|requests=${mock.request_count()}`
        },
    )
}

test "openai_images_media_input_rejected" {
    assert.equal(
        openai_images_media_input_rejected(),
        "Images API only supports text prompts; found image input|requests=0",
    )
}

// A 401 must classify as an invalid request, never as a retry-safe transport
// failure (replaying a 401 is pure waste; replaying a 400 is worse).
function openai_images_http_error_taxonomy() -> string {
    let canned = `{"error":{"message":"Incorrect API key provided","type":"invalid_request_error"}}`;
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.error(401, canned)],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.ImageClient.new(
                model = "gpt-image-1",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            ai.Agent.new(client = cl).run(ImagesDraw@spec("a lamp")).value.base64() catch_all (e) {
                let invalid: ai.errors.InvalidRequest => {
                    `InvalidRequest ${invalid.status_code ?? 0} ${invalid.provider}`
                },
                _ => "wrong error class",
            }
        },
    )
}

test "openai_images_http_error_taxonomy" {
    assert.equal(
        openai_images_http_error_taxonomy(),
        "InvalidRequest 401 openai-images",
    )
}

// ─── vercel.AiGatewayImageClient ─────────────────────────────────────────────

// The gateway protocol lives in headers: the model id is `ai-model-id`, not a
// body field, and the two version headers are exact. `n` defaults to 1.
function gateway_images_request_shape() -> string {
    let canned = `{"images":["aGVsbG8="],"providerMetadata":{"gateway":{"routing":{"provider":"black-forest-labs"}}}}`;
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(canned)],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = vercel.AiGatewayImageClient.new(
                model = "bfl/flux-2-pro",
                api_key = "test-key",
                // trailing slash trimmed, "/image-model" appended
                base_url = `${mock.base_url()}/v4/ai/`,
            );
            let picture = ai.Agent.new(client = cl)
                .run(GatewayDraw@spec("a brass desk lamp"))
                .value;
            let sent = mock.last_request();
            let body = sent.json();
            `${sent.method} ${sent.path()}`
                + `|protocol=${sent.header("ai-gateway-protocol-version") ?? "?"}`
                + `|spec=${sent.header("ai-image-model-specification-version") ?? "?"}`
                + `|model=${sent.header("ai-model-id") ?? "?"}`
                + `|auth_method=${sent.header("ai-gateway-auth-method") ?? "?"}`
                + `|auth=${sent.header("authorization") ?? "?"}`
                + `|ct=${sent.header("content-type") ?? "?"}`
                + `|body_model=${baml.json.path_or<string>(body, ".model", "absent")}`
                + `|n=${baml.json.path_or<int>(body, ".n", 0)}`
                + `|prompt_has_subject=${baml.json.path<string>(body, ".prompt").includes("a brass desk lamp")}`
                + `|b64=${picture.base64()}`
                + `|mime=${picture.mime_type() ?? "?"}`
        },
    )
}

test "gateway_images_request_shape" {
    assert.equal(
        gateway_images_request_shape(),
        "POST /v4/ai/image-model|protocol=0.0.1|spec=4|model=bfl/flux-2-pro"
            + "|auth_method=api-key|auth=Bearer test-key|ct=application/json"
            + "|body_model=absent|n=1|prompt_has_subject=true"
            + "|b64=aGVsbG8=|mime=image/png",
    )
}

// `providerOptions` rides through `request_body`, and its `outputFormat` is
// what the returned bytes actually are — sys_llm hardcoded image/png here.
function gateway_images_output_format_mime() -> string {
    let canned = `{"images":["aGVsbG8=","d29ybGQ="],"providerMetadata":{"gateway":{"routing":{"provider":"black-forest-labs"}}}}`;
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(canned)],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = vercel.AiGatewayImageClient.new(
                model = "bfl/flux-2-pro",
                api_key = "test-key",
                base_url = mock.base_url(),
                n = 2,
                size = "1024x1024",
                seed = 7,
                request_body = baml.json.parse(
                    `{"providerOptions":{"blackForestLabs":{"outputFormat":"jpeg"}}}`,
                ),
            );
            let pictures = ai.Agent.new(client = cl)
                .run(GatewayDrawSet@spec("a brass desk lamp"))
                .value;
            let body = mock.last_request().json();
            let mimes: string[] = [];
            for (let picture in pictures) {
                mimes.push(picture.mime_type() ?? "?");
            }
            `n=${baml.json.path_or<int>(body, ".n", 0)}`
                + `|size=${baml.json.path<string>(body, ".size")}`
                + `|seed=${baml.json.path_or<int>(body, ".seed", 0)}`
                + `|aspectRatio=${baml.json.path_or<string>(body, ".aspectRatio", "absent")}`
                + `|outputFormat=${baml.json.path<string>(body, ".providerOptions.blackForestLabs.outputFormat")}`
                + `|mimes=${mimes.join(",")}`
        },
    )
}

test "gateway_images_output_format_mime" {
    assert.equal(
        gateway_images_output_format_mime(),
        "n=2|size=1024x1024|seed=7|aspectRatio=absent|outputFormat=jpeg|mimes=image/jpeg,image/jpeg",
    )
}

// A `data:` payload carries its own MIME type and wins over every other source.
function gateway_images_data_url_payload() -> string {
    let canned = `{"images":["data:image/webp;base64,aGVsbG8="]}`;
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(canned)],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = vercel.AiGatewayImageClient.new(
                model = "bfl/flux-2-pro",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            let picture = ai.Agent.new(client = cl)
                .run(GatewayDraw@spec("a lamp"))
                .value;
            `${picture.mime_type() ?? "?"}|${picture.base64()}`
        },
    )
}

test "gateway_images_data_url_payload" {
    assert.equal(gateway_images_data_url_payload(), "image/webp|aGVsbG8=")
}

// A `data:` payload that is NOT base64 is a parse failure: storing the URL text
// as the image's payload would hand user code bytes that are not the image.
function gateway_images_data_url_not_base64() -> string {
    let canned = `{"images":["data:image/png,rawpixels"]}`;
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(canned)],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = vercel.AiGatewayImageClient.new(
                model = "bfl/flux-2-pro",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            ai.Agent.new(client = cl).run(GatewayDraw@spec("a lamp")).value.base64() catch_all (e) {
                let failed: ai.errors.ParseFailed => failed.raw_output,
                _ => `unexpected error class: ${e.to_string()}`,
            }
        },
    )
}

test "gateway_images_data_url_not_base64" {
    assert.equal(
        gateway_images_data_url_not_base64(),
        "images[] carried a data: URL with no ;base64, marker: data:image/png,rawpixels",
    )
}

// The v4 protocol's `usage` object is surfaced as the turn's usage (sys_llm
// dropped it), and an unfamiliar `warnings[]` variant must not fail the decode.
function gateway_images_usage() -> string {
    let canned = `{"images":["aGVsbG8="],"warnings":[{"type":"unsupported","feature":"seed","details":"the provider ignores seed"},{"type":"martian","hue":7}],"usage":{"inputTokens":12,"outputTokens":34,"totalTokens":46}}`;
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(canned)],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = vercel.AiGatewayImageClient.new(
                model = "bfl/flux-2-pro",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            let result = ai.Agent.new(client = cl).run(GatewayDraw@spec("a lamp"));
            `${result.value.base64()}`
                + `|${result.usage.input_tokens}/${result.usage.output_tokens}`
        },
    )
}

test "gateway_images_usage" {
    assert.equal(gateway_images_usage(), "aGVsbG8=|12/34")
}

// Warnings are the only record of an option the underlying provider ignored, so
// each one is logged. The log line itself is pinned here (the emit is a
// `log.warn`, which a test cannot observe); every variant of the protocol's
// warning union names either a feature/setting or a free-text message.
function gateway_images_warning_lines() -> string {
    let rendered: string[] = [
        vercel.internal.gateway_images_warning_text(
            vercel.internal.GwImagesWarning {
                type: "unsupported",
                feature: "seed",
                details: "the provider ignores seed",
                setting: null,
                message: null,
            },
        ),
        vercel.internal.gateway_images_warning_text(
            vercel.internal.GwImagesWarning {
                type: "deprecated",
                feature: null,
                details: null,
                setting: "aspectRatio",
                message: "use size instead",
            },
        ),
        vercel.internal.gateway_images_warning_text(
            vercel.internal.GwImagesWarning {
                type: null,
                feature: null,
                details: null,
                setting: null,
                message: null,
            },
        ),
    ];
    rendered.join("\n")
}

test "gateway_images_warning_lines" {
    assert.equal(
        gateway_images_warning_lines(),
        "ai-gateway-images: unsupported warning for seed: the provider ignores seed\n"
            + "ai-gateway-images: deprecated warning for aspectRatio: use size instead\n"
            + "ai-gateway-images: other warning",
    )
}

// Image INPUTS are supported here (unlike /images/generations): an image prompt
// part becomes one `files[]` entry — `{type:"file", mediaType, data}` for bytes,
// `{type:"url", url}` for a link, which is NOT fetched. That is how the endpoint
// does editing and variations (image-model protocol v4).
function gateway_images_image_input_files() -> string {
    let canned = `{"images":["d29ybGQ="]}`;
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(canned)],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = vercel.AiGatewayImageClient.new(
                model = "openai/gpt-image-1",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            let inline_source = baml.media.Image.from_base64("aGVsbG8=", "image/jpeg");
            let edited = ai.Agent.new(client = cl)
                .run(GatewayRedraw@spec(inline_source, "a lamp"))
                .value;
            let inline_body = mock.last_request().json();
            let remote_source = baml.media.Image.from_url(
                "https://example.invalid/cat.png",
                "image/png",
            );
            let _ = ai.Agent.new(client = cl)
                .run(GatewayRedraw@spec(remote_source, "a lamp"))
                .value;
            let remote_body = mock.last_request().json();
            `type=${baml.json.path<string>(inline_body, ".files[0].type")}`
                + `|mediaType=${baml.json.path<string>(inline_body, ".files[0].mediaType")}`
                + `|data=${baml.json.path<string>(inline_body, ".files[0].data")}`
                + `|second_file=${baml.json.path_or<string>(inline_body, ".files[1].type", "absent")}`
                + `|prompt_has_subject=${baml.json.path<string>(inline_body, ".prompt").includes("a lamp")}`
                + `|remote_type=${baml.json.path<string>(remote_body, ".files[0].type")}`
                + `|remote_url=${baml.json.path<string>(remote_body, ".files[0].url")}`
                + `|remote_data=${baml.json.path_or<string>(remote_body, ".files[0].data", "absent")}`
                + `|mask=${baml.json.path_or<string>(remote_body, ".mask", "absent")}`
                + `|out=${edited.base64()}`
        },
    )
}

test "gateway_images_image_input_files" {
    assert.equal(
        gateway_images_image_input_files(),
        "type=file|mediaType=image/jpeg|data=aGVsbG8=|second_file=absent|prompt_has_subject=true"
            + "|remote_type=url|remote_url=https://example.invalid/cat.png"
            + "|remote_data=absent|mask=absent|out=d29ybGQ=",
    )
}

// Audio/video/pdf inputs still have no representation in the protocol.
function gateway_images_audio_input_rejected() -> string {
    let cl = vercel.AiGatewayImageClient.new(model = "bfl/flux-2-pro", api_key = "test-key");
    let clip = baml.media.Audio.from_base64("aGVsbG8=", "audio/mpeg");
    vercel.internal.gateway_images_render(
        cl,
        _images_turn_input(GatewayRedrawAudio@spec(clip)),
    ).url catch_all (e) {
        let invalid: ai.errors.InvalidRequest => invalid.detail,
        _ => `unexpected error class: ${e.to_string()}`,
    }
}

test "gateway_images_audio_input_rejected" {
    assert.equal(
        gateway_images_audio_input_rejected(),
        "Images API only supports text and image prompts; found audio input",
    )
}

// The ported sys_llm NoContent condition.
function gateway_images_no_images_error() -> string {
    let canned = `{"images":[],"providerMetadata":{}}`;
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(canned)],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = vercel.AiGatewayImageClient.new(
                model = "bfl/flux-2-pro",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            ai.Agent.new(client = cl).run(GatewayDraw@spec("a lamp")).value.base64() catch_all (e) {
                let failed: ai.errors.ParseFailed => failed.raw_output,
                _ => `unexpected error class: ${e.to_string()}`,
            }
        },
    )
}

test "gateway_images_no_images_error" {
    assert.equal(
        gateway_images_no_images_error(),
        "response contained no images in images[]",
    )
}

// A 429 from the gateway is rate limiting, not a transport failure.
function gateway_images_http_error_taxonomy() -> string {
    let canned = `{"error":{"message":"rate limited"}}`;
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.error(429, canned)],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = vercel.AiGatewayImageClient.new(
                model = "bfl/flux-2-pro",
                api_key = "test-key",
                base_url = mock.base_url(),
            );
            ai.Agent.new(client = cl).run(GatewayDraw@spec("a lamp")).value.base64() catch_all (e) {
                let limited: ai.errors.RateLimited => `RateLimited ${limited.provider}`,
                _ => "wrong error class",
            }
        },
    )
}

test "gateway_images_http_error_taxonomy" {
    assert.equal(
        gateway_images_http_error_taxonomy(),
        "RateLimited ai-gateway-images",
    )
}

// ─── default endpoints, asserted without a server ────────────────────────────
//
// A mock forces `base_url`, so the DEFAULT endpoints need a serverless check:
// the client's render function is called directly on a spec-built input. These
// two are the ports of sys_llm's `builds_gateway_image_model_request` /
// `defaults_gateway_image_count_to_one` unit tests.

function _images_turn_input<Out>(spec: ai.FunctionSpec<Out>) -> ai.ModelTurnInput {
    let input = ai.ModelTurnInput {
        prompt: spec.prompt_template,
        journal: ai.Journal.new(spec),
        toolbox: spec.tools(),
        output_type: spec.output_type(),
    };
    input
}

function openai_images_default_endpoint() -> string {
    let cl = openai.ImageClient.new(model = "gpt-image-1", api_key = "test-key");
    let req = openai.internal.images_render(cl, _images_turn_input(ImagesDraw@spec("a lamp")));
    `${req.method} ${req.url}`
}

test "openai_images_default_endpoint" {
    assert.equal(
        openai_images_default_endpoint(),
        "POST https://api.openai.com/v1/images/generations",
    )
}

function gateway_images_default_endpoint() -> string {
    let cl = vercel.AiGatewayImageClient.new(
        model = "bfl/flux-2-pro",
        api_key = "test-key",
        request_body = baml.json.parse(
            `{"n":2,"providerOptions":{"blackForestLabs":{"outputFormat":"jpeg"}}}`,
        ),
    );
    let req = vercel.internal.gateway_images_render(
        cl,
        _images_turn_input(GatewayDraw@spec("a lamp")),
    );
    let body = baml.json.parse(req.body);
    `${req.method} ${req.url}`
        + `|n=${baml.json.path_or<int>(body, ".n", 0)}`
        + `|outputFormat=${baml.json.path<string>(body, ".providerOptions.blackForestLabs.outputFormat")}`
        + `|model_header=${req.headers.get("ai-model-id") ?? "?"}`
}

test "gateway_images_default_endpoint" {
    assert.equal(
        gateway_images_default_endpoint(),
        "POST https://ai-gateway.vercel.sh/v4/ai/image-model|n=2|outputFormat=jpeg"
            + "|model_header=bfl/flux-2-pro",
    )
}

// ─── streaming is unsupported, and says so ───────────────────────────────────

function _stream_openai_images() -> string {
    let final_value = ai.stream.from_spec<image | null, image>(ImagesDraw@spec("a lamp")).final();
    `streamed:${final_value}`
}

function openai_images_streaming_unsupported() -> string {
    _stream_openai_images() catch_all (e) {
        let unsupported: ai.errors.StreamingUnsupported => unsupported.client_id,
        _ => "wrong error class",
    }
}

test "openai_images_streaming_unsupported" {
    assert.equal(openai_images_streaming_unsupported(), "openai-images/gpt-image-1")
}

function _stream_gateway_images() -> string {
    let final_value = ai.stream.from_spec<image | null, image>(GatewayDraw@spec("a lamp")).final();
    `streamed:${final_value}`
}

function gateway_images_streaming_unsupported() -> string {
    _stream_gateway_images() catch_all (e) {
        let unsupported: ai.errors.StreamingUnsupported => unsupported.client_id,
        _ => "wrong error class",
    }
}

test "gateway_images_streaming_unsupported" {
    assert.equal(
        gateway_images_streaming_unsupported(),
        "ai-gateway-images/bfl/flux-2-pro",
    )
}

// ─── missing credentials ─────────────────────────────────────────────────────
//
// The most common misconfiguration must reach the caller's `catch_all` as a
// classified failure, not abort the process — user fallback logic (another
// client, a cached answer) can only run if the error is catchable. `base_url`
// points at a dead port so a wrongly-resolved key would surface as a
// NetworkFailure rather than pass silently. Registered only when the variable
// is unset, since the fallback chain ends at the real environment.
//
// `api_key = env.NAME` naming a variable that exists nowhere is the shape under
// test: the ref resolves to null at REQUEST time, the client falls through to
// its canonical variable, and the empty chain becomes a typed failure.

function openai_images_missing_key() -> string {
    let cl = openai.ImageClient.new(
        model = "gpt-image-1",
        api_key = env.BAML_TEST_NO_SUCH_OPENAI_IMAGES_KEY,
        base_url = "http://127.0.0.1:1/v1",
    );
    ai.Agent.new(client = cl).run(ImagesDraw@spec("a lamp")).value.base64() catch_all (e) {
        let invalid: ai.errors.InvalidRequest => `InvalidRequest ${invalid.provider}`,
        _ => `unexpected error class: ${e.to_string()}`,
    }
}

function gateway_images_missing_key() -> string {
    let cl = vercel.AiGatewayImageClient.new(
        model = "bfl/flux-2-pro",
        api_key = env.BAML_TEST_NO_SUCH_GATEWAY_KEY,
        base_url = "http://127.0.0.1:1/v4/ai",
    );
    ai.Agent.new(client = cl).run(GatewayDraw@spec("a lamp")).value.base64() catch_all (e) {
        let invalid: ai.errors.InvalidRequest => `InvalidRequest ${invalid.provider}`,
        _ => `unexpected error class: ${e.to_string()}`,
    }
}

testset "missing_key" {
    if (baml.env.get("OPENAI_API_KEY") == null) {
        test "openai_images" {
            assert.equal(openai_images_missing_key(), "InvalidRequest openai-images")
        }
    } else {
        log.warn("missing_key::openai_images not registered: OPENAI_API_KEY is set")
    }

    if (baml.env.get("AI_GATEWAY_API_KEY") == null) {
        test "gateway_images" {
            assert.equal(gateway_images_missing_key(), "InvalidRequest ai-gateway-images")
        }
    } else {
        log.warn("missing_key::gateway_images not registered: AI_GATEWAY_API_KEY is set")
    }
}

// ─── live ────────────────────────────────────────────────────────────────────

function LiveDrawSquare() -> image {
    client: "openai-images/gpt-image-1"
    prompt: `
        A plain solid red square centered on a white background. No text.
    `
}

// The cheapest real image this endpoint can make: gpt-image-1, one image, the
// smallest supported size, lowest quality — roughly $0.01–$0.02 per run at
// current list prices. Keep n = 1 and quality = "low" when touching this.
function live_openai_images_draws() -> string {
    let cl = openai.ImageClient.new(
        model = "gpt-image-1",
        n = 1,
        size = "1024x1024",
        quality = "low",
        output_format = "png",
    );
    let picture = ai.Agent.new(client = cl).run(LiveDrawSquare@spec()).value;
    `${picture.base64().length() > 1024}|${picture.mime_type() ?? "?"}`
}

testset "live" with testing.Sequential() {
    if (baml.env.get("OPENAI_API_KEY") != null) {
        test "openai_images" {
            assert.equal(live_openai_images_draws(), "true|image/png")
        }
    } else {
        log.warn("live::openai_images not registered: OPENAI_API_KEY is unset")
    }

    // No gateway live leaf: AI_GATEWAY_API_KEY is not in Infisical, so there is
    // no key to gate on. Add one here (same shape as above, gated on
    // AI_GATEWAY_API_KEY) once the account exists.
}
