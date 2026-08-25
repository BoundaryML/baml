// The AWS Bedrock Converse API (`POST /model/{modelId}/converse`), modeled as
// typed wire classes on both sides — PLAN §1.5.
//
// This is the pure-BAML replacement for `crates/sys_llm/src/build_request/
// bedrock.rs`, `crates/sys_llm/src/parse_response/bedrock.rs` and the
// `forks/aws-bedrock` serde model. Only the SigV4 signature stays Rust
// (`root.internal.sign_request`, see auth.baml) because the stdlib has no
// HMAC-SHA256.
//
// Two things about the Converse wire format shape the classes below:
//
//   1. `ContentBlock`, `SystemContentBlock`, `ImageSource`, … are Smithy
//      *unions*, serialized externally tagged: exactly one member key is
//      present (`{"text": "hi"}`, `{"image": {…}}`). BAML models each as a
//      class whose members are all optional, with one static constructor per
//      variant; `_prune_nulls` (below) removes the unset members so the wire
//      bytes carry exactly one key, matching the SDK byte for byte.
//   2. The model id is a URI *path label*, not a body field. It is
//      percent-encoded so an ARN or an inference-profile id stays one path
//      segment (`/` -> %2F, `:` -> %3A).
//
// Streaming is deliberately absent: ConverseStream frames responses as binary
// `application/vnd.amazon.eventstream`, which `baml.http.fetch_sse` cannot
// consume. `root.BedrockClient` therefore does not implement
// `ai.stream.StreamingClient`, and asking for a stream raises
// `ai.errors.StreamingUnsupported`. sys_llm never streamed Bedrock either, so
// this is zero regression (PLAN §6).

// ---------------------------------------------------------------------------
// 1. Model-id path encoding
//
// `forks/aws-bedrock/src/lib.rs:24-62` encodes the model id with `LABEL_SET`,
// a copy of `aws_smithy_http::urlencode::BASE_SET`. Reimplemented here over
// the id's UTF-8 bytes: every byte outside the RFC 3986 *unreserved* set
// (`A-Z a-z 0-9 - . _ ~`) becomes `%XX` with uppercase hex, and every
// non-ASCII byte is always encoded. That set is exactly LABEL_SET's
// complement — space, the sub-delims, `/`, `:`, `%`, the controls and DEL are
// all encoded — so the produced path is byte-identical to the AWS SDK's.
// ---------------------------------------------------------------------------

// True for the RFC 3986 unreserved characters, which never need escaping.
function _is_unreserved_byte(byte: int) -> bool {
    (byte >= 48 && byte <= 57) // 0-9
        || (byte >= 65 && byte <= 90) // A-Z
        || (byte >= 97 && byte <= 122) // a-z
        || byte == 45 // -
        || byte == 46 // .
        || byte == 95 // _
        || byte == 126 // ~
}

/// Percent-encode `value` as a single URI path label: `/` becomes `%2F` and
/// `:` becomes `%3A`, so a Bedrock ARN or inference-profile id survives as one
/// path segment. Also used for query-parameter keys and values, whose SigV4
/// canonical form re-encodes with the same unreserved set.
function encode_path_label(value: string) -> string {
    let hex = "0123456789ABCDEF";
    let out: string[] = [];
    for (let byte in value.to_utf8().to_array()) {
        if (_is_unreserved_byte(byte)) {
            out.push(baml.String.from_code_points([byte]));
        } else {
            // `at` is index-checked; both indices are provably 0..15 here.
            out.push(`%${hex.at(byte / 16) ?? ""}${hex.at(byte % 16) ?? ""}`);
        }
    }
    out.join("")
}

/// The Converse request path for a model id, inference-profile id, or ARN.
function converse_model_path(model_id: string) -> string {
    `/model/${encode_path_label(model_id)}/converse`
}

// ---------------------------------------------------------------------------
// 2. Request wire classes
// ---------------------------------------------------------------------------

/// An object in an S3 bucket, for media the account already stores there.
class S3Location {
    uri: string,
    bucketOwner: string?,
}

/// `ImageSource` / `VideoSource` / `DocumentSource` / `AudioSource` union:
/// base64 `bytes` or an `s3Location`. All four media unions have the same two
/// members in the current Converse API.
class MediaSource {
    bytes: string?,
    s3Location: S3Location?,
}

class ImageBlock {
    format: string, // png | jpeg | gif | webp
    source: MediaSource,
}

class VideoBlock {
    format: string, // mp4 | mpeg | mkv | mov | flv | webm | three_gp
    source: MediaSource,
}

class DocumentBlock {
    format: string, // pdf
    name: string,
    source: MediaSource,
}

/// `AudioSource` is a `bytes` | `s3Location` union exactly like the other three
/// media sources (API_runtime_AudioSource), so it reuses `MediaSource`. An
/// earlier revision modeled bytes only and hard-rejected `s3://` audio; that
/// was wrong.
class AudioBlock {
    // mp3 | opus | wav | aac | flac | mp4 | ogg | mkv | mka | x-aac | m4a |
    // mpeg | mpga | pcm | webm
    format: string,
    source: MediaSource,
}

/// A prompt-caching checkpoint. Everything *before* the checkpoint in the
/// request is eligible for reuse on the next call. `"default"` is the only
/// documented type.
class CachePointBlock {
    type: string,
}

class ToolUseBlock {
    toolUseId: string,
    name: string,
    input: map<string, unknown>,
}

/// One piece of a tool's result. Only `text` is modeled: `ai.events` carries
/// tool output as a string (`ToolCompleted.output` / `ToolFailed.message`), so
/// the `json`/`image`/`document` members of the wire union have no source.
class ToolResultContentBlock {
    text: string,
}

class ToolResultBlock {
    toolUseId: string,
    content: ToolResultContentBlock[],
    status: string?, // success | error
}

/// The `ContentBlock` union of a Converse message. Exactly one member is set;
/// build with the static constructors rather than the literal.
class ContentBlock {
    text: string?,
    image: ImageBlock?,
    video: VideoBlock?,
    document: DocumentBlock?,
    audio: AudioBlock?,
    toolUse: ToolUseBlock?,
    toolResult: ToolResultBlock?,
    cachePoint: CachePointBlock?,

