// BUG 4: `v is image` compiled to constant false; a string|image value
// matched NEITHER arm and fell through the last (elided) match arm.
function bug4() -> string {
    let v: string | image = image.from_base64("aGk=", "image/png");
    if (v is image) {
        "image"
    } else if (v is string) {
        "string"
    } else {
        "neither"    // pre-fix result
    }
}

test "bug4_media_narrowing" {
    assert.equal(bug4(), "image")
}
