function classify_media_json_union(value: image | audio | video | pdf) -> string {
    match (value) {
        image => "image",
        audio => "audio",
        video => "video",
        pdf => "pdf",
    }
}

function media_json_union_roundtrips_every_kind() -> string {
    let image_json = baml.json.to_json(image.from_base64("aGk=", "image/png"));
    let audio_json = baml.json.to_json(audio.from_base64("aGk=", "audio/mpeg"));
    let video_json = baml.json.to_json(video.from_base64("aGk=", "video/mp4"));
    let pdf_json = baml.json.to_json(pdf.from_base64("aGk=", "application/pdf"));

    let image_value = baml.json.from_json<image | audio | video | pdf>(image_json);
    let audio_value = baml.json.from_json<image | audio | video | pdf>(audio_json);
    let video_value = baml.json.from_json<image | audio | video | pdf>(video_json);
    let pdf_value = baml.json.from_json<image | audio | video | pdf>(pdf_json);

    classify_media_json_union(image_value)
        + ","
        + classify_media_json_union(audio_value)
        + ","
        + classify_media_json_union(video_value)
        + ","
        + classify_media_json_union(pdf_value)
}

function media_json_union_respects_reversed_order() -> string {
    let json = baml.json.to_json(image.from_base64("aGk=", "image/png"));
    let value = baml.json.from_json<audio | image>(json);
    match (value) {
        audio => "audio",
        image => "image",
    }
}

function rejects_media_json<T>(json: string) -> bool {
    {
        baml.json.from_string<T>(json);
        false
    } catch (e) {
        baml.json.JsonDecodeError => true
    }
}

test "media JSON unions roundtrip every kind" {
    assert.equal(media_json_union_roundtrips_every_kind(), "image,audio,video,pdf")
}

test "media JSON unions respect reversed order" {
    assert.equal(media_json_union_respects_reversed_order(), "image")
}

test "media JSON rejects missing kind" {
    assert.is_true(rejects_media_json<image>(
        "{\"source\":\"base64\",\"value\":\"aGk=\",\"mime\":\"image/png\"}",
    ))
}

test "media JSON rejects mismatched concrete kind" {
    assert.is_true(rejects_media_json<image>(
        "{\"kind\":\"audio\",\"source\":\"base64\",\"value\":\"aGk=\",\"mime\":\"audio/mpeg\"}",
    ))
}

test "media JSON rejects generic kind" {
    assert.is_true(rejects_media_json<image | audio | video | pdf>(
        "{\"kind\":\"media\",\"source\":\"base64\",\"value\":\"aGk=\"}",
    ))
}