    function of_text(text: string) -> ContentBlock {
        ContentBlock { ..._empty_content_block(), text: text }
    }

    function of_image(image: ImageBlock) -> ContentBlock {
        ContentBlock { ..._empty_content_block(), image: image }
    }

    function of_video(video: VideoBlock) -> ContentBlock {
        ContentBlock { ..._empty_content_block(), video: video }
    }

    function of_document(document: DocumentBlock) -> ContentBlock {
        ContentBlock { ..._empty_content_block(), document: document }
    }

    function of_audio(audio: AudioBlock) -> ContentBlock {
        ContentBlock { ..._empty_content_block(), audio: audio }
    }

    function of_tool_use(tool_use: ToolUseBlock) -> ContentBlock {
        ContentBlock { ..._empty_content_block(), toolUse: tool_use }
    }

    function of_tool_result(tool_result: ToolResultBlock) -> ContentBlock {
        ContentBlock { ..._empty_content_block(), toolResult: tool_result }
    }

    function of_cache_point() -> ContentBlock {
        ContentBlock { ..._empty_content_block(), cachePoint: CachePointBlock { type: "default" } }
    }
}

function _empty_content_block() -> ContentBlock {
    ContentBlock {
        text: null,
        image: null,
        video: null,
        document: null,
        audio: null,
        toolUse: null,
        toolResult: null,
        cachePoint: null,
    }
}

/// The `SystemContentBlock` union. Converse system content is text (or a cache
/// checkpoint) only — media in a system message is rejected, matching
/// `build_request/bedrock.rs:174-176`.
class SystemContentBlock {
    text: string?,
    cachePoint: CachePointBlock?,

    function of_text(text: string) -> SystemContentBlock {
        SystemContentBlock { text: text, cachePoint: null }
    }

    function of_cache_point() -> SystemContentBlock {
        SystemContentBlock { text: null, cachePoint: CachePointBlock { type: "default" } }
    }
}

class Message {
    role: string, // user | assistant
    content: ContentBlock[],
}

/// `inferenceConfig`. Omitted from the body entirely when every member is
/// unset (`forks/aws-bedrock/src/lib.rs:249-256`).
class InferenceConfig {
    maxTokens: int?,
    temperature: float?,
    topP: float?,
    stopSequences: string[]?,
}

/// A tool's parameter schema. The schema is genuinely open-ended (arbitrary
/// JSON Schema authored by the tool), so it is one of the two `baml.json.json`
/// escape hatches in this file — see `_prune_nulls`.
///
/// The Converse wire key is `json`, but that name collides with
/// `pydantic.BaseModel.json` in generated Python SDKs, so the field is
/// `schema` with an explicit wire mapping (the same reserved-word pattern as
/// the chat client's `fn` ↔ `"function"`).
class ToolInputSchema {
    json_schema: baml.json.json,

    implements baml.ToJson {
        function to_json(self) -> baml.json.json {
            let out: map<string, baml.json.json> = {  };
            let _ = out.set("json", self.json_schema);
            out
        }
    }
}

class ToolSpec {
    name: string,
    description: string?,
    inputSchema: ToolInputSchema,
}

/// The `Tool` union: a `toolSpec`, or a `cachePoint` terminating the cacheable
/// tool prefix.
class ToolEntry {
    toolSpec: ToolSpec?,
    cachePoint: CachePointBlock?,

    function of_spec(spec: ToolSpec) -> ToolEntry {
        ToolEntry { toolSpec: spec, cachePoint: null }
    }

    function of_cache_point() -> ToolEntry {
        ToolEntry { toolSpec: null, cachePoint: CachePointBlock { type: "default" } }
    }
}

/// A specific tool the model must call.
class SpecificToolChoice {
    name: string,
}

/// The `ToolChoice` union: `{"auto":{}}`, `{"any":{}}`, or
/// `{"tool":{"name":"…"}}`.
class ToolChoice {
    auto: map<string, unknown>?,
    any: map<string, unknown>?,
    tool: SpecificToolChoice?,
}

class ToolConfig {
    tools: ToolEntry[],
    toolChoice: ToolChoice?,
}

/// The Converse request body. The model id and the credentials are NOT part of
/// it — they live in the URL path and the signed headers respectively
/// (asserted by `bedrock_no_model_or_creds_in_body`,
/// `build_request/bedrock.rs:671-677`).
class ConverseRequest {
    messages: Message[],
    system: SystemContentBlock[]?,
    inferenceConfig: InferenceConfig?,
    toolConfig: ToolConfig?,
    /// Model-specific parameters Converse has no first-class field for
    /// (`top_k`, `anthropic_beta`, …). Open-ended by definition — the second
    /// `baml.json.json` escape hatch.
    additionalModelRequestFields: baml.json.json?,
}

// ---------------------------------------------------------------------------
// 3. Null pruning
//
// `baml.json.to_json` emits `"field": null` for an unset optional, but the
// Converse endpoint expects absent members (the SDK serializer uses
// `skip_serializing_if = Option::is_none` throughout,
// `forks/aws-bedrock/src/lib.rs:269-276`), and an externally tagged union with
// eight null members is not a union at all. So the serialized request is run
// through `_prune_nulls` before it goes on the wire.
//
// Three keys are copied VERBATIM without recursing, because their values are
// authored elsewhere (by the user or by the model) and a `null` inside them is
// meaningful data rather than an unset member:
//
//   * `additionalModelRequestFields` — the model-specific parameter blob;
//   * `json` — a tool's `inputSchema.json` (a JSON Schema may legitimately
//     say `{"default": null}`);
//   * `input` — a replayed `toolUse.input`, i.e. the arguments the MODEL
//     authored for a tool call. Pruning them rewrites history: a tool whose
//     schema has nullable parameters would see its explicit `"filter": null`
//     silently become absent on every replay turn. `input` is the only key
//     with that name in a Converse request body (the tool schema lives under
//     `inputSchema.json`, itself already opaque), so a flat key test is
//     enough.
//
// A `null` value is dropped before the key is even consulted, so a null blob
// (no passthrough configured) still disappears.
// ---------------------------------------------------------------------------

