// The user-facing JEV cases from the TypeSafe adapter's design notes, one
// section per case. Each case pins the exact request `build_request()`
// produces for a BAML snippet a user could write, and the BAML value the
// adapter decodes from a canned JEV response served by ../ns_llm_mock.
//
// The sibling ../ns_llm_typesafeai namespace covers the adapter's internals
// (wire capture, HTTP error taxonomy, malformed bodies, skipped fields). This
// namespace is the user's view: declared types in, request shape out, typed
// value back.
//
// Corpus rules (see ../ns_llm_mock/README.md): every case body is a top-level
// `function` and the `test` block only asserts.

// ─── shared fixtures ─────────────────────────────────────────────────────────

enum Team {
    Billing @description("Payments, charges, refunds, or invoices"),
    Technical @description("Bugs, outages, or integrations"),
    Account @description("Login, identity, or account access"),

    @@description("Which team should handle this ticket?")
}

enum Priority {
    P0 @alias("critical") @description("Production is down or data is at risk"),
    P1 @alias("high"),
    P2 @alias("normal") @description("Everything else"),

    @@description("How should this ticket be prioritized?")
}

type Mood = "calm" | "concerned" | "angry";

class TicketAssessment {
    urgent: bool @description("Does this ticket require immediate attention?"),
    route: Team,
    reply_within_hour: float @description("How likely is it that the customer expects a reply within the hour?"),
    mood: Mood @description("What is the customer's emotional state?"),
}

class Escalation {
    owner: Team @description("Which team should own the escalation, not the first response?"),
}

class Routed {
    route: Team @alias("department"),
    urgent: bool @description("Does this ticket require immediate attention?"),
}

class Sentiment {
    angry: bool @description("Is the customer angry?"),
    churn_risk: float @description("How likely is the customer to cancel?"),
}

class TicketReport {
    route: Team,
    customer: Sentiment,
}

class Party {
    sentiment: Sentiment,
}

class Thread {
    customer: Party,
    agent: Party,
}

class ThreadAliased {
    customer: Sentiment @alias("the customer"),
    agent: Sentiment @alias("the support agent"),
}

class Judged<T> {
    value: T,
    confident: bool @description("Is the judgment about this ticket clear-cut?"),
}

class AssessmentWithConfidence {
    urgent: bool @description("Does this ticket require immediate attention?"),
    route: Team,
    reply_within_hour: float @description("How likely is it that the customer expects a reply within the hour?"),
    mood: Mood @description("What is the customer's emotional state?"),
    route_confidence: float @description("How confident are you in the route?"),
}

class WithNotes {
    route: Team,
    notes: string,
}

class Node {
    next: Node?,
}

type Tree = Tree[] | Team;

class Tagged {
    kind: "ticket",
    route: Team,
}

enum Only {
    One,
}

class Empty {
}

enum Partial {
    Visible @description("Shown"),
    Also,
    Hidden @skip,

    @@description("Pick one")
}

// ─── shared input ────────────────────────────────────────────────────────────

function tier() -> string {
    "enterprise"
}

function message() -> string {
    "My production integration has been down for three hours and nobody has replied. This is unacceptable."
}

function state() -> string {
    "Customer tier: enterprise\nTicket message: My production integration has been down for three hours and nobody has replied. This is unacceptable."
}

// ─── the user's functions, verbatim from the design notes ────────────────────

function IsUrgent(tier: string, message: string) -> bool {
    client: "typesafeai/jev-latest"
    prompt: `
        ${role("instructions")}
        Does this ticket require immediate attention?
        ${role("user")}
        Customer tier: ${tier}
        Ticket message: ${message}
    `
}

function UrgencyProbability(tier: string, message: string) -> float {
    client: "typesafeai/jev-latest"
    prompt: `
        ${role("instructions")}
        Does this ticket require immediate attention?
        ${role("user")}
        Customer tier: ${tier}
        Ticket message: ${message}
    `
}

function IsUrgentNoInstructions(tier: string, message: string) -> bool {
    client: "typesafeai/jev-latest"
    prompt: `
        Customer tier: ${tier}
        Ticket message: ${message}
    `
}

function RouteTicket(tier: string, message: string) -> Team {
    client: "typesafeai/jev-latest"
    prompt: `
        Customer tier: ${tier}
        Ticket message: ${message}
    `
}

function Prioritize(tier: string, message: string) -> Priority {
    client: "typesafeai/jev-latest"
    prompt: `
        Customer tier: ${tier}
        Ticket message: ${message}
    `
}

function RouteTicketWithContext(tier: string, message: string) -> Team {
    client: "typesafeai/jev-latest"
    prompt: `
        ${role("instructions")}
        You are triaging support tickets for an API company.
        ${role("user")}
        Customer tier: ${tier}
        Ticket message: ${message}
    `
}

function ClassifyMood(tier: string, message: string) -> Mood {
    client: "typesafeai/jev-latest"
    prompt: `
        ${role("instructions")}
        What is the customer's emotional state?
        ${role("user")}
        Customer tier: ${tier}
        Ticket message: ${message}
    `
}

function StarRating(review: string) -> 1 | 2 | 3 | 4 | 5 {
    client: "typesafeai/jev-latest"
    prompt: `
        ${role("instructions")}
        How many stars does this review give the product?
        ${role("user")}
        ${review}
    `
}

