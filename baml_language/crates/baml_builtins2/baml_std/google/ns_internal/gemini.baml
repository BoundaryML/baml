// The Gemini `generateContent` wire format, shared by both Google backends:
// root.GoogleClient (generativelanguage.googleapis.com) and root.VertexClient
// (aiplatform.googleapis.com). Only URL construction and authentication differ
// between them, so everything below — typed request classes, prompt/journal
// lowering, the response envelope, the SSE decoder — is written once here and
// consumed from `vertex.baml` in this same namespace.
//
// PLAN §1.5: the request is a tree of typed BAML classes, not `map<string,
// unknown>` construction. `baml.json.json` appears exactly twice: the
// `request_body` passthrough (merged last, `ai.wire.merge_request_body`) and
// the genuinely open-ended tool `parametersJsonSchema`.

// ---------------------------------------------------------------------------
// Request classes (https://ai.google.dev/api/generate-content#request-body).
//
// Every optional field is dropped from the wire body by `_gm_strip_nulls`
// before it is sent, which is what `#[serde(skip_serializing_if =
// "Option::is_none")]` did in the Rust builder: Gemini's `Part` is a union, so
// a part carrying `{"text": "hi", "inlineData": null}` is not the same request
// as one carrying `{"text": "hi"}`.

class GmInlineData {
    mimeType: string,
    data: string,
}

// `mimeType` is optional: the URI forms the API resolves by itself (a YouTube
// watch URL, a Files-API handle) carry no MIME type, and guessing one from the
// URL's spelling puts a wrong type on the wire.
class GmFileData {
    mimeType: string?,
    fileUri: string,
}

// Gemini has no tool-call ids: a call is correlated to its result by NAME.
class GmFunctionCall {
    name: string,
    args: map<string, unknown>,
}

class GmFunctionResponse {
    name: string,
    response: map<string, unknown>,
}

// One `Part` of a content. Exactly one payload field is set; the rest are
// stripped before serialization.
//
// `thoughtSignature` is the exception: it is a SIBLING of the payload field,
// not a payload of its own. Gemini 3 issues one with every `functionCall` it
// emits and REQUIRES it back when that call is replayed in `contents` — a
// bare replayed `functionCall` is rejected with HTTP 400 ("Function call is
// missing a thought_signature in functionCall parts"), which breaks every
// multi-turn tool loop on the 3.x models. See `_gm_encode_call_id`.
class GmPart {
    text: string?,
    inlineData: GmInlineData?,
    fileData: GmFileData?,
    functionCall: GmFunctionCall?,
    functionResponse: GmFunctionResponse?,
    thoughtSignature: string?,

    function of_text(text: string) -> GmPart {
        GmPart {
            text: text,
            inlineData: null,
            fileData: null,
            functionCall: null,
            functionResponse: null,
            thoughtSignature: null,
        }
    }

    function of_inline_data(data: GmInlineData) -> GmPart {
        GmPart {
            text: null,
            inlineData: data,
            fileData: null,
            functionCall: null,
            functionResponse: null,
            thoughtSignature: null,
        }
    }

    function of_file_data(data: GmFileData) -> GmPart {
        GmPart {
            text: null,
            inlineData: null,
            fileData: data,
            functionCall: null,
            functionResponse: null,
            thoughtSignature: null,
        }
    }

    // `signature` is the `thoughtSignature` Gemini issued alongside this call
    // (null for a model that issues none, and for a call replayed from a
    // journal written before it existed).
    function of_function_call(call: GmFunctionCall, signature: string?) -> GmPart {
        GmPart {
            text: null,
            inlineData: null,
            fileData: null,
            functionCall: call,
            functionResponse: null,
            thoughtSignature: signature,
        }
    }

    function of_function_response(response: GmFunctionResponse) -> GmPart {
        GmPart {
            text: null,
            inlineData: null,
            fileData: null,
            functionCall: null,
            functionResponse: response,
            thoughtSignature: null,
        }
    }
}

// A `Content`: one turn's parts plus its role. `systemInstruction` is a
// Content with no role, hence the optional.
class GmContent {
    role: string?,
    parts: GmPart[],
}

// `parametersJsonSchema` takes an unconstrained JSON Schema document (the
// newer field; the legacy `parameters` accepts only the OpenAPI subset).
class GmFunctionDeclaration {
    name: string,
    description: string,
    parametersJsonSchema: map<string, unknown>,
}

class GmTool {
    functionDeclarations: GmFunctionDeclaration[],
}

class GmFunctionCallingConfig {
    mode: string,
}

class GmToolConfig {
    functionCallingConfig: GmFunctionCallingConfig,
}

// The typed subset of `generationConfig` the clients expose as first-class
// options. Everything else the API accepts (thinkingConfig, responseSchema,
// seed, penalties, …) rides the `request_body` passthrough, which merges into
// this object key-by-key rather than replacing it.
class GmGenerationConfig {
    temperature: float?,
    maxOutputTokens: int?,
    topP: float?,
    topK: int?,
    stopSequences: string[]?,
    responseModalities: string[]?,
}

