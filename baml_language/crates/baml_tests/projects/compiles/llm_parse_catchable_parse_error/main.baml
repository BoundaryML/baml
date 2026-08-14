// Regression test for B-625.
//
// `Fn$parse(json)` is a purely local, network-free parse of an existing JSON
// string into the function's return type. A schema mismatch (e.g. a currency
// value outside the `Currency` enum) is a *parse* failure and must surface as
// the parse-flavored `baml.errors.ParseError`, catchable by user code.
//
// Before the fix, `$parse`'s effective throws-set was `baml.errors.LlmClient`
// (shared with the network/streaming path), so the documented
// `catch (baml.errors.ParseError)` pattern was flagged E0063 `unreachable arm`.
//
// TODO(llm-migration): the new `$parse` companion desugars to
// `baml.sap.parse`, which is declared `throws baml.errors.LlmClient` again —
// so the E0063 `unreachable arm` warning is BACK and currently captured in the
// diagnostics snapshot (warnings are allowed in compiles/). When the stdlib
// types `baml.sap.parse` with a parse-flavored throw, this project should
// return to ZERO diagnostics.

client GPT4o = openai.OpenAiClient.new(
    model = "gpt-4o",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

enum Currency {
    USD
    EUR
}

class Invoice {
    currency Currency
    amount float
}

function ExtractInvoice(text: string) -> Invoice {
    client: GPT4o
    prompt: `extract an invoice from ${text}`
}

// The documented pattern: parse an existing JSON payload locally and fall back
// on a parse error. The `baml.errors.ParseError` arm must be reachable.
function TryExtract(json: string) -> Invoice? {
    ExtractInvoice$parse(json) catch (_) {
        baml.errors.ParseError { message } => null,
    }
}