function Mixed(message: string) -> true | "unsure" | 9007199254740993 {
    client: "typesafeai/jev-latest"
    prompt: `
        ${role("instructions")}
        Pick the option that best describes the message.
        ${role("user")}
        ${message}
    `
}

function RouteOrLeave(tier: string, message: string) -> Team? {
    client: "typesafeai/jev-latest"
    prompt: `
        ${role("instructions")}
        Which team should handle this ticket, if any?
        ${role("user")}
        Customer tier: ${tier}
        Ticket message: ${message}
    `
}

function RouteOrLeaveNoInstructions(tier: string, message: string) -> Team? {
    client: "typesafeai/jev-latest"
    prompt: `
        Customer tier: ${tier}
        Ticket message: ${message}
    `
}

function RouteOrHold(tier: string, message: string) -> Team | "hold" {
    client: "typesafeai/jev-latest"
    prompt: `
        ${role("instructions")}
        Which team should handle this ticket, or should it be held?
        ${role("user")}
        Customer tier: ${tier}
        Ticket message: ${message}
    `
}

function Collide(message: string) -> Priority | "high" {
    client: "typesafeai/jev-latest"
    prompt: `
        ${role("instructions")}
        How urgent is this?
        ${role("user")}
        ${message}
    `
}

function CollideReversed(message: string) -> "high" | Priority {
    client: "typesafeai/jev-latest"
    prompt: `
        ${role("instructions")}
        How urgent is this?
        ${role("user")}
        ${message}
    `
}

function AssessTicket(tier: string, message: string) -> TicketAssessment {
    client: "typesafeai/jev-latest"
    prompt: `
        Customer tier: ${tier}
        Ticket message: ${message}
    `
}

function Escalate(tier: string, message: string) -> Escalation {
    client: "typesafeai/jev-latest"
    prompt: `
        Customer tier: ${tier}
        Ticket message: ${message}
    `
}

function RouteAliased(tier: string, message: string) -> Routed {
    client: "typesafeai/jev-latest"
    prompt: `
        Customer tier: ${tier}
        Ticket message: ${message}
    `
}

function AssessTicketWithContext(tier: string, message: string) -> TicketAssessment {
    client: "typesafeai/jev-latest"
    prompt: `
        ${role("instructions")}
        You are triaging support tickets for an API company.
        ${role("user")}
        Customer tier: ${tier}
        Ticket message: ${message}
    `
}

function ReportTicket(tier: string, message: string) -> TicketReport {
    client: "typesafeai/jev-latest"
    prompt: `
        Customer tier: ${tier}
        Ticket message: ${message}
    `
}

function JudgeThread(tier: string, message: string) -> Thread {
    client: "typesafeai/jev-latest"
    prompt: `
        Customer tier: ${tier}
        Ticket message: ${message}
    `
}

function JudgeThreadAliased(tier: string, message: string) -> ThreadAliased {
    client: "typesafeai/jev-latest"
    prompt: `
        Customer tier: ${tier}
        Ticket message: ${message}
    `
}

function JudgeRoute(tier: string, message: string) -> Judged<Team> {
    client: "typesafeai/jev-latest"
    prompt: `
        Customer tier: ${tier}
        Ticket message: ${message}
    `
}

function AssessWithConfidence(tier: string, message: string) -> AssessmentWithConfidence {
    client: "typesafeai/jev-latest"
    prompt: `
        Customer tier: ${tier}
        Ticket message: ${message}
    `
}

function DescribeScreenshot(shot: image) -> Team {
    client: "typesafeai/jev-latest"
    prompt: `Which team should handle this? ${shot}`
}

function TagTicket(tier: string, message: string) -> Tagged {
    client: "typesafeai/jev-latest"
    prompt: `
        Customer tier: ${tier}
        Ticket message: ${message}
    `
}

function TrueOrFalse(tier: string, message: string) -> true | false {
    client: "typesafeai/jev-latest"
    prompt: `
        ${role("instructions")}
        Does this ticket require immediate attention?
        ${role("user")}
        Customer tier: ${tier}
        Ticket message: ${message}
    `
}

function PickPartial(tier: string, message: string) -> Partial {
    client: "typesafeai/jev-latest"
    prompt: `
        Customer tier: ${tier}
        Ticket message: ${message}
    `
}

// ─── helpers ─────────────────────────────────────────────────────────────────

// The request `build_request()` produces, parsed. No credentials, no IO.
function shape(req: baml.http.Request) -> baml.json.json {
    baml.json.parse(req.body)
}

function questions_of(req: baml.http.Request) -> map<string, baml.json.json> {
    baml.json.path<map<string, baml.json.json>>(shape(req), ".questions")
}

// Question IDs may be dotted paths, so look them up as map keys rather than
// as JSON path segments.
function question_of(req: baml.http.Request, id: string) -> map<string, baml.json.json> {
    match (questions_of(req).get(id)) {
        let question: map<string, baml.json.json> => question,
        _ => throw baml.errors.InvalidArgument { message: `no question ${id}` },
    }
}

function instructions_of(req: baml.http.Request, id: string) -> string {
    baml.json.path<string>(question_of(req, id), ".instructions")
}