class GmGenerateContentRequest {
    contents: GmContent[],
    systemInstruction: GmContent?,
    tools: GmTool[]?,
    toolConfig: GmToolConfig?,
    generationConfig: GmGenerationConfig?,
    safetySettings: root.SafetySetting[]?,
}

// ---------------------------------------------------------------------------
// Response classes. `from_string` ignores unknown fields, and every field is
// optional because a candidate may carry only some of them (a streamed
// fragment carries almost none).

class GmRespInlineData {
    mimeType: string?,
    data: string?,
}

class GmRespFileData {
    mimeType: string?,
    fileUri: string?,
}

class GmRespFunctionCall {
    name: string,
    args: map<string, unknown>?,
}

class GmRespPart {
    text: string?,
    // Gemini 2.5 thinking models tag their reasoning parts; the answer is the
    // untagged text.
    thought: bool?,
    thoughtSignature: string?,
    functionCall: GmRespFunctionCall?,
    inlineData: GmRespInlineData?,
    fileData: GmRespFileData?,
}

class GmRespContent {
    role: string?,
    parts: GmRespPart[]?,
}

class GmSafetyRating {
    category: string?,
    probability: string?,
    blocked: bool?,
}

class GmCandidate {
    content: GmRespContent?,
    finishReason: string?,
    finishMessage: string?,
    index: int?,
    safetyRatings: GmSafetyRating[]?,
}

class GmUsageMetadata {
    promptTokenCount: int?,
    candidatesTokenCount: int?,
    // Context-cache hits, billed at the reduced rate.
    cachedContentTokenCount: int?,
    // Thinking tokens; Gemini reports them DISJOINTLY from candidatesTokenCount.
    thoughtsTokenCount: int?,
    toolUsePromptTokenCount: int?,
    totalTokenCount: int?,
}

class GmPromptFeedback {
    blockReason: string?,
    blockReasonMessage: string?,
    safetyRatings: GmSafetyRating[]?,
}

// The `{"error": {...}}` document Google returns for a failed call. It arrives
// with a non-2xx status on the unary path (where `ai.wire.send_as` classifies
// it) and can also arrive as an SSE payload mid-stream.
class GmError {
    code: int?,
    message: string?,
    status: string?,
}

class GmResponse {
    candidates: GmCandidate[]?,
    usageMetadata: GmUsageMetadata?,
    promptFeedback: GmPromptFeedback?,
    modelVersion: string?,
    responseId: string?,
    error: GmError?,
}

// ---------------------------------------------------------------------------
// Serialization helpers.

// Keys whose values are documents supplied by the caller or by reflection, not
// wire fields we own: a `null` inside them is DATA (a JSON Schema default, a
// tool argument) and must survive.
function _gm_is_opaque_key(key: string) -> bool {
    key == "args" || key == "response" || key == "parametersJsonSchema"
}

// Drop every null-valued object entry, recursively — the BAML equivalent of
// serde's `skip_serializing_if = "Option::is_none"`. Array ELEMENTS are never
// dropped (a null element is a positional value), and `_gm_is_opaque_key`
// subtrees are copied verbatim.
function _gm_strip_nulls(value: baml.json.json, opaque: bool) -> baml.json.json {
    if (opaque) {
        return value;
    }
    match (value) {
        let object: map<string, baml.json.json> => {
            let out: map<string, baml.json.json> = {};
            for (let key in object.keys()) {
                let entry = object.get(key);
                match (entry) {
                    null => null,
                    _ => {
                        let _ = out.set(key, _gm_strip_nulls(entry, _gm_is_opaque_key(key)));
                        null
                    },
                };
            }
            out
        },
        let items: baml.json.json[] => {
            items.map((item) -> {
                _gm_strip_nulls(item, false)
            })
        },
        _ => value,
    }
}

// A query-string component. There is no percent-encoder in the stdlib, so this
// covers the characters that would otherwise change the URL's structure plus
// the ones that are simply illegal in a query. Values that need more than this
// (raw UTF-8, control characters) should be encoded by the caller.
function _gm_encode_query(value: string) -> string {
    value.replace_all("%", "%25")
        .replace_all(" ", "%20")
        .replace_all("\"", "%22")
        .replace_all("#", "%23")
        .replace_all("&", "%26")
        .replace_all("+", "%2B")
        .replace_all("/", "%2F")
        .replace_all("<", "%3C")
        .replace_all("=", "%3D")
        .replace_all(">", "%3E")
        .replace_all("?", "%3F")
}

// Append `params` to `url`, respecting a query string the URL already has
// (`:streamGenerateContent?alt=sse`).
function _gm_apply_query(url: string, params: map<string, string>?) -> string {
    if let items: map<string, string> = params {
        let out = url;
        for (let name in items.keys()) {
            let separator = if (out.includes("?")) {
                "&"
            } else {
                "?"
            };
            out
                = `${out}${separator}${_gm_encode_query(name)}=${_gm_encode_query(items.get(name) ?? "")}`;
        }
        out
    } else {
        url
    }
}

