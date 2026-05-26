// BEP-049 — backtick string literal compile fixture.
//
// M1: contents are plain text (no ${...} interpolation yet — that lands in M2).
// All functions below should compile, type-check, and produce the same string
// value as their `"..."` equivalents.

function BacktickOneLiner() -> string {
    `hello world`
}

// An *empty* backtick string is intentionally NOT exercised here: per BEP-049
// §8 case 1, `` `` `` parses as a greedy 2-tick opener with no close (parse
// error). For empty strings, users write `""`.

function BacktickStandardEscapes() -> string {
    `line\nbreak\twith\rcr\0nul`
}

function BacktickEscapedBacktick() -> string {
    `before \` after`
}

function BacktickEscapedDollar() -> string {
    `literal \${name}`
}

function BacktickMultiline() -> string {
    `
        first line
        second line
        third line
    `
}

function BacktickMultilineWithBlankLines() -> string {
    `
        para one

        para two
    `
}

function BacktickMultiTickTwo() -> string {
    ``inline `code` here``
}

function BacktickMultiTickThree() -> string {
    ```nested ``double`` ticks```
}

// Backtick string in an expression position.
function BacktickInCondition() -> int {
    if (`abc`.length() > 0) {
        1
    } else {
        0
    }
}

function BacktickEquality() -> bool {
    `abc` == `abc`
}

// Backtick string as function argument and return value.
function BacktickPassThrough(s: string) -> string {
    s
}

function BacktickLiteralAsArg() -> string {
    BacktickPassThrough(`forwarded`)
}