function criteria_of(req: baml.http.Request, id: string) -> map<string, baml.json.json> {
    baml.json.path<map<string, baml.json.json>>(question_of(req, id), ".criteria")
}

// The full request the design notes show, with the shared state.
function full_request(questions: map<string, baml.json.json>) -> baml.json.json {
    let expected: map<string, baml.json.json> = { "model": "jev-latest", "state": state(), "questions": questions };
    expected
}

function noul(instructions: string) -> baml.json.json {
    let q: map<string, baml.json.json> = { "type": "noul", "instructions": instructions };
    q
}

function choice(instructions: string, criteria: map<string, baml.json.json>) -> baml.json.json {
    let q: map<string, baml.json.json> = { "type": "choice", "instructions": instructions, "criteria": criteria };
    q
}

function team_criteria() -> map<string, baml.json.json> {
    {
        "Billing": "Payments, charges, refunds, or invoices",
        "Technical": "Bugs, outages, or integrations",
        "Account": "Login, identity, or account access",
    }
}

function null_criteria(keys: string[]) -> map<string, baml.json.json> {
    let out: map<string, baml.json.json> = {};
    for (let key in keys) {
        let _ = out.set(key, null);
    }
    out
}

function noul_answer(n: float) -> baml.json.json {
    let a: map<string, baml.json.json> = { "type": "noul", "noul": n };
    a
}

function choice_answer(key: string) -> baml.json.json {
    let a: map<string, baml.json.json> = { "type": "choice", "choice": key };
    a
}

function answers(map_: map<string, baml.json.json>) -> string {
    baml.json.stringify({ "model": "jev-latest", "answers": map_ })
}

function scalar(answer: baml.json.json) -> string {
    answers({ "result": answer })
}

// A decoded value plus the body the mock actually received, so a case can
// check both halves of the contract from one call.
class Outcome<T> {
    value: T,
    sent: baml.json.json,
    count: int,
}

function run<T>(body: string, spec: ai.FunctionSpec<T>) -> Outcome<T> {
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(body)], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let cl = typesafeai.Client.new(api_key = "test-key", base_url = mock.base_url());
        let value = spec.call(client = cl);
        Outcome { value: value, sent: mock.last_request().json(), count: mock.request_count() }
    })
}

// True when the runner rejects before any HTTP request is sent.
function rejected_before_http<T>(spec: ai.FunctionSpec<T>) -> bool {
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(scalar(noul_answer(0.9)))], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let cl = typesafeai.Client.new(api_key = "test-key", base_url = mock.base_url());
        let _ = spec.call(client = cl) catch_all (e) {
            let invalid: ai.errors.InvalidRequest => {
                return mock.request_count() == 0;
            },
            _ => {
                return false;
            },
        };
        false
    })
}

// True when `build_request()` itself raises InvalidRequest.
function build_rejected<T>(spec: ai.FunctionSpec<T>) -> bool {
    let _ = spec.build_request() catch_all (e) {
        let invalid: ai.errors.InvalidRequest => {
            return true;
        },
        _ => {
            return false;
        },
    };
    false
}

// The IsUrgent prompt with a different declared output type, for the
// not-convertible table. Same prompt, same instructions, only the type varies.
function input_for(ty: reflect.Type) -> ai.ModelTurnInput {
    let spec = IsUrgent@spec(tier(), message());
    ai.ModelTurnInput {
        prompt: spec.prompt_template,
        output_type: ty,
        journal: ai.Journal { log: [] },
        toolbox: ai.tools.Toolbox.new([]),
    }
}

function type_rejected(ty: reflect.Type) -> bool {
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(scalar(noul_answer(0.9)))], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let cl = typesafeai.Client.new(api_key = "test-key", base_url = mock.base_url());
        let _ = cl.invoke(input_for(ty)) catch_all (e) {
            let invalid: ai.errors.InvalidRequest => {
                return mock.request_count() == 0;
            },
            _ => {
                return false;
            },
        };
        false
    })
}

// ─── 1.1 bool becomes Noul, thresholded at 0.5 ───────────────────────────────

function case_1_1_request() -> bool {
    let req = IsUrgent@build_request(tier(), message());
    assert.equal(req.method, "POST");
    assert.equal(req.url, "https://api.typesafe.ai/v1/systemone");
    assert.equal(
        shape(req),
        full_request({ "result": noul("Does this ticket require immediate attention?") }),
    );
    true
}

test "1.1 bool request shape" {
    assert.is_true(case_1_1_request())
}

function case_1_1_decode(n: float) -> bool {
    let outcome = run(scalar(noul_answer(n)), IsUrgent@spec(tier(), message()));
    // The body on the wire is the one build_request() previews.
    assert.equal(outcome.sent, shape(IsUrgent@build_request(tier(), message())));
    outcome.value
}

test "1.1 noul 0.91 is true" {
    assert.is_true(case_1_1_decode(0.91))
}

test "1.1 noul 0.5 is true" {
    assert.is_true(case_1_1_decode(0.5))
}

test "1.1 noul 0.49 is false" {
    assert.equal(case_1_1_decode(0.49), false)
}

test "1.1 noul 0.0 is false" {
    assert.equal(case_1_1_decode(0.0), false)
}

