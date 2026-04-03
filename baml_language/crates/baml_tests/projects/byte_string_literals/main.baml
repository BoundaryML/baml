// Byte string literal tests

function ByteStringBasic() -> int {
    let data = b"hello";
    data.length()
}

function ByteStringEmpty() -> int {
    let data = b"";
    data.length()
}

function ByteStringHexEscapes() -> int {
    let data = b"\x48\x65\x6c\x6c\x6f";
    data.length()
}

function ByteStringStandardEscapes() -> int {
    let data = b"\n\t\r\0\\\"";
    data.length()
}

function ByteStringIndexAccess() -> int? {
    let data = b"\x41\x42\x43";
    data.at(0)
}
