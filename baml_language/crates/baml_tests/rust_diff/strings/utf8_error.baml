function decode(value: uint8array) -> string { string.from_utf8(value) }
function main() -> int { decode(b"ok\xF0\x9F"); 0 }
