// BEP-038 media envelopes are self-describing: `{kind, source, value, mime}`.
// A JSON decode of a media type must honour the envelope's `kind` tag, so a
// union of media types discriminates on it instead of collapsing onto the
// union's first member.
//
// Regression coverage for the runtime decoder ignoring `kind` entirely: it
// read only `source`/`value`/`mime`, so nothing could reject a mismatched
// envelope and `ty_serde_to_value`'s first-match-wins union loop always
// answered with the first member — an audio envelope decoded through
// `image | audio` produced an `Image` carrying `mime=audio/mpeg`, and
// reversing the union to `audio | image` made an image envelope decode as
// `Audio`.
//
// Pinned behaviour: a present `kind` that disagrees with the target media kind
// is a decode failure — in a union arm (so the union tries the next member)
// *and* in a direct single-type decode.

// ── Constructors ──────────────────────────────────────────────────────────────

function mju_image() -> image {
    baml.media.Image.from_base64("aGVsbG8=", "image/png")
}

function mju_audio() -> audio {
    baml.media.Audio.from_base64("d29ybGQ=", "audio/mpeg")
}

function mju_video() -> video {
    baml.media.Video.from_url("https://example.com/clip.mp4", "video/mp4")
}

function mju_pdf() -> pdf {
    baml.media.Pdf.from_base64("cGRm", "application/pdf")
}

// Runtime kind of a decoded media value, as a tag string. A union of media
// types exposes no common members, so the kind (and the payload) must be read
// out through a narrowing `match`.
function mju_tag(v: image | audio | video | pdf) -> string {
    match (v) {
        let i: image => "image",
        let a: audio => "audio",
        let vi: video => "video",
        let p: pdf => "pdf",
    }
}

// `<kind>|<mime>` — proves the decoded value kept its own envelope payload.
function mju_describe(v: image | audio | video | pdf) -> string {
    match (v) {
        let i: image => `image|${i.mime_type() ?? "none"}`,
        let a: audio => `audio|${a.mime_type() ?? "none"}`,
        let vi: video => `video|${vi.mime_type() ?? "none"}`,
        let p: pdf => `pdf|${p.mime_type() ?? "none"}`,
    }
}

// ── Group A: primitive media unions, both orders ──────────────────────────────

function mju_audio_through_image_first() -> string {
    mju_describe(baml.json.from_json<image | audio>(baml.json.to_json(mju_audio())))
}

test "media_json_union_audio_decodes_as_audio_when_image_is_first" {
    assert.equal(mju_audio_through_image_first(), "audio|audio/mpeg")
}

function mju_image_through_audio_first() -> string {
    mju_describe(baml.json.from_json<audio | image>(baml.json.to_json(mju_image())))
}

test "media_json_union_image_decodes_as_image_when_audio_is_first" {
    assert.equal(mju_image_through_audio_first(), "image|image/png")
}

// The same round-trip through the string codec rather than the json value.
function mju_string_roundtrip_tags() -> string {
    let a = baml.json.from_string<image | audio>(baml.json.to_string(mju_audio()))
    let i = baml.json.from_string<audio | image>(baml.json.to_string(mju_image()))
    `${mju_tag(a)}|${mju_tag(i)}`
}

test "media_json_union_string_roundtrip_discriminates" {
    assert.equal(mju_string_roundtrip_tags(), "audio|image")
}

// Every member of a four-way primitive union round-trips to its own kind.
function mju_four_way_tags() -> string {
    let i = baml.json.from_json<image | audio | video | pdf>(baml.json.to_json(mju_image()))
    let a = baml.json.from_json<image | audio | video | pdf>(baml.json.to_json(mju_audio()))
    let v = baml.json.from_json<image | audio | video | pdf>(baml.json.to_json(mju_video()))
    let p = baml.json.from_json<image | audio | video | pdf>(baml.json.to_json(mju_pdf()))
    `${mju_tag(i)}|${mju_tag(a)}|${mju_tag(v)}|${mju_tag(p)}`
}

test "media_json_union_four_way_primitive_union" {
    assert.equal(mju_four_way_tags(), "image|audio|video|pdf")
}

// A url-sourced envelope keeps its payload through the discriminating decode.
function mju_video_url_roundtrip() -> string {
    match (baml.json.from_json<image | video>(baml.json.to_json(mju_video()))) {
        let i: image => `image|${i.url() ?? "none"}`,
        let v: video => `video|${v.url() ?? "none"}`,
    }
}

test "media_json_union_preserves_url_payload" {
    assert.equal(
        mju_video_url_roundtrip(),
        "video|https://example.com/clip.mp4",
    )
}

// ── Group B: unions of the `baml.media.*` wrapper classes ─────────────────────

function mju_wrapper_describe(v: baml.media.Image | baml.media.Audio) -> string {
    match (v) {
        let i: baml.media.Image => `image|${i.mime_type() ?? "none"}`,
        let a: baml.media.Audio => `audio|${a.mime_type() ?? "none"}`,
    }
}

