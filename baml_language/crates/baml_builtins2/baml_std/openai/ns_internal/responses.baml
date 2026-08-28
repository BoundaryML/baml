// root.ResponsesClient — one model turn against the OpenAI Responses API.
//
// The wire is modeled as typed BAML classes on BOTH sides (PLAN §1.5): a typed
// request (`OaResponsesRequest` and its item/part/tool unions), typed response
// envelopes whose fields are optional wherever the API may omit them, and typed
// SSE event classes. `baml.json.json` appears only where the shape is genuinely
// the caller's: a tool's `parameters` (an arbitrary JSON Schema), the
// `request_body` passthrough, and the per-message directives carried verbatim
// from `ai.PromptMessage.metadata`. All three are applied to the SERIALIZED
// body, after the typed layer's null-prune, so none of them is rewritten.
//
// Output mode is SAP (the schema rides in the prompt text, tools are native);
// `text.format` is deliberately never set — see the research report's G15.

// ---------------------------------------------------------------------------
// Request — content parts.
//
// Role decides the text part's type: assistant content is `output_text`,
// everything else is `input_text`. Media is user-only, and each kind has its
// own part shape (`input_audio` nests `data`/`format` under an `input_audio`
// object — the Responses API's own shape, which is what the engine sends and
// what sys_llm got wrong by flattening them onto the part).

class OaInputText {
    type: string,
    text: string,
}

class OaOutputText {
    type: string,
    text: string,
}

class OaInputImage {
    type: string,
    image_url: string?,
    file_id: string?,
    // "low" | "high" | "auto" | "original"; the API's own default is "auto".
    detail: string?,
}

class OaAudioPayload {
    data: string,
    format: string,
}

class OaInputAudio {
    type: string,
    input_audio: OaAudioPayload,
}

class OaInputFile {
    type: string,
    filename: string?,
    file_data: string?,
    file_url: string?,
    file_id: string?,
}

type OaInputPart = OaInputText | OaOutputText | OaInputImage | OaInputAudio | OaInputFile;

// ---------------------------------------------------------------------------
// Request — input items. The `input` array mixes chat messages with the
// function-call replay items the journal lowers to.

class OaMessageItem {
    role: string,
    content: OaInputPart[],
}

class OaFunctionCallItem {
    type: string,
    call_id: string,
    name: string,
    arguments: string,
}

class OaFunctionCallOutputItem {
    type: string,
    call_id: string,
    output: string,
}

type OaInputItem = OaMessageItem | OaFunctionCallItem | OaFunctionCallOutputItem;

// ---------------------------------------------------------------------------
// Request — tools and parameters.

class OaFunctionTool {
    type: string,
    name: string,
    description: string,
    // An arbitrary JSON Schema object: the one genuinely open-ended field.
    parameters: baml.json.json,
    strict: bool,
}

// A provider-side tool the request only names, e.g. `{"type":"image_generation"}`.
class OaBuiltinTool {
    type: string,
}

type OaTool = OaFunctionTool | OaBuiltinTool;

// `tool_choice` is either a mode string ("auto"/"none"/"required") or an object
// naming one tool.
class OaToolChoiceObject {
    type: string,
}

class OaReasoningParams {
    effort: string?,
    summary: string?,
}

// `text` carries `verbosity` only: `text.format` stays unset by design (G15).
class OaTextParams {
    verbosity: string?,
}

class OaResponsesRequest {
    model: string,
    input: OaInputItem[],
    store: bool,
    stream: bool?,
    temperature: float?,
    top_p: float?,
    max_output_tokens: int?,
    reasoning: OaReasoningParams?,
    text: OaTextParams?,
    parallel_tool_calls: bool?,
    tools: OaTool[]?,
    tool_choice: (string | OaToolChoiceObject)?,
}

// ---------------------------------------------------------------------------
// Response envelopes. `from_string` ignores unknown fields, and every field the
// API may omit is optional — a missing `usage.input_tokens` must not turn a
// good response into ai.errors.ParseFailed.

class OaContentPart {
    type: string?,
    text: string?,
    refusal: string?,
}

class OaOutputItem {
    type: string,
    id: string?,
    status: string?,
    // function_call
    call_id: string?,
    name: string?,
    arguments: string?,
    // message / reasoning
    content: OaContentPart[]?,
    summary: OaContentPart[]?,
    // image_generation_call
    result: string?,
    revised_prompt: string?,
    output_format: string?,
}

class OaInputTokenDetails {
    cached_tokens: int?,
}

class OaOutputTokenDetails {
    reasoning_tokens: int?,
}

class OaUsage {
    input_tokens: int?,
    output_tokens: int?,
    total_tokens: int?,
    input_tokens_details: OaInputTokenDetails?,
    output_tokens_details: OaOutputTokenDetails?,
}

class OaIncompleteDetails {
    reason: string?,
}