// Apply the caller's `headers` option over the provider defaults. Header names
// are lower-cased, matching what the HTTP layer does on the wire. Auth headers
// are set AFTER this, so a user header can never displace credentials.
function _gm_apply_headers(
    headers: map<string, string>,
    extra: map<string, string>?,
) -> map<string, string> {
    if let items: map<string, string> = extra {
        for (let name in items.keys()) {
            let _ = headers.set(name.to_lower_case(), items.get(name) ?? "");
        }
        headers
    } else {
        headers
    }
}

// The typed `generationConfig`, or null when the client set none of its
// fields — an empty object would still be a wire difference.
function _gm_generation_config(
    temperature: float?,
    max_output_tokens: int?,
    top_p: float?,
    top_k: int?,
    stop_sequences: string[]?,
    response_modalities: string[]?,
) -> GmGenerationConfig? {
    if (
        temperature == null
            && max_output_tokens == null
            && top_p == null
            && top_k == null
            && stop_sequences == null
            && response_modalities == null
    ) {
        null
    } else {
        GmGenerationConfig {
            temperature: temperature,
            maxOutputTokens: max_output_tokens,
            topP: top_p,
            topK: top_k,
            stopSequences: stop_sequences,
            responseModalities: response_modalities,
        }
    }
}

// ---------------------------------------------------------------------------
// Prompt lowering.

// The URL a media part was built from, if it was built from one at all.
function _gm_media_url(media: ai.MediaPart) -> string? {
    match (media) {
        let image: baml.media.Image => image.url(),
        let audio: baml.media.Audio => audio.url(),
        let video: baml.media.Video => video.url(),
        let pdf: baml.media.Pdf => pdf.url(),
    }
}

// URLs the API resolves SERVER-SIDE and that must therefore never be fetched
// and inlined: `gs://` Cloud Storage objects (handled inside `resolve_media`
// itself), Files-API handles, and YouTube videos — a documented `fileData`
// input on both backends. Fetching a YouTube watch URL inlines the watch
// PAGE's HTML as base64 "video", which is garbage on the wire.
function _gm_is_provider_uri(url: string) -> bool {
    url.starts_with("gs://")
        || url.includes("://generativelanguage.googleapis.com/v1beta/files/")
        || url.includes("://www.youtube.com/")
        || url.includes("://youtube.com/")
        || url.includes("://m.youtube.com/")
        || url.includes("://youtu.be/")
}

// A `fileData` URI Gemini resolves by itself carries no MIME type we can know
// (a YouTube watch URL has no extension), so send none rather than the guess
// `resolve_media` derives from the URL's spelling.
function _gm_mime_for_uri(url: string, mime: string) -> string? {
    if (url.starts_with("gs://")) {
        mime
    } else if (url.includes("youtu")) {
        null
    } else {
        mime
    }
}

// One prompt media part -> `inlineData` (base64) or `fileData` (a URI the API
// resolves itself).
//
// `fetch_url` is the backend's URL strategy, mirroring sys_llm's per-provider
// media defaults: the Gemini Developer API has no server-side URL fetch, so an
// http(s) URL is downloaded and inlined (`true`), while Vertex takes the URL
// through as `fileData.fileUri` (`false`) — which also keeps large media out
// of the inline request-size cap. Local files always become base64 and
// existing base64 is retained on both backends, and a `_gm_is_provider_uri`
// URL is passed through even when `fetch_url` is set.
function _gm_media_part(media: ai.MediaPart, fetch_url: bool, preview: bool = false) -> GmPart {
    let source = _gm_media_url(media);
    let passthrough = if let url: string = source {
        _gm_is_provider_uri(url)
    } else {
        false
    };
    let resolved = if (preview) {
        ai.wire.resolve_media_preview(media)
    } else {
        ai.wire.resolve_media(media, fetch_url && !passthrough)
    };
    if (preview && fetch_url && !passthrough && resolved.url != null) {
        throw ai.errors.PreviewUnsupported {
            provider: "google",
            detail: "the Gemini Developer API requires downloaded URL media bytes during request preview",
        }
    }
    if let url: string = resolved.url {
        GmPart.of_file_data(
            GmFileData { mimeType: _gm_mime_for_uri(url, resolved.mime_type), fileUri: url },
        )
    } else {
        GmPart.of_inline_data(
            GmInlineData { mimeType: resolved.mime_type, data: resolved.base64 ?? "" },
        )
    }
}

function _gm_prompt_parts(
    message: ai.PromptMessage,
    fetch_url: bool,
    preview: bool = false,
) -> GmPart[] {
    let parts: GmPart[] = [];
    for (let part in message.parts) {
        match (part) {
            let text: string => {
                parts.push(GmPart.of_text(text));
                null
            },
            let image: baml.media.Image => {
                parts.push(_gm_media_part(image, fetch_url, preview = preview));
                null
            },
            let audio: baml.media.Audio => {
                parts.push(_gm_media_part(audio, fetch_url, preview = preview));
                null
            },
            let video: baml.media.Video => {
                parts.push(_gm_media_part(video, fetch_url, preview = preview));
                null
            },
            let pdf: baml.media.Pdf => {
                parts.push(_gm_media_part(pdf, fetch_url, preview = preview));
                null
            },
        };
    }
    parts
}

