// Shared helpers for client authors: transport with classification, the
// output-format text, and tool schema lowering.
// Mirrors _planv2/pages/02_guides/03_clients/03_writing_a_client.md.

/// Collapse a client's `root.Credential?` option to the value to put on the
/// wire, reading the environment at REQUEST time (never at construction).
///
///   * a literal string resolves to itself, unless it is blank — a
///     whitespace-only credential is treated as "not configured" so the rest of
///     the chain can supply one;
///   * a `baml.env.Ref` resolves to `ref.get()` — `null` when the named
///     variable is unset, which lets the caller continue down its own chain (or
///     raise the same error it raises for an unconfigured option);
///   * `null` falls back to `env_fallback`, the provider's canonical variable
///     name, looked up with `baml.env.get`. A `null` `env_fallback` means the
///     provider has no canonical variable and the result is `null`.
///
/// A BLANK value is "not configured" at EVERY rung, not just the literal one:
/// `export OPENAI_API_KEY=` otherwise resolved to `""` and every client built
/// `Authorization: Bearer ` — a provider 401 in place of the local "no
/// credential configured" error the same client raises when the variable is
/// absent entirely. The two spellings of "I did not set this" now behave alike.
///
/// This IS the whole chain a client needs: explicit credential (a literal or an
/// `env.NAME` ref) → the provider's canonical variable (`env_fallback`).
function resolve_credential(cred: root.Credential?, env_fallback: string?) -> string? {
    let explicit: string? = match (cred) {
        let literal: string => root.internal._configured(literal),
        let named: baml.env.Ref => root.internal._configured(named.get() ?? ""),
        null => null,
    };
    if let value: string = explicit {
        return value;
    }
    if let fallback: string = env_fallback {
        return root.internal._configured(baml.env.get(fallback) ?? "");
    }
    null
}

/// Scrub credential-bearing query parameters out of free text before it becomes
/// an `ai.errors.Failure.detail`.
///
/// Providers that authenticate through the URL — Vertex AI express mode puts
/// the key in `?key=…` — would otherwise leak it: the native HTTP layer renders
/// `reqwest::Error`, whose `Display` embeds the request URL, straight into the
/// transport-failure message.
function redact_url_secrets(text: string) -> string {
    // "key=" also covers "api_key="/"apikey=", "token=" covers
    // "access_token="/"id_token=": the preceding characters are kept verbatim.
    let keys = root.internal._redact_after(text, "key=");
    let tokens = root.internal._redact_after(keys, "token=");
    root.internal._redact_after(tokens, "secret=")
}

/// The environment variables a credential chain consulted, as a readable list,
/// for the "no credential configured" error a client raises when the whole
/// chain came back empty.
///
/// The point is the `Ref` arm: `api_key = env.MY_KEY` pointing at an UNSET
/// variable resolves to `null` and falls through to the rest of the chain, so
/// without this the error would name only the provider's canonical variable and
/// never mention the one the caller actually asked for. Values are never
/// included — only names.
function credential_sources(cred: root.Credential?, canonical_env: string?) -> string {
    let names: string[] = [];
    if let named: baml.env.Ref = cred {
        names.push(named.name);
    }
    if let canonical: string = canonical_env {
        names.push(canonical);
    }
    if (names.length() == 0) {
        "(no environment variable was named)"
    } else {
        names.join(", ")
    }
}

// Send, classify non-2xx via root.errors.classify_http, decode the body as T.
// T = json-shaped envelope class; unknown fields are ignored by the decoder.
function send_as<T>(req: baml.http.Request, provider: string) -> T {
    let resp = baml.http.send(req) catch_all (e) {
        _ => {
            throw root.errors.NetworkFailure {
            provider: provider,
            // redacted: the rendered transport error can embed the request URL,
            // and some providers authenticate through a query parameter.
            detail: `transport failed before a response arrived: ${redact_url_secrets(e.to_string())}`,
            raw_body: null,
        }
        },
    };
    let text = resp.text() catch_all (e) {
        _ => {
            throw root.errors.NetworkFailure {
            provider: provider,
            detail: "the response body could not be read completely",
            raw_body: null,
        }
        },
    };
    if (!resp.ok()) {
        // headers last: they are what fills RateLimited.retry_after_ms on a 429.
        throw root.errors.classify_http(provider, resp.status_code, text, headers = resp.headers)
    }
    baml.json.from_string<T>(text) catch_all (e) {
        _ => throw root.errors.ParseFailed { provider: provider, raw_output: text },
    }
}

