// BUG 3: from_json through a media union ignored the envelope "kind" —
// first union member always won (audio came back as an Image).
function bug3() -> string {
    let a: image | audio = audio.from_base64("aGk=", "audio/mpeg");
    let back = baml.json.from_json<image | audio>(baml.json.to_json(a));
    match (back) {
        let i: image => "image",
        let s: audio => "audio",
    }
}

test "bug3_kind_survives_round_trip" {
    assert.equal(bug3(), "audio")
}
