// The shared OpenAI **Chat Completions** core (`POST {base}/chat/completions`).
//
// One build/parse/SSE implementation serves the whole compatible family —
// openai chat, openai-generic (vLLM / LM Studio / TGI / …), Azure OpenAI,
// Ollama and OpenRouter. The public wrapper classes in `openai/{chat,generic,
// azure,ollama,openrouter}.baml` are *config*: each one instantiates a typed
// `ChatCompat` record and delegates here. Nothing in this file branches on a
// provider NAME — the only per-provider inputs are the typed compat flags,
// which is what keeps a new OpenAI-compatible endpoint a 20-line wrapper.
//
// Wire modelling follows PLAN §1.5: the request, the response envelope and the
// SSE chunks are all real BAML classes. `baml.json.json` appears exactly three
// times — the `request_body` passthrough, a tool's `parameters` schema (which is
// arbitrary JSON Schema by definition), and the per-message directives carried
// verbatim from `ai.PromptMessage.metadata` onto a content part.
//
// One language constraint shapes the request/response classes: `function` is a
// keyword, so it cannot be a BAML field name, and the chat-completions wire
// format uses `{"type":"function","function":{…}}` for both tool declarations
// and tool calls. Those classes therefore carry an `fn` field plus an explicit
// `baml.ToJson` / `baml.FromJson` override that maps it to/from the `function`
// key. Everything else is structural.

// ---------------------------------------------------------------------------
// Local rejections.

/// Every LOCAL, pre-send rejection in the chat family throws ONE error
/// identity: `ai.errors.InvalidRequest` with a null status code — the class
/// `ResponsesClient`, `GoogleClient`, `ImageClient` and the Bedrock client
/// already use.
///
/// It matters that this is an `ai.errors.Failure` and not
/// `baml.errors.InvalidArgument`: `ai.errors.normalize` flattens a non-Failure
/// into an untyped `baml.errors.UnknownError`, and `ai.Fallback` only advances
/// to the next member on a TYPED failure. A missing key in one member of a
/// fallback chain has to fall through, not abort the chain.
function _reject(provider: string, detail: string) -> ai.errors.InvalidRequest {
    ai.errors.InvalidRequest {
        provider: provider,
        status_code: null,
        detail: detail,
        raw_body: null,
    }
}

/// `_reject` for the Azure wrapper, whose config errors are raised before a
/// `ChatCompat` exists to name the provider.
function _azure_reject(detail: string) -> ai.errors.InvalidRequest {
    _reject("azure", detail)
}

// ---------------------------------------------------------------------------
// Compat record + per-request parameters.

// How the endpoint authenticates. Sniffing the scheme from the key's shape (the
// pi-ai smell) is deliberately not an option: each wrapper states it.
enum ChatAuth {
    // `authorization: Bearer <key>` — openai, generic, ollama, openrouter.
    Bearer,
    // `api-key: <key>` — Azure OpenAI, which sends no Authorization header.
    ApiKeyHeader,
    // No credential is attached even when one is configured.
    NoAuth,
}

// Everything that differs between OpenAI-compatible endpoints, as data.
// `base_url` arrives already resolved by the wrapper (explicit option, then the
// named env var, then the provider default), so the core never reads env for it.
class ChatCompat {
    // The wire identity: the `ai.errors.*` provider tag and the `id()` prefix.
    provider: string,
    // Resolved origin + prefix, no trailing slash, e.g. "https://api.openai.com/v1".
    base_url: string,
    // Appended to base_url. Azure carries the deployment in base_url instead.
    path: string,
    auth: ChatAuth,
    // The canonical credential env var, used only to name it in the error when
    // `api_key_required` is set and nothing resolved. Null when the endpoint has
    // no canonical variable (generic, ollama).
    api_key_env: string?,
    // Whether a missing credential is an error rather than an unauthenticated
    // request. False for local/self-hosted endpoints.
    api_key_required: bool,
    // Send an all-text message as `content: "…"` instead of
    // `content: [{"type":"text",…}]`. Strict OpenAI-compatible servers reject
    // the array form; the real OpenAI API accepts both.
    collapse_text_content: bool,
    // Fetch remote image URLs and inline them as base64 data URLs instead of
    // passing the URL through. Ollama's OpenAI-compatible endpoint documents
    // "Base64 encoded image ✓ / Image URL ✗", so an `Image.from_url` prompt is
    // unusable there unless the client resolves it first. Audio and PDFs are
    // always inlined (neither wire part has a URL form).
    inline_image_urls: bool,
    // Send `stream_options: {"include_usage": true}` on streaming requests.
    // Only first-party OpenAI documents it; other servers 400 on it.
    send_stream_options: bool,
    // Which key the first-class `max_tokens` parameter lowers to:
    // "max_tokens" or "max_completion_tokens".
    max_tokens_key: string,
    // A token ceiling injected when the caller set neither `max_tokens` nor
    // `max_completion_tokens` (Azure requires one). Null everywhere else.
    default_max_tokens: int?,
    // The role a role-less prompt message is sent under. `user` everywhere, to
    // match the shipped native clients (PLAN §5); sys_llm used `system`, which
    // silently turned an unmarked prompt into a system instruction.
    default_role: string,
    // Query parameters the endpoint itself requires (Azure `api-version`).
    // User query params are merged over these.
    compat_query: map<string, string>,

    // Every field defaults to the plain-OpenAI shape; a wrapper overrides the
    // handful that differ.
    function new(
        provider: string,
        base_url: string,
        path: string = "/chat/completions",
        auth: ChatAuth = ChatAuth.Bearer,
        api_key_env: string? = null,
        api_key_required: bool = false,
        collapse_text_content: bool = false,
        inline_image_urls: bool = false,
        send_stream_options: bool = false,
        max_tokens_key: string = "max_tokens",
        default_max_tokens: int? = null,
        default_role: string = "user",
        compat_query: map<string, string>? = null,
    ) -> ChatCompat {
        let empty: map<string, string> = {};
        ChatCompat {
            provider: provider,
            base_url: base_url,
            path: path,
            auth: auth,
            api_key_env: api_key_env,
            api_key_required: api_key_required,
            collapse_text_content: collapse_text_content,
            inline_image_urls: inline_image_urls,
            send_stream_options: send_stream_options,
            max_tokens_key: max_tokens_key,
            default_max_tokens: default_max_tokens,
            default_role: default_role,
            compat_query: compat_query ?? empty,
        }
    }
}