// The return type's schema as prompt text, in the standard dialect —
// exactly what ${ctx.output_format} renders in an LLM function block.
function render_output_format(t: type) -> string {
    baml.prompt.render_output_format(t)
}

/// Provider-neutral media payload after resolving local files and, when
/// requested, ordinary HTTP(S) URLs. Google Cloud `gs://` URLs remain URLs.
class ResolvedMedia {
    url: string?,
    base64: string?,
    mime_type: string,
}

/// Resolve BAML media for provider request lowering. This mirrors the media
/// resolution pass in `sys_llm`: local files become base64, existing base64 is
/// retained, and URL fetching is selected by the provider.
function resolve_media(media: ai.MediaPart, fetch_url: bool) -> ResolvedMedia {
    match (media) {
        let image: baml.media.Image => {
            let source = image.file() ?? image.url() ?? "";
            root.internal._resolve_media_content(
                image.url(),
                image.file(),
                image.base64(),
                root.internal._media_mime_type(image.mime_type(), source, "image"),
                fetch_url,
            )
        },
        let audio: baml.media.Audio => {
            let source = audio.file() ?? audio.url() ?? "";
            root.internal._resolve_media_content(
                audio.url(),
                audio.file(),
                audio.base64(),
                root.internal._media_mime_type(audio.mime_type(), source, "audio"),
                fetch_url,
            )
        },
        let video: baml.media.Video => {
            let source = video.file() ?? video.url() ?? "";
            root.internal._resolve_media_content(
                video.url(),
                video.file(),
                video.base64(),
                root.internal._media_mime_type(video.mime_type(), source, "video"),
                fetch_url,
            )
        },
        let pdf: baml.media.Pdf => {
            let source = pdf.file() ?? pdf.url() ?? "";
            root.internal._resolve_media_content(
                pdf.url(),
                pdf.file(),
                pdf.base64(),
                root.internal._media_mime_type(pdf.mime_type(), source, "pdf"),
                fetch_url,
            )
        },
    }
}

// ---------------------------------------------------------------------------
// Request-body passthrough (PLAN P0.3).
//
// Every client models its request as typed classes, serializes that to `json`,
// and then applies the user's `request_body: json?` escape hatch as the LAST
// step before the body is stringified. The merge is deep, and it copies the
// engine's option semantics exactly:
//
//   * an override value that is a MAP merges recursively into a map already at
//     that key (keys the override does not mention survive);
//   * an override value of NULL DELETES the key — that is how a caller removes
//     a default the client injected (`{"max_tokens": null}` drops max_tokens);
//   * everything else (scalars, arrays) REPLACES wholesale — arrays are never
//     element-merged or appended;
//   * an override key absent from the base is added;
//   * an override map landing on a non-map base value replaces it with the
//     override's non-null entries.
//
// Nothing is mutated: `base` and `overrides` are left as the caller built them.

/// Deep-merge a client's `request_body` passthrough into its serialized
/// request. Call this as the LAST step of building the wire body:
///
///     let body = baml.json.to_json(typed_request);
///     let merged = ai.wire.merge_request_body(body, c.request_body);
///     baml.json.stringify(merged)
///
/// Maps merge recursively, a `null` override value DELETES that key (the
/// engine's removal contract — `{"max_tokens": null}` removes the default the
/// client injected), and every other value replaces wholesale. A `null`
/// `overrides` means "no passthrough configured" and returns `base` unchanged;
/// it never deletes the whole body.
function merge_request_body(base: baml.json.json, overrides: baml.json.json?) -> baml.json.json {
    match (overrides) {
        null => base,
        _ => root.internal._merge_json(base, overrides),
    }
}

