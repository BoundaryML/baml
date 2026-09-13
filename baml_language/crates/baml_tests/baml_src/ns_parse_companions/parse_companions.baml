// Tests for parse companion functions (@parse).
//
// `@parse` is pure parsing, NOT a network call: the LLM client below points at
// a dead localhost port and is never contacted. Each test parses `"{}"`
// against a class whose only field is optional/nullable and asserts the field
// materializes as `null` (missing optional field is allowed). Since BAML has
// no host-side instance shape introspection, assertions compare with `==`
// against literal instances. Locals go through top-level helper functions
// (test-block local-boxing VM bug).

client ParseCompanionsTestClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

// ─── missing optional class field (`string?`) ────────────────────────────────

class ParseCompanionsOptPayload {
    text: string?,
}

function ParseCompanionsParseOptPayload() -> ParseCompanionsOptPayload {
    client: ParseCompanionsTestClient
    prompt: `${ctx.output_format()}`
}

function parse_companions_missing_optional_fn() -> bool {
    let parsed = ParseCompanionsParseOptPayload@parse("{}");
    parsed == ParseCompanionsOptPayload { text: null }
}

test "parse_companion_allows_missing_optional_class_field" {
    assert.is_true(parse_companions_missing_optional_fn())
}

// ─── missing nullable alias field (`string | null` alias) ────────────────────
//
// `type MaybeText = string | null` is identical to `string?` — a nullable
// union is optionality, so a missing/null value is bare `null` (not wrapped in
// union metadata), matching the `string?` case above.

type ParseCompanionsMaybeText = string | null;

class ParseCompanionsAliasPayload {
    text: ParseCompanionsMaybeText,
}

function ParseCompanionsParseAliasPayload() -> ParseCompanionsAliasPayload {
    client: ParseCompanionsTestClient
    prompt: `${ctx.output_format()}`
}

function parse_companions_missing_alias_fn() -> bool {
    let parsed = ParseCompanionsParseAliasPayload@parse("{}");
    parsed == ParseCompanionsAliasPayload { text: null }
}

test "parse_companion_allows_missing_nullable_alias_field" {
    assert.is_true(parse_companions_missing_alias_fn())
}

// ─── top-level string and catchable shape errors ────────────────────────────

function ParseCompanionsParseString() -> string {
    client: ParseCompanionsTestClient
    prompt: `${ctx.output_format()}`
}

enum ParseCompanionsCurrency {
    USD,
    EUR,
}

class ParseCompanionsInvoice {
    currency: ParseCompanionsCurrency,
    amount: float,
}

function ParseCompanionsParseInvoice() -> ParseCompanionsInvoice {
    client: ParseCompanionsTestClient
    prompt: `${ctx.output_format()}`
}

test "sap_parse_decodes_a_complete_top_level_json_string" {
    assert.equal(ParseCompanionsParseString@parse(`"Fred"`), "Fred")
}

test "sap_parse_preserves_plain_llm_text" {
    assert.equal(ParseCompanionsParseString@parse("Fred says hello"), "Fred says hello")
}

test "sap_parse_shape_failures_are_catchable_as_parse_errors" {
    let result = ParseCompanionsParseInvoice@parse(`{"currency":"GBP","amount":10}`).currency.to_string() catch (_) {
        baml.errors.ParseError => "parse-error",
        _ => "wrong-error",
    };
    assert.equal(result, "parse-error")
}
