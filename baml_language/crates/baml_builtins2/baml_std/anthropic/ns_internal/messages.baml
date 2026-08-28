// The live Anthropic client: one Messages API call per turn, SAP output
// mode (the schema rides in the prompt text; tools are native).
// Mirrors _planv2/pages/02_guides/03_clients/05_the_built_in_clients.md.
//
// PLAN §1.5 — typed wire classes, not JSON blobs. Every request field, every
// response field and every SSE event this client touches is a real BAML class.
// `baml.json.json` shows up in exactly three places, all of them genuinely
// open-ended: the `request_body` passthrough, a tool's `input_schema`, and the
// `cache_control` directive (a beta-gated shape that arrives verbatim from
// `ai.PromptMessage.metadata`).
//
// Layering, so a future `vertex` package can reuse this: BODY construction
// (`anthropic_messages_request` / `anthropic_messages_body`) is factored away
// from URL/auth/header construction (`_anthropic_request`). Vertex's
// `rawPredict` sends the same body with `model` removed and `anthropic_version`
// added — `AnthropicBodyParams` + `request_body` express exactly that.

// ---------------------------------------------------------------------------
// Local rejections.

/// Every LOCAL, pre-send rejection in the Anthropic client throws ONE error
/// identity: `ai.errors.InvalidRequest` with a null status code — the class
/// the Responses, Google, Images and Bedrock clients already use.
///
/// It matters that this is an `ai.errors.Failure` and not
/// `baml.errors.InvalidArgument`: `ai.errors.normalize` flattens a non-Failure
/// into an untyped `baml.errors.UnknownError`, and `ai.Fallback` only advances
/// to the next member on a TYPED failure. A missing key in one member of a
/// fallback chain has to fall through, not abort the chain.
function _reject(detail: string) -> ai.errors.InvalidRequest {
    ai.errors.InvalidRequest {
        provider: "anthropic",
        status_code: null,
        detail: detail,
        raw_body: null,
    }
}

// ---------------------------------------------------------------------------
// Request: the typed wire model of POST /v1/messages.

// A media payload source. `type` selects which of the other fields the API
// reads: "url" -> url; "base64" -> media_type + data.
class AnthSource {
    type: string,
    media_type: string?,
    data: string?,
    url: string?,
}

class AnthTextBlock {
    type: string,
    text: string,
    // Prompt-caching directive: {"type":"ephemeral"}, or under the extended-TTL
    // beta {"type":"ephemeral","ttl":"1h"}. Deliberately untyped — the shape is
    // beta-gated and arrives verbatim from ai.PromptMessage.metadata.
    cache_control: baml.json.json?,
}

// "image" or "document": one {source} shape, different tags. (There is no
// audio block: the Messages API has no audio input modality at all.)
class AnthMediaBlock {
    type: string,
    source: AnthSource,
    cache_control: baml.json.json?,
}

class AnthToolUseBlock {
    type: string,
    id: string,
    name: string,
    // The arguments object the model produced: arbitrary JSON by definition.
    input: baml.json.json,
    cache_control: baml.json.json?,
}

class AnthToolResultBlock {
    type: string,
    tool_use_id: string,
    content: string,
    is_error: bool,
    cache_control: baml.json.json?,
}

type AnthBlock = AnthTextBlock | AnthMediaBlock | AnthToolUseBlock | AnthToolResultBlock;

class AnthMessageParam {
    role: string,
    content: AnthBlock[],
}

class AnthToolParam {
    name: string,
    description: string,
    // JSON Schema for the argument object: arbitrary JSON by definition.
    input_schema: baml.json.json,
    cache_control: baml.json.json?,
}

class AnthToolChoiceParam {
    type: string,
    name: string?,
    disable_parallel_tool_use: bool?,
}

class AnthThinkingParam {
    type: string,
    budget_tokens: int?,
}

// The complete request body. Optional fields are dropped before serialization
// (see _anth_drop_nulls), so an unset option never reaches the wire.
class AnthropicMessagesRequest {
    model: string,
    max_tokens: int,
    system: AnthBlock[]?,
    messages: AnthMessageParam[],
    tools: AnthToolParam[]?,
    tool_choice: AnthToolChoiceParam?,
    temperature: float?,
    top_p: float?,
    top_k: int?,
    stop_sequences: string[]?,
    thinking: AnthThinkingParam?,
    stream: bool?,
}

// ---------------------------------------------------------------------------
// Response envelope. from_string ignores unknown fields; only what the
// client reads is typed, and EVERY field the API may omit is optional
// (a required int here is a ParseFailed waiting to happen).

class AnthropicContentBlock {
    type: string,
    text: string?,
    thinking: string?,
    // Present on `thinking` blocks under extended thinking. Retained here for
    // completeness; ai.content.Reasoning has no signature slot to carry it, so
    // it is not replayable yet (see the report's shared-file requests).
    signature: string?,
    id: string?,
    name: string?,
    input: map<string, unknown>?,
}

// `usage.output_tokens_details`: the breakdown of `output_tokens`. Under
// extended thinking (this client's own `ThinkingConfig`) `thinking_tokens` is
// the count the model generated as internal reasoning.
class AnthOutputTokensDetails {
    thinking_tokens: int?,
}