function case_1_1_full_response() -> bool {
    let body = `{"model":"jev-latest","answers":{"result":{"type":"noul","noul":0.91}},"usage":{"input_tokens":142,"output_tokens":9}}`;
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(body)], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let cl = typesafeai.Client.new(api_key = "test-key", base_url = mock.base_url());
        let result = ai.Agent.new(client = cl).run(IsUrgent@spec(tier(), message()));
        assert.is_true(result.value);
        assert.equal(result.usage.input_tokens, 142);
        assert.equal(result.usage.output_tokens, 9);
        assert.equal(mock.last_request().header("authorization"), "Bearer test-key");
        true
    })
}

test "1.1 full response carries usage" {
    assert.is_true(case_1_1_full_response())
}

// ─── 1.2 float becomes Noul and keeps the probability ────────────────────────

function case_1_2_request() -> bool {
    assert.equal(
        shape(UrgencyProbability@build_request(tier(), message())),
        shape(IsUrgent@build_request(tier(), message())),
    );
    true
}

test "1.2 float request is identical to bool" {
    assert.is_true(case_1_2_request())
}

function case_1_2_decode(n: float) -> float {
    run(scalar(noul_answer(n)), UrgencyProbability@spec(tier(), message())).value
}

test "1.2 float keeps 0.91" {
    assert.equal(case_1_2_decode(0.91), 0.91)
}

test "1.2 float keeps 0.5" {
    assert.equal(case_1_2_decode(0.5), 0.5)
}

test "1.2 float keeps 0.0" {
    assert.equal(case_1_2_decode(0.0), 0.0)
}

// ─── 1.3 scalar return with no role("instructions") ──────────────────────────

test "1.3 bare bool is rejected by build_request" {
    assert.is_true(build_rejected(IsUrgentNoInstructions@spec(tier(), message())))
}

test "1.3 bare bool is rejected before HTTP" {
    assert.is_true(rejected_before_http(IsUrgentNoInstructions@spec(tier(), message())))
}

// ─── 1.4 enum becomes Choice ─────────────────────────────────────────────────

function case_1_4_request() -> bool {
    assert.equal(
        shape(RouteTicket@build_request(tier(), message())),
        full_request({ "result": choice("Which team should handle this ticket?", team_criteria()) }),
    );
    true
}

test "1.4 enum request shape" {
    assert.is_true(case_1_4_request())
}

function case_1_4_decode() -> Team {
    let body = `{"model":"jev-latest","answers":{"result":{"type":"choice","choice":"Technical","probabilities":{"Billing":0.03,"Technical":0.94,"Account":0.03},"confidence":0.9}},"usage":{"input_tokens":171,"output_tokens":14}}`;
    run(body, RouteTicket@spec(tier(), message())).value
}

test "1.4 enum decodes choice" {
    assert.equal(case_1_4_decode(), Team.Technical)
}

function case_1_4_ignores_probabilities() -> Team {
    // probabilities disagree with choice: only choice is consulted (D3).
    let body = `{"answers":{"result":{"type":"choice","choice":"Billing","probabilities":{"Billing":0.01,"Technical":0.99,"Account":0.0}}}}`;
    run(body, RouteTicket@spec(tier(), message())).value
}

test "1.4 probabilities are not consulted" {
    assert.equal(case_1_4_ignores_probabilities(), Team.Billing)
}

// ─── 1.5 enum with @alias and a variant without @description ─────────────────

function case_1_5_request() -> bool {
    let req = Prioritize@build_request(tier(), message());
    assert.equal(
        questions_of(req),
        {
            "result": choice(
                "How should this ticket be prioritized?",
                {
                    "critical": "Production is down or data is at risk",
                    "high": null,
                    "normal": "Everything else",
                },
            ),
        },
    );
    // `high` is present with an explicit null, not absent.
    assert.is_true(criteria_of(req, "result").has("high"));
    true
}

test "1.5 alias keys and null descriptions" {
    assert.is_true(case_1_5_request())
}

function case_1_5_decode() -> Priority {
    run(scalar(choice_answer("critical")), Prioritize@spec(tier(), message())).value
}

test "1.5 alias key decodes to variant" {
    assert.equal(case_1_5_decode(), Priority.P0)
}

// ─── 1.6 enum with both role("instructions") and @@description ───────────────

function case_1_6_request() -> bool {
    let req = RouteTicketWithContext@build_request(tier(), message());
    let text = instructions_of(req, "result");
    let context = "You are triaging support tickets for an API company.";
    let derived = "Which team should handle this ticket?";
    assert.contains(text, context);
    assert.contains(text, derived);
    assert.is_true(text.starts_with(context));
    assert.is_true(text.ends_with(derived));
    assert.equal(criteria_of(req, "result"), team_criteria());
    assert.equal(baml.json.path<string>(shape(req), ".state"), state());
    true
}

test "1.6 instructions prepend type-derived instruction" {
    assert.is_true(case_1_6_request())
}

// ─── 1.7 string literal union becomes Choice ─────────────────────────────────

function case_1_7_request() -> bool {
    assert.equal(
        questions_of(ClassifyMood@build_request(tier(), message())),
        {
            "result": choice(
                "What is the customer's emotional state?",
                null_criteria(["calm", "concerned", "angry"]),
            ),
        },
    );
    true
}