// The per-request knobs a caller configured on the wrapper client. Typed
// parameters lower into the typed request only when non-null; `request_body` is
// the deep-merge escape hatch applied last (PLAN P0.3).
class ChatParams {
    model: string,
    // Already resolved by the wrapper's `resolved_api_key()`.
    api_key: string?,
    temperature: float?,
    max_tokens: int?,
    max_completion_tokens: int?,
    top_p: float?,
    stop: string[]?,
    seed: int?,
    headers: map<string, string>?,
    query_params: map<string, string>?,
    request_body: baml.json.json?,

    function new(
        model: string,
        api_key: string? = null,
        temperature: float? = null,
        max_tokens: int? = null,
        max_completion_tokens: int? = null,
        top_p: float? = null,
        stop: string[]? = null,
        seed: int? = null,
        headers: map<string, string>? = null,
        query_params: map<string, string>? = null,
        request_body: baml.json.json? = null,
    ) -> ChatParams {
        ChatParams {
            model: model,
            api_key: api_key,
            temperature: temperature,
            max_tokens: max_tokens,
            max_completion_tokens: max_completion_tokens,
            top_p: top_p,
            stop: stop,
            seed: seed,
            headers: headers,
            query_params: query_params,
            request_body: request_body,
        }
    }
}

/// `provider/model` — the client identity used for `ai.Client.id()`, journal
/// attribution, and `ai.wire.sanitize_for_client`.
function chat_client_id(compat: ChatCompat, params: ChatParams) -> string {
    `${compat.provider}/${params.model}`
}

// ---------------------------------------------------------------------------
// Typed request wire classes.

class ChatTextPart {
    type: string,
    text: string,
}

class ChatImageUrl {
    url: string,
}

class ChatImagePart {
    type: string,
    image_url: ChatImageUrl,
}

class ChatInputAudio {
    data: string,
    format: string,
}

class ChatAudioPart {
    type: string,
    input_audio: ChatInputAudio,
}

// Inline document: base64 data URL plus a filename (the API requires one).
class ChatFileData {
    filename: string,
    file_data: string,
}

class ChatFilePart {
    type: string,
    file: ChatFileData,
}

// A content part carrying the per-message provider directives authored on
// `${ai.Role { name: …, metadata: { … } }}`. Chat completions places
// such directives ON A CONTENT PART (`prompt_cache_breakpoint` for OpenAI,
// OpenRouter's Anthropic-style `cache_control`), while the authored directive
// lives on the MESSAGE — so, exactly as the Anthropic client does, the metadata
// attaches to the message's LAST part and everything up to and including it is
// the cached prefix.
//
// The keys are forwarded VERBATIM: this file does not maintain an allow-list of
// which directives an OpenAI-compatible endpoint understands, and "verbatim"
// includes an explicitly authored `null`.
//
// Getting that last part right is why `to_json` PARKS the directives under
// `_chat_directives_key()` rather than splatting them onto the part: the body
// then goes through `_chat_prune_nulls`, whose job is to drop the CLIENT's
// unset options, and it would take an authored `{"cache_control": null}` with
// them. The parked key is opaque to that pass and `_chat_apply_directives`
// splices it back onto the part immediately afterwards, so the key never
// reaches the wire. (The sibling Responses client gets the same result by
// applying its directives to the serialized body after its own prune.)
class ChatExtraPart {
    part: ChatContentPart,
    extra: map<string, baml.json.json>,

    implements baml.ToJson {
        function to_json(self) -> baml.json.json {
            let base = baml.json.to_json(self.part);
            match (base) {
                let object: map<string, baml.json.json> => {
                    let out: map<string, baml.json.json> = {};
                    for (let key in object.keys()) {
                        let _ = out.set(key, object.get(key));
                    }
                    if (self.extra.keys().length() > 0) {
                        let _ = out.set(_chat_directives_key(), baml.json.to_json(self.extra));
                    }
                    out
                },
                _ => base,
            }
        }
    }
}

// The reserved key `ChatExtraPart` parks per-message directives under between
// serialization and `_chat_apply_directives`. It exists only inside that
// window; nothing with this key is ever sent.
function _chat_directives_key() -> string {
    "__baml_directives"
}

type ChatContentPart = ChatTextPart | ChatImagePart | ChatAudioPart | ChatFilePart | ChatExtraPart;

// Chat completions accepts either a bare string or an array of typed parts.
type ChatMessageContent = string | ChatContentPart[];

class ChatToolCallFunction {
    name: string,
    arguments: string, // the arguments object, as a JSON *string*
}

// `{"id":…,"type":"function","function":{"name":…,"arguments":…}}`.
class ChatToolCall {
    id: string,
    type: string,
    fn: ChatToolCallFunction,

    implements baml.ToJson {
        function to_json(self) -> baml.json.json {
            let out: map<string, baml.json.json> = {};
            let _ = out.set("id", self.id);
            let _ = out.set("type", self.type);
            let _ = out.set("function", baml.json.to_json(self.fn));
            out
        }
    }
}

class ChatMessage {
    role: string,
    content: ChatMessageContent?,
    tool_calls: ChatToolCall[]?,
    tool_call_id: string?,
}

class ChatToolFunction {
    name: string,
    description: string,
    // JSON Schema for the arguments object: arbitrary by definition, one of the
    // two sanctioned `json` uses (PLAN §1.5).
    parameters: baml.json.json,
}

class ChatTool {
    type: string,
    fn: ChatToolFunction,

    implements baml.ToJson {
        function to_json(self) -> baml.json.json {
            let out: map<string, baml.json.json> = {};
            let _ = out.set("type", self.type);
            let _ = out.set("function", baml.json.to_json(self.fn));
            out
        }
    }
}

class ChatStreamOptions {
    include_usage: bool,
}

// The complete request body. Null-valued fields are pruned before the body is
// serialized, so an unset option never reaches the wire as `"temperature": null`.
class ChatRequest {
    model: string,
    messages: ChatMessage[],
    temperature: float?,
    max_tokens: int?,
    max_completion_tokens: int?,
    top_p: float?,
    stop: string[]?,
    seed: int?,
    tools: ChatTool[]?,
    tool_choice: string?,
    stream: bool?,
    stream_options: ChatStreamOptions?,
}

// ---------------------------------------------------------------------------
// Typed response wire classes. Every field is optional: OpenAI-compatible
// servers omit freely, and a non-optional int is a ParseFailed waiting to
// happen (PLAN §1.5).

class CcFunctionCall {
    name: string?,
    arguments: string?,
}

class CcToolCall {
    id: string?,
    type: string?,
    // Present on streaming deltas; identifies which call a fragment extends.
    index: int?,
    fn: CcFunctionCall?,

    implements baml.FromJson {
        function from_json(j: baml.json.json) -> Self {
            CcToolCall {
                id: baml.json.to<string?>(baml.json.field(j, "id")),
                type: baml.json.to<string?>(baml.json.field(j, "type")),
                index: baml.json.to<int?>(baml.json.field(j, "index")),
                fn: baml.json.to<CcFunctionCall?>(baml.json.field(j, "function")),
            }
        }
    }
}