class AnthropicUsage {
    // NOTE (cross-provider semantics, accepted): Anthropic's `input_tokens`
    // counts only the tokens AFTER the last cache breakpoint —
    // `total_input_tokens = input_tokens + cache_read_input_tokens +
    // cache_creation_input_tokens` — while `ai.events.Usage.input_tokens`
    // elsewhere (OpenAI) is the full prompt with the cached part as a SUBSET.
    // The provider number is reported verbatim, so a heavily cached prompt
    // reports a small `input_tokens`; normalizing it would need the
    // cache_creation slot `ai.events.Usage` does not have yet (folding cache
    // writes into `cached_input_tokens` would misreport cost — they are billed
    // ABOVE the input rate), so a partial sum here would trade one wrong number
    // for another. Cost accounting must read the cache counters too.
    input_tokens: int?,
    output_tokens: int?,
    output_tokens_details: AnthOutputTokensDetails?,
    // Tokens served FROM the prompt cache (billed at a fraction of the input
    // rate) — this is what `cached_input_tokens` means.
    cache_read_input_tokens: int?,
    // Tokens WRITTEN to the prompt cache on this request. Decoded so the
    // number is not silently lost; ai.events.Usage has no slot for it yet.
    cache_creation_input_tokens: int?,
}

class AnthropicResponse {
    id: string?,
    type: string?,
    role: string?,
    model: string?,
    content: AnthropicContentBlock[],
    stop_reason: string?,
    stop_sequence: string?,
    usage: AnthropicUsage?,
}

// ---------------------------------------------------------------------------
// Serialization helpers.

// Drop null-valued map entries, recursively. BAML serializes an unset optional
// field as an explicit `null`, which the Messages API rejects for several
// parameters; the typed request classes above therefore go through this on
// their way to the wire, which is exactly serde's `skip_serializing_if =
// "Option::is_none"` in the Rust builders this replaces.
//
// Nulls INSIDE arrays are preserved (a JSON Schema `"enum": [null]` survives),
// and the two OPAQUE subtrees below are copied verbatim. The passthrough merge —
// where `null` means "delete this key" — still runs AFTER this, on the
// already-pruned base.
//
// `input` (a replayed tool call's arguments) and `input_schema` (a tool's JSON
// Schema) are user/model JSON whose keys are DATA, not request parameters: a
// null there means "the model answered null" / `"default": null`, not "unset
// option". Pruning them rewrote conversation history on replay — every optional
// tool parameter the model answered with null silently vanished from the
// replayed tool_use, and the generated schema types optionals as
// `["string","null"]` and marks them required, so a null answer is the
// schema-conformant one.
function _anth_verbatim_subtree(key: string) -> bool {
    key == "input" || key == "input_schema"
}

function _anth_drop_nulls(j: baml.json.json) -> baml.json.json {
    match (j) {
        let obj: map<string, baml.json.json> => {
            let out: map<string, baml.json.json> = {};
            for (let key in obj.keys()) {
                let value = obj.get(key);
                match (value) {
                    null => null,
                    _ => {
                        let kept = if (_anth_verbatim_subtree(key)) {
                            value
                        } else {
                            _anth_drop_nulls(value)
                        };
                        let _ = out.set(key, kept);
                        null
                    },
                };
            }
            out
        },
        let arr: baml.json.json[] => {
            let out: baml.json.json[] = [];
            for (let element in arr) {
                out.push(_anth_drop_nulls(element));
            }
            out
        },
        _ => j,
    }
}

// ---------------------------------------------------------------------------
// ai.Prompt lowering.

// An intermediate lowered message: role plus typed wire content blocks.
// tool_results marks a user message that is an open tool_result group.
class AnthropicMessage {
    role: string,
    tool_results: bool,
    blocks: AnthBlock[],
}

// Anthropic carries system content outside the alternating messages list.
// Keep that split provider-local while preserving the authored prompt roles.
class AnthropicPrompt {
    system: AnthBlock[],
    messages: AnthropicMessage[],
}

// Anthropic places a cache breakpoint on a CONTENT BLOCK, but the authored
// directive lives on a MESSAGE (`${ai.Role { name: "user", metadata:
// { "cache_control": ... } }}`). The engine's rule, ported: the directive
// attaches to that message's LAST content block, so everything up to and
// including it is the cached prefix.
function _anth_apply_cache_control(blocks: AnthBlock[], cache_control: baml.json.json?) -> void {
    match (cache_control) {
        null => null,
        _ => {
            if let last: AnthBlock = blocks.at(blocks.length() - 1) {
                match (last) {
                    let text: AnthTextBlock => {
                        text.cache_control = cache_control;
                    },
                    let media: AnthMediaBlock => {
                        media.cache_control = cache_control;
                    },
                    let use: AnthToolUseBlock => {
                        use.cache_control = cache_control;
                    },
                    let result: AnthToolResultBlock => {
                        result.cache_control = cache_control;
                    },
                };
            }
        },
    };
}

// One media part -> its `{type, source}` block. `resolve_media(_, false)`
// passes an ordinary http(s) URL straight through as `source.type: "url"`
// (Anthropic fetches it itself) and inlines everything else as base64 —
// sys_llm's `send_url` default for this provider.
// The four image media types the Messages API accepts on a base64 source. It
// validates the declared `media_type` against the actual bytes, so anything
// else is a 400 — including `image/svg+xml` (which `ai.internal._media_mime_type`
// produces for an `.svg` source) and `image/png` guessed for an unknown
// extension. Checking here keeps the error client-side and specific; the check
// is per-provider because the allowed set is.
function _anth_supported_image_mime(mime: string) -> bool {
    let m = mime.to_lower_case();
    m == "image/jpeg" || m == "image/png" || m == "image/gif" || m == "image/webp"
}