// ---------------------------------------------------------------------------
// Journal sanitization (PLAN P0.6).
//
// The replay hazards every provider shares, fixed once instead of four times.
// pi-ai's `transform-messages.ts` is the reference behavior; what the BAML
// journal can express is a subset, and the differences are called out here
// because they are load-bearing for anyone porting the rest of pi's rules:
//
//   * ERRORED/ABORTED TURNS. `ai.Journal` has no errored-turn marker: a client
//     failure is THROWN as an `ai.errors.Failure`, so a turn that died on the
//     wire never reaches the log at all (`ai.Agent._attempt_turn` appends the
//     AssistantMessage only after `invoke` returns). The residue such a turn
//     CAN leave — an AssistantMessage carrying nothing replayable — is dropped
//     by the empty-turn rule below. The runner's repair loop is deliberately
//     NOT touched: a turn whose answer failed to parse is committed together
//     with the correction UserMessage, and the model needs to see both.
//   * REASONING SIGNATURES. `ai.content.Reasoning` is a readable summary with
//     no signature/encrypted-payload field, so there is no signature to strip
//     and no redacted variant to detect. What IS keyed on identity: reasoning
//     produced by a DIFFERENT client id is dropped, since replaying one
//     model's thinking to another is what the signature would have guarded.
//   * TOOL-CALL ID REWRITING (pi's per-API `normalizeToolCallId`) is not done
//     here: the id shape a provider accepts is that provider's business, and
//     rewriting ids centrally would desynchronize them from the journal.
//
// Client identity is `ai.events.AssistantMessage.client_id`, which the runner
// fills from `Client.id()` — `provider/model` for the built-in clients, so a
// model switch inside one provider counts as a different client, exactly like
// pi's provider+api+model comparison.

/// Make one run's journal safe to replay to `client_id`, and return the
/// sanitized copy. Every client should lower
/// `ai.wire.sanitize_for_client(input.journal, self.id())` instead of
/// `input.journal`. The rules, in the order they apply per event:
///
///  1. An assistant turn from a DIFFERENT `client_id` loses its
///     `ai.content.Reasoning` blocks: one model's thinking is not replayable
///     to another (and providers that check reasoning signatures reject it).
///  2. Empty and whitespace-only `ai.content.Text` blocks are dropped from
///     assistant turns.
///  3. An assistant turn left with no blocks at all is dropped entirely —
///     the reasoning-only turn that lowers to `content: []` and makes
///     Anthropic 400, and the same shape an aborted turn leaves behind.
///     Such a turn never has tool calls, so nothing is orphaned by dropping it.
///  4. A tool call with no `ToolCompleted`/`ToolFailed` before the next
///     assistant or user message (or before the end of the journal) gets a
///     synthesized `ToolFailed { message: "No result provided" }`, because
///     most APIs reject a tool call that was never answered.
///
/// Journal-only events (`RunStarted`, `ToolRequested`, `Usage`,
/// `FinalProduced`) pass through untouched — clients skip them anyway — and,
/// like the clients' own lowering, they never close an open tool-result group.
/// Nothing here mutates the journal it is given.
function sanitize_for_client(j: root.Journal, client_id: string) -> root.Journal {
    let out: root.events.Event[] = [];
    // ids of tool calls emitted by the last assistant turn, still unanswered
    let pending: string[] = [];
    for (let event in j.entries()) {
        match (event) {
            let assistant: root.events.AssistantMessage => {
                root.internal._flush_orphaned_calls(out, pending);
                let blocks = root.internal._wire_blocks(assistant.content, assistant.client_id == client_id);
                if (blocks.length() > 0) {
                    out.push(
                        root.events.AssistantMessage {
                            content: blocks,
                            client_id: assistant.client_id,
                        },
                    );
                    for (let block in blocks) {
                        if let use: root.content.ToolUse = block {
                            pending.push(use.id);
                        }
                    }
                    null
                } else {
                    null
                };
                null
            },
            let user: root.events.UserMessage => {
                root.internal._flush_orphaned_calls(out, pending);
                out.push(user);
                null
            },
            let completed: root.events.ToolCompleted => {
                root.internal._resolve_pending_call(pending, completed.id);
                out.push(completed);
                null
            },
            let failed: root.events.ToolFailed => {
                root.internal._resolve_pending_call(pending, failed.id);
                out.push(failed);
                null
            },
            _ => {
                out.push(event);
                null
            },
        };
    }
    root.internal._flush_orphaned_calls(out, pending);
    root.Journal { log: out }
}