function mju_wrapper_audio() -> string {
    mju_wrapper_describe(
        baml.json.from_json<baml.media.Image | baml.media.Audio>(baml.json.to_json(mju_audio())),
    )
}

test "media_json_union_wrapper_class_union_audio" {
    assert.equal(mju_wrapper_audio(), "audio|audio/mpeg")
}

function mju_wrapper_image() -> string {
    mju_wrapper_describe(
        baml.json.from_json<baml.media.Image | baml.media.Audio>(baml.json.to_json(mju_image())),
    )
}

test "media_json_union_wrapper_class_union_image" {
    assert.equal(mju_wrapper_image(), "image|image/png")
}

// ── Group C: ai.MediaPart, the four-way stdlib wrapper union ──────────────────

function mju_part_tag(v: ai.MediaPart) -> string {
    match (v) {
        let i: baml.media.Image => "image",
        let a: baml.media.Audio => "audio",
        let vi: baml.media.Video => "video",
        let p: baml.media.Pdf => "pdf",
    }
}

function mju_media_part_tags() -> string {
    let i = baml.json.from_json<ai.MediaPart>(baml.json.to_json(mju_image()))
    let a = baml.json.from_json<ai.MediaPart>(baml.json.to_json(mju_audio()))
    let v = baml.json.from_json<ai.MediaPart>(baml.json.to_json(mju_video()))
    let p = baml.json.from_json<ai.MediaPart>(baml.json.to_json(mju_pdf()))
    `${mju_part_tag(i)}|${mju_part_tag(a)}|${mju_part_tag(v)}|${mju_part_tag(p)}`
}

test "media_json_union_ai_media_part_four_way" {
    assert.equal(mju_media_part_tags(), "image|audio|video|pdf")
}

// `ai.content.Media.new` bridges a media primitive into `ai.MediaPart` through
// the envelope, so it must report the kind it was handed.
function mju_content_media_kinds() -> string {
    let i = ai.content.Media.new(mju_image())
    let a = ai.content.Media.new(mju_audio())
    let v = ai.content.Media.new(mju_video())
    let p = ai.content.Media.new(mju_pdf())
    `${i.kind()}|${a.kind()}|${v.kind()}|${p.kind()}`
}

test "media_json_union_ai_content_media_new_kinds" {
    assert.equal(mju_content_media_kinds(), "image|audio|video|pdf")
}

// ── Group D: mixed unions of a class and a media type ─────────────────────────

class MjuNote {
    text: string,
}

function mju_mixed_tag(v: MjuNote | image) -> string {
    match (v) {
        let n: MjuNote => `note:${n.text}`,
        let i: image => `image:${i.mime_type() ?? "none"}`,
    }
}

function mju_mixed_note_first() -> string {
    let n = baml.json.from_json<MjuNote | image>(baml.json.to_json(MjuNote { text: "hi" }))
    let i = baml.json.from_json<MjuNote | image>(baml.json.to_json(mju_image()))
    `${mju_mixed_tag(n)}|${mju_mixed_tag(i)}`
}

test "media_json_union_class_or_image" {
    assert.equal(mju_mixed_note_first(), "note:hi|image:image/png")
}

// The class arm is tried first, so a class whose every field is optional
// structurally accepts any object — including a media envelope. That is the
// generic first-match-wins behaviour of class decoding, independent of the
// media `kind` tag, and it is deliberately not covered here.

// ── Group E: direct (non-union) decode of a mismatched envelope ───────────────
//
// Pinned: the `kind` guard is not union-only. Decoding an audio envelope as
// `image` fails rather than silently producing an `Image` that carries an
// audio payload.

function mju_direct_mismatch() -> string {
    let envelope = baml.json.to_json(mju_audio())
    let decoded: image? = baml.json.from_json<image>(envelope) catch (e) {
        baml.json.JsonDecodeError => null,
    };
    if let i: image = decoded {
        `decoded:${i.mime_type() ?? "none"}`
    } else {
        "decode-error"
    }
}

test "media_json_union_direct_mismatched_kind_fails" {
    assert.equal(mju_direct_mismatch(), "decode-error")
}

function mju_direct_match_still_works() -> string {
    let envelope = baml.json.to_json(mju_audio())
    baml.json.from_json<audio>(envelope).mime_type() ?? "none"
}

test "media_json_union_direct_matching_kind_succeeds" {
    assert.equal(mju_direct_match_still_works(), "audio/mpeg")
}

// A hand-built envelope with no `kind` stays decodable for any media target —
// the tag is the discriminant only when it is present.
function mju_kindless_envelope() -> string {
    let envelope = baml.json.from_string<baml.json.json>(
        "{\"source\":\"base64\",\"value\":\"aGVsbG8=\",\"mime\":\"image/png\"}",
    )
    mju_describe(baml.json.from_json<image>(envelope))
}

test "media_json_union_kindless_envelope_still_decodes" {
    assert.equal(mju_kindless_envelope(), "image|image/png")
}
