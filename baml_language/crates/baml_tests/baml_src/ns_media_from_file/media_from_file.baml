// A media value built from a file holds the file's content, read when the
// value is built. It keeps the file's base name, never its path.
//
// HERMETICITY: every test writes its own file under /tmp and removes it
// before asserting, which also shows that nothing reads the file again.

function media_tmp(name: string) -> string {
    `/tmp/baml_media_${baml.time.Instant.now().to_timestamp_milliseconds()}_${int.random(0, 1000000000)}_${name}`
}

// ============================================================================
// from_file
// ============================================================================

function media_from_file_reads_now() -> string {
    let path = media_tmp("photo.png");
    baml.fs.write(path, "hi");
    let photo = image.from_file(path, null);
    baml.fs.remove(path);
    `${photo.base64()}|${photo.url() ?? "no url"}|${photo.mime_type() ?? "no mime"}`
}

test "from_file holds the content after the file is gone" {
    assert.equal(media_from_file_reads_now(), "aGk=|no url|image/png")
}

function media_from_file_name() -> string {
    let path = media_tmp("report.pdf");
    baml.fs.write(path, "hi");
    let doc = pdf.from_file(path, null);
    baml.fs.remove(path);
    let name = doc.name() ?? "no name";
    if (name.ends_with("_report.pdf") && !name.includes("/")) {
        "base name"
    } else {
        name
    }
}

test "from_file names the value by the file's base name" {
    assert.equal(media_from_file_name(), "base name")
}

function media_from_file_keeps_a_given_mime_type() -> string? {
    let path = media_tmp("clip.mp4");
    baml.fs.write(path, "hi");
    let clip = audio.from_file(path, "audio/x-custom");
    baml.fs.remove(path);
    clip.mime_type()
}

test "from_file keeps a MIME type that is given" {
    assert.equal(media_from_file_keeps_a_given_mime_type(), "audio/x-custom")
}

function media_from_missing_file() -> string {
    {
        video.from_file(media_tmp("missing") + "/nothing.mp4", null);
        "built"
    } catch (e) {
        baml.errors.Io => "io",
        baml.errors.InvalidArgument => "invalid argument",
    }
}

test "from_file fails when the value is built, not at the first request" {
    assert.equal(media_from_missing_file(), "io")
}

// ============================================================================
// from_file_content and names
// ============================================================================

function media_from_file_content() -> string {
    let doc = pdf.from_file_content("/reports/2026/q3.pdf", "aGk=", null);
    `${doc.name() ?? "no name"}|${doc.mime_type() ?? "no mime"}|${doc.base64()}|${doc.url() ?? "no url"}`
}

test "from_file_content names by the base name and infers the MIME type" {
    assert.equal(media_from_file_content(), "q3.pdf|application/pdf|aGk=|no url")
}

function media_other_sources_have_no_name() -> string {
    let from_url = image.from_url("https://example.test/a.png", null);
    let from_base64 = image.from_base64("aGk=", null);
    `${from_url.name() ?? "none"}|${from_base64.name() ?? "none"}`
}

test "only a file gives a name" {
    assert.equal(media_other_sources_have_no_name(), "none|none")
}

// ============================================================================
// infer_mime_type
// ============================================================================

test "infer_mime_type goes by the extension" {
    assert.equal(image.infer_mime_type("photo.JPG"), "image/jpeg");
    assert.equal(image.infer_mime_type("https://example.test/a.webp?size=2"), "image/webp");
    assert.equal(pdf.infer_mime_type("scan.pdf"), "application/pdf");
    // The extension wins over the kind.
    assert.equal(image.infer_mime_type("scan.pdf"), "application/pdf")
}

test "infer_mime_type reads only the last extension of the name" {
    assert.equal(pdf.infer_mime_type("photo.png.pdf"), "application/pdf");
    assert.equal(pdf.infer_mime_type("/scans.png/report.pdf"), "application/pdf");
    // An extension is matched whole: `.avif` is not `.avi`.
    assert.equal(image.infer_mime_type("photo.avif"), "image/png");
    // A URL's query is not part of its extension; a file name's `?` is part of the name.
    assert.equal(image.infer_mime_type("https://example.test/a.gif?as=.png"), "image/gif");
    assert.equal(image.infer_mime_type("what?.gif"), "image/gif")
}

function media_from_file_infers_from_the_name() -> string {
    // The directory looks like an image; the file is a PDF.
    let directory = media_tmp("scans.png");
    let path = `${directory}/q3.pdf`;
    baml.fs.write(path, "hi");
    let doc = pdf.from_file(path, null);
    baml.fs.remove_dir_all(directory);
    `${doc.name() ?? "no name"}|${doc.mime_type() ?? "no mime"}`
}

test "from_file infers the MIME type from the file's name, not its directory" {
    assert.equal(media_from_file_infers_from_the_name(), "q3.pdf|application/pdf")
}

test "infer_mime_type reads a container by the kind" {
    assert.equal(audio.infer_mime_type("a.mp4"), "audio/mp4");
    assert.equal(video.infer_mime_type("a.mp4"), "video/mp4");
    assert.equal(audio.infer_mime_type("a.webm"), "audio/webm");
    assert.equal(video.infer_mime_type("a.webm"), "video/webm")
}

test "infer_mime_type falls back to the usual type of the kind" {
    assert.equal(image.infer_mime_type(""), "image/png");
    assert.equal(audio.infer_mime_type("notes.txt"), "audio/mpeg");
    assert.equal(video.infer_mime_type("clip"), "video/mp4");
    assert.equal(pdf.infer_mime_type("scan"), "application/pdf")
}

// ============================================================================
// JSON
// ============================================================================

function media_json_keeps_the_name() -> string {
    let json = baml.json.to_json(image.from_file_content("/photos/cat.png", "aGk=", null));
    let back = baml.json.from_json<image>(json);
    `${back.name() ?? "no name"}|${back.mime_type() ?? "no mime"}|${back.base64()}`
}

test "a name survives JSON" {
    assert.equal(media_json_keeps_the_name(), "cat.png|image/png|aGk=")
}

function media_json_rejects<T>(json: string) -> bool {
    {
        baml.json.from_string<T>(json);
        false
    } catch (e) {
        baml.json.DecodeError => true
    }
}

test "JSON cannot name a file to read" {
    assert.is_true(media_json_rejects<image>(
        "{\"kind\":\"image\",\"source\":\"file\",\"value\":\"/etc/hosts\",\"mime\":\"image/png\"}",
    ))
}