class CcImageUrl {
    url: string?,
}

// A response content part. Servers that return image output (Ollama,
// OpenRouter) use `image_url` parts alongside `text` ones.
class CcContentPart {
    type: string?,
    text: string?,
    image_url: CcImageUrl?,
}

// Ollama/OpenRouter also report generated images in a sibling `images` array.
class CcMessageImage {
    type: string?,
    image_url: CcImageUrl?,
    url: string?,
}

// Audio output (`gpt-4o-audio-*`), requested through the `request_body`
// passthrough with `{"modalities":["text","audio"],"audio":{"voice":…,"format":…}}`.
// The response carries the spoken bytes and their transcript; the format is NOT
// echoed back, so the mime type is recovered from the request (see
// `_chat_audio_output_mime`).
class CcMessageAudio {
    id: string?,
    data: string?,
    transcript: string?,
    expires_at: int?,
}

class CcMessage {
    role: string?,
    content: (string | CcContentPart[])?,
    images: CcMessageImage[]?,
    audio: CcMessageAudio?,
    // Some OpenAI-compatible servers (DeepSeek, vLLM reasoning models) split
    // chain-of-thought out of `content`.
    reasoning_content: string?,
    refusal: string?,
    tool_calls: CcToolCall[]?,
}

class CcChoice {
    index: int?,
    message: CcMessage?,
    finish_reason: string?,
}

class CcPromptTokensDetails {
    cached_tokens: int?,
}

class CcCompletionTokensDetails {
    reasoning_tokens: int?,
}

class CcUsage {
    prompt_tokens: int?,
    completion_tokens: int?,
    total_tokens: int?,
    prompt_tokens_details: CcPromptTokensDetails?,
    completion_tokens_details: CcCompletionTokensDetails?,
}

class CcResponse {
    id: string?,
    model: string?,
    choices: CcChoice[]?,
    usage: CcUsage?,
}

// ---------------------------------------------------------------------------
// Prompt lowering.

// One prompt message after role resolution, before the content encoding is
// chosen (string vs parts) — merging adjacent same-role messages happens here.
class ChatPromptMessage {
    role: string,
    parts: ChatContentPart[],
}

// `data:<mime>;base64,<data>` for inline payloads, the original URL otherwise.
function _chat_media_url(resolved: ai.wire.ResolvedMedia) -> string {
    if let url: string = resolved.url {
        return url;
    }
    `data:${resolved.mime_type};base64,${resolved.base64 ?? ""}`
}

// `input_audio.format` is the mime subtype, with the one alias the API needs.
// Unknown subtypes are forwarded rather than rejected: OpenAI documents wav/mp3
// but transcodes others, and a compatible server may accept more (this is the
// engine's behaviour; sys_llm hard-errored on anything but wav/mp3).
function _chat_audio_format(mime_type: string) -> string {
    let subtype = if (mime_type.starts_with("audio/")) {
        mime_type.slice(6, mime_type.length())
    } else {
        mime_type
    };
    match (subtype) {
        "mpeg" | "mp3" => "mp3",
        "x-wav" => "wav",
        _ => subtype,
    }
}

// The mime type of an audio OUTPUT payload. The response's `message.audio`
// object carries the bytes but never says what they are — the format is
// whatever the request asked for, so it is read back out of the passthrough
// body (`{"audio":{"format":"wav"}}`). OpenAI accepts wav|mp3|flac|opus|pcm16
// and defaults to nothing, so wav is the assumption when the caller enabled the
// modality some other way.
function _chat_audio_output_mime(request_body: baml.json.json?) -> string {
    let format = baml.json.path_or<string>(request_body, ".audio.format", "wav");
    match (format) {
        "mp3" => "audio/mpeg",
        "pcm16" => "audio/pcm",
        _ => `audio/${format}`,
    }
}

// One media prompt part -> one typed content part. Media is allowed in any
// role: the chat endpoint imposes no role restriction (the engine imposes none
// either; sys_llm's hard error was stricter than the API).
function _chat_media_part(
    media: ai.MediaPart,
    compat: ChatCompat,
    preview: bool = false,
) -> ChatContentPart {
    match (media) {
        let image: baml.media.Image => {
            let resolved = if (preview) {
                ai.wire.resolve_media_preview(image)
            } else {
                ai.wire.resolve_media(image, compat.inline_image_urls)
            };
            if (preview && compat.inline_image_urls && resolved.url != null) {
                throw ai.errors.PreviewUnsupported {
                    provider: compat.provider,
                    detail: "this chat endpoint requires downloaded image bytes during request preview",
                }
            }
            ChatImagePart {
                type: "image_url",
                image_url: ChatImageUrl { url: _chat_media_url(resolved) },
            }
        },
        let audio: baml.media.Audio => {
            // `input_audio` has no URL form, so remote audio is fetched and
            // inlined. The engine put the URL in `data`, which the API rejects.
            let resolved = if (preview) {
                ai.wire.resolve_media_preview(audio)
            } else {
                ai.wire.resolve_media(audio, true)
            };
            if (preview && resolved.url != null) {
                throw ai.errors.PreviewUnsupported {
                    provider: compat.provider,
                    detail: "chat audio input requires downloaded bytes during request preview",
                }
            }
            ChatAudioPart {
                type: "input_audio",
                input_audio: ChatInputAudio {
                    data: resolved.base64 ?? "",
                    format: _chat_audio_format(resolved.mime_type),
                },
            }
        },
        let pdf: baml.media.Pdf => {
            // Chat completions has NO document-by-URL part: the API's `file`
            // object is `{filename, file_data, file_id}` and nothing else —
            // `file_url` + a nested `type: "input_file"` belongs to the
            // Responses API, and sending it here is silently ignored or 400s.
            // So a remote PDF is fetched and inlined, exactly like audio. A
            // scheme the HTTP client cannot fetch (`gs://…`) surfaces as a
            // media-resolution failure rather than a malformed request.
            let resolved = if (preview) {
                ai.wire.resolve_media_preview(pdf)
            } else {
                ai.wire.resolve_media(pdf, true)
            };
            if (preview && resolved.url != null) {
                throw ai.errors.PreviewUnsupported {
                    provider: compat.provider,
                    detail: "chat PDF input requires downloaded bytes during request preview",
                }
            }
            // `resolve_media` hands back a bare URL for a scheme it will not
            // fetch (`gs://`). There is nowhere to put it, and an empty
            // `file_data` would be a silently blank document, so say so.
            let data = match (resolved.base64) {
                let inline: string => inline,
                null => {
                    throw _reject(
                    compat.provider,
                    `OpenAI Chat Completions has no document-by-URL content part, so a PDF must be inlined, and ${resolved.url ?? "this document"} is not a URL this client can fetch (gs:// and other non-HTTP schemes are not supported here). Pass the PDF as base64 or a local file.`,
                )
                },
            };
            ChatFilePart {
                type: "file",
                file: ChatFileData {
                    // The API requires a filename; the basename of the source
                    // is more useful than a constant, and this is the same
                    // derivation the Responses client uses (query stripped,
                    // `document.pdf` when there is nothing to derive).
                    filename: _openai_file_name(pdf.file() ?? pdf.url()),
                    file_data: `data:${resolved.mime_type};base64,${data}`,
                },
            }
        },
        let video: baml.media.Video => {
            throw _reject(
                compat.provider,
                "OpenAI Chat Completions has no video input part; extract frames as images, or use the Realtime API",
            )
        },
    }
}