function _anthropic_media_block(
    media: ai.MediaPart,
    block_type: string,
    preview: bool = false,
) -> AnthMediaBlock {
    let resolved = if (preview) {
        ai.wire.resolve_media_preview(media)
    } else {
        ai.wire.resolve_media(media, false)
    };
    let source = if let url: string = resolved.url {
        // A URL source carries no media_type: Anthropic fetches and sniffs the
        // bytes itself, so the guessed mime never reaches the wire.
        AnthSource { type: "url", media_type: null, data: null, url: url }
    } else {
        if (block_type == "image" && !_anth_supported_image_mime(resolved.mime_type)) {
            throw _reject(
                `Anthropic does not accept ${resolved.mime_type} images: the Messages API supports image/jpeg, image/png, image/gif and image/webp`,
            )
        }
        AnthSource {
            type: "base64",
            media_type: resolved.mime_type,
            data: resolved.base64 ?? "",
            url: null,
        }
    };
    AnthMediaBlock { type: block_type, source: source, cache_control: null }
}

function _anthropic_prompt_blocks(
    message: ai.PromptMessage,
    role: string,
    preview: bool = false,
) -> AnthBlock[] {
    let blocks: AnthBlock[] = [];
    for (let part in message.parts) {
        match (part) {
            let text: string => {
                // The API rejects empty and whitespace-only text blocks, and a
                // rendered prompt routinely produces them around media parts.
                if (text.trim() != "") {
                    blocks.push(AnthTextBlock { type: "text", text: text, cache_control: null });
                }
                null
            },
            let image: baml.media.Image => {
                if (role != "user") {
                    throw _reject(
                        `Anthropic only supports image input in user messages, found ${role}`,
                    )
                }
                blocks.push(_anthropic_media_block(image, "image", preview = preview));
                null
            },
            // The Messages API has NO audio input content block — audio is not
            // an input modality for it at all (the 17 documented input blocks
            // contain nothing of the kind). This used to emit an invented
            // `{"type":"input_audio"}` block, so every audio-bearing request was
            // a guaranteed provider 400; it is a client-side error now, exactly
            // like video below.
            let audio: baml.media.Audio => {
                throw _reject(
                    "Anthropic does not support audio prompt input: the Messages API has no audio content block",
                )
            },
            let pdf: baml.media.Pdf => {
                if (role != "user") {
                    throw _reject(
                        `Anthropic only supports PDF input in user messages, found ${role}`,
                    )
                }
                blocks.push(_anthropic_media_block(pdf, "document", preview = preview));
                null
            },
            let video: baml.media.Video => {
                throw _reject("Anthropic does not support video prompt input")
            },
        };
    }
    blocks
}

function _anthropic_lower_prompt(prompt: ai.Prompt, preview: bool = false) -> AnthropicPrompt {
    let system: AnthBlock[] = [];
    let messages: AnthropicMessage[] = [];
    for (let message in prompt.messages()) {
        let cache_control = message.metadata.get("cache_control");
        if (message.role == "system") {
            let blocks = _anthropic_prompt_blocks(message, "system", preview = preview);
            _anth_apply_cache_control(blocks, cache_control);
            for (let block in blocks) {
                system.push(block);
            }
            null
        } else {
            let role = if (message.role == "assistant") {
                "assistant"
            } else {
                "user"
            };
            let blocks = _anthropic_prompt_blocks(message, role, preview = preview);
            _anth_apply_cache_control(blocks, cache_control);
            // A message that lowered to nothing (all-whitespace text) is
            // dropped: `content: []` is a 400.
            if (blocks.length() > 0) {
                messages.push(AnthropicMessage { role: role, tool_results: false, blocks: blocks });
            }
            null
        };
    }
    AnthropicPrompt { system: system, messages: messages }
}

// ---------------------------------------------------------------------------
// ai.Journal lowering. Anthropic requires strict user/assistant alternation and
// tool_result blocks in a user message immediately after the assistant
// tool_use message, so consecutive tool results group into ONE user message.

function _anthropic_assistant_blocks(content: ai.content.Block[]) -> AnthBlock[] {
    let out: AnthBlock[] = [];
    for (let b in content) {
        match (b) {
            let t: ai.content.Text => {
                if (t.text.trim() != "") {
                    out.push(AnthTextBlock { type: "text", text: t.text, cache_control: null });
                }
                null
            },
            // Dropped: replaying thinking without its signature is rejected
            // by the API, and ai.content.Reasoning has no signature field.
            // ai.wire.sanitize_for_client already drops foreign-client
            // reasoning; this drops the rest.
            let r: ai.content.Reasoning => null,
            // Dropped: a generated image is an output, not replayable input.
            let m: ai.content.Media => null,
            let u: ai.content.ToolUse => {
                out.push(
                    AnthToolUseBlock {
                        type: "tool_use",
                        id: u.id,
                        name: u.name,
                        input: baml.json.to_json(u.args),
                        cache_control: null,
                    },
                );
                null
            },
        };
    }
    out
}

// Append one tool_result block, extending the trailing group when open.
function _anthropic_push_tool_result(
    msgs: AnthropicMessage[],
    id: string,
    content: string,
    is_error: bool,
) -> void {
    let block = AnthToolResultBlock {
        type: "tool_result",
        tool_use_id: id,
        content: content,
        is_error: is_error,
        cache_control: null,
    };
    // .at returns null on an empty list, covering the no-messages case.
    if let m: AnthropicMessage = msgs.at(msgs.length() - 1) {
        if (m.tool_results) {
            m.blocks.push(block);
        } else {
            msgs.push(AnthropicMessage { role: "user", tool_results: true, blocks: [block] });
        }
    } else {
        msgs.push(AnthropicMessage { role: "user", tool_results: true, blocks: [block] });
    }
}

