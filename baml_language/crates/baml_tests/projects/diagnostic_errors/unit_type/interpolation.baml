// BEP-079: a `${…}` that produces no value is held to the unit type.
//
// The syntax decides how an interpolation is read (BEP-049 §4): a block with
// no final expression, or one ending in an `if` with no `else`, produces no
// value. It contributes nothing: no text, and no value for a tag. Its `if` is
// an implicit unit like any other, so a branch that produces a value is an
// error rather than text that silently goes missing.

//baml:tagged_string
function shout(body: () -> baml.TaggedString) -> string { "!" }

// `side` is declared in `implicit_unit.baml`.

function untagged_if_with_value(verbose: bool) -> string {
  `${ if (verbose) { "Explain in detail. " } }Answer.`
}

function untagged_if_let_with_value(note: string?) -> string {
  `${ if let n: string = note { n } }Answer.`
}

function untagged_nested_if_with_value(verbose: bool) -> string {
  `${ { if (verbose) { "Explain in detail. " } } }Answer.`
}

function untagged_else_if_with_value(a: bool, b: bool) -> string {
  `${ if (a) { "one" } else if (b) { "two" } }Answer.`
}

function tagged_if_with_value(verbose: bool) -> string {
  shout`${ if (verbose) { "Explain in detail. " } }Answer.`
}

// A call's `null` result is a value, and one that does not render: the
// statement form is what runs a call for its effect.
function nothing() -> void {}

function untagged_unit_call() -> string {
  `${ nothing() }Answer.`
}

// A tag takes its values as they are, but not one that can only be `null`.
function tagged_unit_call() -> string {
  shout`${ nothing() }Answer.`
}

function PromptUnitCall(question: string) -> string {
  client: "openai/gpt-4o"
  prompt: `${ nothing() }Answer: ${question}`
}

// A value that may be `null` is the tag's to handle.
function tagged_nullable(note: string?) -> string {
  shout`${ note }Answer.`
}

// A `prompt:` is a tagged template too.
function PromptIfWithValue(verbose: bool, question: string) -> string {
  client: "openai/gpt-4o"
  prompt: `${ if (verbose) { "Explain in detail. " } }Answer: ${question}`
}

// --- still fine: the interpolation runs and renders nothing ---

function PromptUnitInterpolation(question: string) -> string {
  client: "openai/gpt-4o"
  prompt: `${ side(); }Answer: ${question}`
}

function unit_interpolations(verbose: bool) -> string {
  let a = `${ side(); }${ if (verbose) { side(); } }Answer.`;
  let b = shout`${ side(); }${ if (verbose) { side(); } }Answer.`;
  a + b
}