function _chat_prompt_parts(
    message: ai.PromptMessage,
    compat: ChatCompat,
    preview: bool = false,
) -> ChatContentPart[] {
    let parts: ChatContentPart[] = [];
    for (let part in message.parts) {
        match (part) {
            let text: string => {
                parts.push(ChatTextPart { type: "text", text: text });
                null
            },
            let image: baml.media.Image => {
                parts.push(_chat_media_part(image, compat, preview = preview));
                null
            },
            let audio: baml.media.Audio => {
                parts.push(_chat_media_part(audio, compat, preview = preview));
                null
            },
            let pdf: baml.media.Pdf => {
                parts.push(_chat_media_part(pdf, compat, preview = preview));
                null
            },
            let video: baml.media.Video => {
                parts.push(_chat_media_part(video, compat, preview = preview));
                null
            },
        };
    }
    parts
}

// Attach a message's authored `metadata` to its LAST content part (see
// ChatExtraPart). A message that lowered to no parts has nowhere to put the
// directive, and an empty metadata map is the common case, so both are no-ops.
function _chat_apply_metadata(
    parts: ChatContentPart[],
    metadata: map<string, baml.json.json>,
) -> void {
    if (metadata.keys().length() == 0) {
        return;
    }
    if let last: ChatContentPart = parts.pop() {
        parts.push(ChatExtraPart { part: last, extra: metadata });
    }
}

// Every part is text — the precondition for `collapse_text_content`. A
// `ChatExtraPart` never qualifies even when it wraps text: collapsing to a bare
// string would drop the directive it carries.
function _chat_all_text(parts: ChatContentPart[]) -> bool {
    let all = true;
    for (let part in parts) {
        match (part) {
            let text: ChatTextPart => null,
            _ => {
                all = false;
                null
            },
        };
    }
    all
}

// Render the accumulated parts into the wire `content` value.
function _chat_content(parts: ChatContentPart[], collapse: bool) -> ChatMessageContent {
    if (collapse && _chat_all_text(parts)) {
        let text = "";
        for (let part in parts) {
            match (part) {
                let t: ChatTextPart => {
                    text = `${text}${t.text}`;
                    null
                },
                _ => null,
            };
        }
        return text;
    }
    parts
}

/// A rendered structural prompt becomes chat-completions messages. A role-less
/// message takes the compat's default role; an authored `tool` role becomes user
/// content, because a wire `tool` message must answer a specific `tool_call_id`
/// and only the journal path has one. Adjacent same-role messages merge, as both
/// prior implementations did. A message's authored `metadata` rides on its last
/// content part (see `ChatExtraPart`).
function chat_lower_prompt(
    prompt: ai.Prompt,
    compat: ChatCompat,
    preview: bool = false,
) -> ChatMessage[] {
    let lowered: ChatPromptMessage[] = [];
    for (let message in prompt.messages()) {
        let role = match (message.role) {
            "" => compat.default_role,
            "tool" => "user",
            _ => message.role,
        };
        let parts = _chat_prompt_parts(message, compat, preview = preview);
        // Placement happens BEFORE the adjacent-same-role merge, so a directive
        // stays on the part it was authored after even when this message is
        // folded into the previous one.
        _chat_apply_metadata(parts, message.metadata);
        let merged = false;
        if let last: ChatPromptMessage = lowered.at(lowered.length() - 1) {
            if (last.role == role) {
                for (let part in parts) {
                    last.parts.push(part);
                }
                merged = true;
            }
        }
        if (!merged) {
            lowered.push(ChatPromptMessage { role: role, parts: parts });
        }
    }
    let out: ChatMessage[] = [];
    for (let message in lowered) {
        out.push(
            ChatMessage {
                role: message.role,
                content: _chat_content(message.parts, compat.collapse_text_content),
                tool_calls: null,
                tool_call_id: null,
            },
        );
    }
    out
}

// ---------------------------------------------------------------------------
// Journal lowering.

/// `ai.Journal` -> chat-completions messages. Assistant turns carry their text
/// in `content` and their calls in `tool_calls`; each result comes back as its
/// own `role: "tool"` message keyed by `tool_call_id`. Journal-only events
/// (`RunStarted`, `ToolRequested`, `Usage`, `FinalProduced`) and reasoning
/// blocks lower to nothing.
///
/// Callers pass the journal through `ai.wire.sanitize_for_client` first, so
/// foreign reasoning, empty turns and unanswered tool calls are already handled.
function chat_lower_journal(j: ai.Journal) -> ChatMessage[] {
    let out: ChatMessage[] = [];
    for (let event in j.entries()) {
        match (event) {
            let user: ai.events.UserMessage => {
                out.push(
                    ChatMessage {
                        role: "user",
                        content: user.content,
                        tool_calls: null,
                        tool_call_id: null,
                    },
                );
                null
            },
            let assistant: ai.events.AssistantMessage => {
                let text = "";
                let calls: ChatToolCall[] = [];
                for (let block in assistant.content) {
                    match (block) {
                        let t: ai.content.Text => {
                            text = `${text}${t.text}`;
                            null
                        },
                        let use: ai.content.ToolUse => {
                            calls
                                .push(
                                    ChatToolCall {
                                        id: use.id,
                                        type: "function",
                                        fn: ChatToolCallFunction {
                                            name: use.name,
                                            arguments: baml.json.stringify(baml.json.to_json(use.args)),
                                        },
                                    },
                                );
                            null
                        },
                        // Reasoning has no replayable wire form here, and a
                        // generated image is an output, not model input.
                        _ => null,
                    };
                }
                let content: ChatMessageContent? = if (text == "") {
                    null
                } else {
                    text
                };
                let tool_calls: ChatToolCall[]? = if (calls.length() == 0) {
                    null
                } else {
                    calls
                };
                if (content != null || tool_calls != null) {
                    out.push(
                        ChatMessage {
                            role: "assistant",
                            content: content,
                            tool_calls: tool_calls,
                            tool_call_id: null,
                        },
                    );
                }
                null
            },
            let completed: ai.events.ToolCompleted => {
                out.push(
                    ChatMessage {
                        role: "tool",
                        content: completed.output,
                        tool_calls: null,
                        tool_call_id: completed.id,
                    },
                );
                null
            },
            let failed: ai.events.ToolFailed => {
                let err: map<string, string> = { "error": failed.message };
                out.push(
                    ChatMessage {
                        role: "tool",
                        content: baml.json.stringify(baml.json.to_json(err)),
                        tool_calls: null,
                        tool_call_id: failed.id,
                    },
                );
                null
            },
            _ => null,
        };
    }
    out
}