test "1.7 string literal union request shape" {
    assert.is_true(case_1_7_request())
}

function case_1_7_decode() -> Mood {
    let body = `{"answers":{"result":{"type":"choice","choice":"angry","probabilities":{"calm":0.02,"concerned":0.18,"angry":0.80}}}}`;
    run(body, ClassifyMood@spec(tier(), message())).value
}

test "1.7 string literal decodes" {
    assert.equal(case_1_7_decode(), "angry")
}

// ─── 1.8 int literal union becomes Choice, not Score ─────────────────────────

function review() -> string {
    "Works fine but the setup docs were out of date."
}

function case_1_8_request() -> bool {
    let req = StarRating@build_request(review());
    assert.equal(baml.json.path<string>(shape(req), ".state"), review());
    assert.equal(
        questions_of(req),
        {
            "result": choice(
                "How many stars does this review give the product?",
                null_criteria(["1", "2", "3", "4", "5"]),
            ),
        },
    );
    true
}

test "1.8 int literal union request shape" {
    assert.is_true(case_1_8_request())
}

function case_1_8_decode() -> int {
    let body = `{"answers":{"result":{"type":"choice","choice":"3","probabilities":{"1":0.02,"2":0.10,"3":0.61,"4":0.24,"5":0.03}}}}`;
    run(body, StarRating@spec(review())).value
}

test "1.8 int literal decodes by table lookup" {
    assert.equal(case_1_8_decode(), 3)
}

// ─── 1.9 bool literal union and bigint literals ──────────────────────────────

function case_1_9_request() -> bool {
    assert.equal(
        criteria_of(Mixed@build_request(message()), "result"),
        null_criteria(["true", "unsure", "9007199254740993"]),
    );
    true
}

test "1.9 mixed literal criteria" {
    assert.is_true(case_1_9_request())
}

function case_1_9_decode(key: string) -> true | "unsure" | 9007199254740993 {
    run(scalar(choice_answer(key)), Mixed@spec(message())).value
}

test "1.9 bool literal decodes" {
    assert.equal(case_1_9_decode("true"), true)
}

test "1.9 string literal decodes" {
    assert.equal(case_1_9_decode("unsure"), "unsure")
}

// An unsuffixed literal that fits in an int is an int literal in BAML;
// spell it `9007199254740993n` in the type to get a bigint back.
test "1.9 large int literal decodes" {
    assert.equal(case_1_9_decode("9007199254740993"), 9007199254740993)
}

// ─── 1.10 optional enum becomes Choice with a "<null>" key ───────────────────

function case_1_10_request() -> bool {
    let req = RouteOrLeave@build_request(tier(), message());
    let expected = team_criteria();
    let _ = expected.set("<null>", null);
    assert.equal(criteria_of(req, "result"), expected);
    // The union has no metadata slot: only role("instructions") contributes (D9).
    assert.equal(instructions_of(req, "result"), "Which team should handle this ticket, if any?");
    true
}

test "1.10 optional enum request shape" {
    assert.is_true(case_1_10_request())
}

function case_1_10_decode(key: string) -> Team? {
    run(scalar(choice_answer(key)), RouteOrLeave@spec(tier(), message())).value
}

test "1.10 optional enum decodes variant" {
    assert.equal(case_1_10_decode("Technical"), Team.Technical)
}

test "1.10 optional enum decodes null" {
    assert.equal(case_1_10_decode("<null>"), null)
}

test "1.10 optional enum without instructions is rejected" {
    assert.is_true(rejected_before_http(RouteOrLeaveNoInstructions@spec(tier(), message())))
}

// ─── 1.11 enum plus literal union ────────────────────────────────────────────

function case_1_11_request() -> bool {
    let expected = team_criteria();
    let _ = expected.set("hold", null);
    assert.equal(criteria_of(RouteOrHold@build_request(tier(), message()), "result"), expected);
    true
}

test "1.11 enum plus literal criteria" {
    assert.is_true(case_1_11_request())
}

function case_1_11_decode(key: string) -> Team | "hold" {
    run(scalar(choice_answer(key)), RouteOrHold@spec(tier(), message())).value
}

test "1.11 enum member decodes" {
    assert.equal(case_1_11_decode("Billing"), Team.Billing)
}

test "1.11 literal member decodes" {
    assert.equal(case_1_11_decode("hold"), "hold")
}

// ─── 1.12 enum variant subset ────────────────────────────────────────────────

// The runner's output-type validation (`_validate_renderable`, E0164) does
// not yet accept an enum-variant subset as a declared LLM function return
// type, so this case drives the client directly with the reflected type. The
// adapter itself handles the subset.
function narrow() -> reflect.Type {
    reflect.Type.of<Team.Billing | Team.Technical>()
}

function case_1_12_request() -> bool {
    let req = typesafeai.Client.new().render(input_for(narrow()));
    assert.equal(
        criteria_of(req, "result"),
        {
            "Billing": "Payments, charges, refunds, or invoices",
            "Technical": "Bugs, outages, or integrations",
        },
    );
    true
}

test "1.12 variant subset criteria" {
    assert.is_true(case_1_12_request())
}

