// root.AiGatewayImageClient — the Vercel AI Gateway image model endpoint
// (POST {base_url}/image-model, base_url defaulting to
// https://ai-gateway.vercel.sh/v4/ai).
//
// A port of sys_llm's `build_request/openai/images.rs` +
// `parse_response/openai/images.rs`, which is where the ground truth lives: the
// model rides in the `ai-model-id` HEADER rather than the body, the body is
// `{prompt, n}` plus whatever the caller passed through, and the response is
// `{images: [<base64>], providerMetadata}`.
//
// One deliberate divergence from that port: sys_llm hardcoded `image/png` for
// every returned image, even when the request asked for jpeg. Here the MIME
// type comes from the payload itself when it is a data URL, then from an
// `outputFormat` in the response's `providerMetadata`, then from one in the
// client's `providerOptions` passthrough, and only then defaults to png.

// ---------------------------------------------------------------------------
// Wire classes.

// The request body. `n` defaults to 1 (sys_llm's `entry("n").or_insert(1)`);
// everything else the gateway understands — `size`, `aspectRatio`, `seed`,
// `providerOptions` — is Vercel AI SDK image-model shape, not OpenAI shape, and
// rides through `request_body`.
// `size` / `aspectRatio` / `seed` are the AI SDK image-model call options; they
// are sent only when the client sets them, so the gateway's own defaults apply
// otherwise. sys_llm never had first-class fields for them — they went through
// the extra-body map, which still works and still wins (the merge is last).
class GwImagesRequest {
    prompt: string,
    n: int?,
    size: string?,
    aspectRatio: string?,
    seed: int?,
    files: GwImagesFile[]?,
}

// One `files[]` element — the AI SDK's `ImageModelV4File`, which is a union of
// `{type:"file", mediaType, data}` (base64 bytes) and `{type:"url", url}`.
// Modeled as one class whose null fields are dropped before serialization, so
// each element reaches the wire as exactly one of those two shapes.
//
// (`mask`, the other editing input, has no first-class field: it is one image
// and rides through `request_body` as `{"mask": {...}}`.)
class GwImagesFile {
    type: string,
    mediaType: string?,
    data: string?,
    url: string?,
}

// The text + files pair a rendered prompt lowers to.
class GwImagesInput {
    text: string,
    files: GwImagesFile[],
}

// `usage` — the gateway's token accounting for the call. Absent for providers
// that do not report it, and every field is optional so a partial object still
// decodes.
class GwImagesUsage {
    inputTokens: int?,
    outputTokens: int?,
    totalTokens: int?,
}

// One `warnings[]` element. The protocol's warning is a discriminated union —
// `{type:"unsupported"|"compatibility", feature, details?}`,
// `{type:"deprecated", setting, message}`, `{type:"other", message}` — flattened
// here into one all-optional class so an unfamiliar variant still decodes.
class GwImagesWarning {
    type: string?,
    feature: string?,
    details: string?,
    setting: string?,
    message: string?,
}

// The response envelope. `images` is a list of base64 payloads (some gateway
// providers return `data:` URLs); `providerMetadata` is an opaque per-provider
// blob and stays `json`.
class GwImagesResponse {
    images: string[]?,
    warnings: GwImagesWarning[]?,
    providerMetadata: baml.json.json?,
    usage: GwImagesUsage?,
}

// ---------------------------------------------------------------------------
// Request construction.

// The client's canonical id, shared by `ai.Client.id` and the error paths.
function _gw_images_client_id(c: root.AiGatewayImageClient) -> string {
    let id = `ai-gateway-images/${c.model}`;
    id
}

// Drop every null-valued key, recursively: a typed request class serializes its
// unset options as explicit nulls, and the wire wants them absent.
function _gw_images_drop_nulls(value: baml.json.json) -> baml.json.json {
    match (value) {
        let object: map<string, baml.json.json> => {
            let out: map<string, baml.json.json> = {};
            for (let key in object.keys()) {
                let entry = object.get(key);
                match (entry) {
                    null => null,
                    _ => {
                        let _ = out.set(key, _gw_images_drop_nulls(entry));
                        null
                    },
                };
            }
            out
        },
        let items: baml.json.json[] => {
            let out: baml.json.json[] = [];
            for (let item in items) {
                out.push(_gw_images_drop_nulls(item));
            }
            out
        },
        _ => value,
    }
}

function _gw_images_media_rejected(kind: string) -> ai.errors.InvalidRequest {
    let detail = `Images API only supports text and image prompts; found ${kind} input`;
    ai.errors.InvalidRequest {
        provider: "ai-gateway-images",
        status_code: null,
        detail: detail,
        raw_body: null,
    }
}

