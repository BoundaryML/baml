// Canonical spellings must resolve both static and instance builtin members.
// These exercise the compiler and VM together, beyond the type-IR registry tests.

test "compiler_alias_media_factories_and_methods" {
    let i = image.from_base64("aGk=", "image/png");
    let a = audio.from_base64("aGk=", "audio/wav");
    let v = video.from_url("https://example.test/movie", "video/mp4");
    let p = pdf.from_base64("aGk=", "application/pdf");
    assert.equal(i.base64(), "aGk=");
    assert.equal(i.mime_type(), "image/png");
    assert.equal(a.mime_type(), "audio/wav");
    assert.equal(v.url(), "https://example.test/movie");
    assert.equal(p.mime_type(), "application/pdf")
}

function iterate_alias(values: string[]) -> string throws never {
    let iterable: baml.iter.Iterable<Item=string, Error=never> = values;
    iterable.iter().collect().join(",")
}

test "compiler_alias_array_interface_dispatch" {
    assert.equal(iterate_alias(["Ada", "Grace"]), "Ada,Grace");
    assert.equal(iterate_alias([]), "")
}

// `baml.media.Image` is the declaration that carries `image`'s members, not a
// second nominal type: both spellings denote the one media type, in
// annotations, parameters, unions built from the class spelling, and
// reflection alike.
function image_mime(value: baml.media.Image) -> string? {
    value.mime_type()
}

test "compiler_alias_media_class_spelling_is_the_primitive" {
    let img = image.from_base64("aGk=", "image/png");
    let w: baml.media.Image = img;
    assert.equal(image_mime(img), "image/png");
    assert.equal(image_mime(w), "image/png");
    assert.equal(w.base64(), "aGk=");

    // `ai.PromptPart = string | MediaPart`, `MediaPart` spelled through the
    // `baml.media.*` classes; a primitive media value is a member directly.
    let p: ai.PromptPart = img;
    let kind = match (p) {
        let i: image => i.mime_type(),
        _ => "not an image",
    };
    assert.equal(kind, "image/png");

    assert.is_true(reflect.Type.of<baml.media.Image>() == reflect.Type.of<image>());
    assert.equal(reflect.Type.of<baml.media.Image>().to_string(), "image");
    assert.equal(reflect.Type.of<baml.media.Audio[]>().to_string(), "audio[]")
}