// ai.RunStarted/ai.ToolRequested/ai.Usage/ai.FinalProduced are not promptable and skip;
// skipped events never close an open tool_result group.
function _anthropic_lower_journal(j: ai.Journal) -> AnthropicMessage[] {
    let msgs: AnthropicMessage[] = [];
    for (let e in j.entries()) {
        match (e) {
            let m: ai.events.UserMessage => {
                msgs.push(
                    AnthropicMessage {
                        role: "user",
                        tool_results: false,
                        blocks: [AnthTextBlock { type: "text", text: m.content, cache_control: null }],
                    },
                );
                null
            },
            let m: ai.events.AssistantMessage => {
                let blocks = _anthropic_assistant_blocks(m.content);
                // An assistant turn that lowers to nothing is dropped rather
                // than sent as `content: []` (the reasoning-only turn that made
                // the API 400). ai.wire.sanitize_for_client removes most of
                // these upstream; this is the belt to its braces.
                if (blocks.length() > 0) {
                    msgs.push(
                        AnthropicMessage { role: "assistant", tool_results: false, blocks: blocks },
                    );
                }
                null
            },
            let c: ai.events.ToolCompleted => {
                _anthropic_push_tool_result(msgs, c.id, c.output, false);
            },
            let f: ai.events.ToolFailed => {
                _anthropic_push_tool_result(msgs, f.id, f.message, true);
            },
            _ => null,
        };
    }
    msgs
}

// The API wants user and assistant to alternate, and several paths here can
// leave two of the same role side by side: a dropped assistant turn (a
// reasoning-only reply lowers to no blocks) leaves the user turns that
// bracketed it adjacent, and the rendered prompt can end on the role the
// journal opens with. The leading-role repair above only fixes an
// assistant-FIRST list, so a mid-conversation clash reached the wire.
//
// Concatenating the block lists of same-role neighbours repairs every such
// shape. Block order is preserved and messages are joined in order, so a
// `cache_control` marker still sits on exactly the block it was placed on.
// `_anthropic_push_tool_result`'s grouping becomes a special case of this.
function _anthropic_render_messages(msgs: AnthropicMessage[]) -> AnthMessageParam[] {
    let out: AnthMessageParam[] = [];
    for (let m in msgs) {
        if let last: AnthMessageParam = out.at(out.length() - 1) {
            if (last.role == m.role) {
                for (let block in m.blocks) {
                    last.content.push(block);
                }
                continue;
            }
        }
        out.push(AnthMessageParam { role: m.role, content: m.blocks.concat([]) });
    }
    out
}

// ---------------------------------------------------------------------------
// Body construction, factored away from URL/auth so a Vertex `rawPredict`
// client can reuse it verbatim.

// Everything the body builder needs that is not in ai.ModelTurnInput. A host
// that is not root.AnthropicClient (Vertex, Bedrock) builds one of these.
class AnthropicBodyParams {
    model: string,
    max_tokens: int,
    temperature: float?,
    top_p: float?,
    top_k: int?,
    stop_sequences: string[]?,
    thinking: root.ThinkingConfig?,
    tool_choice: root.ToolChoice?,
    request_body: baml.json.json?,
    // The id journal sanitization is performed against — `Client.id()`.
    client_id: string,
}

function _anthropic_body_params(c: root.AnthropicClient) -> AnthropicBodyParams {
    AnthropicBodyParams {
        model: c.model,
        max_tokens: c.max_tokens,
        temperature: c.temperature,
        top_p: c.top_p,
        top_k: c.top_k,
        stop_sequences: c.stop_sequences,
        thinking: c.thinking,
        tool_choice: c.tool_choice,
        request_body: c.request_body,
        client_id: c.id(),
    }
}

function _anthropic_tool_params(toolbox: ai.tools.Toolbox) -> AnthToolParam[] {
    let out: AnthToolParam[] = [];
    for (let t in toolbox.list()) {
        out.push(
            AnthToolParam {
                name: t.name,
                description: t.description,
                input_schema: baml.json.to_json(t.input_schema),
                cache_control: null,
            },
        );
    }
    out
}

function _anthropic_tool_choice_param(choice: root.ToolChoice?) -> AnthToolChoiceParam {
    match (choice) {
        // The API's own default, sent explicitly so the wire shape is stable.
        null => AnthToolChoiceParam { type: "auto", name: null, disable_parallel_tool_use: null },
        let c: root.ToolChoice => {
            AnthToolChoiceParam {
                type: c.type,
                name: c.name,
                disable_parallel_tool_use: c.disable_parallel_tool_use,
            }
        },
    }
}

function _anthropic_thinking_param(thinking: root.ThinkingConfig?) -> AnthThinkingParam? {
    match (thinking) {
        null => null,
        let t: root.ThinkingConfig => {
            AnthThinkingParam { type: t.type, budget_tokens: t.budget_tokens }
        },
    }
}

