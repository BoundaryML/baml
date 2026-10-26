// `baml.FromLlmOutput`: a type a model writes as another type, its `Shape`.
//
// What a model is shown (the output format, `baml.json.schema`) and how its
// answer is read (`baml.sap.parse`, `baml.sap.coerce`, an LLM function) agree:
// both use the shape. A type's `@@description` is the hint the model sees.
// Structured outputs and tool arguments are covered in their own namespaces
// (`ns_llm_openai_structured`, `ns_llm_tool_arguments`).
//
// `@parse` and `@render_prompt` never contact the client below. Per the corpus
// convention every case body is a top-level `function` and the `test` block
// only asserts.

client LlmOutputTestClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

// ─── the time types ──────────────────────────────────────────────────────────

class LlmOutputEvent {
    title: string,
    /// When it starts.
    starts_at: baml.time.Instant,
    ends_at: baml.time.Instant?,
}

function LlmOutputExtractEvent(text: string) -> LlmOutputEvent {
    client: LlmOutputTestClient
    prompt: `${text}\n${ctx.output_format()}`
}

function llm_output_event_prompt() -> string {
    LlmOutputExtractEvent@render_prompt("x").text()
}

test "the output format asks for an Instant as a string, with its hint" {
    assert.equal(
        llm_output_event_prompt(),
        "[system]\nx\nAnswer in JSON using this schema:\n{\n  title: string,\n  /// When it starts.\n  /// An RFC 3339 timestamp, for example 2026-01-01T00:00:00Z\n  starts_at: string,\n  /// An RFC 3339 timestamp, for example 2026-01-01T00:00:00Z\n  ends_at: string or null,\n}",
    )
}

function LlmOutputExtractInstant(text: string) -> baml.time.Instant {
    client: LlmOutputTestClient
    prompt: `${text}\n${ctx.output_format()}`
}

function LlmOutputExtractDates(text: string) -> baml.time.PlainDate[] {
    client: LlmOutputTestClient
    prompt: `${text}\n${ctx.output_format()}`
}

test "a time type at the root is asked for by its hint" {
    assert.equal(
        LlmOutputExtractInstant@render_prompt("x").text(),
        "[system]\nx\nAn RFC 3339 timestamp, for example 2026-01-01T00:00:00Z",
    );
    assert.equal(
        LlmOutputExtractDates@render_prompt("x").text(),
        "[system]\nx\nAnswer with a JSON Array using this schema:\n/// An ISO 8601 date, for example 1979-05-27\nstring[]",
    )
}

class LlmOutputSchedule {
    days: baml.time.PlainDate[],
}

function LlmOutputExtractSchedule(text: string) -> LlmOutputSchedule {
    client: LlmOutputTestClient
    prompt: `${text}\n${ctx.output_format()}`
}

test "a list of a time type is a list of strings" {
    assert.equal(
        LlmOutputExtractSchedule@render_prompt("x").text(),
        "[system]\nx\nAnswer in JSON using this schema:\n{\n  /// An ISO 8601 date, for example 1979-05-27\n  days: string[],\n}",
    )
}

/// Each way a model may write `written`, read as `T` and written back with
/// `to_string`: bare, quoted, and fenced in markdown after some prose.
function llm_output_reads<T>(written: string) -> string {
    let fenced = "Here it is:\n```json\n\"" + written + "\"\n```";
    [written, `"${written}"`, fenced]
        .map((text: string) -> string {
            string.from(baml.sap.parse<T>(text))
        })
        .join("|")
}

test "sap reads each time type from the string a model writes" {
    assert.equal(
        llm_output_reads<baml.time.Instant>("2026-01-01T00:00:00Z"),
        "2026-01-01T00:00:00Z|2026-01-01T00:00:00Z|2026-01-01T00:00:00Z",
    );
    assert.equal(
        llm_output_reads<baml.time.ZonedDateTime>("2026-03-18T13:04:27-07:00"),
        "2026-03-18T13:04:27-07:00|2026-03-18T13:04:27-07:00|2026-03-18T13:04:27-07:00",
    );
    assert.equal(
        llm_output_reads<baml.time.PlainDateTime>("1979-05-27T07:32:00"),
        "1979-05-27T07:32:00|1979-05-27T07:32:00|1979-05-27T07:32:00",
    );
    assert.equal(
        llm_output_reads<baml.time.PlainDate>("1979-05-27"),
        "1979-05-27|1979-05-27|1979-05-27",
    );
    assert.equal(llm_output_reads<baml.time.PlainTime>("07:32:00"), "07:32:00|07:32:00|07:32:00")
}