// ---------------------------------------------------------------------------
// Request rendering.

// Keys whose value is a document AUTHORED BY THE CALLER rather than a wire
// field this file owns. Inside them a `null` is data, not an unset option:
//   * `parameters` — a tool's JSON Schema, where `{"default": null}` and
//     `{"type": ["string", "null"]}` sibling nulls are meaningful, and where a
//     strict-schema server validates what it was given.
// The `request_body` passthrough needs no entry here: it is merged AFTER the
// prune (`chat_render_body`), so it is never walked.
function _chat_is_opaque_key(key: string) -> bool {
    // `parameters` is a caller-authored tool schema; the directives subtree is
    // caller-authored too, and both mean exactly what they say — including a
    // deliberate `null`.
    key == "parameters" || key == _chat_directives_key()
}

// Splice each parked directives object back onto the content part that carries
// it, dropping the reserved key. Runs right after `_chat_prune_nulls` and
// before the `request_body` passthrough merge, so the authored directives sit
// on the wire exactly as written and the caller's passthrough still wins last.
function _chat_apply_directives(j: baml.json.json) -> baml.json.json {
    match (j) {
        let object: map<string, baml.json.json> => {
            let out: map<string, baml.json.json> = {};
            for (let key in object.keys()) {
                if (key != _chat_directives_key()) {
                    let _ = out.set(key, _chat_apply_directives(object.get(key)));
                }
            }
            match (object.get(_chat_directives_key())) {
                let directives: map<string, baml.json.json> => {
                    for (let key in directives.keys()) {
                        let _ = out.set(key, directives.get(key));
                    }
                    null
                },
                _ => null,
            };
            out
        },
        let items: baml.json.json[] => {
            items.map((item) -> {
                _chat_apply_directives(item)
            })
        },
        _ => j,
    }
}

// Drop every null-valued map entry, recursively. Typed request classes declare
// their optional fields, and this is what keeps an unset one off the wire
// instead of sending `"seed": null`. Runs BEFORE the passthrough merge, so a
// user's explicit `null` override still deletes a key the client did set.
// `_chat_is_opaque_key` subtrees are copied verbatim (the aws/google rule).
function _chat_prune_nulls(j: baml.json.json) -> baml.json.json {
    match (j) {
        let object: map<string, baml.json.json> => {
            let out: map<string, baml.json.json> = {};
            for (let key in object.keys()) {
                let value = object.get(key);
                match (value) {
                    null => null,
                    _ => {
                        if (_chat_is_opaque_key(key)) {
                            let _ = out.set(key, value);
                            null
                        } else {
                            let _ = out.set(key, _chat_prune_nulls(value));
                            null
                        }
                    },
                };
            }
            out
        },
        let items: baml.json.json[] => {
            items.map((item) -> {
                _chat_prune_nulls(item)
            })
        },
        _ => j,
    }
}

// Does the passthrough body mention `key` at all (even as an explicit null)?
// Used for the Azure max_tokens guard, where "the caller said something about
// this key" is the question, not "what did they say".
function _chat_body_mentions(overrides: baml.json.json?, key: string) -> bool {
    match (overrides) {
        let object: map<string, baml.json.json> => object.has(key),
        _ => false,
    }
}

// Percent-encode one query value per RFC 3986 (unreserved characters survive,
// everything else becomes %XX over its UTF-8 bytes). The stdlib has no URL
// encoder, and Azure's `api-version` plus arbitrary user query params both need
// one.
function _chat_percent_encode(value: string) -> string {
    let unreserved = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_.~";
    let out = "";
    for (let character in value.chars()) {
        if (unreserved.includes(character)) {
            out = `${out}${character}`;
        } else {
            let hex = character.to_utf8().to_hex().to_upper_case();
            let i = 0;
            while (i < hex.length()) {
                out = `${out}%${hex.slice(i, i + 2)}`;
                i += 2;
            }
        }
    }
    out
}

// Endpoint-required query params first, user params merged over them.
function _chat_url(compat: ChatCompat, params: ChatParams) -> string {
    let query: map<string, string> = {};
    for (let key in compat.compat_query.keys()) {
        let _ = query.set(key, compat.compat_query.get(key) ?? "");
    }
    if let user_query: map<string, string> = params.query_params {
        for (let key in user_query.keys()) {
            let _ = query.set(key, user_query.get(key) ?? "");
        }
    }
    let url = `${compat.base_url}${compat.path}`;
    if (query.keys().length() == 0) {
        return url;
    }
    let pairs: string[] = [];
    for (let key in query.keys()) {
        pairs.push(`${_chat_percent_encode(key)}=${_chat_percent_encode(query.get(key) ?? "")}`);
    }
    `${url}?${pairs.join("&")}`
}

// content-type, then the caller's headers, then the credential — so the
// endpoint's own auth header always wins over a user override of the same name.
function _chat_headers(
    compat: ChatCompat,
    params: ChatParams,
    preview: bool = false,
) -> map<string, string> {
    let headers: map<string, string> = { "content-type": "application/json" };
    if let user_headers: map<string, string> = params.headers {
        for (let name in user_headers.keys()) {
            let _ = headers.set(name.to_lower_case(), user_headers.get(name) ?? "");
        }
    }
    if (!preview) {
        let key = params.api_key;
        if (key == null && compat.api_key_required) {
            throw _reject(
                compat.provider,
                `${compat.provider} needs an API key: pass api_key, or set ${compat.api_key_env ?? "the provider's environment variable"}`,
            )
        }
        if let credential: string = key {
            match (compat.auth) {
                ChatAuth.Bearer => {
                    let _ = headers.set("authorization", `Bearer ${credential}`);
                    null
                },
                ChatAuth.ApiKeyHeader => {
                    let _ = headers.set("api-key", credential);
                    null
                },
                ChatAuth.NoAuth => null,
            };
        }
    }
    headers
}

