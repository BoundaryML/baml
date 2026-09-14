// Runtime narrowing of the *primitive* media types (`image`, `audio`,
// `video`, `pdf`) — `is`, `if let`, and `match`, standalone and inside unions.
//
// Regression coverage for the emitter bug where a media leaf had no type tag
// and fell into the tagless-leaf fallback of `is_type`, compiling every media
// test to constant-FALSE: `v is image` was false for an actual image, and in a
// `match` no media arm ever fired (the last arm swallowed the value, so an
// `image` in a `string | image` came out stringified through the `string`
// arm, and an `image` in `image | audio` selected the `audio` arm). Media
// leaves now take the structural value-matcher path, which reports a media
// value's concrete type as the primitive `image`/`audio`/`video`/`pdf` and so
// discriminates by kind.
//
// The `is` operator inspects a value's runtime type, so — as in
// `ns_is_operator` — every check lives inside a function rather than directly
// in a test block.

// ── Constructors ──────────────────────────────────────────────────────────────

function an_image() -> image {
    baml.media.Image.from_base64("aGVsbG8=", "image/png")
}

function an_audio() -> audio {
    baml.media.Audio.from_base64("d29ybGQ=", "audio/wav")
}

function a_video() -> video {
    baml.media.Video.from_url("https://example.com/clip.mp4", "video/mp4")
}

function a_pdf() -> pdf {
    baml.media.Pdf.from_base64("cGRm", "application/pdf")
}

// ── Group A: `is` on a bare media value ───────────────────────────────────────

function image_is_image() -> string {
    let v = an_image()
    `${v is image}|${v is audio}|${v is video}|${v is pdf}`
}

test "media_is_image_positive_and_negatives" {
    assert.equal(image_is_image(), "true|false|false|false")
}

function audio_is_audio() -> string {
    let v = an_audio()
    `${v is image}|${v is audio}|${v is video}|${v is pdf}`
}

test "media_is_audio_positive_and_negatives" {
    assert.equal(audio_is_audio(), "false|true|false|false")
}

function video_is_video() -> string {
    let v = a_video()
    `${v is image}|${v is audio}|${v is video}|${v is pdf}`
}

test "media_is_video_positive_and_negatives" {
    assert.equal(video_is_video(), "false|false|true|false")
}

function pdf_is_pdf() -> string {
    let v = a_pdf()
    `${v is image}|${v is audio}|${v is video}|${v is pdf}`
}

test "media_is_pdf_positive_and_negatives" {
    assert.equal(pdf_is_pdf(), "false|false|false|true")
}

// A media value is not a string, an int, or a class — and a non-media value is
// not media.
function media_vs_non_media() -> string {
    let img = an_image()
    let s = "aGVsbG8="
    `${img is string}|${img is int}|${s is image}|${(42) is image}`
}

test "media_is_disjoint_from_non_media" {
    assert.equal(media_vs_non_media(), "false|false|false|false")
}

// ── Group B: `string | image` — both directions ───────────────────────────────

function string_or_image_holding_image_is() -> string {
    let v: string | image = an_image()
    `${v is image}|${v is string}`
}

test "string_or_image_holding_image_is" {
    assert.equal(string_or_image_holding_image_is(), "true|false")
}

function string_or_image_holding_string_is() -> string {
    let v: string | image = "hello"
    `${v is image}|${v is string}`
}

test "string_or_image_holding_string_is" {
    assert.equal(string_or_image_holding_string_is(), "false|true")
}

// The `string` arm must not swallow the image — this is the exact shape an
// `ai.Agent` answer takes.
function string_or_image_match(v: string | image) -> string {
    match (v) {
        let s: string => `text:${s}`,
        let i: image => `image:${i.base64()}`,
    }
}

test "string_or_image_match_selects_by_kind" {
    assert.equal(string_or_image_match("hello"), "text:hello");
    assert.equal(string_or_image_match(an_image()), "image:aGVsbG8=")
}

// Arm order must not change the outcome.
function string_or_image_match_reversed(v: string | image) -> string {
    match (v) {
        let i: image => `image:${i.base64()}`,
        let s: string => `text:${s}`,
    }
}