class LlmOutputTimes {
    instant: baml.time.Instant,
    zoned: baml.time.ZonedDateTime,
    local: baml.time.PlainDateTime,
    date: baml.time.PlainDate,
    time: baml.time.PlainTime,
}

function LlmOutputExtractTimes(text: string) -> LlmOutputTimes {
    client: LlmOutputTestClient
    prompt: `${text}\n${ctx.output_format()}`
}

// Unquoted keys and values, single quotes, and a trailing comma.
function llm_output_sloppy_times() -> string {
    let times = LlmOutputExtractTimes@parse(
        "Sure!\n```json\n{instant: 2026-01-01T00:00:00Z, 'zoned': '2026-03-18T13:04:27-07:00', local: \"1979-05-27T07:32:00\", date: 1979-05-27, time: '07:32:00',}\n```",
    );
    [
        times.instant.to_string(),
        times.zoned.to_string(),
        times.local.to_string(),
        times.date.to_string(),
        times.time.to_string(),
    ].join("|")
}

test "an LLM function reads every time type from a sloppy answer" {
    assert.equal(
        llm_output_sloppy_times(),
        "2026-01-01T00:00:00Z|2026-03-18T13:04:27-07:00|1979-05-27T07:32:00|1979-05-27|07:32:00",
    )
}

function llm_output_event_schema() -> string {
    let schema = baml.json.schema(LlmOutputEvent.type(), naming = baml.json.Naming.Aliased);
    [
        baml.json.path<string>(schema, ".properties.starts_at.type"),
        baml.json.path<string>(schema, ".properties.starts_at.description"),
        baml.json.stringify(baml.json.path<json>(schema, ".properties.ends_at.type")),
        baml.json.path<string[]>(schema, ".required").join(","),
    ].join("|")
}

test "baml.json.schema describes an Instant field as a string" {
    assert.equal(
        llm_output_event_schema(),
        "string|When it starts.\n\nAn RFC 3339 timestamp, for example 2026-01-01T00:00:00Z|[\"string\",\"null\"]|title,starts_at",
    );
    assert.equal(
        baml.json.stringify(baml.json.schema(reflect.Type.of<baml.time.Instant>())),
        `{"type":"string","description":"An RFC 3339 timestamp, for example 2026-01-01T00:00:00Z"}`,
    )
}

// ─── a user type that is also read from code's JSON ──────────────────────────

/// Developer docs: never shown to a model.
class LlmOutputMoney {
    cents: int,
    currency: string,

    implements baml.FromJson {
        function from_json(j: baml.json.json) -> Self throws baml.json.DecodeError {
            LlmOutputMoney {
                cents: baml.json.to<int>(baml.json.field(j, "amount_cents")),
                currency: baml.json.to<string>(baml.json.field(j, "code")),
            }
        }
    }

    implements baml.FromLlmOutput {
        type Shape = string;

        function from_llm_output(shape: string) -> Self throws baml.json.DecodeError {
            let parts = shape.trim().split(" ");
            if (parts.length() != 2) {
                throw baml.json.DecodeError { message: `expected an amount and a currency, got "${shape}"`, path: "" }
            }
            let cents = (float.parse(parts[0]) * 100.0).iround() catch (_) {
                _ => throw baml.json.DecodeError { message: `"${parts[0]}" is not an amount`, path: "" },
            };
            LlmOutputMoney { cents: cents, currency: parts[1] }
        }
    }

    @@description("An amount and an ISO 4217 currency code, for example 12.50 USD")
}

class LlmOutputOrder {
    item: string,
    total: LlmOutputMoney,
}

function LlmOutputExtractOrder(text: string) -> LlmOutputOrder {
    client: LlmOutputTestClient
    prompt: `${text}\n${ctx.output_format()}`
}

function llm_output_money(money: LlmOutputMoney) -> string {
    `${money.cents} ${money.currency}`
}

function llm_output_money_from_code() -> string {
    llm_output_money(baml.json.decode<LlmOutputMoney>(`{"amount_cents": 1250, "code": "USD"}`))
}

function llm_output_money_from_model() -> string {
    llm_output_money(LlmOutputExtractOrder@parse(`{"item": "Lamp", "total": "12.50 USD"}`).total)
}

