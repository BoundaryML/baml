function capture_bytes(value: uint8array) -> uint8array { value }
function decode_bytes(value: uint8array) -> string { string.from_utf8(value) }
function main() -> int {
    let text = "";
    for (let i = 0; i < 64; i += 1) { text = text + "é🦀"; };
    let view = text.slice(1, 9);
    let bytes = capture_bytes(view.to_utf8());
    if (bytes.length() != view.byte_length()) { return -1; };
    if (decode_bytes(bytes) != view) { return -2; };
    if (decode_bytes(capture_bytes("".to_utf8())) != "") { return -3; };
    if (string.from_utf8(b"hi\x00") != "hi\u{0}") { return -4; };
    if (b"a\xFFz".to_string() != "a�z") { return -5; };
    let failure = decode_bytes(b"\xFF") catch (e) { baml.errors.InvalidArgument => e.message };
    if (!failure.starts_with("string.from_utf8: invalid UTF-8 at byte 0:")) { return -6; };
    return 6;
}