class GooglePrompt {
    system_parts: GmPart[],
    contents: GmContent[],
}

// Gemini spells assistant as `model` and carries system instructions outside
// `contents`. Role-less prompt content is an ordinary user turn.
//
// ALL system messages merge into one `systemInstruction` (PLAN §5: google
// merges, the other providers demote the 2nd+ to user) — matching sys_llm's
// builder and the shipped native behavior.
function google_lower_prompt(
    prompt: ai.Prompt,
    fetch_url: bool = true,
    preview: bool = false,
) -> GooglePrompt {
    let system_parts: GmPart[] = [];
    let contents: GmContent[] = [];
    for (let message in prompt.messages()) {
        let parts = _gm_prompt_parts(message, fetch_url = fetch_url, preview = preview);
        if (message.role == "system") {
            for (let part in parts) {
                system_parts.push(part);
            }
            null
        } else {
            let role = if (message.role == "assistant") {
                "model"
            } else {
                "user"
            };
            contents.push(GmContent { role: role, parts: parts });
            null
        };
    }
    GooglePrompt { system_parts: system_parts, contents: contents }
}

// ---------------------------------------------------------------------------
// Thought signatures (Gemini 3.x).
//
// Gemini 3 returns a `thoughtSignature` on every `functionCall` part and
// REJECTS the request (HTTP 400, "Function call is missing a thought_signature
// in functionCall parts") when that call is replayed in `contents` without it.
// Every multi-turn tool loop replays exactly that, so the signature has to
// survive the round-trip through the journal — and the journal's
// `ai.content.ToolUse` has only `id`, `name` and `args`, none of which is a
// metadata slot.
//
// The id is the one field this client OWNS: Gemini sends no call ids at all,
// `google_parse` synthesizes them (`call_<seq>_<i>`), and they are opaque
// correlation handles everywhere else (`ai.wire` documents id shape as "that
// provider's business" and deliberately never rewrites them). So the signature
// rides the id: `call_0_0~gts~<signature>`. It round-trips through
// AssistantMessage -> ToolRequested -> ToolCompleted untouched, needs no shared
// -file change, and is stripped back off at the one place that rebuilds the
// wire part.
//
// Signatures are base64, which cannot contain `~`, so the marker is
// unambiguous; an id without it (another provider's journal, a journal written
// before this existed, a 2.x model) simply yields no signature.
function _gm_sig_marker() -> string {
    "~gts~"
}

function _gm_encode_call_id(id: string, signature: string?) -> string {
    if let sig: string = signature {
        `${id}${_gm_sig_marker()}${sig}`
    } else {
        id
    }
}

// The `thoughtSignature` carried by a synthesized call id, if it carries one.
function _gm_call_id_signature(id: string) -> string? {
    let marker = _gm_sig_marker();
    if let at: int = id.index_of(marker) {
        id.slice(at + marker.length(), id.length())
    } else {
        null
    }
}

// Gemini correlates tool results by NAME, not id: find the tool name for a
// result id by scanning the ai.AssistantMessage ai.ToolUse blocks in the journal.
function _gm_tool_name_for(j: ai.Journal, id: string) -> string {
    let name = "";
    for (let e in j.entries()) {
        match (e) {
            let a: ai.events.AssistantMessage => {
                for (let b in a.content) {
                    match (b) {
                        let u: ai.content.ToolUse => {
                            if (u.id == id) {
                                name = u.name;
                                null
                            } else {
                                null
                            }
                        },
                        _ => null,
                    };
                }
                null
            },
            _ => null,
        };
    }
    name
}

// ai.Journal -> generateContent `contents`. Only promptable events lower;
// ai.RunStarted, ai.ToolRequested, ai.Usage, and ai.FinalProduced are journal-only.
// Consecutive tool results merge into ONE user content of functionResponse
// parts, matching how Gemini expects results for a multi-call turn.
function google_lower_journal(j: ai.Journal) -> GmContent[] {
    let contents: GmContent[] = [];
    let pending: GmPart[] = []; // functionResponse parts awaiting flush
    for (let e in j.entries()) {
        match (e) {
            let u: ai.events.UserMessage => {
                if (pending.length() > 0) {
                    contents.push(GmContent { role: "user", parts: pending });
                    pending = [];
                    null
                } else {
                    null
                };
                contents.push(GmContent { role: "user", parts: [GmPart.of_text(u.content)] });
                null
            },
            let a: ai.events.AssistantMessage => {
                if (pending.length() > 0) {
                    contents.push(GmContent { role: "user", parts: pending });
                    pending = [];
                    null
                } else {
                    null
                };
                let parts: GmPart[] = [];
                for (let b in a.content) {
                    match (b) {
                        let t: ai.content.Text => {
                            parts.push(GmPart.of_text(t.text));
                            null
                        },
                        let use: ai.content.ToolUse => {
                            // The `thoughtSignature` Gemini 3 issued with this
                            // call rides its id; replaying the call without it
                            // is a hard 400.
                            parts.push(
                                GmPart.of_function_call(
                                    GmFunctionCall { name: use.name, args: use.args },
                                    _gm_call_id_signature(use.id),
                                ),
                            );
                            null
                        },
                        // Reasoning replay needs the `thoughtSignature` Gemini
                        // issued with the thought, which ai.content.Reasoning
                        // does not carry; generated media is an output, not
                        // replayable input. Both lower to nothing.
                        _ => null,
                    };
                }
                if (parts.length() > 0) {
                    contents.push(GmContent { role: "model", parts: parts });
                    null
                } else {
                    null
                };
                null
            },
            let c: ai.events.ToolCompleted => {
                let response: map<string, unknown> = {};
                let _ = response.set("result", c.output);
                pending.push(
                    GmPart.of_function_response(
                        GmFunctionResponse { name: _gm_tool_name_for(j, c.id), response: response },
                    ),
                );
                null
            },
            let f: ai.events.ToolFailed => {
                let response: map<string, unknown> = {};
                let _ = response.set("error", f.message);
                pending.push(
                    GmPart.of_function_response(
                        GmFunctionResponse { name: _gm_tool_name_for(j, f.id), response: response },
                    ),
                );
                null
            },
            _ => null,
        };
    }
    if (pending.length() > 0) {
        contents.push(GmContent { role: "user", parts: pending });
        null
    } else {
        null
    };
    contents
}

