// Internal helpers for MEDIA-SHAPED OUTPUTS, used by `ai.Agent`. Nothing here
// is part of the public ai surface.
//
// A declared output type made of media is produced from the turn's content
// blocks, never from SAP-parsing the terminal text (SAP refuses media). This
// is a port of sys_llm's `parse_llm_output_for_target`
// (crates/sys_llm/src/lib.rs), error messages included: they are the
// documented UX for "you asked for an image and the model also talked".
// Like that port, only `image` has list and text-union forms — `audio[]` and
// friends were never supported and still fall through to the text parse.

// Classify a declared output type as `"<form>:<kind>"`, or `""` when it holds
// no media and the ordinary SAP path applies. `type` values only compare
// structurally (there is no reflection into a type's shape), so the supported
// shapes are enumerated. The nullable list/union forms behave exactly like
// their non-null counterparts once decoded, so they share a form.
function _media_shape(t: type) -> string {
    if (t == type.of<image>()) {
        return "one:image";
    }
    if (t == type.of<audio>()) {
        return "one:audio";
    }
    if (t == type.of<video>()) {
        return "one:video";
    }
    if (t == type.of<pdf>()) {
        return "one:pdf";
    }
    if (t == type.of<image?>()) {
        return "opt:image";
    }
    if (t == type.of<audio?>()) {
        return "opt:audio";
    }
    if (t == type.of<video?>()) {
        return "opt:video";
    }
    if (t == type.of<pdf?>()) {
        return "opt:pdf";
    }
    if (t == type.of<image[]>() || t == type.of<image[]?>()) {
        return "list:image";
    }
    if (t == type.of<(string | image)[]>() || t == type.of<(string | image)[]?>()) {
        return "mixedlist:image";
    }
    if (t == type.of<string | image>() || t == type.of<(string | image)?>()) {
        return "mixed:image";
    }
    ""
}

// A media block's payload as its BEP-038 `{kind, source, value, mime}`
// envelope — the form `baml.json.from_json<Out>` decodes media back from.
function _media_envelope(m: root.content.Media) -> baml.json.json {
    baml.json.to_json(m.media) catch_all (e) {
        _ => {
            throw baml.errors.UnknownError { data: e, message: [e.to_string()] }
        },
    }
}

// The turn's media payloads of `kind`, in order (sys_llm's `media_parts`).
function _media_of_kind(turn: root.ModelTurn, kind: string) -> baml.json.json[] {
    let out: baml.json.json[] = [];
    for (let b in turn.content) {
        if let m: root.content.Media = b {
            if (m.kind() == kind) {
                out.push(_media_envelope(m));
            }
        }
    }
    out
}

// The turn's non-blank text and image payloads, in document order
// (sys_llm's `mixed_text_image_parts`). Blank text is dropped, as are
// reasoning and tool-use blocks, which were never output parts.
function _media_mixed_parts(turn: root.ModelTurn) -> baml.json.json[] {
    let items: baml.json.json[] = [];
    for (let b in turn.content) {
        match (b) {
            let t: root.content.Text => {
                if (t.text.trim() != "") {
                    items.push(t.text);
                }
                null
            },
            let m: root.content.Media => {
                if (m.kind() == "image") {
                    items.push(_media_envelope(m));
                }
                null
            },
            _ => null,
        };
    }
    items
}

function _media_all_text(items: baml.json.json[]) -> bool {
    for (let i in items) {
        if (!(i is string)) {
            return false;
        }
    }
    true
}

function _media_join_text(items: baml.json.json[]) -> baml.json.json {
    let out = "";
    for (let i in items) {
        if let s: string = i {
            out += s;
        }
    }
    out
}

// sys_llm's `reject_mixed_media_only_output`: a media-only output type refuses
// to silently drop whatever else the model said.
function _media_reject_mixed(turn: root.ModelTurn, kind: string, provider: string) -> void {
    let unexpected = 0;
    for (let b in turn.content) {
        match (b) {
            let m: root.content.Media => {
                if (m.kind() != kind) {
                    unexpected = unexpected + 1;
                }
                null
            },
            let t: root.content.Text => {
                if (t.text.trim() != "") {
                    unexpected = unexpected + 1;
                }
                null
            },
            _ => null,
        };
    }
    if (unexpected > 0) {
        throw root.errors.ParseFailed {
            provider: provider,
            raw_output: `Expected only ${kind} output parts, got ${unexpected} non-${kind} part(s). Use a text/image union return type to preserve mixed outputs.`,
        }
    }
}
