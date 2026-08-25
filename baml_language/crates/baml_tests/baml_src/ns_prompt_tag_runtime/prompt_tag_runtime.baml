// BEP-049 §10 (M5) — the built-in `prompt` tag, runtime execution.
//
// Test-block locals are boxed by the VM; helper functions are not.
// Helper functions carry a `pt_` prefix to keep names unique across the corpus.

// ─── shared class definitions ─────────────────────────────────────────────────

class Point {
    x int
    y int
}

class Labeled {
    tag string
    implements baml.ToString {
        function to_string(self) -> string throws never {
            "LBL<" + self.tag + ">"
        }
    }
}

class MediaBundle {
    picture image
    sound audio
    clip video
    document pdf
}

class MediaEnvelope {
    primary MediaBundle
    gallery MediaBundle[]
    lookup map<string, MediaBundle>
}

class LabeledMedia {
    label Labeled
    picture image
}

class RedactedMedia {
    picture image
    implements baml.ToString {
        function to_string(self) -> string throws never {
            "REDACTED"
        }
    }
}

class Person {
    name string
    age int
}

// Variant with optional nickname field (renamed to keep both Person shapes in namespace).
class PersonNick {
    name string
    nickname string?
}

// ─── Role construction (isolation) ────────────────────────────────────────────

// Isolation: does constructing a `Role { name, metadata }` even type-check?
function pt_role_construction_fn() -> ai.Role {
    ai.Role { name: "system", metadata: {} }
}

test "role_construction_isolation" {
    assert.equal(pt_role_construction_fn().name, "system")
}

// ─── prompt tag structural assembly (M5d) ─────────────────────────────────────

// `${role("...")}` markers split the content into chat messages: expect a
// system message "You are helpful." then a user message "Hi World!".
function pt_role_messages_fn() -> string {
    let name = "World"
    let cc = ai.ContextClient { name: "c", provider: "openai", default_role: "user", allowed_roles: ["user"] }
    let ctx = ai.Context { client: cc, tags: {} }
    let render = ai.prompt`${role("system")}You are helpful.${role("user")}Hi ${name}!`
    render(ctx).text()
}

test "prompt_tag_builds_promptast_with_role_messages" {
    assert.equal(
        pt_role_messages_fn(),
        "[system]\nYou are helpful.\n\n[user]\nHi World!"
    )
}

// Ergonomic fallback: bare `prompt`...`` resolves to `ai.prompt`
// (no `ai.` qualifier needed). Same assembly as the qualified form.
function pt_unqualified_prompt_fn() -> string {
    let name = "World"
    let cc = ai.ContextClient { name: "c", provider: "openai", default_role: "user", allowed_roles: ["user"] }
    let ctx = ai.Context { client: cc, tags: {} }
    let render = prompt`${role("system")}You are helpful.${role("user")}Hi ${name}!`
    render(ctx).text()
}

test "unqualified_prompt_tag_resolves_to_baml_llm_prompt" {
    assert.equal(
        pt_unqualified_prompt_fn(),
        "[system]\nYou are helpful.\n\n[user]\nHi World!"
    )
}

// ─── composite interpolation (B-563) ──────────────────────────────────────────

// B-563: `${class_value}` / `${array_value}` inside a `prompt` backtick string
// used to silently render an EMPTY string — the assembler only stringified
// scalars and dropped every composite. They must render via the same implicit
// `to_string` (`string.from`) an ordinary backtick string applies, so the
// `prompt` form is byte-identical to the ordinary-string form (which was never
// broken). The helper renders both and returns them joined by a sentinel.
function pt_class_array_interp_fn() -> string {
    let p = Point { x: 1, y: 2 }
    let xs = [10, 20, 30]
    let cc = ai.ContextClient { name: "c", provider: "openai", default_role: "system", allowed_roles: ["system"] }
    let ctx = ai.Context { client: cc, tags: {} }
    let render = prompt`Point ${p} list ${xs}`
    let from_prompt = render(ctx).text()
    let from_string = `Point ${p} list ${xs}`
    from_prompt + " <=> " + from_string
}