function case_1_12_decode() -> unknown {
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(scalar(choice_answer("Technical")))], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let cl = typesafeai.Client.new(api_key = "test-key", base_url = mock.base_url());
        cl.invoke(input_for(narrow())).parsed_output?.value
    })
}

test "1.12 variant subset decodes" {
    assert.equal(case_1_12_decode(), Team.Technical)
}

// ─── 1.13 key collision in a union is tolerated ──────────────────────────────

function case_1_13_request() -> bool {
    let criteria = criteria_of(Collide@build_request(message()), "result");
    assert.equal(criteria.keys().length(), 3);
    assert.is_true(criteria.has("high"));
    assert.equal(criteria.get("critical"), "Production is down or data is at risk");
    assert.equal(criteria_of(CollideReversed@build_request(message()), "result"), criteria);
    true
}

test "1.13 collided key appears once" {
    assert.is_true(case_1_13_request())
}

function case_1_13_decode() -> string {
    let forward = run(scalar(choice_answer("high")), Collide@spec(message())).value;
    let reversed = run(scalar(choice_answer("high")), CollideReversed@spec(message())).value;
    `${forward}|${reversed}`
}

// D5 says the first member in declaration order wins. The compiler currently
// canonicalizes union member order, so both spellings resolve the same way
// (see ../../../baml_builtins2/baml_std/typesafeai/README.md). This pins the
// current behaviour so a change to union ordering shows up here.
test "1.13 collided key resolves to the same owner in both spellings" {
    assert.equal(case_1_13_decode(), "high|high")
}

// ─── 2.1 flat class becomes one request with one question per field ──────────

function assessment_questions() -> map<string, baml.json.json> {
    {
        "urgent": noul("Does this ticket require immediate attention?"),
        "route": choice("Which team should handle this ticket?", team_criteria()),
        "reply_within_hour": noul("How likely is it that the customer expects a reply within the hour?"),
        "mood": choice("What is the customer's emotional state?", null_criteria(["calm", "concerned", "angry"])),
    }
}

function case_2_1_request() -> bool {
    assert.equal(
        shape(AssessTicket@build_request(tier(), message())),
        full_request(assessment_questions()),
    );
    true
}

test "2.1 flat class request shape" {
    assert.is_true(case_2_1_request())
}

function assessment_body() -> string {
    `{"model":"jev-latest","answers":{"urgent":{"type":"noul","noul":0.91},"route":{"type":"choice","choice":"Technical","probabilities":{"Billing":0.03,"Technical":0.94,"Account":0.03},"confidence":0.9},"reply_within_hour":{"type":"noul","noul":0.77},"mood":{"type":"choice","choice":"angry","probabilities":{"calm":0.02,"concerned":0.18,"angry":0.80},"confidence":0.74}},"usage":{"input_tokens":203,"output_tokens":41}}`
}

function case_2_1_decode() -> bool {
    let value = run(assessment_body(), AssessTicket@spec(tier(), message())).value;
    assert.equal(
        value,
        TicketAssessment {
            urgent: true,
            route: Team.Technical,
            reply_within_hour: 0.77,
            mood: "angry",
        },
    );
    true
}

test "2.1 flat class decodes" {
    assert.is_true(case_2_1_decode())
}

// ─── 2.2 field @description overrides the enum's @@description ───────────────

function case_2_2_request() -> bool {
    let req = Escalate@build_request(tier(), message());
    assert.equal(
        instructions_of(req, "owner"),
        "Which team should own the escalation, not the first response?",
    );
    assert.equal(criteria_of(req, "owner"), team_criteria());
    true
}

test "2.2 field description wins" {
    assert.is_true(case_2_2_request())
}

function case_2_2_decode() -> Escalation {
    run(answers({ "owner": choice_answer("Account") }), Escalate@spec(tier(), message())).value
}

test "2.2 field with overridden instructions decodes" {
    assert.equal(case_2_2_decode(), Escalation { owner: Team.Account })
}

// ─── 2.3 field alias does not change the question ID ─────────────────────────

function case_2_3_request() -> bool {
    assert.equal(
        questions_of(RouteAliased@build_request(tier(), message())),
        {
            "route": choice("department: Which team should handle this ticket?", team_criteria()),
            "urgent": noul("Does this ticket require immediate attention?"),
        },
    );
    true
}

test "2.3 alias prefixes instruction, not id" {
    assert.is_true(case_2_3_request())
}

function case_2_3_decode() -> Routed {
    run(
        answers({ "route": choice_answer("Billing"), "urgent": noul_answer(0.2) }),
        RouteAliased@spec(tier(), message()),
    ).value
}

test "2.3 aliased field decodes by declared name" {
    assert.equal(case_2_3_decode(), Routed { route: Team.Billing, urgent: false })
}

// ─── 2.4 class with role("instructions") prefixes every question ─────────────

function case_2_4_request() -> bool {
    let req = AssessTicketWithContext@build_request(tier(), message());
    let context = "You are triaging support tickets for an API company.";
    let questions = questions_of(req);
    assert.equal(questions.keys().length(), 4);
    for (let id in ["urgent", "route", "reply_within_hour", "mood"]) {
        let text = instructions_of(req, id);
        let local = baml.json.path<string>(assessment_questions().get(id), ".instructions");
        assert.is_true(text.starts_with(context));
        assert.is_true(text.ends_with(local));
    }
    // State is unchanged from 2.1.
    assert.equal(
        baml.json.path<string>(shape(req), ".state"),
        baml.json.path<string>(shape(AssessTicket@build_request(tier(), message())), ".state"),
    );
    true
}

