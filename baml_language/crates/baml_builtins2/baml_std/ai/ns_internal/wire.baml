// Internal helpers behind `ai.wire`: credential normalization, URL redaction,
// request-body merging, and journal sanitization. Nothing here is part of the
// public ai surface.

// A credential value, or `null` when it is blank (see `ai.wire.resolve_credential`).
function _configured(value: string) -> string? {
    if (value.trim() == "") {
        null
    } else {
        value
    }
}

// Replace the value that follows each `marker` with `REDACTED`, up to the first
// delimiter that can end a query value inside a rendered error string.
function _redact_after(text: string, marker: string) -> string {
    let out = "";
    let rest = text;
    while (true) {
        let found = rest.index_of(marker);
        if let start: int = found {
            let head = start + marker.length();
            out += rest.slice(0, head);
            out += "REDACTED";
            let tail = rest.slice(head, rest.length());
            rest = tail.slice(_redact_value_end(tail), tail.length());
        } else {
            return out + rest;
        }
    }
    out
}

function _redact_value_end(tail: string) -> int {
    let cut = tail.length();
    for (let delimiter in ["&", " ", "\"", "'", ")", ",", "\n", "\t"]) {
        if let at: int = tail.index_of(delimiter) {
            if (at < cut) {
                cut = at;
            }
        }
    }
    cut
}

// Recursive merge of one patch value into one base value. Public entry point
// is `ai.wire.merge_request_body`; this is the arm that recurses on objects.
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

// The blocks of one assistant turn that can still go on the wire.
// Reasoning survives only for the client that produced it; empty and
// whitespace-only text blocks are dropped (Anthropic rejects them outright,
// and no provider needs them).
function _wire_blocks(content: root.content.Block[], keep_reasoning: bool) -> root.content.Block[] {
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