class OaErrorBody {
    code: string?,
    message: string?,
    // OpenAI's error category (e.g. "insufficient_quota"); on billing errors
    // the specific `code` ("credit_balance_exhausted") only makes sense
    // together with this.
    type: string?,
}

class OaResponse {
    id: string?,
    model: string?,
    status: string?,
    output: OaOutputItem[]?,
    usage: OaUsage?,
    incomplete_details: OaIncompleteDetails?,
    error: OaErrorBody?,
}

// ---------------------------------------------------------------------------
// Serialization helpers.

// Drop every null-valued map entry, recursively. The typed request classes
// declare optional fields for everything the provider treats as absent, and
// `to_json` renders an unset optional as an explicit null — which is not the
// same request. This runs on the serialized typed request only, BEFORE the
// user's `request_body` merge (where a null legitimately means "delete").
//
// It is a TYPED-LAYER pass and must never be the last word on an open-ended
// subtree: a tool's `parameters` is the caller's own JSON Schema, where
// `"default": null` / `"const": null` are legitimate members rather than unset
// optionals. Those subtrees are written back verbatim afterwards by
// `_openai_restore_tool_schemas`; `request_body` is merged after the prune and
// never sees it at all.
function _openai_prune_nulls(value: baml.json.json) -> baml.json.json {
    match (value) {
        let object: map<string, baml.json.json> => {
            let out: map<string, baml.json.json> = {};
            for (let key in object.keys()) {
                let entry = object.get(key);
                match (entry) {
                    null => null,
                    _ => {
                        let _ = out.set(key, _openai_prune_nulls(entry));
                        null
                    },
                };
            }
            out
        },
        let items: baml.json.json[] => {
            let out: baml.json.json[] = [];
            for (let item in items) {
                out.push(_openai_prune_nulls(item));
            }
            out
        },
        _ => value,
    }
}

// Put the caller's tool schemas back over the pruned body, verbatim. The
// pruned `tools` array is positionally identical to the typed one (prune drops
// map ENTRIES, never array elements), so tool i's schema goes back at index i.
// Only `OaFunctionTool` carries a schema; a builtin tool has nothing to restore.
function _openai_restore_tool_schemas(body: baml.json.json, tools: OaTool[]?) -> baml.json.json {
    let list: OaTool[] = tools ?? [];
    if (list.length() == 0) {
        return body;
    }
    match (body) {
        let object: map<string, baml.json.json> => {
            match (object.get("tools")) {
                let wire: baml.json.json[] => {
                    let index = 0;
                    for (let tool in list) {
                        match (tool) {
                            let fn: OaFunctionTool => {
                                match (wire.at(index)) {
                                    let entry: map<string, baml.json.json> => {
                                        let _ = entry.set("parameters", fn.parameters);
                                        null
                                    },
                                    _ => null,
                                };
                                null
                            },
                            _ => null,
                        };
                        index += 1;
                    }
                    null
                },
                _ => null,
            };
            null
        },
        _ => null,
    };
    body
}

