// root.ImageClient — one call against the OpenAI Images API
// (POST {base_url}/images/generations).
//
// Net-new in the stdlib: neither sys_llm nor the old engine implemented this
// endpoint (sys_llm's `build_request/openai/images.rs` is the Vercel AI Gateway
// image model, ported in the `vercel` package). Every field below is pinned
// against the published OpenAI schema — `CreateImageRequest`, `ImagesResponse`,
// `Image`, `ImageGenUsage` in openai/openai-openapi.
//
// The wire shape is modeled as typed classes on both sides (PLAN §1.5). The
// request class holds one optional field per API parameter; null fields are
// dropped before the body is serialized (`_oai_images_drop_nulls`), so an unset
// option is *absent* on the wire rather than an explicit `null` the API would
// reject. `request_body` is deep-merged last, exactly as the other clients do.

// ---------------------------------------------------------------------------
// Wire classes.

// POST /v1/images/generations. `prompt` and `model` always ride; everything
// else is optional and omitted when the client left it unset.
//
//   * `response_format` (`url` | `b64_json`) is a dall-e-only parameter — the
//     GPT image models always return base64 and reject the field, so the
//     client omits it for them and defaults it to `b64_json` elsewhere (we need
//     bytes, and the dall-e default is `url`).
//   * `output_format` / `output_compression` / `background` / `moderation` are
//     GPT-image-only; `style` is dall-e-3-only. The client sends what it was
//     given and lets the API reject impossible combinations, rather than
//     encoding a model table that goes stale.
class OaImagesRequest {
    model: string,
    prompt: string,
    n: int?,
    size: string?,
    quality: string?,
    background: string?,
    output_format: string?,
    output_compression: int?,
    moderation: string?,
    style: string?,
    user: string?,
    response_format: string?,
}

// One element of `data[]`. `b64_json` is what the GPT image models always
// return; `url` appears only when a dall-e request asked for it (the link
// expires after 60 minutes, so `_oai_images_fetch_inline` pulls the bytes
// before the value leaves the client); `revised_prompt` is dall-e-3's
// rewritten prompt.
class OaImageDatum {
    b64_json: string?,
    url: string?,
    revised_prompt: string?,
}

class OaImagesInputTokensDetails {
    text_tokens: int?,
    image_tokens: int?,
}

// `ImageGenUsage` — GPT image models only, absent for dall-e. Every field is
// optional here: an envelope that omits one must not fail the whole decode.
class OaImagesUsage {
    input_tokens: int?,
    output_tokens: int?,
    total_tokens: int?,
    input_tokens_details: OaImagesInputTokensDetails?,
}

// `ImagesResponse`. The GPT image models echo the settings they actually used
// (`output_format`, `size`, `quality`, `background`), which is where the
// returned images' MIME type comes from.
class OaImagesResponse {
    created: int?,
    data: OaImageDatum[]?,
    background: string?,
    output_format: string?,
    size: string?,
    quality: string?,
    usage: OaImagesUsage?,
}

// ---------------------------------------------------------------------------
// Request construction.

// The client's canonical id, shared by `ai.Client.id` and the error paths.
function _oai_images_client_id(c: root.ImageClient) -> string {
    let id = `openai-images/${c.model}`;
    id
}

// Drop every null-valued key, recursively. A typed request class serializes
// its unset options as explicit nulls; the Images API wants them absent (and
// `merge_request_body`'s null-deletes-a-key contract only makes sense against a
// null-free base).
function _oai_images_drop_nulls(value: baml.json.json) -> baml.json.json {
    match (value) {
        let object: map<string, baml.json.json> => {
            let out: map<string, baml.json.json> = {};
            for (let key in object.keys()) {
                let entry = object.get(key);
                match (entry) {
                    null => null,
                    _ => {
                        let _ = out.set(key, _oai_images_drop_nulls(entry));
                        null
                    },
                };
            }
            out
        },
        let items: baml.json.json[] => {
            let out: baml.json.json[] = [];
            for (let item in items) {
                out.push(_oai_images_drop_nulls(item));
            }
            out
        },
        _ => value,
    }
}

// Media in a prompt is a hard error: /v1/images/generations takes text only
// (image-to-image lives behind /v1/images/edits, which is a multipart endpoint
// and a separate client).
function _oai_images_media_rejected(kind: string) -> ai.errors.InvalidRequest {
    let detail = `Images API only supports text prompts; found ${kind} input`;
    ai.errors.InvalidRequest {
        provider: "openai-images",
        status_code: null,
        detail: detail,
        raw_body: null,
    }
}