// ---------------------------------------------------------------------------
// Body construction — shared by both clients.

// ai.ModelTurnInput -> the serialized generateContent body.
//
// The last step is always `ai.wire.merge_request_body`, so `request_body`
// overrides everything the typed request built (and a null value in it DELETES
// the key the client injected).
//
// `fetch_url` is the backend's media strategy (see `_gm_media_part`): the
// Developer API inlines http(s) media, Vertex passes the URL through.
function gemini_build_body(
    input: ai.ModelTurnInput,
    client_id: string,
    generation_config: GmGenerationConfig?,
    safety_settings: root.SafetySetting[]?,
    request_body: baml.json.json?,
    fetch_url: bool = true,
    preview: bool = false,
) -> string {
    let prompt = input.prompt(ai.wire.render_output_format(input.output_type));
    let lowered = google_lower_prompt(prompt, fetch_url = fetch_url, preview = preview);
    let contents = lowered.contents;
    let journal = ai.wire.sanitize_for_client(input.journal, client_id);
    for (let item in google_lower_journal(journal)) {
        contents.push(item);
    }

    // Gemini requires `contents` to start with a user turn. In the unusual
    // system-only / assistant-first shape, move the system parts into a
    // leading user content instead of sending an invalid request; otherwise
    // keep the authored system role in `systemInstruction`.
    let system: GmContent? = null;
    let first_role = contents.at(0)?.role ?? "";
    if (first_role != "user" && lowered.system_parts.length() > 0) {
        contents = [GmContent { role: "user", parts: lowered.system_parts }].concat(contents);
        null
    } else if (lowered.system_parts.length() > 0) {
        system = GmContent { role: null, parts: lowered.system_parts };
        null
    } else {
        null
    };

    // Gemini answers `contents: []` with an HTTP 400. Say so locally instead,
    // the way the Anthropic (`anthropic_messages_request`) and Bedrock
    // (`converse_request`) clients do for the same shape.
    if (contents.length() == 0) {
        throw ai.errors.InvalidRequest {
            provider: "google",
            status_code: null,
            detail: "the rendered prompt and journal produced no message content",
            raw_body: null,
        }
    }

    let tools: GmTool[]? = null;
    let tool_config: GmToolConfig? = null;
    if (!input.toolbox.is_empty()) {
        let declarations: GmFunctionDeclaration[] = [];
        for (let t in input.toolbox.list()) {
            declarations.push(
                GmFunctionDeclaration {
                    name: t.name,
                    description: t.description,
                    parametersJsonSchema: t.input_schema,
                },
            );
        }
        tools = [GmTool { functionDeclarations: declarations }];
        tool_config
            = GmToolConfig { functionCallingConfig: GmFunctionCallingConfig { mode: "AUTO" } };
        null
    } else {
        null
    };

    let request = GmGenerateContentRequest {
        contents: contents,
        systemInstruction: system,
        tools: tools,
        toolConfig: tool_config,
        generationConfig: generation_config,
        safetySettings: safety_settings,
    };
    let base = _gm_strip_nulls(baml.json.to_json(request), false);
    baml.json.stringify(ai.wire.merge_request_body(base, request_body))
}

// The RPC verb for one call: `:generateContent`, or the SSE streaming variant.
function gemini_method(stream: bool) -> string {
    if (stream) {
        ":streamGenerateContent?alt=sse"
    } else {
        ":generateContent"
    }
}

// ---------------------------------------------------------------------------
// Response parsing — shared by both clients.