function _is_opaque_key(key: string) -> bool {
    key == "additionalModelRequestFields" || key == "json" || key == "input"
}

function _prune_nulls(value: baml.json.json) -> baml.json.json {
    match (value) {
        let object: map<string, baml.json.json> => {
            let out: map<string, baml.json.json> = {};
            for (let key in object.keys()) {
                let member = object.get(key);
                match (member) {
                    null => null,
                    _ => {
                        if (_is_opaque_key(key)) {
                            let _ = out.set(key, member);
                            null
                        } else {
                            let _ = out.set(key, _prune_nulls(member));
                            null
                        }
                    },
                };
            }
            out
        },
        let items: baml.json.json[] => {
            let out: baml.json.json[] = [];
            for (let item in items) {
                out.push(_prune_nulls(item));
            }
            out
        },
        _ => value,
    }
}

// ---------------------------------------------------------------------------
// 4. Media
//
// Ported from `build_request/bedrock.rs:191-346` together with the per-kind
// URL policy in `crates/sys_llm/src/baml_std.rs:340-346`: image, audio and PDF
// URLs are fetched and inlined as base64; video URLs are passed through, and
// Bedrock accepts only `s3://` ones. Anything else — an `https://` video, a
// `gs://` URL — is rejected with the media's URL in the message, exactly like
// `build_request/bedrock.rs:206-213`.
//
// One deliberate widening: an `s3://` URL is never fetched, for ANY kind.
// `baml.http.fetch` cannot speak S3, so inlining one could only ever fail —
// while Bedrock reads S3 objects itself, for all four media unions. So the
// per-kind inline policy applies to http(s) URLs and `s3://` always lowers to
// an `s3Location`.
// ---------------------------------------------------------------------------

// Every LOCAL, pre-send rejection in this file throws ONE error identity:
// `ai.errors.InvalidRequest` with a null status code — the same class
// `converse_url` and `sign` already throw for an unresolvable region or
// missing credentials. It is an `ai.errors.Failure`, so unlike
// `baml.errors.InvalidArgument` it survives `BedrockClient.invoke`'s
// `ai.errors.normalize` with its identity (and its `RetrySafety`) intact
// instead of being flattened into an untyped `baml.errors.UnknownError`.
function _reject(detail: string) -> ai.errors.InvalidRequest {
    ai.errors.InvalidRequest {
        provider: "aws-bedrock",
        status_code: null,
        detail: detail,
        raw_body: null,
    }
}

function _is_s3_url(url: string?) -> bool {
    (url ?? "").starts_with("s3://")
}

function _image_format(mime: string) -> string {
    match (mime) {
        "image/png" => "png",
        "image/jpeg" | "image/jpg" => "jpeg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        _ => throw _reject(`unsupported image format for Bedrock: ${mime}`),
    }
}

// `VideoBlock.format`: mkv | mov | mp4 | webm | flv | mpeg | mpg | wmv |
// three_gp. sys_llm's map (`build_request/bedrock.rs:280-292`) predates `mpg`
// and `wmv`; both are carried here.
function _video_format(mime: string) -> string {
    match (mime) {
        "video/mp4" => "mp4",
        "video/mpeg" => "mpeg",
        "video/mpg" | "video/x-mpg" => "mpg",
        "video/x-ms-wmv" | "video/wmv" => "wmv",
        "video/x-matroska" => "mkv",
        "video/quicktime" => "mov",
        "video/x-flv" => "flv",
        "video/webm" => "webm",
        "video/3gpp" => "three_gp",
        _ => throw _reject(`unsupported video format for Bedrock: ${mime}`),
    }
}

// `AudioBlock.format`: mp3 | opus | wav | aac | flac | mp4 | ogg | mkv | mka |
// x-aac | m4a | mpeg | mpga | pcm | webm. sys_llm carried five of them
// (`build_request/bedrock.rs:267-278`); the enum has widened since, so the
// containers with an unambiguous IANA/common mime type are all mapped.
function _audio_format(mime: string) -> string {
    match (mime) {
        "audio/mpeg" | "audio/mp3" => "mp3",
        "audio/wav" | "audio/x-wav" | "audio/wave" => "wav",
        "audio/flac" | "audio/x-flac" => "flac",
        "audio/ogg" => "ogg",
        "audio/opus" => "opus",
        "audio/aac" => "aac",
        "audio/x-aac" => "x-aac",
        "audio/mp4" => "mp4",
        "audio/m4a" | "audio/x-m4a" => "m4a",
        "audio/x-matroska" => "mka",
        "audio/mpga" => "mpga",
        "audio/pcm" | "audio/x-pcm" | "audio/L16" => "pcm",
        "audio/webm" => "webm",
        _ => throw _reject(`unsupported audio format for Bedrock: ${mime}`),
    }
}

// base64 bytes, else an s3:// location. Any other URL is a hard error: Bedrock
// reads media from the request body or from S3, never from an arbitrary host.
function _media_source(
    resolved: ai.wire.ResolvedMedia,
    kind: string,
    preview: bool = false,
) -> MediaSource {
    if let data: string = resolved.base64 {
        return MediaSource { bytes: data, s3Location: null };
    }
    if let url: string = resolved.url {
        if (url.starts_with("s3://")) {
            return MediaSource { bytes: null, s3Location: S3Location { uri: url, bucketOwner: null } };
        }
        if (preview) {
            throw ai.errors.PreviewUnsupported {
                provider: "aws-bedrock",
                detail: `Bedrock request preview only supports s3:// ${kind} URLs; it cannot download ${url}`,
            }
        }
        throw _reject(`Bedrock requires s3:// URIs for ${kind} URLs, got: ${url}`)
    }
    throw _reject(`Bedrock ${kind} content could not be resolved`)
}

