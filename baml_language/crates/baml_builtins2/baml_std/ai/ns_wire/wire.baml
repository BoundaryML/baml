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
/// This IS the whole chain a client needs: explicit credential (a literal or an
/// `env.NAME` ref) → the provider's canonical variable (`env_fallback`).
function resolve_credential(cred: root.Credential?, env_fallback: string?) -> string? {
    let explicit: string? = match (cred) {
        let literal: string => {
            if (literal.trim() == "") {
                null
            } else {
                literal
            }
        },
        let named: baml.env.Ref => named.get(),
        null => null,
    };
    if let value: string = explicit {
        return value;
    }
    if let fallback: string = env_fallback {
        return baml.env.get(fallback);
    }
    null
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
            detail: `transport failed before a response arrived: ${e.to_string()}`,
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

function _media_mime_type(explicit: string?, source_value: string, kind: string) -> string {
    if let supplied: string = explicit {
        return supplied;
    }
    let source = source_value.to_lower_case();
    if (source.includes(".jpg") || source.includes(".jpeg")) {
        "image/jpeg"
    } else if (source.includes(".png")) {
        "image/png"
    } else if (source.includes(".webp")) {
        "image/webp"
    } else if (source.includes(".gif")) {
        "image/gif"
    } else if (source.includes(".svg")) {
        "image/svg+xml"
    } else if (source.includes(".wav")) {
        "audio/wav"
    } else if (source.includes(".mp3")) {
        "audio/mpeg"
    } else if (source.includes(".flac")) {
        "audio/flac"
    } else if (source.includes(".ogg")) {
        "audio/ogg"
    } else if (source.includes(".webm")) {
        if (kind == "audio") {
            "audio/webm"
        } else {
            "video/webm"
        }
    } else if (source.includes(".mov")) {
        "video/quicktime"
    } else if (source.includes(".mp4")) {
        "video/mp4"
    } else if (source.includes(".avi")) {
        "video/x-msvideo"
    } else if (source.includes(".mkv")) {
        "video/x-matroska"
    } else if (source.includes(".pdf")) {
        "application/pdf"
    } else if (kind == "audio") {
        "audio/mpeg"
    } else if (kind == "video") {
        "video/mp4"
    } else if (kind == "pdf") {
        "application/pdf"
    } else {
        "image/png"
    }
}

function _resolve_media_content(
    url: string?,
    file: string?,
    inline: string,
    mime: string,
    fetch_url: bool,
) -> ResolvedMedia {
    if let path: string = file {
        let handle = baml.fs.open(path, "r");
        let data = handle.bytes().to_base64();
        handle.close();
        return ResolvedMedia { url: null, base64: data, mime_type: mime };
    }
    if (inline != "") {
        return ResolvedMedia { url: null, base64: inline, mime_type: mime };
    }
    if let media_url: string = url {
        if (media_url.starts_with("data:")) {
            if let separator: int = media_url.index_of(";base64,") {
                return ResolvedMedia {
                    url: null,
                    base64: media_url.slice(separator + 8, media_url.length()),
                    mime_type: media_url.slice(5, separator),
                };
            }
        }
        if (!fetch_url || media_url.starts_with("gs://")) {
            return ResolvedMedia { url: media_url, base64: null, mime_type: mime };
        }
        let response = baml.http.fetch(media_url);
        if (!response.ok()) {
            throw root.errors.NetworkFailure {
                provider: "media",
                detail: `media URL ${media_url} returned HTTP ${response.status_code}`,
                raw_body: null,
            }
        }
        let response_mime = response.headers.get("content-type") ?? mime;
        return ResolvedMedia {
            url: null,
            base64: response.bytes().to_base64(),
            mime_type: response_mime.split(";").at(0) ?? mime,
        };
    }
    throw baml.errors.InvalidArgument { message: "media has no URL, file, or base64 content" }
}

/// Resolve BAML media for provider request lowering. This mirrors the media
/// resolution pass in `sys_llm`: local files become base64, existing base64 is
/// retained, and URL fetching is selected by the provider.
function resolve_media(media: ai.MediaPart, fetch_url: bool) -> ResolvedMedia {
    match (media) {
        let image: baml.media.Image => {
            let source = image.file() ?? image.url() ?? "";
            _resolve_media_content(
                image.url(),
                image.file(),
                image.base64(),
                _media_mime_type(image.mime_type(), source, "image"),
                fetch_url,
            )
        },
        let audio: baml.media.Audio => {
            let source = audio.file() ?? audio.url() ?? "";
            _resolve_media_content(
                audio.url(),
                audio.file(),
                audio.base64(),
                _media_mime_type(audio.mime_type(), source, "audio"),
                fetch_url,
            )
        },
        let video: baml.media.Video => {
            let source = video.file() ?? video.url() ?? "";
            _resolve_media_content(
                video.url(),
                video.file(),
                video.base64(),
                _media_mime_type(video.mime_type(), source, "video"),
                fetch_url,
            )
        },
        let pdf: baml.media.Pdf => {
            let source = pdf.file() ?? pdf.url() ?? "";
            _resolve_media_content(
                pdf.url(),
                pdf.file(),
                pdf.base64(),
                _media_mime_type(pdf.mime_type(), source, "pdf"),
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

// Recursive merge of one patch value into one base value. Public entry point
// is `merge_request_body`; this is the arm that recurses on objects.
function _merge_json(base: baml.json.json, patch: baml.json.json) -> baml.json.json {
    match (patch) {
        let patch_object: map<string, baml.json.json> => {
            let out: map<string, baml.json.json> = {};
            match (base) {
                let base_object: map<string, baml.json.json> => {
                    // copy first so the base key order is preserved: `set` on an
                    // existing key keeps that key's original position.
                    for (let key in base_object.keys()) {
                        let _ = out.set(key, base_object.get(key));
                    }
                    null
                },
                // a non-object base is replaced by the override object; the
                // loop below still drops its null (delete) entries.
                _ => null,
            };
            for (let key in patch_object.keys()) {
                let value = patch_object.get(key);
                match (value) {
                    null => {
                        let _ = out.delete(key);
                        null
                    },
                    _ => {
                        // `out.get(key)` is null for a key the base lacks, which
                        // recurses into the "non-object base" arm above.
                        let _ = out.set(key, _merge_json(out.get(key), value));
                        null
                    },
                };
            }
            out
        },
        _ => patch,
    }
}

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
        _ => _merge_json(base, overrides),
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

// The blocks of one assistant turn that can still go on the wire.
// Reasoning survives only for the client that produced it; empty and
// whitespace-only text blocks are dropped (Anthropic rejects them outright,
// and no provider needs them).
function _wire_blocks(
    content: root.content.Block[],
    keep_reasoning: bool,
) -> root.content.Block[] {
    let out: root.content.Block[] = [];
    for (let block in content) {
        match (block) {
            let text: root.content.Text => {
                if (text.text.trim() != "") {
                    out.push(text);
                }
                null
            },
            let reasoning: root.content.Reasoning => {
                if (keep_reasoning) {
                    out.push(reasoning);
                }
                null
            },
            let use: root.content.ToolUse => {
                out.push(use);
                null
            },
            // A generated media output is a product of the turn, not
            // replayable model input; clients decide how (whether) to
            // re-present it.
            let media: root.content.Media => null,
        };
    }
    out
}

// Close every tool call still awaiting a result, in request order, and reset
// the pending list. `out` and `pending` are mutated in place. The repair is a
// FAILED result rather than a successful empty one: the model must be able to
// tell that the call did not run.
function _flush_orphaned_calls(out: root.events.Event[], pending: string[]) -> void {
    for (let id in pending) {
        out.push(root.events.ToolFailed { id: id, message: "No result provided" });
    }
    pending.clear();
}

// Mark one tool call answered.
function _resolve_pending_call(pending: string[], id: string) -> void {
    if let index: int = pending.index_of(id) {
        let _ = pending.remove_at(index);
    }
}

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
                _flush_orphaned_calls(out, pending);
                let blocks = _wire_blocks(assistant.content, assistant.client_id == client_id);
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
                _flush_orphaned_calls(out, pending);
                out.push(user);
                null
            },
            let completed: root.events.ToolCompleted => {
                _resolve_pending_call(pending, completed.id);
                out.push(completed);
                null
            },
            let failed: root.events.ToolFailed => {
                _resolve_pending_call(pending, failed.id);
                out.push(failed);
                null
            },
            _ => {
                out.push(event);
                null
            },
        };
    }
    _flush_orphaned_calls(out, pending);
    root.Journal { log: out }
}