// One base64 payload from an `inlineData` response part -> a media block. The
// media kind follows the MIME type; `ai.content.Media.new` is the only
// supported way to build one (a primitive media value is not assignable to a
// baml.media.* field).
function _gm_output_media(mime: string, data: string) -> ai.content.Media {
    let value: image | audio | video | pdf = if (mime.starts_with("audio/")) {
        baml.media.Audio.from_base64(data, mime)
    } else if (mime.starts_with("video/")) {
        baml.media.Video.from_base64(data, mime)
    } else if (mime == "application/pdf") {
        baml.media.Pdf.from_base64(data, mime)
    } else {
        baml.media.Image.from_base64(data, mime)
    };
    ai.content.Media.new(value)
}

// The finish reasons that mean "the model refused / was blocked" rather than
// "the model finished". Everything else is Complete (an unknown reason must
// never masquerade as a refusal).
function _gm_is_blocked(finish: string) -> bool {
    finish == "SAFETY"
        || finish == "RECITATION"
        || finish == "PROHIBITED_CONTENT"
        || finish == "BLOCKLIST"
        || finish == "SPII"
        || finish == "IMAGE_SAFETY"
}

function _gm_usage(usage: GmUsageMetadata?) -> ai.events.Usage? {
    if let um: GmUsageMetadata = usage {
        ai.events.Usage {
            input_tokens: um.promptTokenCount ?? 0,
            // Gemini reports thinking tokens separately from candidate tokens,
            // so output_tokens is the answer's cost and reasoning_tokens the
            // thinking on top of it.
            output_tokens: um.candidatesTokenCount ?? 0,
            cached_input_tokens: um.cachedContentTokenCount,
            reasoning_tokens: um.thoughtsTokenCount,
        }
    } else {
        null
    }
}

// One decoded response -> canonical content blocks and a stop reason.
// Gemini sends no call ids; synthesize `call_<seq>_<i>` from the journal
// position and the call's index within the turn (plus the `thoughtSignature`
// the call must be replayed with — see `_gm_encode_call_id`).
//
// A document that carries no turn at all (`{}` from a broken proxy, an
// `{"error": …}` body served with a 200) is a FAILURE, not an empty answer:
// silently returning an empty Complete turn makes a `-> string` function
// return "" as a success. `provider` names the backend in that failure.
function google_parse(resp: GmResponse, seq: int, provider: string = "google") -> ai.ModelTurn {
    // An error document that reached this far was served with a 2xx status
    // (`ai.wire.send_as` classifies the non-2xx case). Classify it the same
    // way rather than reading a turn out of a body that has none.
    if let err: GmError = resp.error {
        throw ai.errors.classify_http(
            provider,
            err.code ?? 0,
            err.message ?? baml.json.stringify(baml.json.to_json(resp)),
        )
    }
    let content: ai.content.Block[] = [];
    let has_call = false;
    let call_index = 0;
    let cand: GmCandidate? = resp.candidates?.at(0);
    let finish = cand?.finishReason ?? "";
    let parts: GmRespPart[] = cand?.content?.parts ?? [];
    for (let p in parts) {
        if let fc: GmRespFunctionCall = p.functionCall {
            let args: map<string, unknown> = {};
            if let a: map<string, unknown> = fc.args {
                args = a;
            }
            content.push(
                ai.content.ToolUse {
                    id: _gm_encode_call_id(`call_${seq}_${call_index}`, p.thoughtSignature),
                    name: fc.name,
                    args: args,
                },
            );
            call_index += 1;
            has_call = true;
            null
        } else if let inline: GmRespInlineData = p.inlineData {
            // Image-out models (gemini-*-image) answer with base64 media parts.
            content.push(_gm_output_media(inline.mimeType ?? "image/png", inline.data ?? ""));
            null
        } else {
            if let t: string = p.text {
                if (p.thought ?? false) {
                    content.push(ai.content.Reasoning { summary: t });
                    null
                } else {
                    content.push(ai.content.Text { text: t });
                    null
                }
            } else {
                null
            }
        };
    }
    let blocked = resp.promptFeedback?.blockReason != null;
    // Nothing decoded, nothing blocked, and no finish reason to explain it:
    // the response is not a turn. `{}`, `{"candidates":[]}` and a candidate
    // with no content all land here. A finishReason (STOP with empty parts,
    // MAX_TOKENS spent entirely on thinking) IS the provider explaining
    // itself, so those still return an empty turn.
    if (content.length() == 0 && finish == "" && !blocked) {
        throw ai.errors.ParseFailed {
            provider: provider,
            raw_output: baml.json.stringify(baml.json.to_json(resp)),
        }
    }
    let stop = if (has_call) {
        ai.content.StopReason.ToolUse
    } else if (finish == "MAX_TOKENS") {
        ai.content.StopReason.MaxTokens
    } else if (blocked || _gm_is_blocked(finish)) {
        ai.content.StopReason.Refused
    } else {
        ai.content.StopReason.Complete
    };
    ai.ModelTurn { calls: [], content: content, stop_reason: stop, usage: _gm_usage(resp.usageMetadata) }
}

// ---------------------------------------------------------------------------
// Transport helpers — shared by both clients.