test "prompt_interpolates_class_and_array_like_ordinary_string" {
    // The composite renders (not empty — the B-563 bug produced empty here) and
    // is exactly what the same values yield in an ordinary backtick string.
    assert.equal(
        pt_class_array_interp_fn(),
        "Point Point { x: 1, y: 2 } list [10, 20, 30] <=> Point Point { x: 1, y: 2 } list [10, 20, 30]"
    )
}

// B-563 follow-through: because composites route through `string.from` (the
// real implicit `to_string`), a user `baml.ToString` override is honored in a
// `prompt` exactly as it is in an ordinary backtick string — a sys-op-level
// structural renderer could not do this. `${role(...)}` still splits messages.
function pt_tostring_override_fn() -> string {
    let v = Labeled { tag: "hi" }
    let cc = ai.ContextClient { name: "c", provider: "openai", default_role: "system", allowed_roles: ["system", "user"] }
    let ctx = ai.Context { client: cc, tags: {} }
    let render = prompt`${role("system")}head ${v}${role("user")}tail ${v}`
    render(ctx).text()
}

test "prompt_interpolation_honors_to_string_override" {
    // Both messages render the override, and the roles still split the content.
    assert.equal(
        pt_tostring_override_fn(),
        "[system]\nhead LBL<hi>\n\n[user]\ntail LBL<hi>"
    )
}

// Structural media interpolation (B-1039)

function pt_media_bundle_fn() -> MediaBundle {
    MediaBundle {
        picture: image.from_url("https://example.com/picture.png", "image/png"),
        sound: audio.from_url("https://example.com/sound.wav", "audio/wav"),
        clip: video.from_url("https://example.com/clip.mp4", "video/mp4"),
        document: pdf.from_url("https://example.com/document.pdf", "application/pdf"),
    }
}

function pt_media_string_interp_fn() -> string {
    let picture = image.from_url("https://example.com/picture.png", "image/png")
    `Image ${picture}`
}

test "ordinary_string_interpolation_keeps_media_tostring_behavior" {
    assert.equal(
        pt_media_string_interp_fn(),
        "Image Image { _data: <rust_data> }"
    )
}

function pt_nested_media_fn() -> string {
    let bundle = pt_media_bundle_fn()
    let envelope = MediaEnvelope {
        primary: bundle,
        gallery: [bundle],
        lookup: { "copy": bundle },
    }
    let cc = ai.ContextClient { name: "c", provider: "openai", default_role: "user", allowed_roles: ["user"] }
    let ctx = ai.Context { client: cc, tags: {} }
    let render = prompt`${role("user")}Inspect ${envelope}`
    render(ctx).text()
}

test "prompt_preserves_media_nested_in_classes_arrays_and_maps" {
    let rendered = pt_nested_media_fn()
    assert.contains(rendered, "[user]\nInspect MediaEnvelope")
    assert.contains(rendered, "primary: MediaBundle")
    assert.contains(rendered, "gallery: [MediaBundle")
    assert.contains(rendered, "lookup: {\"copy\": MediaBundle")
    assert.contains(rendered, "image::url(https://example.com/picture.png, loaded=false)")
    assert.contains(rendered, "audio::url(https://example.com/sound.wav, loaded=false)")
    assert.contains(rendered, "video::url(https://example.com/clip.mp4, loaded=false)")
    assert.contains(rendered, "pdf::url(https://example.com/document.pdf, loaded=false)")
    assert.is_true(rendered.includes("rust_data") == false)
}

function pt_nested_tostring_and_media_fn() -> string {
    let value = LabeledMedia {
        label: Labeled { tag: "hi" },
        picture: image.from_url("https://example.com/picture.png", "image/png"),
    }
    let cc = ai.ContextClient { name: "c", provider: "openai", default_role: "user", allowed_roles: ["user"] }
    let ctx = ai.Context { client: cc, tags: {} }
    let render = prompt`${value}`
    render(ctx).text()
}