function _image_block(image: baml.media.Image, preview: bool = false) -> ContentBlock {
    let resolved = if (preview) {
        ai.wire.resolve_media_preview(image)
    } else {
        ai.wire.resolve_media(image, !_is_s3_url(image.url()))
    };
    ContentBlock.of_image(
        ImageBlock {
            format: _image_format(resolved.mime_type),
            source: _media_source(resolved, "image", preview = preview),
        },
    )
}

function _video_block(video: baml.media.Video, preview: bool = false) -> ContentBlock {
    // fetch_url = false: a video is far too large to inline, so Bedrock reads
    // it from S3 and a non-s3 URL is rejected below.
    let resolved = if (preview) {
        ai.wire.resolve_media_preview(video)
    } else {
        ai.wire.resolve_media(video, false)
    };
    ContentBlock.of_video(
        VideoBlock {
            format: _video_format(resolved.mime_type),
            source: _media_source(resolved, "video", preview = preview),
        },
    )
}

function _document_block(pdf: baml.media.Pdf, preview: bool = false) -> ContentBlock {
    let resolved = if (preview) {
        ai.wire.resolve_media_preview(pdf)
    } else {
        ai.wire.resolve_media(pdf, !_is_s3_url(pdf.url()))
    };
    if (resolved.mime_type != "application/pdf") {
        throw _reject(`unsupported document format for Bedrock: ${resolved.mime_type}`)
    }
    ContentBlock.of_document(
        DocumentBlock {
            format: "pdf",
            // Bedrock requires a name; the SDK path used this constant too
            // (`build_request/bedrock.rs:323`).
            name: "document",
            source: _media_source(resolved, "document", preview = preview),
        },
    )
}

function _audio_block(audio: baml.media.Audio, preview: bool = false) -> ContentBlock {
    // `AudioSource` is `bytes | s3Location` like every other media union, so
    // audio takes exactly the image path: inline an http(s) URL, hand an
    // `s3://` one to Bedrock.
    let resolved = if (preview) {
        ai.wire.resolve_media_preview(audio)
    } else {
        ai.wire.resolve_media(audio, !_is_s3_url(audio.url()))
    };
    ContentBlock.of_audio(
        AudioBlock {
            format: _audio_format(resolved.mime_type),
            source: _media_source(resolved, "audio", preview = preview),
        },
    )
}

// ---------------------------------------------------------------------------
// 5. Prompt lowering
// ---------------------------------------------------------------------------

// Converse splits system content off the alternating message list, the same
// shape the Anthropic client uses.
class BedrockPrompt {
    system: SystemContentBlock[],
    messages: Message[],
}

// One rendered prompt message -> Converse content blocks.
function _prompt_content(message: ai.PromptMessage, preview: bool = false) -> ContentBlock[] {
    let blocks: ContentBlock[] = [];
    for (let part in message.parts) {
        match (part) {
            let text: string => {
                // Converse rejects empty and whitespace-only text blocks, and a
                // rendered prompt routinely produces them around media parts —
                // the same filter the Anthropic client applies in
                // `_anthropic_prompt_blocks`.
                if (text.trim() != "") {
                    blocks.push(ContentBlock.of_text(text));
                }
                null
            },
            let image: baml.media.Image => {
                blocks.push(_image_block(image, preview = preview));
                null
            },
            let audio: baml.media.Audio => {
                blocks.push(_audio_block(audio, preview = preview));
                null
            },
            let video: baml.media.Video => {
                blocks.push(_video_block(video, preview = preview));
                null
            },
            let pdf: baml.media.Pdf => {
                blocks.push(_document_block(pdf, preview = preview));
                null
            },
        };
    }
    blocks
}

// System content is text-only; a media part in a system message is rejected
// rather than silently dropped (`build_request/bedrock.rs:174-176`).
function _prompt_system(message: ai.PromptMessage) -> SystemContentBlock[] {
    let blocks: SystemContentBlock[] = [];
    for (let part in message.parts) {
        match (part) {
            let text: string => {
                // Blank text is dropped here too: Converse rejects an empty
                // `text` member wherever it appears.
                if (text.trim() != "") {
                    blocks.push(SystemContentBlock.of_text(text));
                }
                null
            },
            _ => {
                throw _reject("Bedrock system messages do not support media content")
            },
        };
    }
    blocks
}

/// The per-message metadata key that requests a Bedrock prompt-caching
/// checkpoint after the message's content.
///
/// The engine emits `{"cachePoint":{"type":"default"}}` whenever a message
/// carries `cache_control` metadata
/// (`engine/.../aws_client.rs:123-155`). BAML's neutral spelling of that is a
/// `ai.PromptMessage.metadata` entry, and Bedrock's own name for the feature
/// is a cache *point*, so the key is `cache_point` — any non-null value
/// (`true` is the idiomatic one) turns the checkpoint on.
///
/// NOTE: `ai.internal.make_role` currently builds every role marker with empty
/// metadata, so nothing populates this yet from the prompt DSL. Until that
/// lands, use the client's `cache_system_prompt` / `cache_tools` options,
/// which do not depend on per-message metadata.
function _wants_cache_point(message: ai.PromptMessage) -> bool {
    match (message.metadata.get("cache_point")) {
        null => false,
        _ => true,
    }
}

// The Converse conversation roles are `user` and `assistant` only. `system`
// is hoisted out above; every other authored role (including the role-less
// default and `tool`) becomes `user`, matching the native Anthropic client and
// PLAN §5's role-less default. sys_llm hard-errored on unknown roles instead
// (`build_request/bedrock.rs:143-151`); collapsing keeps the four native
// clients consistent with one another.
function _conversation_role(role: string) -> string {
    if (role == "assistant") {
        "assistant"
    } else {
        "user"
    }
}

