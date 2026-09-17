enum TicketRoute {
    Billing @description("Payments and refunds"),
    Technical @description("Bugs and outages"),
}

class TicketAssessment {
    urgent: bool @description("Does the ticket need immediate attention?"),
    route: TicketRoute @description("Which team should handle this ticket?"),
    urgency_probability: float @description("Does the ticket need immediate attention?"),
}

class NestedAssessment {
    ticket: TicketAssessment,
    follow_up: bool @description("Should someone follow up with the customer?"),
}

enum AliasedRoute {
    Billing @alias("billing") @description("Payments and refunds"),
    Technical @alias("technical") @description("Bugs and outages"),
}

enum PriorityChoice {
    Low @alias("low") @description("No rush"),
    High @alias("high") @description("Immediate action"),
}

type LiteralChoice = "calm" | "urgent";

type EnumUnionChoice = TicketRoute | PriorityChoice;

class AliasedAssessment {
    urgent: bool @alias("needsAttention") @description("Does this need immediate attention?"),
    route: AliasedRoute @alias("team") @description("Which team should handle this?"),
}

class UnsupportedNested {
    note: string @description("Summarize the note"),
}

function AssessNestedTicket(message: string) -> NestedAssessment {
    client: "typesafeai/jev-latest"
    prompt: `Ticket: ${message}`
}

function AssessAliasedTicket(message: string) -> AliasedAssessment {
    client: "typesafeai/jev-latest"
    prompt: `Ticket: ${message}`
}

function ChooseLiteral(message: string) -> LiteralChoice {
    client: "typesafeai/jev-latest"
    prompt: `Is this calm or urgent? ${message}`
}

function ChooseEnumUnion(message: string) -> EnumUnionChoice {
    client: "typesafeai/jev-latest"
    prompt: `Classify this as a route or priority: ${message}`
}

function UnsupportedUnion(message: string) -> bool | string {
    client: "typesafeai/jev-latest"
    prompt: `Classify this: ${message}`
}

function AssessUnsupportedNested(message: string) -> UnsupportedNested {
    client: "typesafeai/jev-latest"
    prompt: `Ticket: ${message}`
}

function AssessTicket(message: string) -> TicketAssessment {
    client: "typesafeai/jev-latest"
    prompt: `Ticket: ${message}`
}

function IsUrgent(message: string) -> bool {
    client: "typesafeai/jev-latest"
    prompt: `Does this message convey urgency? ${message}`
}

function IsUrgentWithRoles(message: string) -> bool {
    client: "typesafeai/jev-latest"
    prompt: `${role("system")}Assess urgency.${role("user")}Does this need immediate attention? ${message}`
}

function RouteTicket(message: string) -> TicketRoute {
    client: "typesafeai/jev-latest"
    prompt: `Route this ticket: ${message}`
}

function UrgencyProbability(message: string) -> float {
    client: "typesafeai/jev-latest"
    prompt: `Does this message convey urgency? ${message}`
}

client ScoreClient = typesafeai.Client.new(
    model = "jev-latest",
    score_criteria = ["calm", "concerned", "very angry"],
)

client BadThresholdClient = typesafeai.Client.new(noul_threshold = 1.1)

function BadThreshold(message: string) -> bool {
    client: BadThresholdClient
    prompt: `Is this urgent? ${message}`
}

function Frustration(message: string) -> float {
    client: ScoreClient
    prompt: `Rate the frustration in this message: ${message}`
}

function Unsupported(message: string) -> string {
    client: "typesafeai/jev-latest"
    prompt: `Summarize this message: ${message}`
}

function _answer(
    kind: typesafeai.QuestionKind,
    noul_value: float? = null,
    choice_value: string? = null,
    score_value: float? = null,
) -> typesafeai.Answer {
    typesafeai.Answer {
        type: kind,
        noul: noul_value,
        choice: choice_value,
        score: score_value,
        probabilities: null,
        legend: null,
        confidence: null,
    }
}

