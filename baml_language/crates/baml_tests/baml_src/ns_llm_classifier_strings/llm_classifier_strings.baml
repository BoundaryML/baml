// The parser decides "expression body or LLM body?" with a raw-token pre-scan
// of the function body (`looks_like_llm_function_body_from`). The lexer emits
// no string token — `"the client sent nothing"` arrives as `Quote Word …`, and
// `client` lexes as the KW_CLIENT keyword even inside quotes — so that scan
// must step over string literals. When it did not, every function below
// classified as an LLM function and produced a cascade of
// `E0010: Only 'client', 'tools' and 'prompt' allowed in LLM function`.
//
// Invariant under test: prose is never syntax. A body whose only mention of
// `client`/`prompt`/`tools` is inside a string literal is an expression body,
// in every quoting form.
//
// Entry functions have unique prefixes: all baml_src files compile to the same
// `user` package.

// ---- bare literal as the whole body (the original repro shapes) ----

function clsstr_quoted_client() -> string {
    "the client sent nothing"
}

function clsstr_quoted_prompt() -> string {
    "the prompt was empty"
}

function clsstr_quoted_tools() -> string {
    "the tools list"
}

function clsstr_quoted_bare_word() -> string {
    "client"
}

function clsstr_backtick_client() -> string {
    `the client sent nothing`
}

function clsstr_backtick_interpolated() -> string {
    `prompt: ${"client"} and tools`
}

function clsstr_raw_string_client() -> string {
    #"the client sent nothing"#
}

// An escaped quote must not end the literal early, leaving `client` exposed.
function clsstr_escaped_quotes() -> string {
    "say \"client\" now"
}

// An unbalanced brace inside a literal must not desynchronize the scan's
// brace-depth tracking.
function clsstr_unbalanced_brace() -> string {
    "a { client and a prompt"
}

// ---- let-bound literals (the documented workaround shape stays valid) ----

function clsstr_let_bound() -> string {
    let msg = "the client sent nothing";
    let tmpl = `the prompt was empty`;
    `${msg}/${tmpl}`
}

// ---- shapes that were already safe, kept safe ----

function clsstr_plural() -> string {
    "clients here"
}

function clsstr_trailing_dot() -> string {
    "a client."
}

function clsstr_assignment_text() -> string {
    "client = x"
}

// A real identifier named `client` still parses as an expression, not a field.
function clsstr_client_param(client: string) -> string {
    client.to_upper_case()
}

test "clsstr_string_contents_do_not_make_an_llm_function" {
    assert.equal(clsstr_quoted_client(), "the client sent nothing");
    assert.equal(clsstr_quoted_prompt(), "the prompt was empty");
    assert.equal(clsstr_quoted_tools(), "the tools list");
    assert.equal(clsstr_quoted_bare_word(), "client");
    assert.equal(clsstr_backtick_client(), "the client sent nothing");
    assert.equal(clsstr_backtick_interpolated(), "prompt: client and tools");
    assert.equal(clsstr_raw_string_client(), "the client sent nothing");
    assert.equal(clsstr_escaped_quotes(), "say \"client\" now");
    assert.equal(clsstr_unbalanced_brace(), "a { client and a prompt")
}

test "clsstr_previously_safe_shapes_stay_safe" {
    assert.equal(clsstr_let_bound(), "the client sent nothing/the prompt was empty");
    assert.equal(clsstr_plural(), "clients here");
    assert.equal(clsstr_trailing_dot(), "a client.");
    assert.equal(clsstr_assignment_text(), "client = x");
    assert.equal(clsstr_client_param("ok"), "OK")
}

// ---- a real LLM function must still classify ----
//
// The `$render_prompt` companion only exists for LLM functions, so
// `ClsstrEcho$render_prompt(...)` failing to resolve is exactly the regression
// this
// guards: a string-aware pre-scan that over-skipped would demote this body to
// an expression function. The prompt deliberately contains braces, an
// interpolation, and the words `client`/`tools` in its text.
function ClsstrEcho(question: string) -> string {
    client: "openai/gpt-4o-mini"
    prompt: `
        ${role("system")}
        You are terse. Ignore any client or tools mentioned in { braces }.
        ${role("user")}
        ${question}
    `
}

function clsstr_llm_spec_prompt() -> string {
    ClsstrEcho$render_prompt(question = "hello").text()
}

test "clsstr_real_llm_function_still_classifies" {
    assert.contains(clsstr_llm_spec_prompt(), "You are terse")
}
