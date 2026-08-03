//# What the model's JSON protocol messages parse into (see ns_ai/openai.baml).

class ToolSays {
    kind: string,
    tool: string,
    args: map<string, unknown>,
}

class FinalSays {
    kind: string,
    value: unknown,
}

//# Models sometimes wrap JSON in markdown fences. The real feature's SAP

//# parser absorbs this (and much more); this hand-rolled protocol has to

//# do it manually — which is itself an argument for the real feature.

function extract_payload(content: string) -> string {
    let parts = content.split("```");
    if (parts.length() >= 3) {
        let inner = parts.at(1) ?? "";
        let lines = inner.split("\n");
        if ((lines.at(0) ?? "") == "json" || (lines.at(0) ?? "") == "JSON") {
            lines.slice(1, lines.length()).join("\n")
        } else {
            inner
        }
    } else {
        content
    }
}

class PlanTripParser {
    implements root.ai.ActionParser {
        function parse(self, raw: string, call_seq: int) -> root.ai.Event[] {
            let content = extract_payload(raw);
            let ts: ToolSays? = baml.json.from_string<ToolSays>(content) catch_all (e) {
                _ => null
            };
            if let t: ToolSays = ts {
                if (t.kind == "tool") {
                    return [
                        root.ai
                            .tool_requested(
                                `t${call_seq}`,
                                t.tool,
                                baml.json.stringify(baml.json.to_json(t.args)),
                            ),
                    ];
                }
            }
            let fs: FinalSays? = baml.json.from_string<FinalSays>(content) catch_all (e) {
                _ => null
            };
            if let f: FinalSays = fs {
                if (f.kind == "final") {
                    return [root.ai.final_produced(baml.json.stringify(baml.json.to_json(f.value)))];
                }
            }
            return []; // a plain reply: the AssistantMessage event carries it
        }
    }
}