// The typed tool declarations for this turn, or null when the toolbox is empty.
function _chat_tools(toolbox: ai.tools.Toolbox) -> ChatTool[]? {
    if (toolbox.is_empty()) {
        return null;
    }
    let tools: ChatTool[] = [];
    for (let tool in toolbox.list()) {
        tools
            .push(
                ChatTool {
                    type: "function",
                    fn: ChatToolFunction {
                        name: tool.name,
                        description: tool.description,
                        parameters: baml.json.to_json(tool.input_schema),
                    },
                },
            );
    }
    tools
}

/// The typed body for one turn. Separate from the HTTP framing so tests (and a
/// future Azure/Vertex-style reuse) can assert on the request object itself.
function chat_build_request(
    compat: ChatCompat,
    params: ChatParams,
    input: ai.ModelTurnInput,
    stream: bool,
    preview: bool = false,
) -> ChatRequest {
    let prompt = input.prompt(ai.wire.render_output_format(input.output_type));
    let messages = chat_lower_prompt(prompt, compat, preview = preview);
    let journal = ai.wire.sanitize_for_client(input.journal, chat_client_id(compat, params));
    for (let message in chat_lower_journal(journal)) {
        messages.push(message);
    }

    // `max_tokens` vs `max_completion_tokens`: the first-class parameter lowers
    // to whichever key the endpoint wants, and an explicitly-set
    // max_completion_tokens is always sent under its own name.
    let max_tokens: int? = if (compat.max_tokens_key == "max_completion_tokens") {
        null
    } else {
        params.max_tokens
    };
    let max_completion_tokens: int? = if (compat.max_tokens_key == "max_completion_tokens") {
        params.max_completion_tokens ?? params.max_tokens
    } else {
        params.max_completion_tokens
    };
    // The endpoint's own floor (Azure), applied only when the caller said
    // nothing about either key — including through the passthrough body. An
    // explicit `{"max_tokens": null}` override still removes it, because the
    // merge runs after this.
    if (
        max_tokens == null
            && max_completion_tokens == null
            && !_chat_body_mentions(params.request_body, "max_tokens")
            && !_chat_body_mentions(params.request_body, "max_completion_tokens")
    ) {
        if let floor: int = compat.default_max_tokens {
            if (compat.max_tokens_key == "max_completion_tokens") {
                max_completion_tokens = floor;
            } else {
                max_tokens = floor;
            }
        }
    }

    let tools = _chat_tools(input.toolbox);
    let tool_choice: string? = if (tools == null) {
        null
    } else {
        "auto"
    };
    let stream_flag: bool? = if (stream) {
        true
    } else {
        null
    };
    let stream_options: ChatStreamOptions? = if (stream && compat.send_stream_options) {
        ChatStreamOptions { include_usage: true }
    } else {
        null
    };

    ChatRequest {
        model: params.model,
        messages: messages,
        temperature: params.temperature,
        max_tokens: max_tokens,
        max_completion_tokens: max_completion_tokens,
        top_p: params.top_p,
        stop: params.stop,
        seed: params.seed,
        tools: tools,
        tool_choice: tool_choice,
        stream: stream_flag,
        stream_options: stream_options,
    }
}

/// The serialized body: typed request -> json -> drop unset options -> apply the
/// caller's `request_body` passthrough LAST (PLAN P0.3 / `ai.wire`).
function chat_render_body(compat: ChatCompat, params: ChatParams, request: ChatRequest) -> string {
    let body = _chat_apply_directives(_chat_prune_nulls(baml.json.from<ChatRequest>(request)));
    baml.json.stringify(ai.wire.merge_request_body(body, params.request_body))
}

/// `ai.ModelTurnInput` -> one chat-completions HTTP request.
function chat_render(
    compat: ChatCompat,
    params: ChatParams,
    input: ai.ModelTurnInput,
    stream: bool,
) -> baml.http.Request {
    let request = chat_build_request(compat, params, input, stream, preview = false);
    baml.http.Request {
        method: "POST",
        url: _chat_url(compat, params),
        headers: _chat_headers(compat, params),
        body: chat_render_body(compat, params, request),
    }
}

/// The request one chat `render` would send. Media is lowered through the
/// preview-only resolver and never reads files or downloads URLs.
function chat_preview(
    compat: ChatCompat,
    params: ChatParams,
    input: ai.ModelTurnInput,
) -> baml.http.Request {
    let request = chat_build_request(compat, params, input, false, preview = true);
    baml.http.Request {
        method: "POST",
        url: _chat_url(compat, params),
        headers: _chat_headers(compat, params, preview = true),
        body: chat_render_body(compat, params, request),
    }
}

// ---------------------------------------------------------------------------
// Response parsing.

// One wire `finish_reason` -> the canonical stop reason.
function chat_stop_reason(reason: string) -> ai.content.StopReason {
    match (reason) {
        "tool_calls" | "function_call" => ai.content.StopReason.ToolUse,
        "length" | "max_tokens" => ai.content.StopReason.MaxTokens,
        "content_filter" => ai.content.StopReason.Refused,
        _ => ai.content.StopReason.Complete,
    }
}

// A response image reference -> a media block. Data URLs become inline base64;
// everything else is kept as a URL so the caller can fetch it.
function _chat_media_block(url: string) -> ai.content.Media {
    if (url.starts_with("data:")) {
        if let separator: int = url.index_of(";base64,") {
            return ai
                .content
                .Media
                .new(
                    value = baml
                        .media
                        .Image
                        .from_base64(
                            url.slice(separator + 8, url.length()),
                            url.slice(5, separator),
                        ),
                );
        }
    }
    ai.content.Media.new(value = baml.media.Image.from_url(url, null))
}

// A tool call's `arguments` is a JSON *string* the model produced, so it can be
// malformed (a truncated call, a model that emitted prose). Degrading to `{}`
// would hand the runner an empty-argument invocation of a real tool and tell
// nobody the arguments were discarded, so an undecodable payload is a
// `ParseFailed` carrying the raw string.
function _chat_tool_args(raw: string, provider: string) -> map<string, unknown> {
    baml.json.from_string<map<string, unknown>>(raw) catch_all (e) {
        _ => throw ai.errors.ParseFailed { provider: provider, raw_output: raw },
    }
}

function _chat_push_tool_uses(
    content: ai.content.Block[],
    calls: CcToolCall[],
    provider: string,
) -> void {
    for (let call in calls) {
        let empty: map<string, unknown> = {};
        let args = match (call.fn?.arguments) {
            let raw: string => {
                if (raw.trim() == "") {
                    empty
                } else {
                    _chat_tool_args(raw, provider)
                }
            },
            null => empty,
        };
        content
            .push(ai.content.ToolUse { id: call.id ?? "", name: call.fn?.name ?? "", args: args });
    }
}