test "prompt_preserves_nested_tostring_overrides_beside_media" {
    assert.equal(
        pt_nested_tostring_and_media_fn(),
        "LabeledMedia { label: LBL<hi>, picture: image::url(https://example.com/picture.png, loaded=false) }"
    )
}

function pt_parent_tostring_owns_media_fn() -> string {
    let value = RedactedMedia {
        picture: image.from_url("https://example.com/secret.png", "image/png"),
    }
    let cc = ai.ContextClient { name: "c", provider: "openai", default_role: "user", allowed_roles: ["user"] }
    let ctx = ai.Context { client: cc, tags: {} }
    let render = prompt`${value}`
    render(ctx).text()
}

test "prompt_parent_tostring_override_owns_media_subtree" {
    assert.equal(pt_parent_tostring_owns_media_fn(), "REDACTED")
}

// ${ctx.output_format()} (M5b)

// BEP-049 M5b: `${ctx.output_format()}` renders the return type's schema.
// Build the same schema handle the orchestrator supplies and assert the
// assembled prompt embeds its rendered schema.
function pt_ctx_output_format_fn() -> string {
    let cc = ai.ContextClient { name: "c", provider: "openai", default_role: "user", allowed_roles: ["user"] }
    let of = ai.internal.build_output_format(reflect.Type.of<Person>())
    let ctx = ai.Context { client: cc, tags: {}, _output_format: of }
    let render = ai.prompt`Answer using this schema:
${ctx.output_format()}`
    render(ctx).text()
}

test "prompt_interpolates_ctx_output_format" {
    assert.contains(pt_ctx_output_format_fn(), "Answer using this schema:")
    // The rendered output_format lists the Person fields.
    assert.contains(pt_ctx_output_format_fn(), "name")
    assert.contains(pt_ctx_output_format_fn(), "age")
}

function PtRenderPerson() -> Person {
    client: "openai/gpt-4o-mini"
    prompt: `Make a person.${ctx.output_format()}`
}

function pt_render_prompt_companion_output_format_fn() -> string {
    // `$render_prompt` returns the structural prompt; use `.text()` only when
    // this assertion intentionally wants its readable text projection.
    PtRenderPerson$render_prompt().text()
}

test "render_prompt_companion_interpolates_ctx_output_format" {
    assert.contains(pt_render_prompt_companion_output_format_fn(), "Make a person.")
    assert.contains(pt_render_prompt_companion_output_format_fn(), "name")
    assert.contains(pt_render_prompt_companion_output_format_fn(), "age")
}

// Generated prompt closures use the same structural assembler as the
// built-in `prompt` tag above. This exercises the real @spec path instead of
// constructing a prompt tag by hand.
function PtStructuredGreeting(name: string) -> string {
    client: "openai/gpt-4o-mini"
    tools: []
    prompt: `${role("system")}Follow the rules.${role("user")}Hello ${name}.`
}

function pt_spec_prompt_messages_fn() -> ai.PromptMessage[] {
    PtStructuredGreeting@spec(name = "Ada").prompt().messages()
}

test "spec_prompt_render_preserves_structural_messages" {
    let messages = pt_spec_prompt_messages_fn()
    assert.equal(messages.length(), 2)
    assert.equal(messages[0].role, "system")
    assert.equal(messages[0].content, "Follow the rules.")
    assert.equal(messages[1].role, "user")
    assert.equal(messages[1].content, "Hello Ada.")
}

function pt_render_prompt_messages_fn() -> ai.PromptMessage[] {
    PtStructuredGreeting$render_prompt(name = "Ada").messages()
}

test "render_prompt_companion_returns_promptast_messages" {
    let messages = pt_render_prompt_messages_fn()
    assert.equal(messages.length(), 2)
    assert.equal(messages[0].role, "system")
    assert.equal(messages[1].role, "user")
}