// The rendered prompt as one text block: every text part trimmed, blanks
// dropped, joined with a blank line. Roles carry no meaning for an image
// endpoint, so they are discarded (sys_llm's gateway builder does the same).
// Journal user messages follow the prompt so a re-ask still reaches the model;
// assistant turns and tool traffic have no representation here.
function _oai_images_prompt_text(prompt: ai.Prompt, journal: ai.Journal) -> string {
    let parts: string[] = [];
    for (let message in prompt.messages()) {
        for (let part in message.parts) {
            match (part) {
                let text: string => {
                    let trimmed = text.trim();
                    if (trimmed != "") {
                        parts.push(trimmed);
                    }
                    null
                },
                let image: baml.media.Image => {
                    throw _oai_images_media_rejected("image")
                },
                let audio: baml.media.Audio => {
                    throw _oai_images_media_rejected("audio")
                },
                let video: baml.media.Video => {
                    throw _oai_images_media_rejected("video")
                },
                let pdf: baml.media.Pdf => {
                    throw _oai_images_media_rejected("pdf")
                },
            };
        }
    }
    for (let event in journal.entries()) {
        if let user: ai.events.UserMessage = event {
            let trimmed = user.content.trim();
            if (trimmed != "") {
                parts.push(trimmed);
            }
        }
    }
    let joined = parts.join("\n\n");
    if (joined == "") {
        throw ai.errors.InvalidRequest {
            provider: "openai-images",
            status_code: null,
            detail: "Images API request requires a text prompt",
            raw_body: null,
        }
    }
    joined
}

// `response_format` is dall-e-only. An explicit setting always wins; otherwise
// the GPT image models get the field omitted (they reject it and always answer
// base64) and everything else gets `b64_json`, because a `url` answer — the
// dall-e default — is not what a media output type wants.
function _oai_images_response_format(model: string, explicit: string?) -> string? {
    if let chosen: string = explicit {
        return chosen;
    }
    if (model.starts_with("gpt-image")) {
        return null;
    }
    "b64_json"
}

function _oai_images_trim_slash(url: string) -> string {
    if (url.ends_with("/")) {
        return url.slice(0, url.length() - 1);
    }
    url
}

// Percent-escape the characters that would otherwise change a URL's structure.
// `baml.http` has no query-string builder; this covers what a header/query
// passthrough realistically carries.
function _oai_images_escape_query(raw: string) -> string {
    let escaped = raw
        .replace_all("%", "%25")
        .replace_all(" ", "%20")
        .replace_all("&", "%26")
        .replace_all("=", "%3D")
        .replace_all("?", "%3F")
        .replace_all("#", "%23");
    escaped
}

function _oai_images_with_query(url: string, params: map<string, string>?) -> string {
    let pairs: string[] = [];
    if let configured: map<string, string> = params {
        for (let key in configured.keys()) {
            pairs.push(
                `${_oai_images_escape_query(key)}=${_oai_images_escape_query(configured.get(key) ?? "")}`,
            );
        }
    }
    if (pairs.length() == 0) {
        return url;
    }
    let separator = if (url.includes("?")) {
        "&"
    } else {
        "?"
    };
    `${url}${separator}${pairs.join("&")}`
}

// Provider defaults, then the user's `headers` (lowercased, as the HTTP layer
// stores them), then auth — the credential is applied last so a stray
// passthrough header cannot silently unset it.
function _oai_images_headers(c: root.ImageClient) -> map<string, string> {
    let headers: map<string, string> = { "content-type": "application/json" };
    if let extra: map<string, string> = c.headers {
        for (let name in extra.keys()) {
            let _ = headers.set(name.to_lower_case(), extra.get(name) ?? "");
        }
    }
    let _ = headers.set("authorization", `Bearer ${c.resolved_api_key()}`);
    headers
}

// ai.ModelTurnInput -> one Images API request.
function images_render(c: root.ImageClient, input: ai.ModelTurnInput) -> baml.http.Request {
    if (!input.toolbox.is_empty()) {
        throw ai.errors.InvalidRequest {
            provider: "openai-images",
            status_code: null,
            detail: "the Images API has no tool calling; run tools with a chat client instead",
            raw_body: null,
        }
    }
    let journal = ai.wire.sanitize_for_client(input.journal, _oai_images_client_id(c));
    // The schema block an ordinary client injects is noise for an image
    // endpoint (`Return an image output.` is the only shape it can produce),
    // so the rendered prompt carries the authored text alone.
    let text = _oai_images_prompt_text(input.prompt(""), journal);
    let request = OaImagesRequest {
        model: c.model,
        prompt: text,
        n: c.n,
        size: c.size,
        quality: c.quality,
        background: c.background,
        output_format: c.output_format,
        output_compression: c.output_compression,
        moderation: c.moderation,
        style: c.style,
        user: c.user,
        response_format: _oai_images_response_format(c.model, c.response_format),
    };
    let base = _oai_images_drop_nulls(baml.json.to_json(request));
    let merged = ai.wire.merge_request_body(base, c.request_body);
    let url = _oai_images_with_query(
        `${_oai_images_trim_slash(c.resolved_base_url() ?? "https://api.openai.com/v1")}/images/generations`,
        c.query_params,
    );
    baml.http.Request {
        method: "POST",
        url: url,
        headers: _oai_images_headers(c),
        body: baml.json.stringify(merged),
    }
}

