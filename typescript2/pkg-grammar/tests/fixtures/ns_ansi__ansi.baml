// Namespace: root.ansi — ANSI terminal styling helpers.
//
// BAML string literals don't have an \x1b / \e escape, so we build the ESC
// byte (codepoint 27) with string.from_code_points, then wrap text in SGR
// codes. Honors the NO_COLOR convention (https://no-color.org): if the
// NO_COLOR env var is set to anything, styling is a no-op.
//
// Used from the root namespace as root.ansi.red("..."), root.ansi.bold(...), etc.

function esc() -> string {
    string.from_code_points([27])
}

// Wrap `text` in an SGR parameter (e.g. "31" for red) and reset afterwards.
function sgr(code: string, text: string) -> string {
    match (baml.env.get("NO_COLOR")) {
        null => esc() + "[" + code + "m" + text + esc() + "[0m",
        _ => text,
    }
}

// Attributes
function bold(text: string) -> string {
    sgr("1", text)
}

function dim(text: string) -> string {
    sgr("2", text)
}

function italic(text: string) -> string {
    sgr("3", text)
}

function underline(text: string) -> string {
    sgr("4", text)
}

// Foreground colors
function red(text: string) -> string {
    sgr("31", text)
}

function green(text: string) -> string {
    sgr("32", text)
}

function yellow(text: string) -> string {
    sgr("33", text)
}

function blue(text: string) -> string {
    sgr("34", text)
}

function magenta(text: string) -> string {
    sgr("35", text)
}

function cyan(text: string) -> string {
    sgr("36", text)
}

function gray(text: string) -> string {
    sgr("90", text)
}
