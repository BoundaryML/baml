// root.GoogleClient — one model turn against the Gemini generateContent API.
// Phase 1: SAP output mode (schema text in the prompt), native function
// calling. Mirrors _planv2/pages/02_guides/03_clients/05_the_built_in_clients.md.

// Envelope classes for the response body. from_string ignores unknown
// fields, so these type only what the parse reads.
class GmFunctionCall {
    name: string,
    args: map<string, unknown>?,
}

class GmPart {
    text: string?,
    thought: bool?,
    functionCall: GmFunctionCall?,
}

class GmContent {
    parts: GmPart[]?,
}

class GmCandidate {
    content: GmContent?,
    finishReason: string?,
}

class GmUsageMetadata {
    promptTokenCount: int?,
    candidatesTokenCount: int?,
}

class GmPromptFeedback {
    blockReason: string?,
}

class GmResponse {
    candidates: GmCandidate[]?,
    usageMetadata: GmUsageMetadata?,
    promptFeedback: GmPromptFeedback?,
}

// One {role, parts} content object.
function _gm_content(role: string, parts: map<string, unknown>[]) -> map<string, unknown> {
    let m: map<string, unknown> = {};
    let _ = m.set("role", role);
    let _ = m.set("parts", parts);
    m
}

// One {text} part.
function _gm_text_part(text: string) -> map<string, unknown> {
    let p: map<string, unknown> = {};
    let _ = p.set("text", text);
    p
}

// One {functionResponse: {name, response}} part.
function _gm_function_response(
    name: string,
    response: map<string, unknown>,
) -> map<string, unknown> {
    let fr: map<string, unknown> = {};
    let _ = fr.set("name", name);
    let _ = fr.set("response", response);
    let part: map<string, unknown> = {};
    let _ = part.set("functionResponse", fr);
    part
}

class GooglePrompt {
    system_parts: map<string, unknown>[],
    contents: map<string, unknown>[],
    first_role: string,
}