test "code's JSON goes through FromJson and a model's answer through FromLlmOutput" {
    assert.equal(llm_output_money_from_code(), "1250 USD");
    assert.equal(llm_output_money_from_model(), "1250 USD");
    assert.equal(llm_output_money(baml.sap.coerce<LlmOutputMoney>("7.25 EUR")), "725 EUR")
}

test "the schema and the output format show the shape and the description only" {
    assert.equal(
        baml.json.stringify(baml.json.schema(LlmOutputMoney.type())),
        `{"type":"string","description":"An amount and an ISO 4217 currency code, for example 12.50 USD"}`,
    );
    assert.equal(
        LlmOutputExtractOrder@render_prompt("x").text(),
        "[system]\nx\nAnswer in JSON using this schema:\n{\n  item: string,\n  /// An amount and an ISO 4217 currency code, for example 12.50 USD\n  total: string,\n}",
    )
}

// ─── when from_llm_output throws ─────────────────────────────────────────────

function llm_output_money_or_text(text: string) -> string {
    match (baml.sap.parse<LlmOutputMoney | string>(text)) {
        let money: LlmOutputMoney => `money: ${llm_output_money(money)}`,
        let other: string => `string: ${other}`,
    }
}

function llm_output_instant_or_text(text: string) -> string {
    match (baml.sap.parse<baml.time.Instant | string>(text)) {
        let instant: baml.time.Instant => `instant: ${instant.to_string()}`,
        let other: string => `string: ${other}`,
    }
}

test "a refused value moves a union on to its next member" {
    assert.equal(llm_output_money_or_text("12.50 USD"), "money: 1250 USD");
    assert.equal(llm_output_money_or_text("twelve bucks"), "string: twelve bucks");
    assert.equal(llm_output_instant_or_text("2026-01-01T00:00:00Z"), "instant: 2026-01-01T00:00:00Z");
    assert.equal(llm_output_instant_or_text("next tuesday"), "string: next tuesday")
}

function llm_output_refused_parse() -> string {
    let money = baml.sap.parse<LlmOutputMoney>("twelve bucks") catch (e) {
        baml.errors.ParseError => return e.message,
    };
    llm_output_money(money)
}

function llm_output_refused_coerce() -> string {
    let instant = baml.sap.coerce<baml.time.Instant>("next tuesday") catch (e) {
        baml.errors.ParseError => return e.message,
    };
    instant.to_string()
}

test "a refused value with no other member is a ParseError carrying the message" {
    assert.contains(llm_output_refused_parse(), "\"twelve\" is not an amount");
    assert.contains(llm_output_refused_coerce(), "Instant.parse: cannot parse \"next tuesday\"")
}

// ─── streaming ───────────────────────────────────────────────────────────────

class LlmOutputMeeting {
    title: string,
    at: baml.time.Instant?,
}

class LlmOutputStrictMeeting {
    title: string,
    at: baml.time.Instant,
}

function LlmOutputExtractMeeting(input: string) -> LlmOutputMeeting {
    client: LlmOutputTestClient
    prompt: `${input}\n${ctx.output_format()}`
}

function LlmOutputExtractStrictMeeting(input: string) -> LlmOutputStrictMeeting {
    client: LlmOutputTestClient
    prompt: `${input}\n${ctx.output_format()}`
}

/// The timestamp arrives split across two chunks.
function llm_output_meeting_chunks() -> string[] {
    ["{\"title\": \"Sync\", \"at\": \"2026-01-", "01T09:30:00Z\"", "}"]
}

// Each partial as `<title>@<at>`, then the final `at`.
function llm_output_stream_meeting() -> string {
    let stream = ai.stream.from_spec<LlmOutputMeeting>(
        LlmOutputExtractMeeting@spec("x"),
        client = root.streaming_partial_parse.ChunkedStreamClient { chunks: llm_output_meeting_chunks() },
    );
    let seen: string[] = [];
    while let partial: LlmOutputMeeting = stream.next() {
        seen.push(`${partial.title}@${partial.at?.to_string() ?? "null"}`);
    }
    `${seen.join(",")}|${stream.final().at?.to_string() ?? "null"}`
}