// MULTI-SYSTEM: every `system` message merges into the one top-level `system`
// array, deliberately diverging from PLAN §5's "others: demote 2nd+ to user"
// row. That row exists for providers whose wire format has room for exactly
// one system string, where a second one has nowhere to go; Converse's `system`
// IS a `SystemContentBlock[]`, so merging is the format's own answer and
// demoting would silently move authored instructions into the conversation.
// The native Anthropic client (`messages.baml:308-337`) merges for the same
// reason.
function _lower_prompt(prompt: ai.Prompt, preview: bool = false) -> BedrockPrompt {
    let system: SystemContentBlock[] = [];
    let messages: Message[] = [];
    for (let message in prompt.messages()) {
        if (message.role == "system") {
            for (let block in _prompt_system(message)) {
                system.push(block);
            }
            if (_wants_cache_point(message)) {
                system.push(SystemContentBlock.of_cache_point());
            }
            null
        } else {
            let content = _prompt_content(message, preview = preview);
            // A message that lowered to nothing (all-whitespace text) is
            // dropped: `content: []` is a ValidationException, and a lone
            // `cachePoint` with nothing to cache is no better.
            if (content.length() > 0) {
                if (_wants_cache_point(message)) {
                    content.push(ContentBlock.of_cache_point());
                }
                messages.push(Message { role: _conversation_role(message.role), content: content });
            }
            null
        };
    }
    BedrockPrompt { system: system, messages: messages }
}

// ---------------------------------------------------------------------------
// 6. Journal lowering (NET-NEW: neither sys_llm nor the engine replayed tools
// on Bedrock). Mirrors the Anthropic client: Converse wants `toolResult`
// blocks inside a user message that directly follows the assistant message
// holding the matching `toolUse`, so consecutive results group into ONE user
// message.
// ---------------------------------------------------------------------------

// A lowered message plus whether it is an open tool-result group.
class PendingMessage {
    message: Message,
    tool_results: bool,
}

function _assistant_content(content: ai.content.Block[]) -> ContentBlock[] {
    let out: ContentBlock[] = [];
    for (let block in content) {
        match (block) {
            let text: ai.content.Text => {
                out.push(ContentBlock.of_text(text.text));
                null
            },
            // Dropped: Converse replays reasoning as `reasoningContent` with
            // the provider's opaque `signature`, which `ai.content.Reasoning`
            // (a readable summary) cannot reproduce; replaying it unsigned is
            // rejected.
            let reasoning: ai.content.Reasoning => null,
            // Dropped: a generated media payload is an output, not replayable
            // model input.
            let media: ai.content.Media => null,
            let use: ai.content.ToolUse => {
                out.push(
                    ContentBlock.of_tool_use(
                        ToolUseBlock { toolUseId: use.id, name: use.name, input: use.args },
                    ),
                );
                null
            },
        };
    }
    out
}

/// `ToolResultBlock.status` "is only supported by Amazon Nova and Anthropic
/// Claude 3 and 4 models" (API_runtime_ToolResultBlock). Sending it to any
/// other family ships a member that model does not accept, so it is gated on a
/// substring of the model id — which covers a bare id
/// (`anthropic.claude-3-5-…`), a cross-region inference profile
/// (`us.amazon.nova-…`) and a full ARN
/// (`…:foundation-model/anthropic.claude-…`) alike. An unrecognized model (a
/// custom-model ARN, a provisioned-throughput ARN, Mistral/Llama/DeepSeek)
/// omits the field; the result's text still carries the error message, so only
/// the flag is lost.
function _supports_tool_result_status(model: string) -> bool {
    let id = model.to_lower_case();
    id.includes("anthropic.claude") || id.includes("amazon.nova")
}

// Append one toolResult block, extending the trailing group when it is open.
function _push_tool_result(
    pending: PendingMessage[],
    id: string,
    output: string,
    is_error: bool,
    with_status: bool,
) -> void {
    let status: string? = if (!with_status) {
        null
    } else if (is_error) {
        "error"
    } else {
        "success"
    };
    let block = ContentBlock.of_tool_result(
        ToolResultBlock {
            toolUseId: id,
            content: [ToolResultContentBlock { text: output }],
            status: status,
        },
    );
    // `.at` is null on an empty list, which covers the no-messages case.
    if let last: PendingMessage = pending.at(pending.length() - 1) {
        if (last.tool_results) {
            last.message.content.push(block);
            return;
        }
    }
    pending.push(
        PendingMessage { message: Message { role: "user", content: [block] }, tool_results: true },
    );
}

// `ai.RunStarted` / `ai.ToolRequested` / `ai.Usage` / `ai.FinalProduced` are
// journal-only and skip; skipping never closes an open tool-result group.
function _lower_journal(journal: ai.Journal, with_status: bool) -> Message[] {
    let pending: PendingMessage[] = [];
    for (let event in journal.entries()) {
        match (event) {
            let user: ai.events.UserMessage => {
                pending.push(
                    PendingMessage {
                        message: Message {
                            role: "user",
                            content: [ContentBlock.of_text(user.content)],
                        },
                        tool_results: false,
                    },
                );
                null
            },
            let assistant: ai.events.AssistantMessage => {
                let content = _assistant_content(assistant.content);
                // An assistant turn that lowers to nothing is dropped rather
                // than sent as `content: []`, which Converse rejects with a
                // ValidationException. A reasoning-only turn is exactly that
                // shape: `_assistant_content` drops Reasoning and Media, and
                // `ai.wire.sanitize_for_client` KEEPS same-client reasoning,
                // so nothing upstream removes it. Mirrors the Anthropic
                // client's guard in `_anthropic_lower_journal`.
                if (content.length() > 0) {
                    pending.push(
                        PendingMessage {
                            message: Message { role: "assistant", content: content },
                            tool_results: false,
                        },
                    );
                }
                null
            },
            let completed: ai.events.ToolCompleted => {
                _push_tool_result(pending, completed.id, completed.output, false, with_status);
            },
            let failed: ai.events.ToolFailed => {
                _push_tool_result(pending, failed.id, failed.message, true, with_status);
            },
            _ => null,
        };
    }
    let out: Message[] = [];
    for (let entry in pending) {
        out.push(entry.message);
    }
    out
}

// ---------------------------------------------------------------------------
// 7. toolConfig
// ---------------------------------------------------------------------------

