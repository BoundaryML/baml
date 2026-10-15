// `${param}` and `${param.field}` written directly in an LLM function's
// `prompt:` are bound: `ai.PromptMessage.bound_parts` keeps the run each one
// rendered to as an `ai.Bound`, while `parts` stays exactly what it was.
//
// Helpers use a `pb_` prefix to keep names unique across the shared compile.

class PbTicket {
    message: string,
    tier: string,
}

class PbLoud {
    word: string,

    implements baml.ToString {
        function to_string(self) -> string throws never {
            self.word.to_upper_case()
        }
    }
}

function PbTriage(ticket: PbTicket, note: string, shot: image) -> bool {
    client: "openai/gpt-4o-mini"
    prompt: `
        ${role("system")}
        Decide urgency.

        ${role("user")}
        Tier: ${ticket.tier}
        Message: ${ticket.message} Note: ${note.trim()}
        ${for (let x in [1, 2])}${x}${endfor}
        ${shot}
    `
}

function PbShadowed(note: string) -> bool {
    client: "openai/gpt-4o-mini"
    prompt: `
        Before: ${note}
        ${for (let note in ["loop"])}${note}${endfor}
    `
}

function PbEdges(pad: string, shot: image, loud: PbLoud) -> bool {
    client: "openai/gpt-4o-mini"
    prompt: `
        ${role("user")}
        ${pad}
        ${shot}
        ${pad} and ${loud}
    `
}

function pb_png() -> image {
    baml.media.Image.from_base64("iVBORw0KGgo=", "image/png")
}

function pb_messages<T>(spec: ai.FunctionSpec<T>) -> ai.PromptMessage[] {
    spec.prompt_template(ai.internal.build_output_format(reflect.Type.of<bool>())).messages()
}

function pb_describe(part: ai.BoundPromptPart) -> string {
    match (part) {
        let text: string => baml.json.stringify(text),
        let bound: ai.Bound => {
            let inner: string[] = [];
            for (let p in bound.parts) {
                inner.push(pb_describe(p));
            }
            `[${bound.path}: ${inner.join(" ")}]`
        },
        _ => "<media>",
    }
}

function pb_describe_parts(parts: ai.PromptPart[]) -> string {
    let out: string[] = [];
    for (let p in parts) {
        out.push(pb_describe(p));
    }
    out.join(" ")
}

function pb_describe_bound(parts: ai.BoundPromptPart[]) -> string {
    let out: string[] = [];
    for (let p in parts) {
        out.push(pb_describe(p));
    }
    out.join(" ")
}

// `bound_parts` with every `ai.Bound` unwrapped and adjacent strings joined,
// which must reproduce `parts`.
function pb_unbind(parts: ai.BoundPromptPart[]) -> ai.PromptPart[] {
    let flat: ai.PromptPart[] = [];
    for (let p in parts) {
        match (p) {
            let bound: ai.Bound => {
                for (let inner in bound.parts) {
                    flat.push(inner);
                }
                null
            },
            let part: ai.PromptPart => {
                flat.push(part);
                null
            },
        };
    }
    let out: ai.PromptPart[] = [];
    for (let p in flat) {
        let joined = false;
        if let text: string = p {
            if let last: string = out.at(out.length() - 1) {
                out[out.length() - 1] = last + text;
                joined = true;
            }
        }
        if (!joined) {
            out.push(p);
        }
    }
    out
}

function pb_parts_match(messages: ai.PromptMessage[]) -> bool {
    for (let m in messages) {
        if (pb_describe_parts(m.parts) != pb_describe_parts(pb_unbind(m.bound_parts))) {
            return false;
        }
    }
    true
}

function pb_triage() -> ai.PromptMessage[] {
    pb_messages(PbTriage@spec(PbTicket { message: "Down.", tier: "gold" }, " hi ", pb_png()))
}

test "parameter paths are bound, computed and loop interpolations are not" {
    let user = pb_triage()[1];
    assert.equal(
        pb_describe_bound(user.bound_parts),
        "\"Tier: \" [ticket.tier: \"gold\"] \"\\nMessage: \" [ticket.message: \"Down.\"] \" Note: hi\\n12\" [shot: <media>]",
    )
}

test "parts are unchanged by binding" {
    let messages = pb_triage();
    assert.equal(pb_describe_parts(messages[0].parts), "\"Decide urgency.\"");
    assert.equal(
        pb_describe_parts(messages[1].parts),
        "\"Tier: gold\\nMessage: Down. Note: hi\\n12\" <media>",
    );
    assert.is_true(pb_parts_match(messages))
}

test "a name the template binds is never bound" {
    let message = pb_messages(PbShadowed@spec("outer"))[0];
    assert.equal(pb_describe_bound(message.bound_parts), "\"Before: outer\\nloop\"");
    assert.is_true(pb_parts_match([message]))
}

test "edge trimming and ToString overrides render as unbound" {
    let message = pb_messages(PbEdges@spec("  padded  ", pb_png(), PbLoud { word: "hey" }))[0];
    assert.equal(pb_describe_parts(message.parts), "\"padded\" <media> \"padded   and HEY\"");
    assert.equal(
        pb_describe_bound(message.bound_parts),
        "[pad: \"padded\"] [shot: <media>] [pad: \"padded  \"] \" and \" [loud: \"HEY\"]",
    );
    assert.is_true(pb_parts_match([message]))
}
