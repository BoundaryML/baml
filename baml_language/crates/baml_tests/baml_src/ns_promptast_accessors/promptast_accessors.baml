// B-627 — `ai.Prompt` readable accessors.
//
// `ai.Prompt` (returned by `<Fn>$render_prompt` and the `prompt` tag) is a
// `$rust_type`-backed handle. Before B-627 it had no way to read the rendered
// prompt from BAML — `.text()`/`.messages()` didn't exist and `to_string`
// leaked the Rust `Debug` of the opaque handle. These tests pin the new
// accessors and the readable `to_string`.
//
// Helpers use a `paa_` prefix to ensure unique names across the shared
// compile (function locals aren't boxed by the VM, unlike test-block locals).

// ─── shared assembly ──────────────────────────────────────────────────────────

// A `prompt` body with `${role(...)}` markers, assembled against a hand-built
// `Context` — the shared fixture every accessor test evaluates against.
function paa_render_ast() -> ai.Prompt {
    let cc = ai.ContextClient { name: "c", provider: "openai", default_role: "system", allowed_roles: ["system", "user"] }
    let ctx = ai.Context { client: cc, tags: {}, _output_format: ai.internal.build_output_format(reflect.Type.of<string>()) }
    let render = prompt`${role("system")}You are helpful.${role("user")}Hi World!`
    render(ctx)
}

// ─── .text() ──────────────────────────────────────────────────────────────────

function paa_text_fn() -> string {
    paa_render_ast().text()
}

test "text_renders_role_headed_prompt" {
    assert.equal(paa_text_fn(), "[system]\nYou are helpful.\n\n[user]\nHi World!")
}

// ─── to_string (via the `baml.ToString` override) ─────────────────────────────

// `string.from` routes through the `baml.ToString` override.
function paa_to_string_fn() -> string {
    string.from(paa_render_ast())
}

test "to_string_is_readable_not_rust_debug" {
    assert.equal(paa_to_string_fn(), "[system]\nYou are helpful.\n\n[user]\nHi World!")
    // The bug: `to_string` used to leak the opaque handle's Rust `Debug`
    // (`Prompt { _data: <rust_data> }`, `Adt(...)`, `String(...)`). Must not.
    assert.is_true(paa_to_string_fn().includes("Adt(") == false)
    assert.is_true(paa_to_string_fn().includes("String(") == false)
    assert.is_true(paa_to_string_fn().includes("$rust_type") == false)
    assert.is_true(paa_to_string_fn().includes("_data") == false)
    assert.is_true(paa_to_string_fn().includes("rust_data") == false)
}

// ─── .messages() ──────────────────────────────────────────────────────────────

// Fold the structured messages back into a string so the assertion needs no
// nested-value inspection: `role=content` per message, `;`-separated.
function paa_fold_messages_fn() -> string {
    let out = ""
    for (let m in paa_render_ast().messages()) {
        out += m.role + "=" + m.content + ";"
    }
    out
}

test "messages_expose_role_and_content" {
    assert.equal(paa_fold_messages_fn(), "system=You are helpful.;user=Hi World!;")
}

// The element type is the new `ai.PromptMessage` class — pinned by this
// helper's declared return type type-checking.
function paa_single_message_fn() -> ai.PromptMessage[] {
    let cc = ai.ContextClient { name: "c", provider: "openai", default_role: "system", allowed_roles: ["system"] }
    let ctx = ai.Context { client: cc, tags: {}, _output_format: ai.internal.build_output_format(reflect.Type.of<string>()) }
    let render = prompt`Just a system prompt.`
    render(ctx).messages()
}

test "messages_yields_promptmessage_instances" {
    assert.equal(paa_single_message_fn().length(), 1)
    assert.equal(paa_single_message_fn()[0].content, "Just a system prompt.")
}

// ── Ported from tests/promptast_accessors.rs ────────────────────────────────

// `ai.Role.metadata` is the per-message channel providers read for directives
// like Anthropic `cache_control`. `collect_structured_messages` used to drop
// it; it must reach `ai.PromptMessage.metadata` intact, and a message that
// carried none must read as an empty map (never null).
function pa_message_metadata_survives_assembly() -> string {
    let cc = ai.ContextClient { name: "c", provider: "anthropic", default_role: "user", allowed_roles: ["system", "user"] };
    let ctx = ai.Context { client: cc, tags: {}, _output_format: ai.internal.build_output_format(reflect.Type.of<string>()) };
    let cached = ai.Role { name: "system", metadata: { "cache_control": { "type": "ephemeral" } } };
    let render = prompt`${cached}Long shared preamble.${role("user")}Hi World!`;
    let out: string[] = [];
    for (let m in render(ctx).messages()) {
        out.push(`${m.role}=${baml.json.stringify(baml.json.to_json(m.metadata))}`)
    }
    out.join(";")
}

test "message_metadata_survives_prompt_assembly" {
    assert.equal(
        pa_message_metadata_survives_assembly(),
        "system={\"cache_control\":{\"type\":\"ephemeral\"}};user={}",
    )
}

// Every media kind interpolated into a prompt must survive as its own
// structural part, interleaved with the surrounding text runs.
function pa_message_parts_preserve_media() -> string {
    let cc = ai.ContextClient { name: "c", provider: "openai", default_role: "user", allowed_roles: ["user"] };
    let ctx = ai.Context { client: cc, tags: {}, _output_format: ai.internal.build_output_format(reflect.Type.of<string>()) };
    let img = image.from_url("https://example.com/photo.png", "image/png");
    let sound = audio.from_url("https://example.com/sound.mp3", "audio/mpeg");
    let movie = video.from_url("https://example.com/movie.mp4", "video/mp4");
    let document = pdf.from_url("https://example.com/document.pdf", "application/pdf");
    let render = prompt`before:${img}:${sound}:${movie}:${document}:after`;
    let kinds: string[] = [];
    for (let part in render(ctx).messages()[0].parts) {
        match (part) {
            let text: string => kinds.push(`text=${text}`),
            let image: baml.media.Image => kinds.push(`image=${image.url() ?? ""}`),
            let audio: baml.media.Audio => kinds.push(`audio=${audio.url() ?? ""}`),
            let video: baml.media.Video => kinds.push(`video=${video.url() ?? ""}`),
            let pdf: baml.media.Pdf => kinds.push(`pdf=${pdf.url() ?? ""}`),
        }
    }
    kinds.join("|")
}

test "message_parts_preserve_all_media_structurally" {
    assert.equal(
        pa_message_parts_preserve_media(),
        "text=before:|image=https://example.com/photo.png|text=:|audio=https://example.com/sound.mp3|text=:|video=https://example.com/movie.mp4|text=:|pdf=https://example.com/document.pdf|text=:after",
    )
}