// `tool_choice` option -> the Converse union. "auto" and "any" are the two
// documented modes; anything else names a specific tool.
function _tool_choice(choice: string) -> ToolChoice {
    let empty: map<string, unknown> = { };
    match (choice) {
        "auto" => ToolChoice { auto: empty, any: null, tool: null },
        "any" | "required" => ToolChoice { auto: null, any: empty, tool: null },
        _ => ToolChoice { auto: null, any: null, tool: SpecificToolChoice { name: choice } },
    }
}

function _tool_config(
    toolbox: ai.tools.Toolbox,
    tool_choice: string?,
    cache_tools: bool,
) -> ToolConfig? {
    if (toolbox.is_empty()) {
        return null;
    }
    let entries: ToolEntry[] = [];
    for (let tool in toolbox.list()) {
        entries.push(
            ToolEntry.of_spec(
                ToolSpec {
                    name: tool.name,
                    description: tool.description,
                    inputSchema: ToolInputSchema { json_schema: baml.json.to_json(tool.input_schema) },
                },
            ),
        );
    }
    if (cache_tools) {
        entries.push(ToolEntry.of_cache_point());
    }
    // `toolChoice` is omitted unless the caller asked for one: it is optional
    // in the API, `auto` is the default, and several Bedrock model families
    // reject the field outright.
    let selection: ToolChoice? = match (tool_choice) {
        null => null,
        let choice: string => _tool_choice(choice),
    };
    ToolConfig { tools: entries, toolChoice: selection }
}

// ---------------------------------------------------------------------------
// 8. Request assembly
// ---------------------------------------------------------------------------

function _inference_config(client: root.BedrockClient) -> InferenceConfig? {
    if let limit: int = client.max_tokens {
        // The wire field is an i32 (`forks/aws-bedrock/src/lib.rs:240`);
        // sys_llm rejected anything wider rather than letting it wrap
        // (`build_request/bedrock.rs:359-369`).
        if (limit > 2147483647 || limit < 0) {
            throw _reject(
                `max_tokens value ${limit} is out of the supported range (0..=2147483647)`,
            )
        }
    }
    let stops: string[]? = match (client.stop_sequences) {
        null => null,
        let sequences: string[] => {
            if (sequences.length() == 0) {
                null
            } else {
                sequences
            }
        },
    };
    if (client.max_tokens == null
        && client.temperature == null
        && client.top_p == null
        && stops == null) {
        return null;
    }
    InferenceConfig {
        maxTokens: client.max_tokens,
        temperature: client.temperature,
        topP: client.top_p,
        stopSequences: stops,
    }
}

// A system-only or assistant-first prompt has no leading user message, which
// Converse rejects ("A conversation must start with a user message"). Fold the
// system blocks into a leading user message in that case — the same fallback
// the Anthropic client uses (`anthropic/ns_internal/messages.baml:270-289`).
function _system_as_user(system: SystemContentBlock[]) -> Message {
    let content: ContentBlock[] = [];
    for (let block in system) {
        if let text: string = block.text {
            content.push(ContentBlock.of_text(text));
            null
        } else {
            content.push(ContentBlock.of_cache_point());
            null
        };
    }
    Message { role: "user", content: content }
}

// Converse requires strict user/assistant alternation, and several paths here
// can put two same-role messages side by side: a dropped assistant turn (a
// reasoning-only reply lowers to no content) leaves the user turns that
// bracketed it adjacent, the rendered prompt can end on the same role the
// journal opens with, and consecutive toolResults arrive as separate `user`
// messages when a journal-only event splits their group.
//
// Merging by concatenating content lists repairs every one of those, not just
// the shape each guard was written for. Block order inside each message is
// preserved and messages are joined in order, so a `cachePoint` still sits
// after exactly the blocks it was marking. `_push_tool_result`'s grouping
// becomes a special case of this.
function _merge_adjacent_roles(messages: Message[]) -> Message[] {
    let out: Message[] = [];
    for (let message in messages) {
        if let last: Message = out.at(out.length() - 1) {
            if (last.role == message.role) {
                for (let block in message.content) {
                    last.content.push(block);
                }
                continue;
            }
        }
        out.push(Message { role: message.role, content: message.content.concat([]) });
    }
    out
}

/// The typed Converse body for one model turn, before serialization.
function converse_request(
    client: root.BedrockClient,
    input: ai.ModelTurnInput,
    preview: bool = false,
) -> ConverseRequest {
    let prompt = _lower_prompt(
        input.prompt(ai.internal.build_output_format(input.output_type)),
        preview = preview,
    );
    let system = prompt.system;
    if (client.cache_system_prompt && system.length() > 0) {
        system.push(SystemContentBlock.of_cache_point());
    }
    let messages = prompt.messages;
    let journal_messages = _lower_journal(
        ai.wire.sanitize_for_client(input.journal, client.id()),
        _supports_tool_result_status(client.model),
    );
    for (let message in journal_messages) {
        messages.push(message);
    }

    // Converse requires a non-empty message list whose first message is a
    // `user` one. Two shapes miss that; both mirror the Anthropic client
    // (`messages.baml:556-577`).
    let leading_role = messages.at(0)?.role ?? "";
    // (a) System-only, or assistant-first WITH system content: promote the
    //     system blocks into the leading user message rather than flattening
    //     an otherwise well-formed prompt.
    let folded = leading_role != "user" && system.length() > 0;
    // (b) Assistant-first with NO system content: nothing to promote, so
    //     synthesize the leading user turn the API insists on. Without this
    //     the request is a guaranteed ValidationException.
    let synthesized = messages.length() > 0 && leading_role != "user" && system.length() == 0;
    let wire_messages = _merge_adjacent_roles(
        if (folded) {
            [_system_as_user(system)].concat(messages)
        } else if (synthesized) {
            [Message { role: "user", content: [ContentBlock.of_text("(continue)")] }].concat(
                messages,
            )
        } else {
            messages
        },
    );
    if (wire_messages.length() == 0) {
        throw _reject("the rendered prompt and journal produced no message content")
    }
    let wire_system: SystemContentBlock[]? = if (folded || system.length() == 0) {
        null
    } else {
        system
    };

    ConverseRequest {
        messages: wire_messages,
        system: wire_system,
        inferenceConfig: _inference_config(client),
        toolConfig: _tool_config(input.toolbox, client.tool_choice, client.cache_tools),
        additionalModelRequestFields: client.additional_model_request_fields,
    }
}