test "typesafeai request shapes and unsupported type" {
    let yes_no = IsUrgent@build_request("my payout failed");
    assert.equal(yes_no.url, "https://api.typesafe.ai/v1/systemone");
    assert.equal(yes_no.headers.get("authorization"), null);
    let yes_no_body = baml.json.parse(yes_no.body);
    let yes_no_object: map<string, baml.json.json> = yes_no_body else {
        throw "body is not an object"
    };
    assert.equal(yes_no_object.get("model"), "jev-latest");
    assert.is_true(yes_no.body.includes("\"type\":\"noul\""));
    assert.is_true(yes_no.body.includes("my payout failed"));

    let probability = UrgencyProbability@build_request("please respond now").body;
    assert.is_true(probability.includes("\"type\":\"noul\""));

    let choice = RouteTicket@build_request("the site is down").body;
    assert.is_true(choice.includes("\"type\":\"choice\""));
    assert.is_true(choice.includes("Payments and refunds"));
    assert.is_true(choice.includes("Bugs and outages"));

    let score = Frustration@build_request("I am furious").body;
    assert.is_true(score.includes("\"type\":\"score\""));
    assert.is_true(score.includes("very angry"));

    let error = Unsupported@build_request("hello").url catch_all (e) {
        _ => e.to_string()
    };
    assert.is_true(error.includes("TypeSafe AI cannot produce BAML output type string"))
}

test "typesafeai preserves prompt roles in structured state" {
    let request = IsUrgentWithRoles@build_request("Please help now");
    let body = baml.json.parse(request.body);
    assert.equal(baml.json.path<string>(body, ".state.messages[0].role"), "system");
    assert.equal(baml.json.path<string>(body, ".state.messages[0].content"), "Assess urgency.");
    assert.equal(baml.json.path<string>(body, ".state.messages[1].role"), "user");
    assert.is_true(
        baml.json.path<string>(body, ".state.messages[1].content").includes("Please help now"),
    )
}

test "typesafeai preserves text conversation history" {
    let input = ai.ModelTurnInput {
        prompt: IsUrgent@spec("Need help now").prompt_template,
        journal: ai.Journal {
            log: [
                ai.events.UserMessage.new("Earlier request"),
                ai.events.AssistantMessage {
                    content: [ai.content.Text { text: "Earlier reply" }],
                    client_id: "typesafeai/jev-latest",
                },
            ],
        },
        toolbox: ai.tools.Toolbox.new([]),
        output_type: reflect.Type.of<bool>(),
    };
    let request = typesafeai.Client.new().render(input);
    let body = baml.json.parse(request.body);
    assert.equal(baml.json.path<string>(body, ".state.messages[1].content"), "Earlier request");
    assert.equal(baml.json.path<string>(body, ".state.messages[2].role"), "assistant");
    assert.equal(baml.json.path<string>(body, ".state.messages[2].content"), "Earlier reply")
}

test "typesafeai batches flat class fields" {
    let request = AssessTicket@build_request("My payout failed and I need it today");
    assert.is_true(request.body.includes("\"urgent\""));
    assert.is_true(request.body.includes("\"route\""));
    assert.is_true(request.body.includes("\"urgency_probability\""));
    assert.is_true(request.body.includes("Does the ticket need immediate attention?"));
    assert.is_true(request.body.includes("Payments and refunds"));

    let response = typesafeai.Response {
        model: "jev-latest",
        answers: {
            "urgent": _answer("noul", noul_value = 0.9),
            "route": _answer("choice", choice_value = "Billing"),
            "urgency_probability": _answer("noul", noul_value = 0.9),
        },
        usage: typesafeai.Usage { input_tokens: 0, output_tokens: 0 },
    };
    let result_text = typesafeai.internal._text(response, reflect.Type.of<TicketAssessment>(), 0.5);
    let result: map<string, baml.json.json> = baml.json.parse(result_text) else {
        throw "class response was not an object"
    };
    assert.equal(result.get("urgent"), true);
    assert.equal(result.get("route"), "Billing");
    assert.equal(result.get("urgency_probability"), 0.9);
    let parsed = baml.sap.parse<TicketAssessment>(result_text);
    assert.equal(parsed.urgent, true);
    assert.equal(parsed.route, TicketRoute.Billing)
}