// One image prompt part as a `files[]` element. Media is NOT fetched here: the
// protocol takes a `{type:"url"}` reference directly, so a remote image is
// handed to the gateway as a link and only local/base64/`data:` sources are
// inlined.
function _gw_images_file(media: ai.MediaPart) -> GwImagesFile {
    let resolved = ai.wire.resolve_media(media, false);
    if let encoded: string = resolved.base64 {
        return GwImagesFile {
            type: "file",
            mediaType: resolved.mime_type,
            data: encoded,
            url: null,
        };
    }
    if let location: string = resolved.url {
        return GwImagesFile { type: "url", mediaType: null, data: null, url: location };
    }
    throw ai.errors.InvalidRequest {
        provider: "ai-gateway-images",
        status_code: null,
        detail: "image input carried no base64 content, file, or URL",
        raw_body: null,
    }
}

// The rendered prompt lowered to the endpoint's two inputs.
//
// Text: every text part, trimmed, blanks dropped, joined with a blank line —
// sys_llm's `prompt_to_text`, roles discarded. Journal user messages follow so
// a re-ask still reaches the model.
//
// Files: image parts become `files[]` entries, which is how this endpoint does
// image editing / variations (image-model protocol v4). The other media kinds
// have no representation and are still rejected.
function _gw_images_input(prompt: ai.Prompt, journal: ai.Journal) -> GwImagesInput {
    let parts: string[] = [];
    let files: GwImagesFile[] = [];
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
                    files.push(_gw_images_file(image));
                    null
                },
                let audio: baml.media.Audio => {
                    throw _gw_images_media_rejected("audio")
                },
                let video: baml.media.Video => {
                    throw _gw_images_media_rejected("video")
                },
                let pdf: baml.media.Pdf => {
                    throw _gw_images_media_rejected("pdf")
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
            provider: "ai-gateway-images",
            status_code: null,
            detail: "Images API request requires a text prompt",
            raw_body: null,
        }
    }
    GwImagesInput { text: joined, files: files }
}

function _gw_images_trim_slash(url: string) -> string {
    if (url.ends_with("/")) {
        return url.substring(0, url.length() - 1);
    }
    url
}

// Percent-escape the characters that would otherwise change a URL's structure.
function _gw_images_escape_query(raw: string) -> string {
    let escaped = raw
        .replace_all("%", "%25")
        .replace_all(" ", "%20")
        .replace_all("&", "%26")
        .replace_all("=", "%3D")
        .replace_all("?", "%3F")
        .replace_all("#", "%23");
    escaped
}