/// The typed Messages request for one model turn. Public so a Vertex/Bedrock
/// client can build the identical body and re-target it.
function anthropic_messages_request(
    params: AnthropicBodyParams,
    input: ai.ModelTurnInput,
    stream: bool,
    preview: bool = false,
) -> AnthropicMessagesRequest {
    let prompt = input.prompt(ai.internal.build_output_format(input.output_type));
    let lowered_prompt = _anthropic_lower_prompt(prompt, preview = preview);
    let system = lowered_prompt.system;
    let lowered = lowered_prompt.messages;
    // Sanitize BEFORE lowering: foreign-client reasoning, empty turns and
    // orphaned tool calls are repaired once, in ai.wire, for every provider.
    for (
        let message in _anthropic_lower_journal(ai.wire.sanitize_for_client(input.journal, params.client_id))
    ) {
        lowered.push(message);
    }

    // The API requires a non-empty message list that starts with a user
    // message. Normal role-less prompts already lower to user; these two flags
    // cover the shapes that do not.
    let first_role = lowered.at(0)?.role ?? "";
    // System-only (or assistant-first WITH system content): promote the system
    // blocks into a leading user message — the old first-turn fallback — rather
    // than flattening an otherwise well-formed prompt.
    let promote_system = first_role != "user" && system.length() > 0;
    // Assistant-first with NO system content: there is nothing to promote, so
    // synthesize the leading user turn the API insists on. Without this the
    // request is a guaranteed 400.
    let synthesize_user = lowered.length() > 0 && first_role != "user" && system.length() == 0;
    let ordered: AnthropicMessage[] = if (promote_system) {
        [AnthropicMessage { role: "user", tool_results: false, blocks: system }].concat(lowered)
    } else if (synthesize_user) {
        [
            AnthropicMessage {
                role: "user",
                tool_results: false,
                blocks: [AnthTextBlock { type: "text", text: "(continue)", cache_control: null }],
            },
        ]
            .concat(lowered)
    } else {
        lowered
    };
    let messages = _anthropic_render_messages(ordered);
    if (messages.length() == 0) {
        throw ai.errors.InvalidRequest {
            provider: "anthropic",
            status_code: null,
            detail: "the rendered prompt and journal produced no message content",
            raw_body: null,
        }
    }

    // Promoted system blocks are already the first user message; sending them
    // twice would duplicate the instructions.
    let system_param: AnthBlock[]? = if (promote_system || system.length() == 0) {
        null
    } else {
        system
    };
    let has_tools = !input.toolbox.is_empty();
    let tool_params: AnthToolParam[]? = if (has_tools) {
        _anthropic_tool_params(input.toolbox)
    } else {
        null
    };
    let choice: AnthToolChoiceParam? = if (has_tools) {
        _anthropic_tool_choice_param(params.tool_choice)
    } else {
        null
    };
    let stream_flag: bool? = if (stream) {
        true
    } else {
        null
    };
    AnthropicMessagesRequest {
        model: params.model,
        max_tokens: params.max_tokens,
        system: system_param,
        messages: messages,
        tools: tool_params,
        tool_choice: choice,
        temperature: params.temperature,
        top_p: params.top_p,
        top_k: params.top_k,
        stop_sequences: params.stop_sequences,
        thinking: _anthropic_thinking_param(params.thinking),
        stream: stream_flag,
    }
}

/// The serialized request body: typed request -> json -> drop unset optionals
/// -> deep-merge the caller's `request_body` passthrough (LAST, per P0.3).
function anthropic_messages_body(
    params: AnthropicBodyParams,
    input: ai.ModelTurnInput,
    stream: bool,
    preview: bool = false,
) -> string {
    let request = anthropic_messages_request(params, input, stream, preview = preview);
    let base = _anth_drop_nulls(baml.json.to_json(request));
    baml.json.stringify(ai.wire.merge_request_body(base, params.request_body))
}

// ---------------------------------------------------------------------------
// The client: URL, headers, auth.

// `?a=1&b=2` for the client's query_params, or "" when there are none.
// Values are used verbatim: the stdlib has no percent-encoder yet.
function _anthropic_query_suffix(query_params: map<string, string>?) -> string {
    match (query_params) {
        null => "",
        let params: map<string, string> => {
            let pairs: string[] = [];
            for (let key in params.keys()) {
                pairs.push(`${key}=${params.get(key) ?? ""}`);
            }
            if (pairs.length() > 0) {
                `?${pairs.join("&")}`
            } else {
                ""
            }
        },
    }
}

// Defaults, then the caller's headers, then auth. User headers come FIRST so
// `anthropic-beta` (and `anthropic-dangerous-direct-browser-access`, which the
// WASM playground needs) are reachable and `anthropic-version` is overridable;
// `x-api-key` is applied last so a stray header cannot break authentication.
function _anthropic_headers(c: root.AnthropicClient, preview: bool = false) -> map<string, string> {
    let headers: map<string, string> = { "anthropic-version": "2023-06-01", "content-type": "application/json" };
    if let extra: map<string, string> = c.headers {
        for (let key in extra.keys()) {
            let _ = headers.set(key.to_lower_case(), extra.get(key) ?? "");
        }
    }
    if (!preview) {
        let _ = headers.set("x-api-key", c.resolved_api_key());
    }
    headers
}

// The shared request builder: identical body for both paths, plus the
// `stream: true` flag on the streaming one.
function _anthropic_request(
    c: root.AnthropicClient,
    input: ai.ModelTurnInput,
    stream: bool,
    preview: bool = false,
) -> baml.http.Request {
    let base = c.resolved_base_url() ?? "https://api.anthropic.com";
    baml.http.Request {
        method: "POST",
        url: `${base}/v1/messages${_anthropic_query_suffix(c.query_params)}`,
        headers: _anthropic_headers(c, preview = preview),
        body: anthropic_messages_body(_anthropic_body_params(c), input, stream, preview = preview),
    }
}

