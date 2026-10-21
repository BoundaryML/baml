// Every prompt reaches the Claude Code CLI as one stream-json user message on
// stdin: text as text blocks, media as content blocks. The process-level
// transport (argv and stdin) is covered in tests/claude_code_media.rs.

function CcLook(picture: image) -> string {
    client: "claude-code/haiku"
    prompt: `
        ${role("user")}
        Look at ${picture}
    `
}

function CcRead(doc: pdf) -> string {
    client: "claude-code/haiku"
    prompt: `
        ${role("user")}
        Read ${doc}
    `
}

function CcListen(clip: audio) -> string {
    client: "claude-code/haiku"
    prompt: `
        ${role("user")}
        Listen to ${clip}
    `
}

function cc_render(spec: ai.FunctionSpec<string>) -> ai.Prompt {
    spec.prompt_template(ai.internal.build_output_format(spec.output_type()))
}

// The stdin message, parsed.
function cc_stdin(spec: ai.FunctionSpec<string>, suffix: string) -> json {
    baml.json.parse(claude_code.internal.stdin_message(cc_render(spec), suffix).trim())
}

function cc_error(spec: ai.FunctionSpec<string>) -> string {
    let _ = claude_code.internal.stdin_message(cc_render(spec), "") catch_all (e) {
        let r: ai.errors.InvalidRequest => {
            return `InvalidRequest: ${r.detail}`;
        },
        _ => {
            return `unexpected: ${e.to_string()}`;
        },
    };
    "accepted"
}

function CcGreet(name: string) -> string {
    client: "claude-code/haiku"
    prompt: `
        ${role("system")}
        Be brief.
        ${role("user")}
        Greet ${name}
    `
}

// A text-only prompt is one text block that reads exactly as `text()`
// renders it, suffix included.
function cc_text_blocks() -> string {
    let suffix = "\n\nConversation so far: none";
    let spec = CcGreet@spec("Ada");
    let line = cc_stdin(spec, suffix);
    let content = baml.json.path_or<json[]>(line, ".message.content", []);
    let same = baml.json.path_or<string>(line, ".message.content[0].text", "-")
        == `${cc_render(spec).text()}${suffix}`;
    `${content.length()}|${baml.json.path_or<string>(line, ".message.content[0].type", "-")}|${same}`
}

test "claude_code_text_prompt_is_one_text_block" {
    assert.equal(cc_text_blocks(), "1|text|true")
}

// The image lands in place of its placeholder, after the same `[role]` header
// text() renders (the renderer trims text that faces a media part), and the
// suffix follows as its own text block.
function cc_image_blocks() -> string {
    let line = cc_stdin(
        CcLook@spec(baml.media.Image.from_base64("SGVsbG8=", "image/png")),
        "\n\nConversation so far: none",
    );
    [
        baml.json.path_or<string>(line, ".type", "-"),
        baml.json.path_or<string>(line, ".message.role", "-"),
        baml.json.path_or<string>(line, ".message.content[0].text", "-"),
        baml.json.path_or<string>(line, ".message.content[1].type", "-"),
        baml.json.path_or<string>(line, ".message.content[1].source.type", "-"),
        baml.json.path_or<string>(line, ".message.content[1].source.media_type", "-"),
        baml.json.path_or<string>(line, ".message.content[1].source.data", "-"),
        baml.json.path_or<string>(line, ".message.content[2].text", "-"),
    ]
        .join("|")
}

test "claude_code_image_becomes_a_content_block" {
    assert.equal(
        cc_image_blocks(),
        "user|user|[user]\nLook at|image|base64|image/png|SGVsbG8=|\n\nConversation so far: none",
    )
}

function cc_url_image_source() -> string {
    let line = cc_stdin(CcLook@spec(baml.media.Image.from_url("https://example.com/a.png", null)), "");
    `${baml.json.path_or<string>(line, ".message.content[1].source.type", "-")}|${baml.json.path_or<string>(line, ".message.content[1].source.url", "-")}`
}

test "claude_code_url_image_passes_through" {
    assert.equal(cc_url_image_source(), "url|https://example.com/a.png")
}

function cc_pdf_block() -> string {
    let line = cc_stdin(CcRead@spec(baml.media.Pdf.from_base64("JVBERg==", "application/pdf")), "");
    `${baml.json.path_or<string>(line, ".message.content[1].type", "-")}|${baml.json.path_or<string>(line, ".message.content[1].source.media_type", "-")}`
}

test "claude_code_pdf_becomes_a_document_block" {
    assert.equal(cc_pdf_block(), "document|application/pdf")
}

function cc_image_media_type(mime: string) -> string {
    let line = cc_stdin(CcLook@spec(baml.media.Image.from_base64("SGVsbG8=", mime)), "");
    baml.json.path_or<string>(line, ".message.content[1].source.media_type", "-")
}

test "claude_code_lowercases_image_mime" {
    assert.equal(cc_image_media_type("IMAGE/PNG"), "image/png")
}

test "claude_code_rejects_unsupported_image_mime" {
    let got = cc_error(CcLook@spec(baml.media.Image.from_base64("PHN2Zy8+", "image/svg+xml")));
    assert.contains(got, "InvalidRequest: ");
    assert.contains(got, "image/svg+xml")
}

test "claude_code_rejects_audio" {
    assert.equal(
        cc_error(CcListen@spec(baml.media.Audio.from_base64("QUJD", "audio/mp3"))),
        "InvalidRequest: claude-code does not support audio prompt input",
    )
}