/// One decoded response -> canonical content blocks, stop reason and usage.
///
/// `raw` is the response text, used only to populate `ai.errors.ParseFailed`
/// when the envelope carries no usable choice.
///
/// Divergence from both prior implementations, deliberate: more than one choice
/// is not an error here. `n > 1` is reachable through the `request_body`
/// passthrough, and rejecting the response the caller asked for would be
/// hostile; the first choice is used and the rest ignored.
function chat_parse(
    response: CcResponse,
    provider: string,
    raw: string,
    audio_mime: string = "audio/wav",
) -> ai.ModelTurn {
    let choices: CcChoice[] = response.choices ?? [];
    let choice: CcChoice = match (choices.at(0)) {
        let first: CcChoice => first,
        null => throw ai.errors.ParseFailed { provider: provider, raw_output: raw },
    };
    let message: CcMessage = match (choice.message) {
        let m: CcMessage => m,
        null => throw ai.errors.ParseFailed { provider: provider, raw_output: raw },
    };

    let content: ai.content.Block[] = [];
    if let reasoning: string = message.reasoning_content {
        if (reasoning != "") {
            content.push(ai.content.Reasoning { summary: reasoning });
        }
    }
    match (message.content) {
        let text: string => {
            content.push(ai.content.Text { text: text });
            null
        },
        let parts: CcContentPart[] => {
            for (let part in parts) {
                let kind = part.type ?? "";
                if (kind == "image_url") {
                    if let url: string = part.image_url?.url {
                        content.push(_chat_media_block(url));
                    }
                    null
                } else {
                    if let text: string = part.text {
                        content.push(ai.content.Text { text: text });
                    }
                    null
                };
            }
            null
        },
        null => null,
    };
    // Ollama / OpenRouter report generated images in a sibling array.
    let images: CcMessageImage[] = message.images ?? [];
    for (let image in images) {
        if let url: string = image.image_url?.url ?? image.url {
            content.push(_chat_media_block(url));
        }
    }
    // Audio output: the transcript is text the model produced, the bytes are a
    // media block. Both would otherwise be dropped on the floor.
    if let audio: CcMessageAudio = message.audio {
        if let transcript: string = audio.transcript {
            if (transcript != "") {
                content.push(ai.content.Text { text: transcript });
            }
        }
        if let data: string = audio.data {
            if (data != "") {
                content
                    .push(
                        ai.content
                            .Media
                            .new(
                                value = baml.media.Audio.from_base64(data, audio_mime),
                                provider_id = audio.id,
                            ),
                    );
            }
        }
    }
    let calls: CcToolCall[] = message.tool_calls ?? [];
    _chat_push_tool_uses(content, calls, provider);
    if let refusal: string = message.refusal {
        content.push(ai.content.Text { text: refusal });
    }

    let stop = if (calls.length() > 0) {
        ai.content.StopReason.ToolUse
    } else if (message.refusal != null) {
        ai.content.StopReason.Refused
    } else {
        match (choice.finish_reason) {
            let reason: string => chat_stop_reason(reason),
            null => ai.content.StopReason.Complete,
        }
    };

    let usage: ai.events.Usage? = match (response.usage) {
        let u: CcUsage => {
            ai.events.Usage {
                input_tokens: u.prompt_tokens ?? 0,
                output_tokens: u.completion_tokens ?? 0,
                cached_input_tokens: u.prompt_tokens_details?.cached_tokens,
                reasoning_tokens: u.completion_tokens_details?.reasoning_tokens,
            }
        },
        null => null,
    };

    ai.ModelTurn { content: content, stop_reason: stop, usage: usage }
}

// Transport + classification for the non-streaming path. This mirrors
// `ai.wire.send_as` but decodes through `baml.json.to`, which honours the
// `baml.FromJson` override that maps the `function` wire key onto `CcToolCall.fn`.
function _chat_send(
    request: baml.http.Request,
    provider: string,
    audio_mime: string = "audio/wav",
) -> ai.ModelTurn {
    let response = baml.http.send(request) catch_all (e) {
        let timeout: baml.errors.Timeout => throw timeout,
        _ => {
            throw ai.errors.NetworkFailure {
                provider: provider,
                detail: `transport failed before a response arrived: ${e.to_string()}`,
                raw_body: null,
            }
        },
    };
    let text = response.text() catch_all (e) {
        _ => {
            throw ai.errors.NetworkFailure {
                provider: provider,
                detail: "the response body could not be read completely",
                raw_body: null,
            }
        },
    };
    if (!response.ok()) {
        // `headers` is what carries a 429's `Retry-After` / `retry-after-ms`
        // into `RateLimited.retry_after_ms`, which `ai.clients.Backoff` honours
        // over its own computed delay.
        throw ai.errors.classify_http(
            provider,
            response.status_code,
            text,
            headers = response.headers,
        )
    }
    let decoded = baml.json.to<CcResponse>(baml.json.parse(text)) catch_all (e) {
        _ => throw ai.errors.ParseFailed { provider: provider, raw_output: text },
    };
    chat_parse(decoded, provider, text, audio_mime = audio_mime)
}

/// The one internal entry point: render, send, normalize — one model turn.
function chat_invoke(
    compat: ChatCompat,
    params: ChatParams,
    input: ai.ModelTurnInput,
) -> ai.ModelTurn {
    let request = chat_render(compat, params, input, false);
    _chat_send(request, compat.provider, audio_mime = _chat_audio_output_mime(params.request_body))
}

// ---------------------------------------------------------------------------
// Streaming (chat-completions SSE).
//
// Wire shape: every event is `data: {chunk}` with no `event:` name, terminated
// by the literal `data: [DONE]`. Text arrives as `choices[0].delta.content`;
// `choices[0].finish_reason` appears on the last content chunk; when
// `stream_options.include_usage` was sent, a final choice-less chunk carries
// `usage`.

class CcDelta {
    role: string?,
    content: string?,
    // A refusal streams in its own field, never in `content`. Without this the
    // turn ends empty and the caller never learns the model declined — the
    // non-streaming path surfaces `message.refusal` as text, so this does too.
    refusal: string?,
    reasoning_content: string?,
    tool_calls: CcToolCall[]?,
}

class CcStreamChoice {
    index: int?,
    delta: CcDelta?,
    finish_reason: string?,
}

class CcStreamChunk {
    id: string?,
    model: string?,
    choices: CcStreamChoice[]?,
    usage: CcUsage?,
}

// One tool call being assembled across chunks. The wire sends the id and name
// once, then the arguments JSON in fragments.
class ChatToolCallBuffer {
    index: int,
    id: string,
    name: string,
    arguments: string,
}