test "typesafeai converts typed answers to BAML parse text" {
    let noul = typesafeai.Response {
        model: "jev-latest",
        answers: { "result": _answer("noul", noul_value = 0.82) },
        usage: typesafeai.Usage { input_tokens: 0, output_tokens: 0 },
    };
    assert.equal(typesafeai.internal._text(noul, reflect.Type.of<bool>(), 0.5), "true");
    assert.equal(typesafeai.internal._text(noul, reflect.Type.of<float>(), 0.5), "0.82");

    let choice = typesafeai.Response {
        model: "jev-latest",
        answers: { "result": _answer("choice", choice_value = "Billing") },
        usage: typesafeai.Usage { input_tokens: 0, output_tokens: 0 },
    };
    assert.equal(
        typesafeai.internal._text(choice, reflect.Type.of<TicketRoute>(), 0.5),
        "\"Billing\"",
    );

    let score = typesafeai.Response {
        model: "jev-latest",
        answers: { "result": _answer("score", score_value = 1.6) },
        usage: typesafeai.Usage { input_tokens: 0, output_tokens: 0 },
    };
    assert.equal(typesafeai.internal._text(score, reflect.Type.of<float>(), 0.5), "1.6")
}

test "typesafeai supports nested fixed shape classes" {
    let request = AssessNestedTicket@build_request("My payout failed");
    assert.is_true(request.body.includes("ticket.urgent"));
    assert.is_true(request.body.includes("ticket.route"));
    assert.is_true(request.body.includes("follow_up"));
    let response = typesafeai.Response {
        model: "jev-latest",
        answers: {
            "ticket.urgent": _answer("noul", noul_value = 0.9),
            "ticket.route": _answer("choice", choice_value = "Billing"),
            "ticket.urgency_probability": _answer("noul", noul_value = 0.9),
            "follow_up": _answer("noul", noul_value = 0.8),
        },
        usage: typesafeai.Usage { input_tokens: 0, output_tokens: 0 },
    };
    let text = typesafeai.internal._text(response, reflect.Type.of<NestedAssessment>(), 0.5);
    let parsed = baml.sap.parse<NestedAssessment>(text);
    assert.equal(parsed.ticket.route, TicketRoute.Billing);
    assert.equal(parsed.ticket.urgent, true);
    assert.equal(parsed.follow_up, true)
}

test "typesafeai native API covers structured state and all questions" {
    let ts_client = typesafeai.Client.new();
    let questions: map<string, typesafeai.Question> = {
        "urgent": typesafeai.noul(
            { "question": "Is this urgent?" },
            criteria = { "true": "Requires immediate attention", "false": "Can wait" },
        ),
        "route": typesafeai.choice(
            ["Which team?", "Select exactly one."],
            { "billing": null, "technical": "Bugs or outages" },
        ),
        "frustration": typesafeai.score("How frustrated is the customer?", ["Calm", "Concerned", "Very angry"]),
    };
    let req = ts_client.build_system_one_request(
        { "conversation": [{ "role": "user", "content": "I need my refund today" }] },
        questions,
    );
    assert.equal(req.headers.get("authorization"), null);
    assert.is_true(req.body.includes("conversation"));
    assert.is_true(req.body.includes("Requires immediate attention"));
    assert.is_true(req.body.includes("\"billing\":null"));
    assert.is_true(req.body.includes("Select exactly one."));
    assert.is_true(req.body.includes("Very angry"));
    let invalid = ts_client
        .build_system_one_request(
            "hello",
            { "bad": typesafeai.score("Rate this", ["only one level"]) },
        )
        .url catch_all (e) {
        _ => e.to_string()
    };
    assert.is_true(invalid.includes("at least two levels"))
}

