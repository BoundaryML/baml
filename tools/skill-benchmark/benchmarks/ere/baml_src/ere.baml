class Capture {
    text: string,
    start: int,
    end: int,
}

class EreMatch {
    /// Capture 0 is the complete match. Later entries follow opening-parenthesis order.
    captures: Capture?[],
}

/// Search `input` using the ERE-ASCII pattern defined in `SPEC.md`.
function exec_ere(pattern: string, input: string) -> EreMatch? throws baml.errors.ParseError {
    throw baml.errors.ParseError { message: "not implemented" }
}