// `baml.http.fetch_sse` rejects a non-2xx response by throwing an
// `baml.errors.Io` whose message embeds the status and the body
// (sys_native/src/io_impls.rs). Recover that status so an SSE failure lands in
// the SAME taxonomy as a unary one: a 400 must NOT become a retry-safe
// NetworkFailure, or a Retry policy would replay an invalid request forever.
function _gm_sse_failure(provider: string, error: unknown) -> ai.errors.Failure {
    // `baml.errors.Io.message` is the raw text; anything else is rendered.
    // Redacted because Vertex express mode signs the request with `?key=…` in
    // the URL, and the rendered transport error can carry the URL.
    let text = ai.wire.redact_url_secrets(
        match (error) {
            let io: baml.errors.Io => io.message,
            _ => error.to_string(),
        },
    );
    let marker = "failed with status ";
    if let start: int = text.index_of(marker) {
        let rest = text.slice(start + marker.length(), text.length());
        let head = rest.split(":").at(0) ?? "";
        let status = baml.Int.parse(head.trim()) catch_all (e) {
            _ => 0,
        };
        if (status > 0) {
            let body = if let colon: int = rest.index_of(":") {
                rest.slice(colon + 1, rest.length()).trim()
            } else {
                ""
            };
            return ai.errors.classify_http(provider, status, body);
        }
    }
    ai.errors.NetworkFailure { status_code: null,
        provider: provider,
        detail: `transport failed before a response arrived: ${text}`,
        raw_body: null,
    }
}

// Route failures from the shared Google / Vertex stream opener. Timeout and
// cancellation keep their typed control-flow meaning; only transport errors
// enter the provider failure taxonomy.
function _gm_route_sse_open(
    provider: string,
    open: () -> baml.http.SseStream throws unknown,
) -> baml.http.SseStream {
    open() catch_all (e) {
        let timeout: baml.errors.Timeout => throw timeout,
        let cancelled: baml.panics.Cancelled => throw cancelled,
        _ => throw _gm_sse_failure(provider, e),
    }
}

// Open the SSE connection, classifying the failure precisely.
function _gm_open_sse(
    req: baml.http.Request,
    provider: string,
    request_timeout_ms: int? = null,
    time_to_first_token_timeout_ms: int? = null,
) -> baml.http.SseStream {
    _gm_route_sse_open(provider, () -> {
        baml.http.fetch_sse(
            req,
            timeout = baml.time.Duration.from_milliseconds(request_timeout_ms ?? 0),
            first_event_timeout = baml.time.Duration.from_milliseconds(time_to_first_token_timeout_ms ?? 0),
        )
    })
}

// One SseStream batch -> provider-agnostic stream events.
//
// Wire shape: each SSE `data:` payload is a GmResponse fragment; text deltas
// ride in candidates[0].content.parts[].text, the last fragment carries
// finishReason, and usageMetadata (cumulative) appears on fragments.
//
// Termination: Gemini's SSE has no terminal sentinel — the fragment carrying
// `finishReason` IS the end of the turn, and the connection then closes. That
// fragment is decoded as `ai.stream.TurnDone`, which lets both stream call
// sites pass `require_terminal`: a connection cut mid-answer then surfaces as
// an `ai.errors.NetworkFailure` instead of a silently truncated Complete turn.
//
// A `data:` payload that is not decodable is a failure too. Dropping it (what
// this used to do) turns a cut or a proxy's error page into missing text that
// no caller can see.
function _gm_decode_batch(batch: string, provider: string) -> ai.stream.StreamEvent[] {
    let out: ai.stream.StreamEvent[] = [];
    for (let raw in ai.stream.decode_sse_batch(batch)) {
        let payload = (raw.data ?? "").trim();
        // A comment/keep-alive carries no data; `[DONE]` is not Gemini's
        // spelling but a proxy in front of it may add one.
        if (payload == "" || payload == "[DONE]") {
            null
        } else {
            let resp: GmResponse = baml.json.from_string<GmResponse>(payload) catch_all (e) {
                _ => {
                    throw ai.errors.ParseFailed { provider: provider, raw_output: payload }
                },
            };
            // A mid-stream `{"error": …}` payload is a real failure: raise
            // it instead of ending the turn silently on whatever text
            // arrived first.
            if let err: GmError = resp.error {
                throw ai.errors.classify_http(provider, err.code ?? 0, err.message ?? payload)
            }
            let cand: GmCandidate? = resp.candidates?.at(0);
            let parts: GmRespPart[] = cand?.content?.parts ?? [];
            for (let p in parts) {
                if let t: string = p.text {
                    if (p.thought ?? false) {
                        null
                    } else {
                        out.push(ai.stream.TextDelta { text: t });
                        null
                    }
                } else {
                    null
                };
            }
            let finish = cand?.finishReason ?? "";
            let blocked = resp.promptFeedback?.blockReason != null;
            let stop: ai.content.StopReason? = if (finish == "MAX_TOKENS") {
                ai.content.StopReason.MaxTokens
            } else if (blocked || _gm_is_blocked(finish)) {
                ai.content.StopReason.Refused
            } else if (finish != "") {
                ai.content.StopReason.Complete
            } else {
                null
            };
            let usage: GmUsageMetadata? = resp.usageMetadata;
            if (stop != null || usage != null) {
                out.push(
                    ai.stream.TurnMeta {
                        stop_reason: stop,
                        input_tokens: usage?.promptTokenCount,
                        output_tokens: usage?.candidatesTokenCount,
                    },
                );
                null
            } else {
                null
            };
            // The stop reason arrived: this fragment ends the turn. Emitted
            // AFTER the TurnMeta above so the folded stop reason and the
            // cumulative usage on the same fragment are both seen first.
            if (stop != null) {
                out.push(ai.stream.TurnDone {  });
                null
            } else {
                null
            };
            null
        };
    }
    out
}