test "typesafeai models endpoint request" {
    let request = typesafeai.Client.new().build_list_models_request();
    assert.equal(request.method, "GET");
    assert.equal(request.url, "https://api.typesafe.ai/v1/models");
    assert.equal(request.body, "");
    assert.equal(request.headers.get("authorization"), null)
}

test "typesafeai rejects an unresolved base URL" {
    let ts_client = typesafeai.Client.new(base_url = env.TYPESAFE_TEST_UNSET_BASE_URL);
    let error = ts_client.build_list_models_request().url catch_all (e) {
        _ => e.to_string()
    };
    assert.is_true(error.includes("base_url did not resolve"))
}

test "typesafeai honors class and enum aliases" {
    let request = AssessAliasedTicket@build_request("I need a refund");
    assert.is_true(request.body.includes("\"urgent\""));
    assert.is_true(request.body.includes("\"billing\""));
    assert.is_true(request.body.includes("\"technical\""));
    let response = typesafeai.Response {
        model: "jev-latest",
        answers: {
            "urgent": _answer("noul", noul_value = 0.9),
            "route": _answer("choice", choice_value = "billing"),
        },
        usage: typesafeai.Usage { input_tokens: 0, output_tokens: 0 },
    };
    let text = typesafeai.internal._text(response, reflect.Type.of<AliasedAssessment>(), 0.5);
    assert.is_true(text.includes("needsAttention"));
    assert.is_true(text.includes("team"));
    let parsed = baml.sap.parse<AliasedAssessment>(text);
    assert.equal(parsed.urgent, true);
    assert.equal(parsed.route, AliasedRoute.Billing)
}

test "typesafeai rejects invalid typed outputs before HTTP" {
    let bad_threshold = BadThreshold@build_request("test").url catch_all (e) {
        _ => e.to_string()
    };
    assert.is_true(bad_threshold.includes("noul_threshold must be between 0 and 1"));
    let unsupported = AssessUnsupportedNested@build_request("hello").url catch_all (e) {
        _ => e.to_string()
    };
    assert.is_true(unsupported.includes("output type string"));
    let no_questions = typesafeai.Client.new().build_system_one_request("test", {}).url catch_all (e) {
        _ => e.to_string()
    };
    assert.is_true(no_questions.includes("at least one question"))
}

test "typesafeai maps finite unions to Choice" {
    let literals = ChooseLiteral@build_request("help now");
    assert.is_true(literals.body.includes("\"calm\":null"));
    assert.is_true(literals.body.includes("\"urgent\":null"));
    let literal_response = typesafeai.Response {
        model: "jev-latest",
        answers: { "result": _answer("choice", choice_value = "urgent") },
        usage: typesafeai.Usage { input_tokens: 0, output_tokens: 0 },
    };
    let literal_text = typesafeai.internal._text(literal_response, reflect.Type.of<LiteralChoice>(), 0.5);
    let parsed_literal = baml.sap.parse<LiteralChoice>(literal_text);
    assert.equal(parsed_literal, "urgent");

    let enums = ChooseEnumUnion@build_request("refund now");
    assert.is_true(enums.body.includes("\"Billing\""));
    assert.is_true(enums.body.includes("\"high\""));
    let enum_response = typesafeai.Response {
        model: "jev-latest",
        answers: { "result": _answer("choice", choice_value = "high") },
        usage: typesafeai.Usage { input_tokens: 0, output_tokens: 0 },
    };
    let enum_text = typesafeai.internal._text(enum_response, reflect.Type.of<EnumUnionChoice>(), 0.5);
    let parsed_enum = baml.sap.parse<EnumUnionChoice>(enum_text);
    assert.equal(parsed_enum, PriorityChoice.High);

    let unsupported = UnsupportedUnion@build_request("hello").url catch_all (e) {
        _ => e.to_string()
    };
    assert.is_true(unsupported.includes("must contain only string literals or enums"))
}
