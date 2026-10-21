// `baml fmt` re-indents (de-dents) multi-line backtick string interiors to one
// level past the surrounding block, preserving the auto-dedented (BEP-049 §12)
// runtime value. Every body below is written mis-indented on purpose; the
// formatter snapshot shows the normalized result.

// Interior pushed far past the block: pulled back to one level.
function FarIndented() -> string {
    `
                    deeply
                    pushed in
    `
}

// Ragged interior: only the common leading prefix is stripped, so the relative
// indentation between lines survives.
function Ragged() -> string {
    `
            header
                bullet one
                bullet two
            footer
    `
}

// Blank lines inside the body are preserved.
function WithBlankLines() -> string {
    `
              para one

              para two
    `
}

// Single-line ${...} interpolations move with the surrounding text.
function WithInterps(name: string, count: int) -> string {
    `
                name:  ${name}
                count: ${count}
    `
}

// First line flush with the opening tick, second line deeper: the deeper indent
// is part of the value, so it is preserved.
function FirstLineFlush() -> string {
    `first
          nested under first`
}

// A `${for}` block tag triggers §13 whitespace control, so the interior is left
// verbatim instead of re-indented.
function ForLoopVerbatim(xs: string[]) -> string {
    `${for (let x in xs)}- ${x}
${endfor}`
}

// A multi-line ${...} interpolation is left verbatim (its inner lines must not
// move), so this over-indented body is NOT pulled back like the others above.
function MultilineInterpVerbatim() -> string {
    `
                value: ${
                    "x"
                }
    `
}