// Percent-encode one query-string component (RFC 3986 unreserved set kept).
function _openai_percent_encode(raw: string) -> string {
    let out = "";
    for (let byte in raw.to_utf8().to_array()) {
        let unreserved = (byte >= 48 && byte <= 57)
            || (byte >= 65 && byte <= 90)
            || (byte >= 97 && byte <= 122)
            || byte == 45
            || byte == 46
            || byte == 95
            || byte == 126;
        if (unreserved) {
            out += baml.String.from_code_points([byte]);
        } else {
            out += "%" + baml.Uint8Array.from_array([byte]).to_hex().to_upper_case();
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Prompt lowering.

// The filename an `input_file` part carries. The API wants one alongside
// `file_data`, and the basename of the source is more useful than a constant.
function _openai_file_name(source: string?) -> string {
    if let raw: string = source {
        let without_query = raw.split("?").at(0) ?? raw;
        let segments = without_query.split("/");
        let last = segments.at(segments.length() - 1) ?? "";
        if (last.trim() != "") {
            return last.trim();
        }
    };
    "document.pdf"
}

// The Responses API's `input_audio.format`. mp3 and wav are the documented
// formats; the remaining mappings are carried over from sys_llm so a provider
// that widens the set keeps working.
function _openai_audio_format(mime_type: string) -> string {
    match (mime_type) {
        "audio/wav" | "audio/x-wav" => "wav",
        "audio/mpeg" | "audio/mp3" => "mp3",
        "audio/flac" | "audio/x-flac" => "flac",
        "audio/ogg" => "ogg",
        "audio/webm" => "webm",
        _ => mime_type,
    }
}

function _openai_media_role_error(kind: string, role: string) -> ai.errors.Failure {
    let detail = `OpenAI Responses only supports ${kind} input in user messages, found ${role}`;
    ai.errors.InvalidRequest {
        provider: "openai",
        status_code: null,
        detail: detail,
        raw_body: null,
    }
}

// A rendered structural prompt message becomes typed Responses content parts.
function _openai_prompt_content(
    message: ai.PromptMessage,
    role: string,
    preview: bool = false,
) -> OaInputPart[] {
    let content: OaInputPart[] = [];
    for (let part in message.parts) {
        match (part) {
            let text: string => {
                if (role == "assistant") {
                    content.push(OaOutputText { type: "output_text", text: text });
                    null
                } else {
                    content.push(OaInputText { type: "input_text", text: text });
                    null
                };
                null
            },
            let image: baml.media.Image => {
                // The legacy OpenAI Responses lowering accepted image parts in
                // the system message produced for a role-less prompt. Keep
                // that compatibility while still rejecting assistant/tool
                // authored media, which has no valid Responses input shape.
                if (role != "user" && role != "system") {
                    throw _openai_media_role_error("image", role)
                }
                let resolved = if (preview) {
                    ai.wire.resolve_media_preview(image)
                } else {
                    ai.wire.resolve_media(image, false)
                };
                let image_url = resolved.url ?? `data:${resolved.mime_type};base64,${resolved.base64 ?? ""}`;
                content.push(
                    OaInputImage {
                        type: "input_image",
                        image_url: image_url,
                        file_id: null,
                        detail: "auto",
                    },
                );
                null
            },
            let audio: baml.media.Audio => {
                if (role != "user") {
                    throw _openai_media_role_error("audio", role)
                }
                // audio has no URL form on the wire: always inline base64.
                let resolved = if (preview) {
                    ai.wire.resolve_media_preview(audio)
                } else {
                    ai.wire.resolve_media(audio, true)
                };
                if (preview && resolved.url != null) {
                    throw ai.errors.PreviewUnsupported {
                        provider: "openai",
                        detail: "OpenAI Responses audio input requires downloaded bytes during request preview",
                    }
                }
                content.push(
                    OaInputAudio {
                        type: "input_audio",
                        input_audio: OaAudioPayload {
                            data: resolved.base64 ?? "",
                            format: _openai_audio_format(resolved.mime_type),
                        },
                    },
                );
                null
            },
            let pdf: baml.media.Pdf => {
                if (role != "user") {
                    throw _openai_media_role_error("PDF", role)
                }
                let resolved = if (preview) {
                    ai.wire.resolve_media_preview(pdf)
                } else {
                    ai.wire.resolve_media(pdf, false)
                };
                let name = _openai_file_name(pdf.file() ?? pdf.url());
                if let url: string = resolved.url {
                    content.push(
                        OaInputFile {
                            type: "input_file",
                            filename: name,
                            file_data: null,
                            file_url: url,
                            file_id: null,
                        },
                    );
                    null
                } else {
                    content.push(
                        OaInputFile {
                            type: "input_file",
                            filename: name,
                            file_data: `data:${resolved.mime_type};base64,${resolved.base64 ?? ""}`,
                            file_url: null,
                            file_id: null,
                        },
                    );
                    null
                };
                null
            },
            let video: baml.media.Video => {
                throw ai.errors.InvalidRequest {
                    provider: "openai",
                    status_code: null,
                    detail: "OpenAI Responses does not support video prompt input",
                    raw_body: null,
                }
            },
        };
    }
    content
}

// Role-less content uses the ordinary user role; `tool` is not a valid
// free-standing Responses message role, so authored tool context is user
// content (actual tool outputs come from the journal path below).
function openai_lower_prompt(prompt: ai.Prompt, preview: bool = false) -> OaInputItem[] {
    let items: OaInputItem[] = [];
    for (let message in prompt.messages()) {
        let role = match (message.role) {
            "" | "tool" => "user",
            _ => message.role,
        };
        items.push(
            OaMessageItem {
                role: role,
                content: _openai_prompt_content(message, role, preview = preview),
            },
        );
    }
    items
}

// Per-message provider directives (`ai.PromptMessage.metadata`, authored as
// `${ai.Role { name: "user", metadata: { … } }}`), in prompt order and
// aligned one-to-one with the items `openai_lower_prompt` produced — the prompt
// items are always the FIRST items of `input`, the journal's are appended after.
//
// The Responses API has no per-message directive slot, so PLAN §5's accepted
// placement for this provider is the CONTENT PART: the keys are forwarded
// verbatim onto the LAST content item of the message they were authored on
// (the same "everything up to and including this block" rule the engine used).
// Applied to the SERIALIZED body, after the null-prune, because a directive is
// a provider shape: an explicit null in it is the caller's, not an unset field.
function openai_prompt_metadata(prompt: ai.Prompt) -> map<string, baml.json.json>[] {
    let out: map<string, baml.json.json>[] = [];
    for (let message in prompt.messages()) {
        out.push(message.metadata);
    }
    out
}

function _openai_merge_onto_last_part(
    item: baml.json.json,
    directives: map<string, baml.json.json>,
) -> void {
    match (item) {
        let message: map<string, baml.json.json> => {
            match (message.get("content")) {
                let parts: baml.json.json[] => {
                    match (parts.at(parts.length() - 1)) {
                        let part: map<string, baml.json.json> => {
                            for (let key in directives.keys()) {
                                let _ = part.set(key, directives.get(key));
                            }
                            null
                        },
                        _ => null,
                    }
                },
                _ => null,
            }
        },
        _ => null,
    }
}

function _openai_apply_message_metadata(
    body: baml.json.json,
    metadata: map<string, baml.json.json>[],
) -> baml.json.json {
    match (body) {
        let object: map<string, baml.json.json> => {
            match (object.get("input")) {
                let items: baml.json.json[] => {
                    let index = 0;
                    for (let directives in metadata) {
                        if (directives.keys().length() > 0) {
                            _openai_merge_onto_last_part(items.at(index), directives);
                        }
                        index += 1;
                    }
                    null
                },
                _ => null,
            };
            null
        },
        _ => null,
    };
    body
}

// ai.Journal -> Responses API input items. Only promptable events lower;
// ai.RunStarted, ai.ToolRequested, ai.Usage, and ai.FinalProduced are
// journal-only. Callers pass the SANITIZED journal (see _openai_request).
function openai_lower_journal(j: ai.Journal) -> OaInputItem[] {
    let items: OaInputItem[] = [];
    for (let e in j.entries()) {
        match (e) {
            let u: ai.events.UserMessage => {
                items.push(
                    OaMessageItem {
                        role: "user",
                        content: [OaInputText { type: "input_text", text: u.content }],
                    },
                );
                null
            },
            let a: ai.events.AssistantMessage => {
                for (let b in a.content) {
                    match (b) {
                        let t: ai.content.Text => {
                            items.push(
                                OaMessageItem {
                                    role: "assistant",
                                    content: [OaOutputText { type: "output_text", text: t.text }],
                                },
                            );
                            null
                        },
                        let use: ai.content.ToolUse => {
                            items.push(
                                OaFunctionCallItem {
                                    type: "function_call",
                                    call_id: use.id,
                                    name: use.name,
                                    arguments: baml.json.stringify(baml.json.to_json(use.args)),
                                },
                            );
                            null
                        },
                        // ai.Reasoning does not round-trip yet (the API wants
                        // the encrypted item back, which ai.content.Reasoning
                        // does not carry), and a generated ai.content.Media is
                        // an output, not replayable input.
                        _ => null,
                    };
                }
                null
            },
            let c: ai.events.ToolCompleted => {
                items.push(
                    OaFunctionCallOutputItem {
                        type: "function_call_output",
                        call_id: c.id,
                        output: c.output,
                    },
                );
                null
            },
            let f: ai.events.ToolFailed => {
                let err: map<string, unknown> = {};
                let _ = err.set("error", f.message);
                items.push(
                    OaFunctionCallOutputItem {
                        type: "function_call_output",
                        call_id: f.id,
                        output: baml.json.stringify(baml.json.to_json(err)),
                    },
                );
                null
            },
            _ => null,
        };
    }
    items
}

// ---------------------------------------------------------------------------
// Request assembly.

// Does the declared output type ask for images? This is the port of sys_llm's
// `image_generation_mode` (lib.rs:465-508): every media shape that can hold an
// `image` enables the provider-side image_generation tool. sys_llm set
// `tool_choice` for both Required and Available, so the two modes only ever
// differed in that one is unreachable — one predicate is enough.
function _openai_wants_image(output_type: reflect.Type) -> bool {
    ai.internal._media_shape(output_type).ends_with(":image")
}

function _openai_tools(toolbox: ai.tools.Toolbox, wants_image: bool) -> OaTool[]? {
    let tools: OaTool[] = [];
    for (let t in toolbox.list()) {
        tools.push(
            OaFunctionTool {
                type: "function",
                name: t.name,
                description: t.description,
                parameters: baml.json.to_json(t.input_schema),
                strict: false,
            },
        );
    }
    if (wants_image) {
        tools.push(OaBuiltinTool { type: "image_generation" });
    }
    if (tools.length() == 0) {
        null
    } else {
        tools
    }
}

function _openai_tool_choice(
    tools: OaTool[]?,
    wants_image: bool,
) -> (string | OaToolChoiceObject)? {
    if (wants_image) {
        return OaToolChoiceObject { type: "image_generation" };
    }
    match (tools) {
        null => null,
        _ => "auto",
    }
}

function _openai_reasoning(c: root.ResponsesClient) -> OaReasoningParams? {
    if (c.reasoning_effort == null && c.reasoning_summary == null) {
        null
    } else {
        OaReasoningParams { effort: c.reasoning_effort, summary: c.reasoning_summary }
    }
}

function _openai_text(c: root.ResponsesClient) -> OaTextParams? {
    if (c.verbosity == null) {
        null
    } else {
        OaTextParams { verbosity: c.verbosity }
    }
}

// content-type, then the caller's headers (lowercased), then authorization —
// credentials are the client's own business and always win.
function _openai_headers(c: root.ResponsesClient, preview: bool = false) -> map<string, string> {
    let headers: map<string, string> = { "content-type": "application/json" };
    if let extra: map<string, string> = c.headers {
        for (let name in extra.keys()) {
            let _ = headers.set(name.to_lower_case(), extra.get(name) ?? "");
        }
    };
    if (!preview) {
        let _ = headers.set("authorization", `Bearer ${c.resolved_api_key()}`);
    }
    headers
}

function _openai_url(c: root.ResponsesClient) -> string {
    let base = c.resolved_base_url() ?? "https://api.openai.com/v1";
    let trimmed = if (base.ends_with("/")) {
        base.slice(0, base.length() - 1)
    } else {
        base
    };
    let url = `${trimmed}/responses`;
    if let params: map<string, string> = c.query_params {
        let pairs: string[] = [];
        for (let key in params.keys()) {
            pairs.push(
                `${_openai_percent_encode(key)}=${_openai_percent_encode(params.get(key) ?? "")}`,
            );
        }
        if (pairs.length() > 0) {
            return `${url}?${pairs.join("&")}`;
        }
    };
    url
}

// ai.ModelTurnInput -> one Responses API request.
function openai_render(c: root.ResponsesClient, input: ai.ModelTurnInput) -> baml.http.Request {
    _openai_request(c, input, false, preview = false)
}

// The request one `render` would send. This path never reads local media or
// downloads URLs; provider-unsupported URL forms return PreviewUnsupported.
function openai_preview(c: root.ResponsesClient, input: ai.ModelTurnInput) -> baml.http.Request {
    _openai_request(c, input, false, preview = true)
}

// The shared request builder: identical body for both paths, plus the
// `stream: true` flag on the streaming one.
function _openai_request(
    c: root.ResponsesClient,
    input: ai.ModelTurnInput,
    stream: bool,
    preview: bool = false,
) -> baml.http.Request {
    let prompt = input.prompt(ai.internal.build_output_format(input.output_type));
    let items = openai_lower_prompt(prompt, preview = preview);
    // sanitize BEFORE lowering: foreign reasoning, empty turns and orphaned
    // tool calls are everyone's problem, fixed once in ai.wire.
    let journal = ai.wire.sanitize_for_client(input.journal, c.id());
    for (let item in openai_lower_journal(journal)) {
        items.push(item);
    }
    let wants_image = _openai_wants_image(input.output_type);
    let tools = _openai_tools(input.toolbox, wants_image);
    let request = OaResponsesRequest {
        model: c.model,
        input: items,
        store: c.store,
        stream: if (stream) {
            true
        } else {
            null
        },
        temperature: c.temperature,
        top_p: c.top_p,
        max_output_tokens: c.max_output_tokens,
        reasoning: _openai_reasoning(c),
        text: _openai_text(c),
        parallel_tool_calls: c.parallel_tool_calls,
        tools: tools,
        tool_choice: _openai_tool_choice(tools, wants_image),
    };
    // typed request -> json -> drop unset optionals -> put the caller's own
    // open-ended JSON back (tool schemas, then the per-message directives that
    // ride on a content part) -> user passthrough LAST.
    let pruned = _openai_restore_tool_schemas(_openai_prune_nulls(baml.json.to_json(request)), tools);
    let body = ai.wire.merge_request_body(
        _openai_apply_message_metadata(pruned, openai_prompt_metadata(prompt)),
        c.request_body,
    );
    baml.http.Request {
        method: "POST",
        url: _openai_url(c),
        headers: _openai_headers(c, preview = preview),
        body: baml.json.stringify(body),
    }
}

// ---------------------------------------------------------------------------
// Response parsing.

// The mime type of an image_generation_call payload.
function _openai_image_mime(output_format: string?) -> string {
    let format = (output_format ?? "").trim().to_lower_case();
    match (format) {
        "jpeg" | "jpg" => "image/jpeg",
        "webp" => "image/webp",
        _ => "image/png",
    }
}

// Error codes that name a TRANSIENT provider-side failure. The Responses API's
// own `response.failed` example is `{"code":"server_error", …}` — a 500
// delivered inside a 200 body — and the same failure arriving as an HTTP status
// classifies as ai.errors.NetworkFailure (ai.errors.classify_http maps >=500
// there). Classifying the in-body form as InvalidRequest instead would make it
// permanently non-retryable under ai.Retry's default judgment, purely because
// of which envelope the provider chose.
function _openai_transient_code(code: string) -> bool {
    code.includes("server_error")
        || code.includes("timeout")
        || code.includes("timed_out")
        || code.includes("overloaded")
        || code.includes("unavailable")
        || code.includes("try_again")
}

// A provider-reported error becomes the precise ai.errors class, never a
// blanket NetworkFailure: a retry-safe classification on a 400-shaped failure
// is what makes a Retry policy replay an invalid request forever.
function _openai_error_failure(err: OaErrorBody) -> ai.errors.Failure {
    // Billing errors split the signal across `type` ("insufficient_quota")
    // and `code` ("credit_balance_exhausted"), so classify on both.
    let code = ((err.code ?? "") + " " + (err.type ?? "")).to_lower_case();
    let detail = err.message ?? "the OpenAI Responses request failed";
    let raw = baml.json.stringify(baml.json.to_json(err));
    if (code.includes("rate_limit") || code.includes("quota") || code.includes("credit_balance")) {
        ai.errors.RateLimited {
            status_code: null,
            provider: "openai",
            retry_after_ms: null,
            raw_body: raw,
        }
    } else if (code.includes("content_filter") || code.includes("moderation") || code.includes("refusal")) {
        ai.errors.Refused { provider: "openai", reason: detail, raw_body: raw }
    } else if (_openai_transient_code(code)) {
        ai.errors.NetworkFailure {
            status_code: null,
            provider: "openai",
            detail: detail,
            raw_body: raw,
        }
    } else {
        ai.errors.InvalidRequest {
            provider: "openai",
            status_code: null,
            detail: detail,
            raw_body: raw,
        }
    }
}

function _openai_response_failure(resp: OaResponse) -> ai.errors.Failure {
    match (resp.error) {
        let err: OaErrorBody => _openai_error_failure(err),
        null => {
            _openai_error_failure(
                OaErrorBody {
                    code: null,
                    message: `the response ended with status ${resp.status ?? "failed"}`,
                },
            )
        },
    }
}

// One decoded response -> canonical content blocks and a stop reason.
function openai_parse(resp: OaResponse) -> ai.ModelTurn {
    // A 200 whose response object failed (background mode, and every
    // `response.failed` stream event) is an error, not an empty turn.
    if ((resp.status ?? "") == "failed" || resp.error != null) {
        throw _openai_response_failure(resp)
    }
    let content: ai.content.Block[] = [];
    let has_call = false;
    let refused = false;
    let output: OaOutputItem[] = resp.output ?? [];
    for (let item in output) {
        match (item.type) {
            "function_call" => {
                has_call = true;
                let args: map<string, unknown> = {};
                if let s: string = item.arguments {
                    // Provider-malformed output, not a bug in the caller's code:
                    // the raw baml JsonParseError would escape as an untyped
                    // UnknownError, so it is classified like every other
                    // undecodable provider payload (ai.wire.send_as does the
                    // same for the response body).
                    args
                        = baml.json.from_string<map<string, unknown>>(s) catch_all (e) {
                            _ => throw ai.errors.ParseFailed { provider: "openai", raw_output: s },
                        };
                };
                content.push(
                    ai.content.ToolUse { id: item.call_id ?? "", name: item.name ?? "", args: args },
                );
                null
            },
            "message" => {
                let parts: OaContentPart[] = item.content ?? [];
                for (let part in parts) {
                    let kind = part.type ?? "";
                    match (kind) {
                        "output_text" => {
                            content.push(ai.content.Text { text: part.text ?? "" });
                            null
                        },
                        "refusal" => {
                            refused = true;
                            content.push(ai.content.Text { text: part.refusal ?? "" });
                            null
                        },
                        _ => null,
                    };
                }
                null
            },
            "reasoning" => {
                let parts: OaContentPart[] = item.summary ?? [];
                for (let part in parts) {
                    if let txt: string = part.text {
                        content.push(ai.content.Reasoning { summary: txt });
                        null
                    } else {
                        null
                    };
                }
                null
            },
            "image_generation_call" => {
                // The provider-side image tool already ran: its result is turn
                // content, NOT a tool call the runner has to execute.
                if let encoded: string = item.result {
                    let mime = _openai_image_mime(item.output_format);
                    content.push(
                        ai.content.Media.new(
                            baml.media.Image.from_base64(encoded, mime),
                            provider_id = item.id,
                            revised_prompt = item.revised_prompt,
                        ),
                    );
                    null
                } else {
                    null
                };
                null
            },
            _ => null,
        };
    }
    let stop = if (has_call) {
        ai.content.StopReason.ToolUse
    } else if (refused) {
        ai.content.StopReason.Refused
    } else if ((resp.status ?? "") == "incomplete") {
        if ((resp.incomplete_details?.reason ?? "") == "content_filter") {
            ai.content.StopReason.Refused
        } else {
            ai.content.StopReason.MaxTokens
        }
    } else {
        ai.content.StopReason.Complete
    };
    let usage: ai.events.Usage? = if let u: OaUsage = resp.usage {
        ai.events.Usage {
            input_tokens: u.input_tokens ?? 0,
            output_tokens: u.output_tokens ?? 0,
            cached_input_tokens: u.input_tokens_details?.cached_tokens,
            reasoning_tokens: u.output_tokens_details?.reasoning_tokens,
        }
    } else {
        null
    };
    ai.ModelTurn { calls: [], content: content, stop_reason: stop, usage: usage }
}

// The one internal entry point: render, send, normalize — one model turn.
function invoke(c: root.ResponsesClient, input: ai.ModelTurnInput) -> ai.ModelTurn {
    let req = openai_render(c, input);
    let sent = ai.wire.send_as_with_wire<OaResponse>(
        req,
        "openai",
        c.id(),
        capture_wire = c.capture_wire,
        request_timeout_ms = c.request_timeout_ms,
    );
    let turn = openai_parse(sent.value);
    ai.ModelTurn { ...turn, calls: [sent.call] }
}

// ---------------------------------------------------------------------------
// Streaming (Responses API SSE).
//
// Every event carries `type`; the terminal ones carry the whole response object,
// which yields the stop reason and usage through the same openai_parse as the
// non-streaming path. `response.failed` and a bare `error` event THROW the
// classified failure instead of ending the turn.

class OaStreamEvent {
    type: string?,
    // output_text.delta / function_call_arguments.delta / reasoning deltas
    delta: string?,
    // output_text.done
    text: string?,
    // function_call_arguments.done
    arguments: string?,
    item_id: string?,
    output_index: int?,
    // which content part WITHIN the item — required on the text delta/done
    // events, absent on the function-call ones.
    content_index: int?,
    item: OaOutputItem?,
    response: OaResponse?,
    // a bare `error` event puts the failure at the top level ...
    code: string?,
    message: string?,
    // ... but immediate rejections (e.g. quota exhaustion) arrive as the plain
    // HTTP error envelope `{"error": {...}}` on the SSE data line instead.
    error: OaErrorBody?,
}

// What the decoder must remember ACROSS batches: which text items already
// streamed deltas (so a trailing `output_text.done` does not duplicate them),
// the assembled function-call arguments, and the model name the API resolved.
class OaStreamState {
    model: string?,
    text_started: map<string, bool>,
    call_arguments: map<string, string>,

    function new() -> OaStreamState {
        let started: map<string, bool> = {};
        let buffered: map<string, string> = {};
        OaStreamState { model: null, text_started: started, call_arguments: buffered }
    }
}

// Which CONTENT PART an event belongs to. `item_id` is the API's own item key
// (`output_index` is the fallback for events that omit it), and `content_index`
// distinguishes the parts within one item: a message may carry several text
// parts, and keying by item alone would let a delta on part 0 suppress the
// `output_text.done` that is the only delivery of part 1.
function _openai_event_key(event: OaStreamEvent) -> string {
    let item = if let id: string = event.item_id {
        id
    } else if let index: int = event.output_index {
        `#${index}`
    } else {
        ""
    };
    if let part: int = event.content_index {
        `${item}:${part}`
    } else {
        item
    }
}

function _openai_stream_failure(event: OaStreamEvent) -> ai.errors.Failure {
    if let response: OaResponse = event.response {
        return _openai_response_failure(response);
    };
    if let nested: OaErrorBody = event.error {
        return _openai_error_failure(nested);
    };
    _openai_error_failure(OaErrorBody { code: event.code, message: event.message, type: null })
}

// One decoded SSE event -> provider-agnostic stream events.
function _openai_stream_events(
    event: OaStreamEvent,
    sse_name: string?,
    state: OaStreamState,
) -> ai.stream.StreamEvent[] {
    let out: ai.stream.StreamEvent[] = [];
    let kind = event.type ?? sse_name ?? "";
    // A top-level `error` envelope only ever appears on failures; surface it
    // even when the rejection line carries no recognizable event type.
    if (kind != "response.failed" && kind != "error") {
        if let nested: OaErrorBody = event.error {
            throw _openai_error_failure(nested);
        };
    };
    match (kind) {
        "response.created" | "response.in_progress" => {
            // The resolved model id arrives here. ai.stream.TurnMeta has no
            // model field yet, so it is remembered rather than emitted.
            if let started: OaResponse = event.response {
                state.model = started.model;
            };
            null
        },
        "response.output_text.delta" => {
            if let delta: string = event.delta {
                let _ = state.text_started.set(_openai_event_key(event), true);
                out.push(ai.stream.TextDelta { text: delta });
            };
            null
        },
        "response.output_text.done" => {
            // The full text of the item. Emitted only when no delta arrived for
            // it, so a server that sends both does not duplicate the text.
            let key = _openai_event_key(event);
            if (!(state.text_started.get(key) ?? false)) {
                if let full: string = event.text {
                    let _ = state.text_started.set(key, true);
                    out.push(ai.stream.TextDelta { text: full });
                };
                null
            } else {
                null
            };
            null
        },
        "response.function_call_arguments.delta" => {
            // ai.stream.StreamEvent cannot carry tool calls (PLAN §6), so the
            // arguments accumulate here and the completed call is read from the
            // terminal event's response object.
            if let delta: string = event.delta {
                let key = _openai_event_key(event);
                let _ = state.call_arguments.set(key, (state.call_arguments.get(key) ?? "") + delta);
            };
            null
        },
        "response.function_call_arguments.done" => {
            if let full: string = event.arguments {
                let _ = state.call_arguments.set(_openai_event_key(event), full);
            };
            null
        },
        "response.failed" | "error" => throw _openai_stream_failure(event),
        "response.completed" | "response.incomplete" => {
            if let final_response: OaResponse = event.response {
                let turn = openai_parse(final_response);
                out.push(
                    ai.stream.TurnMeta {
                        stop_reason: turn.stop_reason,
                        input_tokens: turn.usage?.input_tokens,
                        output_tokens: turn.usage?.output_tokens,
                    },
                );
            };
            out.push(ai.stream.TurnDone {  });
            null
        },
        _ => null,
    };
    out
}

// One SseStream batch -> provider-agnostic stream events.
function _openai_decode_batch(batch: string, state: OaStreamState) -> ai.stream.StreamEvent[] {
    let out: ai.stream.StreamEvent[] = [];
    for (let raw in ai.stream.decode_sse_batch(batch)) {
        if let data: string = raw.data {
            let decoded: OaStreamEvent? = baml.json.from_string<OaStreamEvent>(data) catch_all (e) {
                _ => null,
            };
            if let event: OaStreamEvent = decoded {
                for (let e in _openai_stream_events(event, raw.event, state)) {
                    out.push(e);
                }
            };
            null
        };
    }
    out
}

// A failure opening the SSE connection. `baml.http.fetch_sse` reports a non-2xx
// status as an Io error whose message carries the status and body, so the
// status is recovered and classified exactly like ai.wire.send_as does — a 401
// or 429 on the streaming path must NOT be reported as a retry-safe transport
// failure.
function _openai_sse_failure(error: unknown) -> ai.errors.Failure {
    let text = match (error) {
        let io: baml.errors.Io => io.message,
        _ => error.to_string(),
    };
    let marker = "SSE request failed with status ";
    if let at: int = text.index_of(marker) {
        let rest = text.slice(at + marker.length(), text.length());
        if let separator: int = rest.index_of(":") {
            let code = baml.Int.parse(rest.slice(0, separator).trim()) catch_all (e) {
                _ => 0,
            };
            if (code > 0) {
                return ai.errors.classify_http(
                    "openai",
                    code,
                    rest.slice(separator + 1, rest.length()).trim(),
                );
            };
        };
    };
    ai.errors.NetworkFailure {
        status_code: null,
        provider: "openai",
        detail: `transport failed before a response arrived: ${text}`,
        raw_body: null,
    }
}

function _openai_stream_open_failure(error: unknown) -> never {
    match (error) {
        let timeout: baml.errors.Timeout => throw timeout,
        let cancelled: baml.panics.Cancelled => throw cancelled,
        _ => throw _openai_sse_failure(error),
    }
}

// The streaming entry point: the same request as `invoke` with `stream: true`,
// wrapped in an ai.TurnStream over the SSE connection.
function invoke_stream(c: root.ResponsesClient, input: ai.ModelTurnInput) -> ai.stream.TurnStream {
    let req = _openai_request(c, input, true, preview = false);
    let started = baml.time.Instant.now();
    let sse = baml.http.fetch_sse(
        req,
        timeout = baml.time.Duration.from_milliseconds(c.request_timeout_ms ?? 0),
        first_event_timeout = baml.time.Duration.from_milliseconds(c.time_to_first_token_timeout_ms ?? 0),
    ) catch_all (e) {
        _ => _openai_stream_open_failure(e),
    };
    let state = OaStreamState.new();
    // The Responses protocol always ends a live turn with an explicit terminal
    // event (`response.completed` / `response.incomplete` / `response.failed`),
    // so a connection that closes without one is a TRUNCATED turn, never a
    // complete one — opt into ai.stream's strict termination contract so the
    // partial text is reported as ai.errors.NetworkFailure instead of being
    // handed back as a successful answer.
    let stream = ai.stream.TurnStream.from_sse(
        sse,
        (batch: string) -> {
            _openai_decode_batch(batch, state) catch_all (e) {
                _ => throw ai.errors.normalize(e),
            }
        },
        require_terminal = "openai",
    );
    let call = ai.events.LLMCall {
        client_name: c.id(),
        provider: "openai",
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
        http_response: ai.events.HttpResponse { status: sse.status_code, headers: sse.headers, body: null },
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