/// The request one `invoke` would send, for tests and for debugging.
function anthropic_render(c: root.AnthropicClient, input: ai.ModelTurnInput) -> baml.http.Request {
    _anthropic_request(c, input, false, preview = false)
}

/// The request one `render` would send. URL media remains a URL reference;
/// local files are rejected before any filesystem read.
function anthropic_preview(c: root.AnthropicClient, input: ai.ModelTurnInput) -> baml.http.Request {
    _anthropic_request(c, input, false, preview = true)
}

// One wire stop_reason string -> the canonical stop reason.
//
// `end_turn`/`stop_sequence` are the only reasons that mean "the model said
// everything it meant to say". `pause_turn`, `model_context_window_exceeded`
// and anything Anthropic ships next are TRUNCATIONS, so they map to MaxTokens
// (the runner's "incomplete answer" outcome) rather than silently reporting a
// truncated candidate as a successful completion.
function _anthropic_stop_reason(s: string) -> ai.content.StopReason {
    match (s) {
        "end_turn" | "stop_sequence" => ai.content.StopReason.Complete,
        "tool_use" => ai.content.StopReason.ToolUse,
        "refusal" => ai.content.StopReason.Refused,
        // max_tokens, pause_turn, model_context_window_exceeded, unknown.
        _ => ai.content.StopReason.MaxTokens,
    }
}

/// One decoded response -> canonical content blocks, stop reason and usage.
function anthropic_parse(resp: AnthropicResponse) -> ai.ModelTurn {
    let content: ai.content.Block[] = [];
    for (let b in resp.content) {
        match (b.type) {
            "text" => {
                content.push(ai.content.Text { text: b.text ?? "" });
                null
            },
            "thinking" => {
                content.push(ai.content.Reasoning { summary: b.thinking ?? "" });
                null
            },
            "tool_use" => {
                let empty_args: map<string, unknown> = {};
                content.push(
                    ai.content.ToolUse {
                        id: b.id ?? "",
                        name: b.name ?? "",
                        args: b.input ?? empty_args,
                    },
                );
                null
            },
            // redacted_thinking and server-tool blocks: nothing canonical to
            // build from them yet.
            _ => null,
        };
    }

    let stop = match (resp.stop_reason) {
        null => ai.content.StopReason.Complete,
        let s: string => _anthropic_stop_reason(s),
    };

    let usage: ai.events.Usage? = match (resp.usage) {
        let u: AnthropicUsage => {
            ai.events.Usage {
                input_tokens: u.input_tokens ?? 0,
                output_tokens: u.output_tokens ?? 0,
                // Cache READS are the cached-input tokens. Cache WRITES
                // (cache_creation_input_tokens) are billed above the normal
                // input rate, so folding them in here would misreport cost;
                // ai.events.Usage needs its own slot for them.
                cached_input_tokens: u.cache_read_input_tokens,
                // Extended thinking: the reasoning share of output_tokens.
                reasoning_tokens: u.output_tokens_details?.thinking_tokens,
            }
        },
        null => null,
    };

    ai.ModelTurn { calls: [], content: content, stop_reason: stop, usage: usage }
}

// The one internal entry point: render, send, normalize — one model turn.
function invoke(c: root.AnthropicClient, input: ai.ModelTurnInput) -> ai.ModelTurn {
    let req = _anthropic_request(c, input, false, preview = false);
    let sent = ai.wire.send_as_with_wire<AnthropicResponse>(
        req,
        "anthropic",
        c.id(),
        capture_wire = c.capture_wire,
        request_timeout_ms = c.request_timeout_ms,
    );
    let turn = anthropic_parse(sent.value);
    ai.ModelTurn { ...turn, calls: [sent.call] }
}

// ---------------------------------------------------------------------------
// Streaming (Messages API SSE).
//
// Wire events: `message_start` carries `message.usage` (input + cache tokens)
// and the resolved model; `content_block_start` opens a block and names a
// tool_use call; `content_block_delta` carries `text_delta` / `input_json_delta`
// / `thinking_delta` / `signature_delta`; `message_delta` carries
// `delta.stop_reason` and cumulative `usage.output_tokens`; `message_stop` ends
// the turn; `ping` is a keepalive; `error` is a MID-STREAM failure that must
// not be mistaken for a short but successful turn.

// Streaming usage objects carry only some fields per event, so every field
// is optional (unlike the non-streaming AnthropicUsage).
class AnthStreamUsage {
    input_tokens: int?,
    output_tokens: int?,
    cache_read_input_tokens: int?,
    cache_creation_input_tokens: int?,
    output_tokens_details: AnthOutputTokensDetails?,
}

class AnthStreamDelta {
    type: string?,
    text: string?,
    // input_json_delta: one fragment of the tool's argument JSON.
    partial_json: string?,
    thinking: string?,
    signature: string?,
    stop_reason: string?,
    stop_sequence: string?,
}

// The block `content_block_start` opens.
class AnthStreamContentBlock {
    type: string?,
    id: string?,
    name: string?,
    text: string?,
    thinking: string?,
}

class AnthStreamMessage {
    id: string?,
    model: string?,
    stop_reason: string?,
    usage: AnthStreamUsage?,
}

class AnthStreamError {
    type: string?,
    message: string?,
}

// The envelope of one SSE `data:` payload.
class AnthStreamEvent {
    type: string?,
    index: int?,
    delta: AnthStreamDelta?,
    content_block: AnthStreamContentBlock?,
    message: AnthStreamMessage?,
    usage: AnthStreamUsage?,
    error: AnthStreamError?,
}