/// The stateful SSE decoder for one streamed turn.
///
/// `ai.stream.StreamEvent` is `TextDelta | TurnMeta | TurnDone` and
/// `TurnStream.final_turn()` only ever produces a single `Text` block, so
/// streamed tool calls cannot reach the runner today (PLAN §6 — extending
/// StreamEvent is its own stdlib change). They are still fully assembled here:
/// `tool_uses()` returns them once the stream ends, which is what a future
/// StreamEvent variant will emit and what the tests assert on.
class ChatStreamDecoder {
    provider: string,
    calls: ChatToolCallBuffer[],

    function new(provider: string) -> ChatStreamDecoder {
        ChatStreamDecoder { provider: provider, calls: [] }
    }

    // Extend (or open) the buffer for one tool-call delta.
    function _absorb_call(self, delta: CcToolCall, position: int) -> void {
        let index = delta.index ?? position;
        let existing: ChatToolCallBuffer? = self.calls.find((call) -> {
            call.index == index
        });
        let buffer = match (existing) {
            let found: ChatToolCallBuffer => found,
            null => {
                let fresh = ChatToolCallBuffer { index: index, id: "", name: "", arguments: "" };
                self.calls.push(fresh);
                fresh
            },
        };
        if let id: string = delta.id {
            buffer.id = id;
        }
        if let name: string = delta.fn?.name {
            buffer.name = `${buffer.name}${name}`;
        }
        if let fragment: string = delta.fn?.arguments {
            buffer.arguments = `${buffer.arguments}${fragment}`;
        }
    }

    /// One `SseStream.next()` batch -> provider-agnostic stream events.
    function decode(self, batch: string) -> ai.stream.StreamEvent[] {
        let out: ai.stream.StreamEvent[] = [];
        for (let raw in ai.stream.decode_sse_batch(batch)) {
            if let data: string = raw.data {
                if (data.trim() == "[DONE]") {
                    out.push(ai.stream.TurnDone {  });
                } else {
                    let parsed: CcStreamChunk? = baml.json.to<CcStreamChunk>(baml.json.parse(data)) catch_all (e) {
                        _ => null,
                    };
                    // A payload that claims to be a chunk (`{…}`) but does not
                    // decode is DATA LOSS, not noise: silently skipping it drops
                    // whatever text or tool fragment it carried. Anything else —
                    // a blank line, an OpenRouter `: PROCESSING` keepalive, a
                    // vendor sentinel — is still ignored.
                    if (parsed == null && data.trim().starts_with("{")) {
                        throw ai.errors.ParseFailed { provider: self.provider, raw_output: data }
                    }
                    if let chunk: CcStreamChunk = parsed {
                        let choices: CcStreamChoice[] = chunk.choices ?? [];
                        if let choice: CcStreamChoice = choices.at(0) {
                            if let text: string = choice.delta?.content {
                                if (text != "") {
                                    out.push(ai.stream.TextDelta { text: text });
                                }
                            }
                            if let refusal: string = choice.delta?.refusal {
                                if (refusal != "") {
                                    out.push(ai.stream.TextDelta { text: refusal });
                                }
                            }
                            let deltas: CcToolCall[] = choice.delta?.tool_calls ?? [];
                            let position = 0;
                            for (let call in deltas) {
                                self._absorb_call(call, position);
                                position += 1;
                            }
                            if let reason: string = choice.finish_reason {
                                out.push(
                                    ai.stream.TurnMeta {
                                        stop_reason: chat_stop_reason(reason),
                                        input_tokens: null,
                                        output_tokens: null,
                                    },
                                );
                            }
                        }
                        if let usage: CcUsage = chunk.usage {
                            out.push(
                                ai.stream.TurnMeta {
                                    stop_reason: null,
                                    input_tokens: usage.prompt_tokens,
                                    output_tokens: usage.completion_tokens,
                                },
                            );
                        }
                    }
                }
            }
        }
        out
    }

    /// The tool calls assembled from the stream, in wire order.
    function tool_uses(self) -> ai.content.ToolUse[] {
        let out: ai.content.ToolUse[] = [];
        for (let buffer in self.calls) {
            let empty: map<string, unknown> = {};
            let args = if (buffer.arguments.trim() == "") {
                empty
            } else {
                _chat_tool_args(buffer.arguments, self.provider)
            };
            out.push(ai.content.ToolUse { id: buffer.id, name: buffer.name, args: args });
        }
        out
    }
}

// `baml.http.fetch_sse` reports a non-2xx as `baml.errors.Io` whose message
// embeds the status and body: "SSE request failed with status 400: {…}". The
// status is the only thing standing between a 400 and a retry-safe
// NetworkFailure that a Retry wrapper would happily replay, so it is recovered
// here and run through the same classifier the non-streaming path uses.
function _chat_stream_failure(provider: string, detail: string) -> ai.errors.Failure {
    let marker = "SSE request failed with status ";
    if let start: int = detail.index_of(marker) {
        let rest = detail.slice(start + marker.length(), detail.length());
        if let colon: int = rest.index_of(":") {
            let code = int.parse(rest.slice(0, colon)) catch_all (e) {
                _ => 0,
            };
            if (code > 0) {
                return ai.errors.classify_http(provider, code, rest.slice(colon + 1, rest.length()).trim());
            }
        }
    }
    ai.errors.NetworkFailure {
        provider: provider,
        detail: `transport failed before a response arrived: ${detail}`,
        raw_body: null,
    }
}

function _chat_stream_open_failure(provider: string, error: unknown) -> never {
    match (error) {
        let timeout: baml.errors.Timeout => throw timeout,
        let cancelled: baml.panics.Cancelled => throw cancelled,
        _ => throw _chat_stream_failure(provider, error.to_string()),
    }
}

/// The streaming entry point: the same request as `chat_invoke` with
/// `stream: true`, wrapped in an `ai.stream.TurnStream` over the SSE connection.
function chat_invoke_stream(
    compat: ChatCompat,
    params: ChatParams,
    input: ai.ModelTurnInput,
) -> ai.stream.TurnStream {
    let request = chat_render(compat, params, input, true);
    let decoder = ChatStreamDecoder.new(provider = compat.provider);
    let sse = baml.http.fetch_sse(request) catch_all (e) {
        _ => _chat_stream_open_failure(compat.provider, e),
    };
    // Chat completions has an explicit terminator — the literal `data: [DONE]`
    // — so a connection that ends without it lost data. `require_terminal` makes
    // the stream throw `ai.errors.NetworkFailure` there instead of reporting the
    // partial text as a completed turn, which is what silently fed truncated
    // answers to `require_done` / `Retry`.
    ai.stream
        .TurnStream
        .from_sse(
            sse,
            (batch) -> {
                decoder.decode(batch)
            },
            require_terminal = compat.provider,
        )
}