// ---------------------------------------------------------------------------
// root.GoogleClient — the Gemini Developer API backend.

// ai.ModelTurnInput -> one generateContent request.
function google_render(c: root.GoogleClient, input: ai.ModelTurnInput) -> baml.http.Request {
    _google_request(c, input, false, preview = false)
}

/// The request one `render` would send. URL media unsupported by the Gemini
/// Developer API is rejected before any download is attempted.
function google_preview(c: root.GoogleClient, input: ai.ModelTurnInput) -> baml.http.Request {
    _google_request(c, input, false, preview = true)
}

// The shared request builder: identical body for both paths; streaming swaps
// the RPC verb for `:streamGenerateContent?alt=sse`.
function _google_request(
    c: root.GoogleClient,
    input: ai.ModelTurnInput,
    stream: bool,
    preview: bool = false,
) -> baml.http.Request {
    let body = gemini_build_body(
        input,
        c.id(),
        _gm_generation_config(
            c.temperature,
            c.max_output_tokens,
            c.top_p,
            c.top_k,
            c.stop_sequences,
            c.response_modalities,
        ),
        c.safety_settings,
        c.request_body,
        preview = preview,
    );
    let base = c.resolved_base_url() ?? "https://generativelanguage.googleapis.com/v1beta";
    let url = _gm_apply_query(`${base}/models/${c.model}${gemini_method(stream)}`, c.query_params);
    let headers: map<string, string> = { "content-type": "application/json" };
    let with_user = _gm_apply_headers(headers, c.headers);
    if (!preview || with_user.get("x-goog-api-key") == null) {
        let api_key = if (preview) {
            "<PREVIEW>"
        } else {
            c.resolved_api_key()
        };
        let _ = with_user.set("x-goog-api-key", api_key);
    }
    baml.http.Request { method: "POST", url: url, headers: with_user, body: body }
}

// The one internal entry point: render, send, normalize — one model turn.
function gemini_invoke(c: root.GoogleClient, input: ai.ModelTurnInput) -> ai.ModelTurn {
    // ai.Journal position before this turn commits; the ai.AssistantMessage
    // the runner appends will land at this index, so ids synthesized
    // from it are stable across re-renders.
    let seq = input.journal.entries().length();
    let req = google_render(c, input);
    let sent = ai.wire.send_as_with_wire<GmResponse>(
        req,
        "google",
        c.id(),
        capture_wire = c.capture_wire,
    );
    let turn = google_parse(sent.value, seq);
    ai.ModelTurn { ...turn, calls: [sent.call] }
}

function _google_decode_batch(batch: string) -> ai.stream.StreamEvent[] {
    _gm_decode_batch(batch, "google")
}

// The streaming entry point: the same request body as `gemini_invoke` against
// the `:streamGenerateContent?alt=sse` verb, wrapped in an ai.TurnStream.
function gemini_invoke_stream(
    c: root.GoogleClient,
    input: ai.ModelTurnInput,
) -> ai.stream.TurnStream {
    let req = _google_request(c, input, true);
    let started = baml.time.Instant.now();
    let sse = _gm_open_sse(req, "google");
    // Strict termination: the `finishReason` fragment decodes as TurnDone, so
    // a connection that closes before it is a truncated turn, not an answer.
    let stream = ai.stream.TurnStream.from_sse(
        sse,
        (batch: string) -> {
            _google_decode_batch(batch) catch_all (e) {
                _ => throw ai.errors.normalize(e),
            }
        },
        require_terminal = "google",
    );
    let call = ai.events.LLMCall {
        client_name: c.id(),
        provider: "google",
        timing: ai.events.Timing {
            start_time_utc_ms: started.to_timestamp_milliseconds(),
            duration_ms: null,
            ttft_ms: null,
        },
        http_request: if (c.capture_wire) {
            baml.http.Request {
                method: req.method,
                url: ai.wire.redact_url_secrets(req.url),
                headers: ai.wire.redact_request_headers(req.headers),
                body: req.body,
            }
        } else {
            null
        },
        http_response: ai.events.HttpResponse {
            status: sse.status_code,
            headers: sse.headers,
            body: null,
        },
        usage: null,
        selected: true,
        sse_responses: if (c.capture_wire) {
            []
        } else {
            null
        },
    };
    stream._calls = [call];
    stream._wire_call = call;
    stream._call_started = started;
    stream._ttft_from = baml.time.Instant.now();
    stream._capture_sse = c.capture_wire;
    stream
}