/// The Converse endpoint for this client: the `endpoint_url` override when set
/// (which bypasses region resolution entirely), else
/// `https://bedrock-runtime.<region>.amazonaws.com`.
function converse_url(client: root.BedrockClient, preview: bool = false) -> string {
    let path = converse_model_path(client.model);
    if let endpoint: string = client.resolved_endpoint_url() {
        // A trailing slash on the override would double up with the path.
        let base = if (endpoint.ends_with("/")) {
            endpoint.slice(0, endpoint.length() - 1)
        } else {
            endpoint
        };
        return `${base}${path}`;
    }
    // An explicit region wins outright and never touches the resolver — the
    // same short-circuit sys_llm had (`auth_request/bedrock.rs:164-166`), and
    // it keeps URL construction free of io when the caller was explicit.
    if let explicit: string = client.region {
        return `https://bedrock-runtime.${explicit}.amazonaws.com${path}`;
    }
    if (preview) {
        return `https://bedrock-runtime.preview-region.amazonaws.com${path}`;
    }
    // Reading the profile can fail on io; that is a configuration problem, not
    // a transport one, so it stays a typed InvalidRequest rather than becoming
    // an UnknownError at the `ai.Client` boundary.
    let region = root.internal.resolve_region(null, client.profile) catch_all (e) {
        _ => {
            throw ai.errors.InvalidRequest {
                provider: "aws-bedrock",
                status_code: null,
                detail: `AWS region resolution failed: ${e.to_string()}`,
                raw_body: null,
            }
        },
    };
    if let resolved: string = region {
        return `https://bedrock-runtime.${resolved}.amazonaws.com${path}`;
    }
    throw ai.errors.InvalidRequest {
        provider: "aws-bedrock",
        status_code: null,
        detail: "no AWS region: set the client's `region` option, AWS_REGION, AWS_DEFAULT_REGION, or a `region` key on the active profile (or set `endpoint_url` to bypass region resolution)",
        raw_body: null,
    }
}

// `base` with `extra` applied over it, header names lowercased so a user
// override replaces the client's own header instead of duplicating it (SigV4
// canonicalizes on the lowercased name).
function _merge_headers(
    base: map<string, string>,
    extra: map<string, string>?,
) -> map<string, string> {
    let out: map<string, string> = { };
    for (let name in base.keys()) {
        let _ = out.set(name.to_lower_case(), base.get(name) ?? "");
    }
    match (extra) {
        null => null,
        let overrides: map<string, string> => {
            for (let name in overrides.keys()) {
                let _ = out.set(name.to_lower_case(), overrides.get(name) ?? "");
            }
            null
        },
    };
    out
}

// Query parameters are percent-encoded with the same unreserved set SigV4's
// canonical query uses, so signing them round-trips.
function _apply_query(url: string, params: map<string, string>?) -> string {
    match (params) {
        null => url,
        let query: map<string, string> => {
            let pairs: string[] = [];
            for (let name in query.keys()) {
                pairs.push(
                    `${encode_path_label(name)}=${encode_path_label(query.get(name) ?? "")}`,
                );
            }
            if (pairs.length() == 0) {
                url
            } else if (url.includes("?")) {
                `${url}&${pairs.join("&")}`
            } else {
                `${url}?${pairs.join("&")}`
            }
        },
    }
}

/// The complete, UNSIGNED Converse request: typed body -> JSON -> null-pruned
/// -> `request_body` passthrough merged in, plus the user's headers and query
/// parameters. Signing must come after this and nothing may touch the request
/// afterwards, so this is the seam request-shape tests assert on.
function build_request(
    client: root.BedrockClient,
    input: ai.ModelTurnInput,
    preview: bool = false,
) -> baml.http.Request {
    let body = _prune_nulls(baml.json.to_json(converse_request(client, input, preview = preview)));
    let merged = ai.wire.merge_request_body(body, client.request_body);
    baml.http.Request {
        method: "POST",
        url: _apply_query(converse_url(client, preview = preview), client.query_params),
        headers: _merge_headers(
            { "content-type": "application/json", "accept": "application/json" },
            client.headers,
        ),
        body: baml.json.stringify(merged),
    }
}

/// SigV4-sign the request. This is the LAST mutation: the signature covers the
/// method, URL, headers and body exactly as built above.
function sign(client: root.BedrockClient, request: baml.http.Request) -> baml.http.Request {
    root.internal.sign_request(
        request,
        root.internal.AwsSignOptions {
            region: client.region,
            profile: client.profile,
            access_key_id: client.access_key_id,
            secret_access_key: client.secret_access_key,
            session_token: client.session_token,
            service: "bedrock",
        },
    ) catch_all (e) {
        // Signing fails when no credentials resolve, or when the credential
        // chain's own IO fails. Nothing was sent, so the failure is retry-safe
        // — but it is emphatically not a NetworkFailure, and a blanket
        // `UnknownError` would lose that.
        _ => {
            throw ai.errors.InvalidRequest {
                provider: "aws-bedrock",
                status_code: null,
                detail: `AWS SigV4 signing failed: ${e.to_string()}`,
                raw_body: null,
            }
        },
    }
}

// ---------------------------------------------------------------------------
// 9. Response wire classes
//
// Every member is optional: Converse omits `usage` on some model families,
// omits `output.message` on a guardrail intervention, and adds members over
// time. A non-optional field here would be a `ParseFailed` waiting to happen
// (PLAN §1.5).
// ---------------------------------------------------------------------------

class ReasoningText {
    text: string?,
    signature: string?,
}

class ReasoningContent {
    reasoningText: ReasoningText?,
    /// Present instead of `reasoningText` when the provider redacted the
    /// thinking; opaque bytes with nothing readable to surface.
    redactedContent: string?,
}

class ResponseToolUse {
    toolUseId: string?,
    name: string?,
    input: map<string, unknown>?,
}

