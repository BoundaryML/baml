// String escapes, byte strings, and raw strings.

function JsonEscapes() -> string {
    "\"\\\/\b\f\n\r\t\u0041 it\'s \`ok\` \$5 \0"
}

function CodePointEscapes() -> string {
    "\x41\x7F \u00E9\u4E2D \u{1F600}\u{1F_600} \uD83D\uDE00 \uD83D\u{DE00}"
}

function InvalidEscapes() -> string {
    "\d+ \v \a \e \101 \xE9 \x4 \u12 \U0001F600 \N{GRINNING FACE}"
}

function BacktickEscapes(name: string) -> string {
    `Hi ${name}\n\u{1F600} \${literal} \x41 \d`
}

function ByteEscapes() -> uint8array {
    b"\xFF\x00\'\b\f\/\`\$\\\" \u{E9} \d é"
}

function RawStrings() -> string {
    let pattern = r"\d+\.\w";
    let quoted = r"say \"hi\"";
    let r = "not a prefix";
    pattern + quoted + r
}
