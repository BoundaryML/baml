// root.google.GoogleClient — one model turn against the Gemini generateContent API.
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

// Gemini correlates tool results by NAME, not id: find the tool name for a
// result id by scanning the root.ai.AssistantMessage root.ai.ToolUse blocks in the journal.
function _gm_tool_name_for(j: root.ai.Journal, id: string) -> string {
    let name = "";
    for (let e in j.entries()) {
        match (e) {
            let a: root.ai.AssistantMessage => {
                for (let b in a.content) {
                    match (b) {
                        let u: root.ai.ToolUse => {
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

// root.ai.Journal -> generateContent `contents`. Only promptable events lower;
// root.ai.RunStarted, root.ai.ToolRequested, root.ai.Usage, and root.ai.FinalProduced are journal-only.
// Consecutive tool results merge into ONE user content of functionResponse
// parts, matching how Gemini expects results for a multi-call turn.
function google_lower_journal(j: root.ai.Journal) -> map<string, unknown>[] {
    let contents: map<string, unknown>[] = [];
    let pending: map<string, unknown>[] = []; // functionResponse parts awaiting flush
    for (let e in j.entries()) {
        match (e) {
            let u: root.ai.UserMessage => {
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
            let a: root.ai.AssistantMessage => {
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
                        let t: root.ai.Text => {
                            parts.push(_gm_text_part(t.text));
                            null
                        },
                        let use: root.ai.ToolUse => {
                            let fc: map<string, unknown> = {};
                            let _ = fc.set("name", use.name);
                            let _ = fc.set("args", use.args);
                            let part: map<string, unknown> = {};
                            let _ = part.set("functionCall", fc);
                            parts.push(part);
                            null
                        },
                        // root.ai.Reasoning lowers to nothing in this phase.
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
            let c: root.ai.ToolCompleted => {
                let response: map<string, unknown> = {};
                let _ = response.set("result", c.output);
                pending.push(_gm_function_response(_gm_tool_name_for(j, c.id), response));
                null
            },
            let f: root.ai.ToolFailed => {
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

// root.ai.ModelTurnInput -> one generateContent request.
function google_render(
    c: root.google.GoogleClient,
    input: root.ai.ModelTurnInput,
) -> baml.http.Request {
    let instructions = input.prompt.render_text(root.ai.render_output_format(input.output_type));
    let lowered = google_lower_journal(input.journal);
    let body: map<string, unknown> = {};
    // DEVIATION from the wire page: the instructions travel as the leading
    // user content on EVERY turn, and systemInstruction is never sent. With
    // systemInstruction + lowered journal, a later turn's contents begin with
    // the model's functionCall turn, and the API rejects that: "Please ensure
    // that function call turn comes immediately after a user turn or after a
    // function response turn."
    let contents: map<string, unknown>[] = [_gm_content("user", [_gm_text_part(instructions)])];
    for (let item in lowered) {
        contents.push(item);
    }
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
    baml.http.Request {
        method: "POST",
        url: `${c.base_url ?? "https://generativelanguage.googleapis.com/v1beta"}/models/${c.model}:generateContent`,
        headers: { "x-goog-api-key": c.api_key, "content-type": "application/json" },
        body: baml.json.stringify(baml.json.to_json(body)),
    }
}

// One decoded response -> canonical content blocks and a stop reason.
// Gemini sends no call ids; synthesize `call_<seq>_<i>` from the journal
// position and the call's index within the turn.
function google_parse(resp: GmResponse, seq: int) -> root.ai.ModelTurn {
    let content: root.ai.ContentBlock[] = [];
    let has_call = false;
    let call_index = 0;
    let finish = "";
    if let cands: GmCandidate[] = resp.candidates {
        if let cand: GmCandidate = cands.at(0) {
            finish = cand.finishReason ?? "";
            if let cc: GmContent = cand.content {
                if let parts: GmPart[] = cc.parts {
                    for (let p in parts) {
                        if let fc: GmFunctionCall = p.functionCall {
                            let args: map<string, unknown> = {};
                            if let a: map<string, unknown> = fc.args {
                                args = a;
                            }
                            content
                                .push(
                                    root.ai.ToolUse {
                                        id: `call_${seq}_${call_index}`,
                                        name: fc.name,
                                        args: args,
                                    },
                                );
                            call_index += 1;
                            has_call = true;
                            null
                        } else {
                            if let t: string = p.text {
                                if (p.thought ?? false) {
                                    content.push(root.ai.Reasoning { summary: t });
                                    null
                                } else {
                                    content.push(root.ai.Text { text: t });
                                    null
                                }
                            } else {
                                null
                            }
                        };
                    }
                    null
                } else {
                    null
                }
            } else {
                null
            }
        } else {
            null
        }
    } else {
        null
    };
    let blocked = if let pf: GmPromptFeedback = resp.promptFeedback {
        pf.blockReason != null
    } else {
        false
    };
    let stop = if (has_call) {
        root.ai.StopReason.ToolUse
    } else if (finish == "MAX_TOKENS") {
        root.ai.StopReason.MaxTokens
    } else if (blocked || finish == "SAFETY" || finish == "RECITATION" || finish == "PROHIBITED_CONTENT") {
        root.ai.StopReason.Refused
    } else {
        root.ai.StopReason.Complete
    };
    let usage: root.ai.Usage? = if let um: GmUsageMetadata = resp.usageMetadata {
        root.ai.Usage {
            input_tokens: um.promptTokenCount ?? 0,
            output_tokens: um.candidatesTokenCount ?? 0,
            cached_input_tokens: null,
            reasoning_tokens: null,
        }
    } else {
        null
    };
    root.ai.ModelTurn { content: content, stop_reason: stop, usage: usage }
}

// The one internal entry point: render, send, normalize — one model turn.
function invoke(c: root.google.GoogleClient, input: root.ai.ModelTurnInput) -> root.ai.ModelTurn {
    // root.ai.Journal position before this turn commits; the root.ai.AssistantMessage
    // the runner appends will land at this index, so ids synthesized
    // from it are stable across re-renders.
    let seq = input.journal.entries().length();
    let req = google_render(c, input);
    let resp = root.ai.send_as<GmResponse>(req, "google");
    google_parse(resp, seq)
}