// ---------------------------------------------------------------------------
// Response parsing.

// `output_format` names the encoding of the bytes in `b64_json`. The response
// echoes it for the GPT image models; otherwise the client's own setting is the
// best available answer, and png is the API default.
function _oai_images_mime(format: string?) -> string {
    let normalized = (format ?? "png").to_lower_case();
    match (normalized) {
        "jpeg" | "jpg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => "image/png",
    }
}

// A `url`-mode datum, fetched and inlined. dall-e's `response_format: "url"`
// answers with a link that OpenAI expires after 60 minutes, and a `-> image`
// value whose `base64()` is empty is useless to user code — so the bytes are
// pulled here, once, and the value handed on carries real content. The MIME
// type comes from the fetch's own `content-type` when it sent one.
function _oai_images_fetch_inline(location: string, fallback_mime: string) -> image {
    let response = baml.http.fetch(location) catch_all (e) {
        _ => throw ai.errors.NetworkFailure {
            provider: "openai-images",
            detail: `fetching the generated image at ${location} failed: ${e.to_string()}`,
            raw_body: null,
        },
    };
    if (!response.ok()) {
        throw ai.errors.NetworkFailure {
            provider: "openai-images",
            detail: `fetching the generated image at ${location} returned HTTP ${response.status_code}`,
            raw_body: null,
        }
    }
    let reported = (response.headers.get("content-type") ?? "").split(";").at(0) ?? "";
    let mime = if (reported.trim() == "") {
        fallback_mime
    } else {
        reported.trim()
    };
    baml.media.Image.from_base64(response.bytes().to_base64(), mime)
}

// One `data[]` element as a canonical media block, or null when it carried
// neither base64 nor a URL.
function _oai_images_block(datum: OaImageDatum, mime: string) -> ai.content.Media? {
    if let encoded: string = datum.b64_json {
        return ai.content.Media.new(
            value = baml.media.Image.from_base64(encoded, mime),
            revised_prompt = datum.revised_prompt,
        );
    }
    if let location: string = datum.url {
        return ai.content.Media.new(
            value = _oai_images_fetch_inline(location, mime),
            revised_prompt = datum.revised_prompt,
        );
    }
    null
}

// One decoded response -> one model turn of media blocks. An image endpoint
// produces no text and never stops early, so the stop reason is always
// Complete; `usage` is present for the GPT image models only.
function images_parse(c: root.ImageClient, resp: OaImagesResponse) -> ai.ModelTurn {
    let mime = _oai_images_mime(resp.output_format ?? c.output_format);
    let data: OaImageDatum[] = resp.data ?? [];
    let content: ai.content.Block[] = [];
    for (let datum in data) {
        if let block: ai.content.Media = _oai_images_block(datum, mime) {
            content.push(block);
        }
    }
    if (content.length() == 0) {
        throw ai.errors.ParseFailed {
            provider: "openai-images",
            raw_output: "the images response contained no image data in data[]",
        }
    }
    let usage: ai.events.Usage? = if let reported: OaImagesUsage = resp.usage {
        ai.events.Usage {
            input_tokens: reported.input_tokens ?? 0,
            output_tokens: reported.output_tokens ?? 0,
            cached_input_tokens: null,
            reasoning_tokens: null,
        }
    } else {
        null
    };
    ai.ModelTurn {
        content: content,
        stop_reason: ai.content.StopReason.Complete,
        usage: usage,
    }
}

// The one internal entry point: render, send, normalize — one model turn.
// There is no streaming counterpart: the Images API's `stream`/`partial_images`
// mode emits partially-rendered previews, which `ai.stream.StreamEvent` (text
// deltas) cannot carry, so `root.ImageClient` deliberately does not implement
// ai.stream.StreamingClient.
function images_invoke(c: root.ImageClient, input: ai.ModelTurnInput) -> ai.ModelTurn {
    let req = images_render(c, input);
    let resp = ai.wire.send_as<OaImagesResponse>(req, "openai-images");
    images_parse(c, resp)
}