class ResponseContentBlock {
    text: string?,
    toolUse: ResponseToolUse?,
    reasoningContent: ReasoningContent?,
}

class ResponseMessage {
    role: string?,
    content: ResponseContentBlock[]?,
}

class ConverseOutput {
    message: ResponseMessage?,
}

/// Converse token accounting. `cacheReadInputTokens` / `cacheWriteInputTokens`
/// are reported for prompt-cached requests — sys_llm hardcoded
/// `cached_input_tokens: None` claiming Converse never reports them
/// (`parse_response/bedrock.rs:124`), which was simply wrong; the engine read
/// them (`aws_client.rs:1481-1484`). Fixed here.
class ConverseUsage {
    inputTokens: int?,
    outputTokens: int?,
    totalTokens: int?,
    cacheReadInputTokens: int?,
    cacheWriteInputTokens: int?,
}

class ConverseMetrics {
    latencyMs: int?,
}

class ConverseResponse {
    output: ConverseOutput?,
    stopReason: string?,
    usage: ConverseUsage?,
    metrics: ConverseMetrics?,
    additionalModelResponseFields: baml.json.json?,
}

/// True for the two `stopReason` values that say the turn's own content is
/// unusable: `malformed_model_output` (the model emitted something the service
/// could not assemble into a message) and `malformed_tool_use` (it emitted a
/// tool call the service could not assemble into a `toolUse` block).
///
/// Neither is a stop *reason* in the `ai.content.StopReason` sense — there is
/// no answer to stop on — so `parse_response` throws `ai.errors.ParseFailed`
/// with the whole response as `raw_output` instead of returning a turn. That
/// is the honest identity (it IS a failure to produce parseable output), it is
/// `RetrySafety.Safe` so a Retry policy re-asks, and it keeps the raw bytes
/// for the caller. `MaxTokens` would falsely blame the token limit and
/// `Refused` would falsely claim the model declined.
function _is_malformed_stop(raw: string) -> bool {
    raw == "malformed_model_output" || raw == "malformed_tool_use"
}

/// One wire `stopReason` -> the canonical stop reason
/// (`parse_response/bedrock.rs:110-116`, extended with the guardrail and
/// failure reasons the Converse API documents).
function stop_reason(raw: string) -> ai.content.StopReason {
    match (raw) {
        "tool_use" => ai.content.StopReason.ToolUse,
        // `model_context_window_exceeded`: prompt + output overflowed the
        // context window. Semantically the twin of `max_tokens` — the
        // generation was cut short, so it is emphatically not a clean answer.
        "max_tokens" | "model_context_window_exceeded" => ai.content.StopReason.MaxTokens,
        "content_filtered" | "guardrail_intervened" => ai.content.StopReason.Refused,
        // Unreachable from `parse_response`, which throws first. Kept
        // non-Complete so a direct caller of this mapper never reads a
        // malformed turn as a finished one.
        let r if _is_malformed_stop(r) => ai.content.StopReason.Refused,
        // end_turn, stop_sequence, and anything Bedrock adds later.
        _ => ai.content.StopReason.Complete,
    }
}

/// Decode one Converse response into canonical content blocks.
///
/// Unlike sys_llm (`parse_response/bedrock.rs:74-104`) this does NOT error on
/// an empty content list or on a turn with no text blocks: a tool-only turn is
/// the normal `stopReason: "tool_use"` shape and erroring on it would make
/// tool calling impossible. A missing `output.message` is still a hard error —
/// that response is malformed, not empty.
function parse_response(response: ConverseResponse) -> ai.ModelTurn {
    // `malformed_model_output` / `malformed_tool_use`: the service is telling
    // us the generation is unusable. Returning a turn would hand an agent loop
    // a truncated or garbled "answer" (see `_is_malformed_stop`).
    if let raw: string = response.stopReason {
        if (_is_malformed_stop(raw)) {
            throw ai.errors.ParseFailed {
                provider: "aws-bedrock",
                raw_output: baml.json.stringify(baml.json.to_json(response)),
            }
        }
    }
    let message = response.output?.message;
    if (message == null) {
        throw ai.errors.ParseFailed {
            provider: "aws-bedrock",
            raw_output: baml.json.stringify(baml.json.to_json(response)),
        }
    }
    let content: ai.content.Block[] = [];
    let blocks: ResponseContentBlock[] = message.content ?? [];
    for (let block in blocks) {
        if let text: string = block.text {
            content.push(ai.content.Text { text: text });
            null
        } else if let use: ResponseToolUse = block.toolUse {
            let empty: map<string, unknown> = { };
            content.push(
                ai.content.ToolUse {
                    id: use.toolUseId ?? "",
                    name: use.name ?? "",
                    args: use.input ?? empty,
                },
            );
            null
        } else if let reasoning: ReasoningContent = block.reasoningContent {
            if let summary: string = reasoning.reasoningText?.text {
                content.push(ai.content.Reasoning { summary: summary });
                null
            } else {
                null
            };
            null
        } else {
            null
        };
    }

    let stop = match (response.stopReason) {
        null => ai.content.StopReason.Complete,
        let raw: string => stop_reason(raw),
    };

    let usage: ai.events.Usage? = match (response.usage) {
        null => null,
        let reported: ConverseUsage => {
            ai.events.Usage {
                input_tokens: reported.inputTokens ?? 0,
                output_tokens: reported.outputTokens ?? 0,
                cached_input_tokens: reported.cacheReadInputTokens,
                // Converse reports no separate reasoning-token count; the
                // thinking tokens are already inside outputTokens.
                reasoning_tokens: null,
            }
        },
    };

    ai.ModelTurn { content: content, stop_reason: stop, usage: usage }
}

// ---------------------------------------------------------------------------
// 10. The one internal entry point: build, sign, send, parse.
// ---------------------------------------------------------------------------

function invoke(client: root.BedrockClient, input: ai.ModelTurnInput) -> ai.ModelTurn {
    let request = sign(client, build_request(client, input, preview = false));
    parse_response(ai.wire.send_as<ConverseResponse>(request, "aws-bedrock"))
}