// Gemini spells assistant as `model` and carries system instructions outside
// `contents`. Role-less prompt content is an ordinary user turn.
function google_lower_prompt(prompt: ai.Prompt) -> GooglePrompt {
    let system_parts: map<string, unknown>[] = [];
    let contents: map<string, unknown>[] = [];
    let first_role = "";
    for (let message in prompt.messages()) {
        let part = _gm_text_part(message.content);
        if (message.role == "system") {
            system_parts.push(part);
            null
        } else {
            let role = if (message.role == "assistant") {
                "model"
            } else {
                "user"
            };
            if (first_role == "") {
                first_role = role;
                null
            } else {
                null
            };
            contents.push(_gm_content(role, [part]));
            null
        };
    }
    GooglePrompt { system_parts: system_parts, contents: contents, first_role: first_role }
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
function google_lower_journal(j: ai.Journal) -> map<string, unknown>[] {
    let contents: map<string, unknown>[] = [];
    let pending: map<string, unknown>[] = []; // functionResponse parts awaiting flush
    for (let e in j.entries()) {
        match (e) {
            let u: ai.events.UserMessage => {
                if (pending.length() > 0) {
                    contents.push(_gm_content("user", pending));
                    pending = [];
                    null
                } else {
                    null
                };
                contents.push(_gm_content("user", [_gm_text_part(u.content)]));
                null
            },
            let a: ai.events.AssistantMessage => {
                if (pending.length() > 0) {
                    contents.push(_gm_content("user", pending));
                    pending = [];
                    null
                } else {
                    null
                };
                let parts: map<string, unknown>[] = [];
                for (let b in a.content) {
                    match (b) {
                        let t: ai.content.Text => {
                            parts.push(_gm_text_part(t.text));
                            null
                        },
                        let use: ai.content.ToolUse => {
                            let fc: map<string, unknown> = {};
                            let _ = fc.set("name", use.name);
                            let _ = fc.set("args", use.args);
                            let part: map<string, unknown> = {};
                            let _ = part.set("functionCall", fc);
                            parts.push(part);
                            null
                        },
                        // ai.Reasoning lowers to nothing in this phase.
                        _ => null,
                    };
                }
                if (parts.length() > 0) {
                    contents.push(_gm_content("model", parts));
                    null
                } else {
                    null
                };
                null
            },
            let c: ai.events.ToolCompleted => {
                let response: map<string, unknown> = {};
                let _ = response.set("result", c.output);
                pending.push(_gm_function_response(_gm_tool_name_for(j, c.id), response));
                null
            },
            let f: ai.events.ToolFailed => {
                let response: map<string, unknown> = {};
                let _ = response.set("error", f.message);
                pending.push(_gm_function_response(_gm_tool_name_for(j, f.id), response));
                null
            },
            _ => null,
        };
    }
    if (pending.length() > 0) {
        contents.push(_gm_content("user", pending));
        null
    } else {
        null
    };
    contents
}

// ai.ModelTurnInput -> one generateContent request.
function google_render(c: root.GoogleClient, input: ai.ModelTurnInput) -> baml.http.Request {
    _google_request(c, input, false)
}

// The shared request builder: identical body for both paths; streaming
// swaps the method for `:streamGenerateContent?alt=sse`.
function _google_request(
    c: root.GoogleClient,
    input: ai.ModelTurnInput,
    stream: bool,
) -> baml.http.Request {
    let prompt = input.prompt(ai.wire.render_output_format(input.output_type));
    let prompt_parts = google_lower_prompt(prompt);
    let contents = prompt_parts.contents;
    for (let item in google_lower_journal(input.journal)) {
        contents.push(item);
    }
    let body: map<string, unknown> = {};
    // A system-only/assistant-first prompt cannot satisfy Gemini's required
    // leading user turn. In that unusual shape, preserve the old fallback by
    // moving the system parts into a leading user content. Otherwise keep the
    // authored system role in `systemInstruction`.
    let first_role = prompt_parts.first_role;
    if (first_role != "user" && prompt_parts.system_parts.length() > 0) {
        contents = [_gm_content("user", prompt_parts.system_parts)].concat(contents);
        null
    } else if (prompt_parts.system_parts.length() > 0) {
        let system_instruction: map<string, unknown> = {};
        let _ = system_instruction.set("parts", prompt_parts.system_parts);
        let _ = body.set("systemInstruction", system_instruction);
        null
    } else {
        null
    };
    let _ = body.set("contents", contents);
    if (!input.toolbox.is_empty()) {
        let decls: map<string, unknown>[] = [];
        for (let t in input.toolbox.list()) {
            let d: map<string, unknown> = {};
            let _ = d.set("name", t.name);
            let _ = d.set("description", t.description);
            let _ = d.set("parametersJsonSchema", t.input_schema);
            decls.push(d);
        }
        let group: map<string, unknown> = {};
        let _ = group.set("functionDeclarations", decls);
        let _ = body.set("tools", [group]);
        let fcc: map<string, unknown> = {};
        let _ = fcc.set("mode", "AUTO");
        let tc: map<string, unknown> = {};
        let _ = tc.set("functionCallingConfig", fcc);
        let _ = body.set("toolConfig", tc);
        null
    } else {
        null
    };
    let method = if (stream) {
        ":streamGenerateContent?alt=sse"
    } else {
        ":generateContent"
    };
    baml.http.Request {
        method: "POST",
        url: `${c.base_url ?? "https://generativelanguage.googleapis.com/v1beta"}/models/${c.model}${method}`,
        headers: {
            "x-goog-api-key": c.api_key ?? baml.env.get_or_panic("GOOGLE_API_KEY"),
            "content-type": "application/json",
        },
        body: baml.json.stringify(baml.json.to_json(body)),
    }
}

// One decoded response -> canonical content blocks and a stop reason.
// Gemini sends no call ids; synthesize `call_<seq>_<i>` from the journal
// position and the call's index within the turn.
function google_parse(resp: GmResponse, seq: int) -> ai.ModelTurn {
    let content: ai.content.Block[] = [];
    let has_call = false;
    let call_index = 0;
    let cand: GmCandidate? = resp.candidates?.at(0);
    let finish = cand?.finishReason ?? "";
    let parts: GmPart[] = cand?.content?.parts ?? [];
    for (let p in parts) {
        if let fc: GmFunctionCall = p.functionCall {
            let args: map<string, unknown> = {};
            if let a: map<string, unknown> = fc.args {
                args = a;
            }
            content.push(
                ai.content.ToolUse { id: `call_${seq}_${call_index}`, name: fc.name, args: args },
            );
            call_index += 1;
            has_call = true;
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
    let stop = if (has_call) {
        ai.content.StopReason.ToolUse
    } else if (finish == "MAX_TOKENS") {
        ai.content.StopReason.MaxTokens
    } else if (blocked || finish == "SAFETY" || finish == "RECITATION" || finish == "PROHIBITED_CONTENT") {
        ai.content.StopReason.Refused
    } else {
        ai.content.StopReason.Complete
    };
    let usage: ai.events.Usage? = if let um: GmUsageMetadata = resp.usageMetadata {
        ai.events.Usage {
            input_tokens: um.promptTokenCount ?? 0,
            output_tokens: um.candidatesTokenCount ?? 0,
            cached_input_tokens: null,
            reasoning_tokens: null,
        }
    } else {
        null
    };
    ai.ModelTurn { content: content, stop_reason: stop, usage: usage }
}

// The one internal entry point: render, send, normalize — one model turn.
function invoke(c: root.GoogleClient, input: ai.ModelTurnInput) -> ai.ModelTurn {
    // ai.Journal position before this turn commits; the ai.AssistantMessage
    // the runner appends will land at this index, so ids synthesized
    // from it are stable across re-renders.
    let seq = input.journal.entries().length();
    let req = google_render(c, input);
    let resp = ai.wire.send_as<GmResponse>(req, "google");
    google_parse(resp, seq)
}

// ---------------------------------------------------------------------------
// Streaming (`:streamGenerateContent?alt=sse`).
//
// Wire chunks observed: each SSE `data:` payload is a GmResponse fragment;
// text deltas ride in candidates[0].content.parts[].text, the last chunk
// carries finishReason, and usageMetadata (cumulative) appears on chunks.
// There is no explicit terminal event: the stream simply closes.

// One SseStream batch -> provider-agnostic stream events.
function _google_decode_batch(batch: string) -> ai.stream.StreamEvent[] {
    let out: ai.stream.StreamEvent[] = [];
    for (let raw in ai.stream.decode_sse_batch(batch)) {
        if let data: string = raw.data {
            let chunk: GmResponse? = baml.json.from_string<GmResponse>(data) catch_all (e) {
                _ => null,
            };
            if let resp: GmResponse = chunk {
                let cand: GmCandidate? = resp.candidates?.at(0);
                let parts: GmPart[] = cand?.content?.parts ?? [];
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
                } else if (
                    blocked
                        || finish == "SAFETY"
                        || finish == "RECITATION"
                        || finish == "PROHIBITED_CONTENT"
                ) {
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

// The streaming entry point: the same request body as `invoke` against the
// `:streamGenerateContent?alt=sse` method, wrapped in an ai.TurnStream.
function invoke_stream(c: root.GoogleClient, input: ai.ModelTurnInput) -> ai.stream.TurnStream {
    let req = _google_request(c, input, true);
    let sse = baml.http.fetch_sse(req) catch_all (e) {
        _ => {
            throw ai.errors.NetworkFailure {
                provider: "google",
                detail: `transport failed before a response arrived: ${e.to_string()}`,
                raw_body: null,
            }
        },
    };
    ai.stream.TurnStream.from_sse(sse, _google_decode_batch)
}