function llm_output_stream_strict_meeting() -> string {
    let stream = ai.stream.from_spec<LlmOutputStrictMeeting>(
        LlmOutputExtractStrictMeeting@spec("x"),
        client = root.streaming_partial_parse.ChunkedStreamClient { chunks: llm_output_meeting_chunks() },
    );
    let seen: string[] = [];
    while let partial: LlmOutputStrictMeeting = stream.next() {
        seen.push(`${partial.title}@${partial.at.to_string()}`);
    }
    `${seen.join(",")}|${stream.final().at.to_string()}`
}

test "a half-written timestamp streams as null, then as the full value" {
    assert.equal(
        llm_output_stream_meeting(),
        "Sync@null,Sync@2026-01-01T09:30:00Z,Sync@2026-01-01T09:30:00Z|2026-01-01T09:30:00Z",
    )
}

test "a half-written timestamp in a required field holds back its class" {
    assert.equal(
        llm_output_stream_strict_meeting(),
        "Sync@2026-01-01T09:30:00Z,Sync@2026-01-01T09:30:00Z|2026-01-01T09:30:00Z",
    )
}

// ─── a shape that holds its own type ─────────────────────────────────────────

class LlmOutputTreeShape {
    label: string,
    children: LlmOutputTree[],
}

class LlmOutputTree {
    label: string,
    kids: LlmOutputTree[],

    implements baml.FromLlmOutput {
        type Shape = LlmOutputTreeShape;

        function from_llm_output(shape: LlmOutputTreeShape) -> Self throws never {
            LlmOutputTree { label: shape.label.to_upper_case(), kids: shape.children }
        }
    }

    @@description("A tree of labels")
}

function LlmOutputExtractTree(text: string) -> LlmOutputTree {
    client: LlmOutputTestClient
    prompt: `${text}\n${ctx.output_format()}`
}

/// Each label of `tree`, depth first, as `from_llm_output` wrote it.
function llm_output_tree_labels(tree: LlmOutputTree) -> string {
    [tree.label].concat(tree.kids.map((kid: LlmOutputTree) -> string { llm_output_tree_labels(kid) })).join(",")
}

function llm_output_parse_tree() -> string {
    llm_output_tree_labels(
        LlmOutputExtractTree@parse(
            `{"label": "root", "children": [{"label": "a", "children": []}, {"label": "b", "children": [{"label": "c", "children": []}]}]}`,
        ),
    )
}

/// Where the schema of a tree's children points: back to the shape's definition.
function llm_output_tree_schema_ref() -> string {
    let schema = baml.json.schema(LlmOutputTree.type());
    baml.json.path<string>(
        schema,
        `.$defs["from_llm_output.LlmOutputTreeShape"].properties.children.items.$ref`,
    )
}

test "a shape that holds its own type through a class is built inside out" {
    assert.equal(llm_output_parse_tree(), "ROOT,A,B,C");
    assert.equal(
        LlmOutputExtractTree@render_prompt("x").text(),
        "[system]\nx\nfrom_llm_output.LlmOutputTreeShape {\n  label: string,\n  /// A tree of labels\n  children: from_llm_output.LlmOutputTreeShape[],\n}\n\nAnswer in JSON using this schema:\n/// A tree of labels\nfrom_llm_output.LlmOutputTreeShape",
    );
    assert.equal(llm_output_tree_schema_ref(), "#/$defs/from_llm_output.LlmOutputTreeShape")
}

// ─── an enum ─────────────────────────────────────────────────────────────────

enum LlmOutputPriority {
    Low
    High
}

implement baml.FromLlmOutput for LlmOutputPriority {
    type Shape = int;

    function from_llm_output(shape: int) -> Self throws never {
        if (shape >= 5) {
            LlmOutputPriority.High
        } else {
            LlmOutputPriority.Low
        }
    }
}

class LlmOutputTicket {
    title: string,
    priority: LlmOutputPriority,
}

function LlmOutputExtractTicket(text: string) -> LlmOutputTicket {
    client: LlmOutputTestClient
    prompt: `${text}\n${ctx.output_format()}`
}

function llm_output_ticket() -> string {
    let ticket = LlmOutputExtractTicket@parse(`{"title": "Bug", "priority": 7}`);
    let schema = baml.json.schema(LlmOutputTicket.type());
    `${ticket.priority}|${baml.json.path<string>(schema, ".properties.priority.type")}`
}

test "an enum can be written as its shape too" {
    assert.equal(llm_output_ticket(), "High|integer");
    assert.contains(LlmOutputExtractTicket@render_prompt("x").text(), "  priority: int,")
}