test "2.4 every question is prefixed" {
    assert.is_true(case_2_4_request())
}

function case_2_4_decode() -> TicketAssessment {
    run(assessment_body(), AssessTicketWithContext@spec(tier(), message())).value
}

test "2.4 prefixed class decodes" {
    assert.equal(
        case_2_4_decode(),
        TicketAssessment {
            urgent: true,
            route: Team.Technical,
            reply_within_hour: 0.77,
            mood: "angry",
        },
    )
}

// ─── 2.5 nested class flattens to dotted question IDs ────────────────────────

function case_2_5_request() -> bool {
    assert.equal(
        questions_of(ReportTicket@build_request(tier(), message())),
        {
            "route": choice("Which team should handle this ticket?", team_criteria()),
            "customer.angry": noul("customer.angry: Is the customer angry?"),
            "customer.churn_risk": noul("customer.churn_risk: How likely is the customer to cancel?"),
        },
    );
    true
}

test "2.5 nested class request shape" {
    assert.is_true(case_2_5_request())
}

function case_2_5_decode() -> TicketReport {
    run(
        answers(
            {
                "route": choice_answer("Technical"),
                "customer.angry": noul_answer(0.88),
                "customer.churn_risk": noul_answer(0.42),
            },
        ),
        ReportTicket@spec(tier(), message()),
    ).value
}

test "2.5 nested class rebuilds the tree" {
    assert.equal(
        case_2_5_decode(),
        TicketReport {
            route: Team.Technical,
            customer: Sentiment { angry: true, churn_risk: 0.42 },
        },
    )
}

// ─── 2.6 two levels of nesting ───────────────────────────────────────────────

function case_2_6_request() -> bool {
    let req = JudgeThread@build_request(tier(), message());
    let questions = questions_of(req);
    let keys = questions.keys();
    keys.sort();
    assert.equal(
        keys,
        [
            "agent.sentiment.angry",
            "agent.sentiment.churn_risk",
            "customer.sentiment.angry",
            "customer.sentiment.churn_risk",
        ],
    );
    assert.equal(
        instructions_of(req, "customer.sentiment.angry"),
        "customer.sentiment.angry: Is the customer angry?",
    );
    assert.equal(
        instructions_of(req, "agent.sentiment.angry"),
        "agent.sentiment.angry: Is the customer angry?",
    );
    assert.is_true(
        instructions_of(req, "customer.sentiment.angry")
            != instructions_of(req, "agent.sentiment.angry"),
    );
    true
}

test "2.6 two-level nesting ids and distinct instructions" {
    assert.is_true(case_2_6_request())
}

function case_2_6_decode() -> Thread {
    run(
        answers(
            {
                "customer.sentiment.angry": noul_answer(0.9),
                "customer.sentiment.churn_risk": noul_answer(0.6),
                "agent.sentiment.angry": noul_answer(0.1),
                "agent.sentiment.churn_risk": noul_answer(0.0),
            },
        ),
        JudgeThread@spec(tier(), message()),
    ).value
}

test "2.6 two-level nesting decodes" {
    assert.equal(
        case_2_6_decode(),
        Thread {
            customer: Party { sentiment: Sentiment { angry: true, churn_risk: 0.6 } },
            agent: Party { sentiment: Sentiment { angry: false, churn_risk: 0.0 } },
        },
    )
}

// ─── 2.7 aliases override prefix segments but not IDs ────────────────────────

function case_2_7_request() -> bool {
    assert.equal(
        questions_of(JudgeThreadAliased@build_request(tier(), message())),
        {
            "customer.angry": noul("the customer.angry: Is the customer angry?"),
            "customer.churn_risk": noul("the customer.churn_risk: How likely is the customer to cancel?"),
            "agent.angry": noul("the support agent.angry: Is the customer angry?"),
            "agent.churn_risk": noul("the support agent.churn_risk: How likely is the customer to cancel?"),
        },
    );
    true
}

test "2.7 aliased segments in instructions only" {
    assert.is_true(case_2_7_request())
}

function case_2_7_decode() -> ThreadAliased {
    run(
        answers(
            {
                "customer.angry": noul_answer(0.9),
                "customer.churn_risk": noul_answer(0.6),
                "agent.angry": noul_answer(0.1),
                "agent.churn_risk": noul_answer(0.3),
            },
        ),
        JudgeThreadAliased@spec(tier(), message()),
    ).value
}

test "2.7 aliased parents decode by declared name" {
    assert.equal(
        case_2_7_decode(),
        ThreadAliased {
            customer: Sentiment { angry: true, churn_risk: 0.6 },
            agent: Sentiment { angry: false, churn_risk: 0.3 },
        },
    )
}

// ─── 2.8 generic class ───────────────────────────────────────────────────────

function case_2_8_request() -> bool {
    assert.equal(
        questions_of(JudgeRoute@build_request(tier(), message())),
        {
            "value": choice("Which team should handle this ticket?", team_criteria()),
            "confident": noul("Is the judgment about this ticket clear-cut?"),
        },
    );
    true
}

