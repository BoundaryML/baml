function is_type_runtime_value() -> bool {
    let value: type | int = reflect.type_of<int>();
    value is type
}

function is_type_rejects_non_type() -> bool {
    let value: type | int = 7;
    value is type
}

function if_let_binds_type_runtime_value() -> string {
    let value: type | int = reflect.type_of<string>();
    if let runtime_type: type = value {
        runtime_type.to_string()
    } else {
        "not type"
    }
}

function is_distinguishes_media_runtime_values() -> bool {
    let image_value: image | audio | video | pdf = image.from_base64("aGk=", "image/png");
    let audio_value: image | audio | video | pdf = audio.from_base64("aGk=", "audio/mpeg");
    let video_value: image | audio | video | pdf = video.from_base64("aGk=", "video/mp4");
    let pdf_value: image | audio | video | pdf = pdf.from_base64("aGk=", "application/pdf");
    (image_value is image)
        && !(image_value is audio)
        && (audio_value is audio)
        && !(audio_value is video)
        && (video_value is video)
        && !(video_value is pdf)
        && (pdf_value is pdf)
        && !(pdf_value is image)
}

function classify_type_runtime_value(value: type | int) -> string {
    match (value) {
        let runtime_type: type => runtime_type.to_string(),
        int => "int",
    }
}

function classify_media_runtime_value(value: image | audio | video | pdf) -> string {
    match (value) {
        image => "image",
        audio => "audio",
        video => "video",
        pdf => "pdf",
    }
}

function match_recognizes_type_runtime_values() -> bool {
    classify_type_runtime_value(reflect.type_of<string>()) == "string"
        && classify_type_runtime_value(7) == "int"
}

function match_distinguishes_media_runtime_values() -> bool {
    classify_media_runtime_value(image.from_base64("aGk=", "image/png")) == "image"
        && classify_media_runtime_value(audio.from_base64("aGk=", "audio/mpeg")) == "audio"
        && classify_media_runtime_value(video.from_base64("aGk=", "video/mp4")) == "video"
        && classify_media_runtime_value(pdf.from_base64("aGk=", "application/pdf")) == "pdf"
}

test "is recognizes type runtime values" {
    assert.is_true(is_type_runtime_value())
}

test "is rejects non-type runtime values" {
    assert.is_true(!is_type_rejects_non_type())
}

test "if let binds type runtime values" {
    assert.equal(if_let_binds_type_runtime_value(), "string")
}

test "is distinguishes media runtime values" {
    assert.is_true(is_distinguishes_media_runtime_values())
}

test "match recognizes type runtime values" {
    assert.is_true(match_recognizes_type_runtime_values())
}

test "match distinguishes media runtime values" {
    assert.is_true(match_distinguishes_media_runtime_values())
}