// One tool call being assembled across `input_json_delta` events.
class AnthStreamToolCall {
    index: int,
    id: string,
    name: string,
    args_json: string,
}

/// What a stream carried that `ai.stream.StreamEvent` cannot yet express:
/// tool calls, extended thinking, cache-token counts and the raw stop reason.
/// The decoder folds them here so nothing is silently dropped; wiring them to
/// the consumer needs the stdlib extensions listed in the workstream report.
class AnthStreamState {
    thinking: string,
    signature: string,
    tool_calls: AnthStreamToolCall[],
    cache_read_input_tokens: int?,
    cache_creation_input_tokens: int?,
    // usage.output_tokens_details.thinking_tokens, folded the same way the
    // cache counters are: ai.stream.TurnMeta has no reasoning slot, so the
    // non-streaming path's Usage.reasoning_tokens has no streaming twin yet.
    reasoning_tokens: int?,
    stop_reason_raw: string?,
    model: string?,

    function new() -> AnthStreamState {
        AnthStreamState {
            thinking: "",
            signature: "",
            tool_calls: [],
            cache_read_input_tokens: null,
            cache_creation_input_tokens: null,
            reasoning_tokens: null,
            stop_reason_raw: null,
            model: null,
        }
    }

    /// The assembled tool calls as canonical content blocks. An argument
    /// fragment stream that never completed decodes as an empty argument map
    /// rather than throwing.
    function tool_uses(self) -> ai.content.ToolUse[] {
        let out: ai.content.ToolUse[] = [];
        for (let call in self.tool_calls) {
            let empty: map<string, unknown> = {};
            let args = if (call.args_json.trim() == "") {
                empty
            } else {
                baml.json.from_string<map<string, unknown>>(call.args_json) catch_all (e) {
                    _ => empty,
                }
            };
            out.push(ai.content.ToolUse { id: call.id, name: call.name, args: args });
        }
        out
    }
}

function _anth_stream_call_at(state: AnthStreamState, index: int?) -> AnthStreamToolCall? {
    let wanted = index ?? -1;
    for (let call in state.tool_calls) {
        if (call.index == wanted) {
            return call;
        }
    }
    null
}

// A mid-stream `error` event -> the precise failure. Same discipline as
// ai.errors.classify_http: only genuinely transient conditions get a
// retry-safe transport classification; a bad request stays a bad request so
// ai.clients.Retry does not replay it.
function _anth_stream_failure(error: AnthStreamError?, raw: string) -> ai.errors.Failure {
    let kind = error?.type ?? "";
    let detail = error?.message ?? kind;
    match (kind) {
        "rate_limit_error" => {
            ai.errors.RateLimited {
                status_code: null,
                provider: "anthropic",
                retry_after_ms: null,
                raw_body: raw,
            }
        },
        "overloaded_error" | "api_error" | "timeout_error" => {
            ai.errors.NetworkFailure {
                status_code: null,
                provider: "anthropic",
                detail: `${kind}: ${detail}`,
                raw_body: raw,
            }
        },
        // authentication_error, permission_error, invalid_request_error,
        // not_found_error, request_too_large, billing_error, and anything new.
        _ => {
            ai.errors.InvalidRequest {
                provider: "anthropic",
                status_code: null,
                detail: `${kind}: ${detail}`,
                raw_body: raw,
            }
        },
    }
}