function _gw_images_with_query(url: string, params: map<string, string>?) -> string {
    let pairs: string[] = [];
    if let configured: map<string, string> = params {
        for (let key in configured.keys()) {
            pairs.push(
                `${_gw_images_escape_query(key)}=${_gw_images_escape_query(configured.get(key) ?? "")}`,
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

// Gateway protocol headers, then the user's `headers` (lowercased), then auth —
// `authorization` + `ai-gateway-auth-method` are applied last so a passthrough
// header cannot silently unset the credential.
function _gw_images_headers(c: root.AiGatewayImageClient) -> map<string, string> {
    let headers: map<string, string> = {
        "content-type": "application/json",
        "ai-gateway-protocol-version": "0.0.1",
        "ai-image-model-specification-version": "4",
        "ai-model-id": c.model,
    };
    if let extra: map<string, string> = c.headers {
        for (let name in extra.keys()) {
            let _ = headers.set(name.to_lower_case(), extra.get(name) ?? "");
        }
    }
    let _ = headers.set("authorization", `Bearer ${c.resolved_api_key()}`);
    let _ = headers.set("ai-gateway-auth-method", "api-key");
    headers
}

// ai.ModelTurnInput -> one gateway image-model request.
function gateway_images_render(
    c: root.AiGatewayImageClient,
    input: ai.ModelTurnInput,
) -> baml.http.Request {
    if (!input.toolbox.is_empty()) {
        throw ai.errors.InvalidRequest {
            provider: "ai-gateway-images",
            status_code: null,
            detail: "the gateway image endpoint has no tool calling; run tools with a chat client instead",
            raw_body: null,
        }
    }
    let journal = ai.wire.sanitize_for_client(input.journal, _gw_images_client_id(c));
    // No schema block: the only thing this endpoint can return is an image.
    let lowered = _gw_images_input(input.prompt(""), journal);
    let files: GwImagesFile[]? = if (lowered.files.length() == 0) {
        null
    } else {
        lowered.files
    };
    let request = GwImagesRequest {
        prompt: lowered.text,
        n: c.n ?? 1,
        size: c.size,
        aspectRatio: c.aspect_ratio,
        seed: c.seed,
        files: files,
    };
    let base = _gw_images_drop_nulls(baml.json.to_json(request));
    let merged = ai.wire.merge_request_body(base, c.request_body);
    let url = _gw_images_with_query(
        `${_gw_images_trim_slash(c.resolved_base_url() ?? "https://ai-gateway.vercel.sh/v4/ai")}/image-model`,
        c.query_params,
    );
    baml.http.Request {
        method: "POST",
        url: url,
        headers: _gw_images_headers(c),
        body: baml.json.stringify(merged),
    }
}

// ---------------------------------------------------------------------------
// Response parsing.

// The first `outputFormat` anywhere in a JSON blob. Gateway providers nest it
// under their own name (`providerOptions.blackForestLabs.outputFormat`), and
// the response echoes provider settings back under `providerMetadata`, so the
// search is structural rather than path-based.
function _gw_images_find_output_format(value: baml.json.json?) -> string? {
    if let object: map<string, baml.json.json> = value {
        for (let key in object.keys()) {
            let entry = object.get(key);
            if (key == "outputFormat" || key == "output_format") {
                if let name: string = entry {
                    return name;
                }
            }
            if let nested: string = _gw_images_find_output_format(entry) {
                return nested;
            }
        }
    }
    null
}

function _gw_images_mime(format: string?) -> string {
    let normalized = (format ?? "png").to_lower_case();
    if (normalized == "jpeg" || normalized == "jpg") {
        "image/jpeg"
    } else if (normalized == "webp") {
        "image/webp"
    } else if (normalized == "gif") {
        "image/gif"
    } else {
        "image/png"
    }
}

// One `images[]` element as an image value. Raw base64 is the documented shape;
// a `data:<mime>;base64,` payload carries its own MIME type and wins. A `data:`
// URL in any OTHER encoding (percent-escaped bytes) is a parse failure, not
// base64 — storing the URL text as the image's payload would hand user code
// bytes that are not the image.
function _gw_images_value(encoded: string, fallback_mime: string) -> image {
    if (encoded.starts_with("data:")) {
        if let marker: int = encoded.index_of(";base64,") {
            let mime = encoded.substring(5, marker);
            let data = encoded.substring(marker + 8, encoded.length());
            return baml.media.Image.from_base64(data, mime);
        }
        let preview = if (encoded.length() > 48) {
            `${encoded.substring(0, 48)}...`
        } else {
            encoded
        };
        throw ai.errors.ParseFailed {
            provider: "ai-gateway-images",
            raw_output: `images[] carried a data: URL with no ;base64, marker: ${preview}`,
        }
    }
    baml.media.Image.from_base64(encoded, fallback_mime)
}

// One `warnings[]` entry as a log line. Every variant of the protocol's warning
// union names either a `feature`/`setting` or a free-text `message`, so this
// renders whichever the provider sent.
function gateway_images_warning_text(warning: GwImagesWarning) -> string {
    let subject = warning.feature ?? warning.setting;
    let detail = warning.details ?? warning.message;
    let head = `ai-gateway-images: ${warning.type ?? "other"} warning`;
    let named = if let subject_name: string = subject {
        `${head} for ${subject_name}`
    } else {
        head
    };
    if let text: string = detail {
        `${named}: ${text}`
    } else {
        named
    }
}

// One decoded response -> one model turn of media blocks. The endpoint reports
// no finish reason, so the stop reason is always Complete (sys_llm's
// `FinishReason::Stop`); `usage` is carried through when the provider sent it
// (sys_llm dropped it). Any `warnings[]` — options the underlying provider
// ignored — are logged, since nothing else in the turn records them.
function gateway_images_parse(
    c: root.AiGatewayImageClient,
    resp: GwImagesResponse,
) -> ai.ModelTurn {
    let warnings: GwImagesWarning[] = resp.warnings ?? [];
    for (let warning in warnings) {
        log.warn(gateway_images_warning_text(warning));
    }
    let requested = _gw_images_find_output_format(resp.providerMetadata)
        ?? _gw_images_find_output_format(c.request_body);
    let fallback_mime = _gw_images_mime(requested);
    let images: string[] = resp.images ?? [];
    let content: ai.content.Block[] = [];
    for (let encoded in images) {
        if (encoded.trim() != "") {
            content.push(
                ai.content.Media.new(value = _gw_images_value(encoded, fallback_mime)),
            );
        }
    }
    if (content.length() == 0) {
        throw ai.errors.ParseFailed {
            provider: "ai-gateway-images",
            raw_output: "response contained no images in images[]",
        }
    }
    let usage: ai.events.Usage? = if let reported: GwImagesUsage = resp.usage {
        ai.events.Usage {
            input_tokens: reported.inputTokens ?? 0,
            output_tokens: reported.outputTokens ?? 0,
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

// The one internal entry point: render, send, normalize — one model turn. The
// endpoint does not stream, so there is no streaming counterpart.
function gateway_images_invoke(
    c: root.AiGatewayImageClient,
    input: ai.ModelTurnInput,
) -> ai.ModelTurn {
    let req = gateway_images_render(c, input);
    let resp = ai.wire.send_as<GwImagesResponse>(req, "ai-gateway-images");
    gateway_images_parse(c, resp)
}
