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