function PtStructuredImage(photo: image) -> string {
    client: "openai/gpt-4o-mini"
    tools: []
    prompt: `${role("user")}Inspect ${photo}`
}

function pt_spec_prompt_media_fn() -> string {
    let photo = image.from_url("https://example.com/spec-photo.png", "image/png")
    PtStructuredImage@spec(photo = photo).prompt().messages()[0].content
}

test "spec_prompt_render_keeps_media_in_promptast" {
    // `messages()` is the readable inspection projection. The Rust-level test
    // additionally pattern-matches the underlying PromptAstSimple::Media node.
    assert.contains(
        pt_spec_prompt_media_fn(),
        "image::url(https://example.com/spec-photo.png, loaded=false)",
    )
}

// BEP-049 M5b.2: `${ctx.output_format(prefix=..., ...)}` re-renders the
// return type's schema with caller options. `Context._output_format` carries
// the prebuilt schema handle; a non-default `prefix` must appear in the
// assembled prompt, proving the option took effect. Exercises two infra paths:
// a method call on a body-param inside a template, and an io-function with
// optional params called with most omitted.
function pt_output_format_prefix_fn() -> string {
    let cc = ai.ContextClient { name: "c", provider: "openai", default_role: "user", allowed_roles: ["user"] }
    let rt = reflect.Type.of<Person>()
    let ctx = ai.Context { client: cc, tags: {}, _output_format: ai.internal.build_output_format(rt) }
    let render = ai.prompt`${ctx.output_format(prefix = "Use this exact schema:")}`
    render(ctx).text()
}

test "prompt_interpolates_ctx_output_format_options" {
    // The custom `prefix` option is applied and the schema lists Person fields.
    assert.contains(pt_output_format_prefix_fn(), "Use this exact schema:")
    assert.contains(pt_output_format_prefix_fn(), "name")
    assert.contains(pt_output_format_prefix_fn(), "age")
}

// Regression: a method io-sysop (`$rust_io_function` instance method) called
// with a LATER optional arg provided and an EARLIER one omitted. The call
// plan's `param_index` is receiver-relative (self stripped), but the sys-op
// default arena is indexed self-inclusive — so the omitted `prefix` read
// `self`'s (absent) default → `OmittedArg` → engine panic. Here
// `quote_class_fields = true` is provided while `prefix` (and everything
// before it) is omitted. Must render the schema, not panic.
function pt_output_format_omitted_leading_fn() -> string {
    let cc = ai.ContextClient { name: "c", provider: "openai", default_role: "user", allowed_roles: ["user"] }
    let rt = reflect.Type.of<Person>()
    let ctx = ai.Context { client: cc, tags: {}, _output_format: ai.internal.build_output_format(rt) }
    let render = ai.prompt`${ctx.output_format(quote_class_fields = true)}`
    render(ctx).text()
}

test "prompt_output_format_omits_leading_optional_arg" {
    // The rendered schema lists the Person fields (and rendering did not panic).
    assert.contains(pt_output_format_omitted_leading_fn(), "name")
    assert.contains(pt_output_format_omitted_leading_fn(), "age")
}

// `render_null_as = "omit"` replaces the default `string or null` rendering of
// the optional `nickname` field with `string or omit`.
function pt_output_format_null_as_omit_fn() -> string {
    let cc = ai.ContextClient { name: "c", provider: "openai", default_role: "user", allowed_roles: ["user"] }
    let rt = reflect.Type.of<PersonNick>()
    let ctx = ai.Context { client: cc, tags: {}, _output_format: ai.internal.build_output_format(rt) }
    let render = ai.prompt`${ctx.output_format(render_null_as = "omit")}`
    render(ctx).text()
}

test "prompt_output_format_can_render_null_as_omit" {
    // Custom null rendering applies to output_format().
    assert.contains(pt_output_format_null_as_omit_fn(), "nickname: string or omit,")
    // …and the default null rendering is replaced.
    assert.is_true(pt_output_format_null_as_omit_fn().includes("string or null") == false)
}