test "string_or_image_match_is_arm_order_independent" {
    assert.equal(string_or_image_match_reversed("hello"), "text:hello");
    assert.equal(string_or_image_match_reversed(an_image()), "image:aGVsbG8=")
}

function string_or_image_if_let(v: string | image) -> string {
    if let p: image = v {
        `image:${p.base64()}`
    } else {
        // The `else` branch narrows to the remaining member, `string`.
        `text:${v}`
    }
}

test "string_or_image_if_let_narrows" {
    assert.equal(string_or_image_if_let(an_image()), "image:aGVsbG8=");
    assert.equal(string_or_image_if_let("hi"), "text:hi")
}

// ── Group C: primitive media unions pick the right arm ────────────────────────

function image_or_audio_match(v: image | audio) -> string {
    match (v) {
        let i: image => `image:${i.base64()}`,
        let a: audio => `audio:${a.base64()}`,
    }
}

test "image_or_audio_match_selects_by_kind" {
    assert.equal(image_or_audio_match(an_image()), "image:aGVsbG8=");
    assert.equal(image_or_audio_match(an_audio()), "audio:d29ybGQ=")
}

// The audio arm is declared FIRST here: before the fix, the first-declared
// media arm won for every media value.
function audio_or_image_match(v: image | audio) -> string {
    match (v) {
        let a: audio => `audio:${a.base64()}`,
        let i: image => `image:${i.base64()}`,
    }
}

test "image_or_audio_match_is_arm_order_independent" {
    assert.equal(audio_or_image_match(an_image()), "image:aGVsbG8=");
    assert.equal(audio_or_image_match(an_audio()), "audio:d29ybGQ=")
}

function four_kind_match(v: image | audio | video | pdf) -> string {
    match (v) {
        let p: pdf => "pdf",
        let vi: video => "video",
        let a: audio => "audio",
        let i: image => "image",
    }
}

test "four_way_media_union_match" {
    assert.equal(four_kind_match(an_image()), "image");
    assert.equal(four_kind_match(an_audio()), "audio");
    assert.equal(four_kind_match(a_video()), "video");
    assert.equal(four_kind_match(a_pdf()), "pdf")
}

// A media union member alongside a class and a primitive.
class Note { text: string }

function mixed_union_match(v: Note | image | int) -> string {
    match (v) {
        let n: Note => `note:${n.text}`,
        let i: image => `image:${i.base64()}`,
        let k: int => `int:${k}`,
    }
}

test "media_in_mixed_union_match" {
    assert.equal(mixed_union_match(Note { text: "a" }), "note:a");
    assert.equal(mixed_union_match(an_image()), "image:aGVsbG8=");
    assert.equal(mixed_union_match(7), "int:7")
}

// ── Group D: iterating a union array ──────────────────────────────────────────

function iterate_string_or_image() -> string {
    let items: (string | image)[] = [
        "one",
        an_image(),
        "two",
        an_image(),
    ];
    let out: string[] = [];
    for (let item in items) {
        out.push(string_or_image_match(item));
    }
    out.join(",")
}

test "iterate_and_match_string_or_image_array" {
    assert.equal(
        iterate_string_or_image(),
        "text:one,image:aGVsbG8=,text:two,image:aGVsbG8=",
    )
}

// The array element type itself narrows: `(string | image)[]` is not
// `string[]`, and a media array is discriminated by kind.
function media_array_is_tests() -> string {
    let imgs: image[] = [an_image()];
    let mixed: (string | image)[] = ["a", an_image()];
    `${imgs is image[]}|${imgs is audio[]}|${imgs is string[]}|${mixed is (string | image)[]}`
}

test "media_array_element_type_narrows" {
    assert.equal(media_array_is_tests(), "true|false|false|true")
}

// ── Group E: optional media ───────────────────────────────────────────────────

function optional_image_match(v: image?) -> string {
    match (v) {
        null => "none",
        let i: image => `image:${i.base64()}`,
    }
}

test "optional_media_narrows" {
    assert.equal(optional_image_match(an_image()), "image:aGVsbG8=");
    assert.equal(optional_image_match(null), "none")
}