test "2.8 generic class request shape" {
    assert.is_true(case_2_8_request())
}

function case_2_8_decode() -> Judged<Team> {
    run(
        answers({ "value": choice_answer("Technical"), "confident": noul_answer(0.95) }),
        JudgeRoute@spec(tier(), message()),
    ).value
}

test "2.8 generic class decodes" {
    assert.equal(case_2_8_decode(), Judged<Team> { value: Team.Technical, confident: true })
}

// ─── 3.1 evidence is discarded at the typed layer ────────────────────────────

function case_3_1_request() -> bool {
    let questions = questions_of(AssessWithConfidence@build_request(tier(), message()));
    assert.equal(questions.keys().length(), 5);
    assert.equal(questions.get("route_confidence"), noul("How confident are you in the route?"));
    true
}

test "3.1 a confidence field is a fifth independent question" {
    assert.is_true(case_3_1_request())
}

function case_3_1_envelope() -> bool {
    let body = assessment_body();
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(body)], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let cl = typesafeai.Client.new(api_key = "test-key", base_url = mock.base_url());
        let seen: string[] = [];
        let _ = AssessTicket@spec(tier(), message()).call(client = cl, on_event = (event: ai.events.Event) -> {
            match (event) {
                let call: ai.events.LLMCall => {
                    seen.push(call.http_response?.body ?? "<none>");
                    null
                },
                _ => null,
            };
        });
        assert.equal(seen, [body]);
        true
    })
}

test "3.1 the full envelope is visible through the call hook" {
    assert.is_true(case_3_1_envelope())
}

// ─── 4 not JEV-convertible ───────────────────────────────────────────────────

test "4 reject string" {
    assert.is_true(type_rejected(reflect.Type.of<string>()))
}

test "4 reject int" {
    assert.is_true(type_rejected(reflect.Type.of<int>()))
}

test "4 reject bigint" {
    assert.is_true(type_rejected(reflect.Type.of<bigint>()))
}

test "4 reject string array" {
    assert.is_true(type_rejected(reflect.Type.of<string[]>()))
}

test "4 reject enum array" {
    assert.is_true(type_rejected(reflect.Type.of<Team[]>()))
}

test "4 reject map" {
    assert.is_true(type_rejected(reflect.Type.of<map<string, Team>>()))
}

test "4 reject optional bool" {
    assert.is_true(type_rejected(reflect.Type.of<bool?>()))
}

test "4 reject optional float" {
    assert.is_true(type_rejected(reflect.Type.of<float?>()))
}

test "4 reject optional class" {
    assert.is_true(type_rejected(reflect.Type.of<TicketAssessment?>()))
}

test "4 reject class union" {
    assert.is_true(type_rejected(reflect.Type.of<TicketAssessment | Sentiment>()))
}

test "4 reject mixed structural union" {
    assert.is_true(type_rejected(reflect.Type.of<Team | Sentiment>()))
}

test "4 reject class with unsupported leaf" {
    assert.is_true(type_rejected(reflect.Type.of<WithNotes>()))
}

test "4 reject recursive class" {
    assert.is_true(type_rejected(reflect.Type.of<Node>()))
}

test "4 reject recursive alias" {
    assert.is_true(type_rejected(reflect.Type.of<Tree>()))
}

test "4 reject image output" {
    assert.is_true(type_rejected(reflect.Type.of<image>()))
}

test "4 reject audio output" {
    assert.is_true(type_rejected(reflect.Type.of<audio>()))
}

test "4 reject pdf output" {
    assert.is_true(type_rejected(reflect.Type.of<pdf>()))
}

test "4 reject image input" {
    assert.is_true(
        rejected_before_http(
            DescribeScreenshot@spec(baml.media.Image.from_url("https://example.invalid/shot.png", null)),
        ),
    )
}

// ─── 5 cases the design notes left open ──────────────────────────────────────

test "5 standalone literal is rejected" {
    assert.is_true(type_rejected(reflect.Type.of<"approved">()))
}

test "5 single-variant enum is rejected" {
    assert.is_true(type_rejected(reflect.Type.of<Only>()))
}

test "5 empty class is rejected" {
    assert.is_true(type_rejected(reflect.Type.of<Empty>()))
}

test "5 literal-typed field rejects the class" {
    assert.is_true(rejected_before_http(TagTicket@spec(tier(), message())))
}

function case_5_true_or_false() -> bool {
    assert.equal(
        shape(TrueOrFalse@build_request(tier(), message())),
        shape(IsUrgent@build_request(tier(), message())),
    );
    true
}

test "5 true | false plans as noul" {
    assert.is_true(case_5_true_or_false())
}

function case_5_true_or_false_decode() -> true | false {
    run(scalar(noul_answer(0.7)), TrueOrFalse@spec(tier(), message())).value
}

test "5 true | false decodes" {
    assert.equal(case_5_true_or_false_decode(), true)
}

function case_5_skipped_variant() -> bool {
    assert.equal(
        criteria_of(PickPartial@build_request(tier(), message()), "result"),
        { "Visible": "Shown", "Also": null },
    );
    true
}

test "5 skipped variant is excluded from criteria" {
    assert.is_true(case_5_skipped_variant())
}