/// One `SseStream.next()` batch -> provider-agnostic stream events, folding
/// everything StreamEvent cannot carry into `state`.
function anthropic_decode_batch(batch: string, state: AnthStreamState) -> ai.stream.StreamEvent[] {
    let out: ai.stream.StreamEvent[] = [];
    for (let raw in ai.stream.decode_sse_batch(batch)) {
        if let data: string = raw.data {
            let ev: AnthStreamEvent? = baml.json.from_string<AnthStreamEvent>(data) catch_all (e) {
                _ => null,
            };
            if let event: AnthStreamEvent = ev {
                let t = event.type ?? raw.event ?? "";
                match (t) {
                    "error" => {
                        throw _anth_stream_failure(event.error, data)
                    },
                    "message_start" => {
                        if let message: AnthStreamMessage = event.message {
                            state.model = message.model;
                            if let usage: AnthStreamUsage = message.usage {
                                state.cache_read_input_tokens
                                    = usage.cache_read_input_tokens ?? state.cache_read_input_tokens;
                                state.cache_creation_input_tokens
                                    = usage.cache_creation_input_tokens
                                        ?? state.cache_creation_input_tokens;
                                state.reasoning_tokens
                                    = usage.output_tokens_details?.thinking_tokens
                                        ?? state.reasoning_tokens;
                            }
                            null
                        } else {
                            null
                        };
                        out.push(
                            ai.stream.TurnMeta {
                                stop_reason: null,
                                input_tokens: event.message?.usage?.input_tokens,
                                output_tokens: event.message?.usage?.output_tokens,
                            },
                        );
                        null
                    },
                    "content_block_start" => {
                        if let block: AnthStreamContentBlock = event.content_block {
                            let kind = block.type ?? "";
                            match (kind) {
                                "tool_use" => {
                                    state.tool_calls.push(
                                        AnthStreamToolCall {
                                            index: event.index ?? state.tool_calls.length(),
                                            id: block.id ?? "",
                                            name: block.name ?? "",
                                            args_json: "",
                                        },
                                    );
                                    null
                                },
                                "thinking" => {
                                    state.thinking = `${state.thinking}${block.thinking ?? ""}`;
                                    null
                                },
                                _ => {
                                    // A text block may open with content already in it.
                                    if let text: string = block.text {
                                        if (text != "") {
                                            out.push(ai.stream.TextDelta { text: text });
                                        }
                                    }
                                    null
                                },
                            };
                            null
                        } else {
                            null
                        };
                        null
                    },
                    "content_block_delta" => {
                        if let d: AnthStreamDelta = event.delta {
                            let kind = d.type ?? "";
                            match (kind) {
                                "text_delta" => {
                                    if let text: string = d.text {
                                        out.push(ai.stream.TextDelta { text: text });
                                    }
                                    null
                                },
                                "input_json_delta" => {
                                    if let call: AnthStreamToolCall = _anth_stream_call_at(state, event.index) {
                                        call.args_json = `${call.args_json}${d.partial_json ?? ""}`;
                                    }
                                    null
                                },
                                "thinking_delta" => {
                                    // Deliberately NOT a TextDelta: thinking is not part
                                    // of the answer the parser will see.
                                    state.thinking = `${state.thinking}${d.thinking ?? ""}`;
                                    null
                                },
                                "signature_delta" => {
                                    state.signature = `${state.signature}${d.signature ?? ""}`;
                                    null
                                },
                                _ => null,
                            };
                            null
                        } else {
                            null
                        };
                        null
                    },
                    "message_delta" => {
                        if let usage: AnthStreamUsage = event.usage {
                            state.cache_read_input_tokens
                                = usage.cache_read_input_tokens ?? state.cache_read_input_tokens;
                            state.cache_creation_input_tokens
                                = usage.cache_creation_input_tokens
                                    ?? state.cache_creation_input_tokens;
                            state.reasoning_tokens
                                = usage.output_tokens_details?.thinking_tokens
                                    ?? state.reasoning_tokens;
                        }
                        let stop: ai.content.StopReason? = match (event.delta?.stop_reason) {
                            null => null,
                            let s: string => {
                                state.stop_reason_raw = s;
                                _anthropic_stop_reason(s)
                            },
                        };
                        out.push(
                            ai.stream.TurnMeta {
                                stop_reason: stop,
                                input_tokens: event.usage?.input_tokens,
                                output_tokens: event.usage?.output_tokens,
                            },
                        );
                        null
                    },
                    "message_stop" => {
                        out.push(ai.stream.TurnDone {  });
                        null
                    },
                    // content_block_stop and ping carry nothing to fold.
                    _ => null,
                };
                null
            } else {
                null
            };
            null
        } else {
            null
        };
    }
    out
}

// `fetch_sse` fails a non-2xx status itself, throwing baml.errors.Io with the
// status and body in its message ("SSE request failed with status 401: …").
// Recover the status and route it through the SAME classifier the non-streaming
// path uses: a 400 or 401 must never come back as a retry-safe NetworkFailure,
// or ai.clients.Retry cheerfully replays a request that can never succeed.
function _anthropic_sse_failure(message: string) -> ai.errors.Failure {
    let prefix = "SSE request failed with status ";
    if (message.starts_with(prefix)) {
        let rest = message.slice(prefix.length(), message.length());
        if let separator: int = rest.index_of(":") {
            let status = baml.Int.parse(rest.slice(0, separator)) catch_all (e) {
                _ => 0,
            };
            if (status > 0) {
                return ai.errors.classify_http(
                    "anthropic",
                    status,
                    rest.slice(separator + 1, rest.length()).trim(),
                );
            }
        }
    }
    ai.errors.NetworkFailure {
        status_code: null,
        provider: "anthropic",
        detail: `transport failed before a response arrived: ${message}`,
        raw_body: null,
    }
}

function _anthropic_stream_open_failure(error: baml.errors.Io | baml.errors.Timeout) -> never {
    match (error) {
        let timeout: baml.errors.Timeout => throw timeout,
        let io: baml.errors.Io => throw _anthropic_sse_failure(io.message),
    }
}

// The streaming entry point: the same request as `invoke` with
// `stream: true`, wrapped in an ai.TurnStream over the SSE connection.
function invoke_stream(c: root.AnthropicClient, input: ai.ModelTurnInput) -> ai.stream.TurnStream {
    let req = _anthropic_request(c, input, true, preview = false);
    let started = baml.time.Instant.now();
    let sse = baml.http.fetch_sse(
        req,
        timeout = baml.time.Duration.from_milliseconds(c.request_timeout_ms ?? 0),
        first_event_timeout = baml.time.Duration.from_milliseconds(c.time_to_first_token_timeout_ms ?? 0),
    ) catch (e) {
        baml.errors.Io => _anthropic_stream_open_failure(e),
        baml.errors.Timeout => _anthropic_stream_open_failure(e),
    };
    let state = AnthStreamState.new();
    // `require_terminal`: the Messages stream ends with an explicit
    // `message_stop`, which the decoder yields as TurnDone. A connection that
    // closes before it (proxy timeout, dropped socket, provider killing the
    // response mid-generation) is a TRUNCATED turn, and without this opt-in the
    // stream would report the partial text as a successful, complete answer.
    let stream = ai.stream.TurnStream.from_sse(
        sse,
        (batch: string) -> {
            anthropic_decode_batch(batch, state) catch_all (e) {
                _ => throw ai.errors.normalize(e),
            }
        },
        require_terminal = "anthropic",
    );
    let call = ai.events.LLMCall {
        client_name: c.id(),
        provider: "anthropic",
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
