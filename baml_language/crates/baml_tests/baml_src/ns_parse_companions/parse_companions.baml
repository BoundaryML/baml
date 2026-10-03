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

// ─── enum variants match by `@description` ──────────────────────────────────
//
// The output format renders a variant as `<name>: <description>`, so a model
// often answers with the description. That names the variant, in a scalar
// field and in every element of a list alike.

enum ParseCompanionsDay {
    ONE @description("MONDAY"),
    TWO @description("TUESDAY"),
}

class ParseCompanionsDayEntry {
    day: ParseCompanionsDay,
    note: string,
}

test "sap_parse_matches_an_enum_variant_by_its_description" {
    let parsed = baml.sap.parse<ParseCompanionsDayEntry>(`{"day": "MONDAY", "note": "rest"}`);
    assert.equal(parsed, ParseCompanionsDayEntry { day: ParseCompanionsDay.ONE, note: "rest" })
}

test "sap_parse_matches_an_enum_description_case_insensitively" {
    assert.equal(baml.sap.parse<ParseCompanionsDay>(`Tuesday`), ParseCompanionsDay.TWO)
}

test "sap_parse_matches_a_rendered_enum_line" {
    assert.equal(baml.sap.parse<ParseCompanionsDay>(`ONE: MONDAY`), ParseCompanionsDay.ONE)
}

test "sap_parse_keeps_list_elements_whose_enum_is_named_by_description" {
    let parsed = baml.sap.parse<ParseCompanionsDayEntry[]>(
        `[{"day": "MONDAY", "note": "rest"}, {"day": "TWO", "note": "run"}]`,
    );
    assert.equal(
        parsed,
        [
            ParseCompanionsDayEntry { day: ParseCompanionsDay.ONE, note: "rest" },
            ParseCompanionsDayEntry { day: ParseCompanionsDay.TWO, note: "run" },
        ],
    )
}

// The description the prompt shows is the `@description` followed by the
// `///` docstring, and that rendered text is what the parser matches.

enum ParseCompanionsMeal {
    /// eaten in the morning
    A,
    /// eaten at noon
    B @description("LUNCH"),
}

function ParseCompanionsParseMeal() -> ParseCompanionsMeal {
    client: ParseCompanionsTestClient
    prompt: `${ctx.output_format()}`
}

test "sap_parse_matches_an_enum_variant_by_its_docstring" {
    assert.equal(baml.sap.parse<ParseCompanionsMeal>(`eaten in the morning`), ParseCompanionsMeal.A)
}

test "sap_parse_matches_the_description_and_docstring_as_rendered" {
    assert.equal(baml.sap.parse<ParseCompanionsMeal>(`LUNCH eaten at noon`), ParseCompanionsMeal.B);
    assert.equal(
        baml.sap.parse<ParseCompanionsMeal>(`B: LUNCH eaten at noon`),
        ParseCompanionsMeal.B,
    )
}

test "sap_parse_matches_the_enum_lines_the_prompt_renders" {
    let prompt = ParseCompanionsParseMeal@render_prompt().text();
    assert.is_true(prompt.includes("- A: eaten in the morning"));
    assert.is_true(prompt.includes("- B: LUNCH eaten at noon"))
}

// ─── @skip fields ────────────────────────────────────────────────────────────
//
// The parser never reads a `@skip` field (the prompt does not show it), so a
// parsed instance holds its type's empty value: "" for a string, the zero of a
// number, false, an empty list, null for an optional field, an enum's first
// variant, and a class of its fields' empty values.

enum ParseCompanionsSkipColor {
    Red,
    Green,
}

class ParseCompanionsSkipInner {
    label: string,
    count: int,
}

class ParseCompanionsSkipPerson {
    name: string,
    age: int @skip,
    score: float @skip,
    active: bool @skip,
    tags: string[] @skip,
    nick: string? @skip,
    color: ParseCompanionsSkipColor @skip,
    inner: ParseCompanionsSkipInner @skip,
}

function parse_companions_skipped_fields_fn() -> bool {
    let parsed = baml.sap.parse<ParseCompanionsSkipPerson>("{\"name\": \"Ada\", \"age\": 37}");
    parsed == ParseCompanionsSkipPerson {
        name: "Ada",
        age: 0,
        score: 0.0,
        active: false,
        tags: [],
        nick: null,
        color: ParseCompanionsSkipColor.Red,
        inner: ParseCompanionsSkipInner { label: "", count: 0 },
    }
}

test "sap_parse_fills_skipped_fields_with_empty_values" {
    assert.is_true(parse_companions_skipped_fields_fn())
}
