enum Team {
    Billing @description("Payments and refunds"),
    Technical @alias("tech") @description("Bugs and outages"),
    Account,
    Hidden @skip,
    @@description("Which team should handle this ticket?")
}

class Sentiment {
    angry: bool @description("Is the customer angry?"),
    churn: float @description("How likely is cancellation?"),
}

class Report {
    route: Team @alias("department"),
    customer: Sentiment @alias("the customer"),
    agent: Sentiment @alias("the agent"),
    mood: "calm" | "angry" @description("What is the mood?"),
}

class Judged<T> {
    value: T,
    confident: bool @description("Is the judgment clear?"),
}

class Override {
    route: Team @description("Who owns the escalation?"),
}

class Node {
    next: Node?,
}

class DirectNode {
    next: DirectNode,
}

class WithNotes {
    route: Team,
    notes: string,
}

class Empty {
}

class WithSkipped {
    route: Team,
    notes: string? @skip,
    internal: unknown @skip,
    flag: bool? @skip,
    media: image? @skip,
}

class InvalidSkipped {
    route: Team,
    notes: string @skip,
}

function SkipFields(message: string) -> WithSkipped {
    client: "typesafeai/jev-latest"
    prompt: `
        ${message}
    `
}

type Tree = Tree[] | Team;

function Urgent(message: string) -> bool {
    client: "typesafeai/jev-latest"
    prompt: `
        ${role("instructions")}

        Does this ticket require immediate attention?

        ${role("user")}

        ${message}
    `
}

function Probability(message: string) -> float {
    client: typesafeai.Client.new()
    prompt: `
        ${role("instructions")}

        How likely is urgency?

        ${role("user")}

        ${message}
    `
}

function Route(message: string) -> Team {
    client: "typesafeai/jev-latest"
    prompt: `
        ${message}
    `
}

function Assess(message: string) -> Report {
    client: "typesafeai/jev-latest"
    prompt: `
        ${role("instructions")}

        You are triaging support tickets.

        ${role("user")}

        ${message}
    `
}

function Generic(message: string) -> Judged<Team> {
    client: "typesafeai/jev-latest"
    prompt: `
        ${message}
    `
}

function Optional(message: string) -> Team? {
    client: "typesafeai/jev-latest"
    prompt: `
        ${role("instructions")}

        Pick a team, if any.

        ${role("user")}

        ${message}
    `
}

function Collide(message: string) -> Team | "tech" {
    client: "typesafeai/jev-latest"
    prompt: `
        ${role("instructions")}

        Pick an option.

        ${role("user")}

        ${message}
    `
}

function CollideReversed(message: string) -> "tech" | Team {
    client: "typesafeai/jev-latest"
    prompt: `
        ${role("instructions")}

        Pick an option.

        ${role("user")}

        ${message}
    `
}

function input(ty: reflect.Type, instructions: bool = true) -> ai.ModelTurnInput {
    let spec = if (instructions) {
        Urgent@spec("Production is down.")
    } else {
        // Use the same closure type with a prompt lacking instructions.
        Bare@spec("Production is down.")
    };
    ai.ModelTurnInput {
        prompt: spec.prompt_template,
        output_type: ty,
        journal: ai.Journal { log: [] },
        toolbox: ai.tools.Toolbox.new([]),
    }
}

function Bare(message: string) -> bool {
    client: "typesafeai/jev-latest"
    prompt: `
        ${message}
    `
}

function noul_body(n: float) -> string {
    baml.json.stringify(
        {
            "answers": { "result": { "type": "noul", "noul": n } },
            "usage": { "input_tokens": 142, "output_tokens": 9 },
        },
    )
}

function choice_body(key: string) -> string {
    // Deliberately contradict probabilities: only choice determines the value.
    baml.json.stringify(
        {
            "answers": {
                "result": {
                    "type": "choice",
                    "choice": key,
                    "probabilities": { "wrong": 1.0 },
                    "confidence": 0.8,
                },
            },
        },
    )
}

function noun_thresholds() -> bool {
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(noul_body(0.5))], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let cl = typesafeai.Client.new(api_key = "test-key", base_url = mock.base_url());
        let result = ai.Agent.new(client = cl).run(Urgent@spec("Production is down."));
        assert.is_true(result.value);
        assert.equal(result.usage.input_tokens, 142);
        assert.equal(result.usage.output_tokens, 9);
        let sent = mock.last_request();
        assert.equal(sent.path(), "/systemone");
        assert.equal(sent.header("authorization"), "Bearer test-key");
        assert.equal(baml.json.path<string>(sent.json(), ".model"), "jev-latest");
        // The prompt names `message`, so the argument is the state.
        assert.equal(
            baml.json.path<map<string, string>>(sent.json(), ".state"),
            { "message": "Production is down." },
        );
        assert.equal(baml.json.path<string>(sent.json(), ".questions.result.type"), "noul");
        assert.equal(
            baml.json.path<string>(sent.json(), ".questions.result.instructions"),
            "Does this ticket require immediate attention?\n\n`message`",
        );
        let p = ai.Agent.new(client = cl).run(Probability@spec("Production is down."));
        assert.equal(p.value, 0.5);
        true
    })
}

test "noul threshold, float, request and usage" {
    assert.is_true(noun_thresholds())
}

function below_threshold(n: float) -> bool {
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(noul_body(n))], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        ai.Agent.new(client = typesafeai.Client.new(api_key = "k", base_url = mock.base_url()))
            .run(Urgent@spec("ticket"))
            .value
    })
}

test "noul false below 0.5" {
    assert.equal(below_threshold(0.49), false)
}

test "noul zero" {
    assert.equal(below_threshold(0.0), false)
}

test "noul one" {
    assert.is_true(below_threshold(1.0))
}

function enum_and_collisions() -> bool {
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(choice_body("tech"))], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let cl = typesafeai.Client.new(api_key = "k", base_url = mock.base_url());
        assert.equal(ai.Agent.new(client = cl).run(Route@spec("outage")).value, Team.Technical);
        let body = mock.last_request().json();
        assert.equal(
            baml.json.path<string>(body, ".questions.result.instructions"),
            "`message`\n\nWhich team should handle this ticket?",
        );
        let criteria = baml.json.path<map<string, baml.json.json>>(body, ".questions.result.criteria");
        assert.equal(criteria.get("tech"), "Bugs and outages");
        assert.is_true(criteria.has("Account"));
        assert.equal(criteria.get("Account"), null);
        assert.equal(criteria.has("Hidden"), false);
        // Reflection canonicalizes both spellings to the same order.
        // The first reflected member owns a collided key.
        assert.equal(ai.Agent.new(client = cl).run(Collide@spec("outage")).value, "tech");
        assert.equal(ai.Agent.new(client = cl).run(CollideReversed@spec("outage")).value, "tech");
        true
    })
}

test "enum metadata and first-owner collisions" {
    assert.is_true(enum_and_collisions())
}

function optional_null() -> bool {
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(choice_body("<null>"))], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let cl = typesafeai.Client.new(api_key = "k", base_url = mock.base_url());
        let value = ai.Agent.new(client = cl).run(Optional@spec("nothing to route")).value;
        assert.equal(value, null);
        assert.equal(
            baml.json.path<string>(mock.last_request().json(), ".questions.result.instructions"),
            "Pick a team, if any.\n\n`message`",
        );
        true
    })
}

test "optional enum null" {
    assert.is_true(optional_null())
}

function literal_decode(ty: reflect.Type, key: string) -> unknown {
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(choice_body(key))], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let cl = typesafeai.Client.new(api_key = "k", base_url = mock.base_url());
        let turn = cl.invoke(input(ty));
        turn.parsed_output?.value
    })
}

test "client defaults keep a lazy credential and the TypeSafe base URL" {
    let cl = typesafeai.Client.new();
    let credential: baml.env.Ref = cl.api_key else {
        throw "expected an environment credential reference"
    };
    assert.equal(credential.name, "TYPESAFE_API_KEY");
    assert.equal(cl.base_url, "https://api.typesafe.ai/v1");
    let request = cl.render(input(reflect.Type.of<bool>()));
    assert.equal(request.url, "https://api.typesafe.ai/v1/systemone");
    assert.equal(request.headers.get("authorization"), null)
}

test "client constructor keeps explicit credentials and endpoint overrides" {
    let cl = typesafeai.Client.new(api_key = "test-key", base_url = "http://localhost:1234");
    assert.equal(cl.api_key, "test-key");
    assert.equal(cl.base_url, "http://localhost:1234");
    assert.equal(cl.render(input(reflect.Type.of<bool>())).url, "http://localhost:1234/systemone")
}

test "int literal union" {
    assert.equal(literal_decode(reflect.Type.of<1 | 2 | 3>(), "2"), 2)
}

test "string literal union" {
    assert.equal(literal_decode(reflect.Type.of<"calm" | "angry">(), "angry"), "angry")
}

test "bool literal in mixed union" {
    assert.equal(literal_decode(reflect.Type.of<true | "unsure">(), "true"), true)
}

test "enum variant subset" {
    assert.equal(
        literal_decode(reflect.Type.of<Team.Billing | Team.Technical>(), "tech"),
        Team.Technical,
    )
}

test "bigint literal preserves precision" {
    assert.equal(
        literal_decode(reflect.Type.of<9007199254740993n | "unsure">(), "9007199254740993"),
        9007199254740993n,
    )
}

test "negative int and bigint choice keys have no suffix" {
    assert.equal(literal_decode(reflect.Type.of<-12 | "unsure">(), "-12"), -12);
    assert.equal(
        literal_decode(reflect.Type.of<-9007199254740993n | "unsure">(), "-9007199254740993"),
        -9007199254740993n,
    )
}

type EscapedChoice = "a\"b\\c" | "unsure";

test "false and raw string choice keys" {
    assert.equal(literal_decode(reflect.Type.of<false | "unsure">(), "false"), false);
    assert.equal(literal_decode(reflect.Type.of<EscapedChoice>(), "a\"b\\c"), "a\"b\\c")
}

class ExactChoice {
    // The enum's wire key is "tech"; the literal's is "Technical".
    // JSON serializes both values as "Technical", losing their identity.
    route: Team | "Technical" @description("Choose a team or the literal label"),
}

function ExactChoiceResult(message: string) -> ExactChoice {
    client: "typesafeai/jev-latest"
    prompt: `
        ${message}
    `
}

function exact_class_choice(key: string) -> Team | "Technical" {
    let body = baml.json.stringify({ "answers": { "route": { "type": "choice", "choice": key } } });
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(body)], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let cl = typesafeai.Client.new(api_key = "k", base_url = mock.base_url());
        ai.Agent.new(client = cl).run(ExactChoiceResult@spec("outage")).value.route
    })
}

test "class JSON reconstruction currently resolves ambiguous strings to enums" {
    assert.equal(exact_class_choice("tech"), Team.Technical);
    // TODO(json): preserve the selected literal when it shares an enum's JSON representation.
    // Both choices currently roundtrip through the same JSON string.
    assert.equal(exact_class_choice("Technical"), Team.Technical)
}

function MediaInput(media: image | audio | video | pdf) -> bool {
    client: "typesafeai/jev-latest"
    prompt: `
        ${role("instructions")}

        Is it urgent?

        ${role("user")}

        ${media}
    `
}

function rejects_media(media: image | audio | video | pdf) -> bool {
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(noul_body(0.9))], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let cl = typesafeai.Client.new(api_key = "k", base_url = mock.base_url());
        let result = ai.Agent.new(client = cl).run(MediaInput@spec(media)).value catch_all (e) {
            let invalid: ai.errors.InvalidRequest => {
                assert.equal(mock.request_count(), 0);
                return true;
            },
            _ => {
                return false;
            },
        };
        false
    })
}

test "reject audio input" {
    assert.is_true(rejects_media(baml.media.Audio.from_base64("QUJD", "audio/mpeg")))
}

test "reject video input" {
    assert.is_true(rejects_media(baml.media.Video.from_base64("QUJD", "video/mp4")))
}

test "reject pdf input" {
    assert.is_true(rejects_media(baml.media.Pdf.from_base64("QUJD", "application/pdf")))
}

function png() -> image {
    baml.media.Image.from_base64("iVBORw0KGgo=", "image/png")
}

function jpeg() -> image {
    baml.media.Image.from_base64("/9j/4AA=", "image/jpeg")
}

function ImageInput(note: string, first: image, second: image, again: image) -> bool {
    client: "typesafeai/jev-latest"
    prompt: `
        ${role("instructions")}

        Is it urgent?

        ${role("user")}

        ${note} ${first}

        ${second} then ${again}
    `
}

function InstructionImage(reference: image, photo: image) -> bool {
    client: "typesafeai/jev-latest"
    prompt: `
        ${role("instructions")}

        Reference photo: ${reference}

        ${role("user")}

        ${photo}
    `
}

function image_request() -> bool {
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(noul_body(0.9))], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let cl = typesafeai.Client.new(api_key = "k", base_url = mock.base_url());
        let result = ai.Agent.new(client = cl).run(ImageInput@spec("Photo:", png(), jpeg(), png()));
        assert.is_true(result.value);
        let sent = mock.last_request();
        assert.equal(sent.header("typesafe-danger-zone-images"), "true");
        assert.equal(
            baml.json.path<map<string, string>>(sent.json(), ".state"),
            {
                "note": "Photo:",
                "first": "<baml-image-0>",
                "second": "<baml-image-1>",
                "again": "<baml-image-0>",
            },
        );
        assert.equal(
            baml.json.path<string>(sent.json(), ".questions.result.instructions"),
            "Is it urgent?\n\n`note` `first` `second` then `again`",
        );
        assert.equal(
            baml.json.path<map<string, string>>(sent.json(), ".assets['<baml-image-0>']"),
            { "type": "image", "format": "png", "data_base64": "iVBORw0KGgo=" },
        );
        assert.equal(
            baml.json.path<map<string, string>>(sent.json(), ".assets['<baml-image-1>']"),
            { "type": "image", "format": "jpg", "data_base64": "/9j/4AA=" },
        );
        assert.equal(baml.json.path<map<string, unknown>>(sent.json(), ".assets").length(), 2);
        true
    })
}

test "image input becomes referenced assets" {
    assert.is_true(image_request())
}

function text_request() -> bool {
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(noul_body(0.9))], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let cl = typesafeai.Client.new(api_key = "k", base_url = mock.base_url());
        let _ = ai.Agent.new(client = cl).run(Urgent@spec("Production is down."));
        let sent = mock.last_request();
        assert.equal(sent.header("typesafe-danger-zone-images"), null);
        assert.equal(baml.json.path_or<unknown>(sent.json(), ".assets", "absent"), "absent");
        true
    })
}

test "text-only request has no assets or image header" {
    assert.is_true(text_request())
}

test "image in instructions is part of each question" {
    let cl = typesafeai.Client.new();
    let req = cl.render(
        ai.ModelTurnInput {
            prompt: InstructionImage@spec(png(), jpeg()).prompt_template,
            output_type: reflect.Type.of<Sentiment>(),
            journal: ai.Journal { log: [] },
            toolbox: ai.tools.Toolbox.new([]),
        },
    );
    let body = baml.json.parse(req.body);
    assert.equal(req.headers.get("typesafe-danger-zone-images"), "true");
    assert.equal(baml.json.path<string>(body, ".state"), "<baml-image-1>");
    assert.equal(
        baml.json.path<string>(body, ".questions.angry.instructions"),
        "Reference photo: <baml-image-0>\n\nIs the customer angry?",
    );
    assert.equal(
        baml.json.path<string>(body, ".questions.churn.instructions"),
        "Reference photo: <baml-image-0>\n\nHow likely is cancellation?",
    )
}

test "image in history is referenced in state order" {
    let req = typesafeai.Client.new().render(
        ai.ModelTurnInput {
            prompt: Urgent@spec("Production is down.").prompt_template,
            output_type: reflect.Type.of<bool>(),
            journal: ai.Journal {
                log: [
                    ai.events.UserMessage.of(["Before:", png()]),
                    ai.events.AssistantMessage {
                        content: [ai.content.Text { text: "Noted." }, ai.content.Media.new(jpeg())],
                        client_id: "typesafeai/jev-latest",
                    },
                ],
            },
            toolbox: ai.tools.Toolbox.new([]),
        },
    );
    let body = baml.json.parse(req.body);
    assert.equal(
        baml.json.path<string>(body, ".state"),
        "Production is down.\n\nBefore: <baml-image-0>\n\nNoted.\n\n<baml-image-1>",
    );
    assert.equal(baml.json.path<string>(body, ".assets['<baml-image-1>'].format"), "jpg")
}

function rejects_preview(media: image) -> bool {
    let req = typesafeai.Client.new().render(
        ai.ModelTurnInput {
            prompt: MediaInput@spec(media).prompt_template,
            output_type: reflect.Type.of<bool>(),
            journal: ai.Journal { log: [] },
            toolbox: ai.tools.Toolbox.new([]),
        },
    ) catch_all (e) {
        let unsupported: ai.errors.PreviewUnsupported => {
            return true;
        },
        _ => {
            return false;
        },
    };
    false
}

test "image URL preview is unsupported" {
    assert.is_true(
        rejects_preview(baml.media.Image.from_url("https://example.invalid/image.png", null)),
    )
}

function ReservedText(message: string) -> bool {
    client: "typesafeai/jev-latest"
    prompt: `
        ${role("instructions")}

        Is it urgent?

        ${role("user")}

        ${message}
    `
}

function rejects_reserved_text() -> bool {
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(noul_body(0.9))], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let cl = typesafeai.Client.new(api_key = "k", base_url = mock.base_url());
        let result = ai.Agent.new(client = cl).run(ReservedText@spec("see <baml-image-0>")).value catch_all (e) {
            let invalid: ai.errors.InvalidRequest => {
                assert.equal(mock.request_count(), 0);
                return true;
            },
            _ => {
                return false;
            },
        };
        false
    })
}

test "reject text that looks like an image reference" {
    assert.is_true(rejects_reserved_text())
}

class Visit {
    note: string,
    snapshot: image,
}

function NamedVisit(visit: Visit, unused: string, nothing: string?) -> bool {
    client: "typesafeai/jev-latest"
    prompt: `Is someone at the door in ${visit.snapshot}? Note: ${nothing}`
}

function bound_request() -> baml.json.json {
    let spec = NamedVisit@spec(Visit { note: "evening", snapshot: png() }, "not sent", null);
    baml.json.parse(spec.build_request(client = typesafeai.Client.new()).body)
}

test "named parameters become the state and backtick references" {
    let body = bound_request();
    assert.equal(
        baml.json.path<baml.json.json>(body, ".state"),
        { "visit": { "note": "evening", "snapshot": "<baml-image-0>" }, "nothing": null },
    );
    assert.equal(
        baml.json.path<string>(body, ".questions.result.instructions"),
        "Is someone at the door in `visit.snapshot`? Note: `nothing`",
    );
    assert.equal(
        baml.json.path<string>(body, ".assets['<baml-image-0>'].data_base64"),
        "iVBORw0KGgo=",
    )
}

function bound_history<T>(spec: ai.FunctionSpec<T>) -> baml.http.Request {
    typesafeai.Client.new().render(
        ai.ModelTurnInput {
            prompt: spec.prompt_template,
            output_type: reflect.Type.of<bool>(),
            journal: ai.Journal {
                log: [
                    ai.events.RunStarted { spec_name: spec.name(), arguments: spec.arguments() },
                    ai.events.UserMessage.of(["Before:", png()]),
                ],
            },
            toolbox: ai.tools.Toolbox.new([]),
        },
    )
}

function HistoryParam(message: string) -> bool {
    client: "typesafeai/jev-latest"
    prompt: `Is ${message} urgent?`
}

function NamesHistory(history: string) -> bool {
    client: "typesafeai/jev-latest"
    prompt: `Is ${history} urgent?`
}

test "prior turns go under history in the state" {
    let body = baml.json.parse(bound_history(HistoryParam@spec("now")).body);
    assert.equal(
        baml.json.path<baml.json.json>(body, ".state"),
        { "message": "now", "history": ["Before: <baml-image-0>"] },
    )
}

function rejects_history_name() -> bool {
    let _ = bound_history(NamesHistory@spec("earlier")) catch_all (e) {
        let invalid: ai.errors.InvalidRequest => {
            return true;
        },
        _ => {
            return false;
        },
    };
    false
}

test "a parameter named history is rejected when there are prior turns" {
    assert.is_true(rejects_history_name())
}

function NamedAudio(clip: audio) -> bool {
    client: "typesafeai/jev-latest"
    prompt: `Is ${clip} loud?`
}

function rejects_named_audio() -> bool {
    let _ = NamedAudio@build_request(baml.media.Audio.from_base64("QUJD", "audio/mpeg")) catch_all (e) {
        let invalid: ai.errors.InvalidRequest => {
            return true;
        },
        _ => {
            return false;
        },
    };
    false
}

test "reject a named audio argument" {
    assert.is_true(rejects_named_audio())
}

test "reject webp image input" {
    assert.is_true(rejects_media(baml.media.Image.from_base64("UklGRg==", "image/webp")))
}

function wire_capture() -> bool {
    let body = noul_body(0.9);
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(body)], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let cl = typesafeai.Client.new(api_key = "private-key", base_url = mock.base_url());
        let turn = cl.invoke(input(reflect.Type.of<bool>()));
        assert.equal(turn.calls.at(0)?.http_response?.body, body);
        assert.equal(turn.calls.at(0)?.http_request?.headers.get("authorization"), "REDACTED");
        let quiet = typesafeai.Client.new(
            api_key = "private-key",
            base_url = mock.base_url(),
            capture_wire = false,
        );
        let no_body = quiet.invoke(input(reflect.Type.of<bool>()));
        assert.equal(no_body.calls.at(0)?.http_response?.body, null);
        true
    })
}

test "wire capture preserves envelope and redacts credentials" {
    assert.is_true(wire_capture())
}

function http_error(status: int) -> string {
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.with_headers(status, "failure", { "retry-after": "3" })], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let cl = typesafeai.Client.new(api_key = "k", base_url = mock.base_url());
        let result = cl.invoke(input(reflect.Type.of<bool>())).terminal_text() catch_all (e) {
            let limited: ai.errors.RateLimited => {
                return `limited:${limited.retry_after_ms ?? 0}`;
            },
            let invalid: ai.errors.InvalidRequest => {
                return "invalid";
            },
            let unavailable: ai.errors.NetworkFailure => {
                return "unavailable";
            },
            _ => {
                return "other";
            },
        };
        "no error"
    })
}

test "http validation error" {
    assert.equal(http_error(422), "invalid")
}

test "http rate limit preserves retry delay" {
    assert.equal(http_error(429), "limited:3000")
}

function runtime_shorthand() -> string {
    let provider = "typesafeai";
    ai.clients.resolve(`${provider}/jev-latest`).id()
}

test "runtime shorthand" {
    assert.equal(runtime_shorthand(), "typesafeai/jev-latest")
}

function nested_report() -> bool {
    let canned = `{"answers":{"route":{"type":"choice","choice":"tech"},"customer.angry":{"type":"noul","noul":0.88},"customer.churn":{"type":"noul","noul":0.42},"agent.angry":{"type":"noul","noul":0.1},"agent.churn":{"type":"noul","noul":0.2},"mood":{"type":"choice","choice":"angry"}}}`;
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(canned)], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let cl = typesafeai.Client.new(api_key = "k", base_url = mock.base_url());
        let report = ai.Agent.new(client = cl).run(Assess@spec("outage")).value;
        assert.equal(report.route, Team.Technical);
        assert.is_true(report.customer.angry);
        assert.equal(report.customer.churn, 0.42);
        assert.equal(report.agent.angry, false);
        assert.equal(report.mood, "angry");
        let questions = baml.json.path<map<string, baml.json.json>>(mock.last_request().json(), ".questions");
        assert.equal(questions.keys().length(), 6);
        let customer = baml.json.path<string>(questions.get("customer.angry"), ".instructions");
        assert.contains(customer, "You are triaging support tickets.");
        assert.contains(customer, "the customer.angry: Is the customer angry?");
        assert.contains(
            baml.json.path<string>(questions.get("agent.angry"), ".instructions"),
            "the agent.angry:",
        );
        assert.contains(
            baml.json.path<string>(questions.get("route"), ".instructions"),
            "department: Which team should handle this ticket?",
        );
        true
    })
}

test "nested classes flatten and rebuild" {
    assert.is_true(nested_report())
}

function generic_class() -> bool {
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(`{"answers":{"value":{"type":"choice","choice":"tech"},"confident":{"type":"noul","noul":0.9}}}`)], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let cl = typesafeai.Client.new(api_key = "k", base_url = mock.base_url());
        let value = ai.Agent.new(client = cl).run(Generic@spec("outage")).value;
        value.value == Team.Technical && value.confident
    })
}

test "realized generic class" {
    assert.is_true(generic_class())
}

type BigintChoice = 123456789012345678901234567890n | -123456789012345678901234567890n | "unsure";

class BigintReport {
    selected: Judged<BigintChoice>,
}

function BigintClassResult(message: string) -> BigintReport {
    client: "typesafeai/jev-latest"
    prompt: `
        ${role("instructions")}

        Choose the matching identifier.

        ${role("user")}

        ${message}
    `
}

function bigint_class_choice(key: string) -> BigintChoice {
    let body = baml.json.stringify(
        {
            "answers": {
                "selected.value": { "type": "choice", "choice": key },
                "selected.confident": { "type": "noul", "noul": 0.9 },
            },
        },
    );
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(body)], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let cl = typesafeai.Client.new(api_key = "k", base_url = mock.base_url());
        let result = ai.Agent.new(client = cl).run(BigintClassResult@spec(key)).value;
        assert.is_true(result.selected.confident);
        result.selected.value
    })
}

test "bigint choices retain precision through nested generic class JSON reconstruction" {
    assert.equal(
        bigint_class_choice("123456789012345678901234567890"),
        123456789012345678901234567890n,
    );
    assert.equal(
        bigint_class_choice("-123456789012345678901234567890"),
        -123456789012345678901234567890n,
    )
}

test "bigint class union still decodes its string alternative" {
    assert.equal(bigint_class_choice("unsure"), "unsure")
}

function rejects(ty: reflect.Type, instructions: bool = true) -> bool {
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok("{}")], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let cl = typesafeai.Client.new(api_key = "k", base_url = mock.base_url());
        let rejected = cl.invoke(input(ty, instructions = instructions)).terminal_text() catch_all (e) {
            let invalid: ai.errors.InvalidRequest => {
                assert.equal(mock.request_count(), 0);
                return true;
            },
            _ => {
                return false;
            },
        };
        false
    })
}

test "reject string" {
    assert.is_true(rejects(reflect.Type.of<string>()))
}

test "reject int" {
    assert.is_true(rejects(reflect.Type.of<int>()))
}

test "reject bigint" {
    assert.is_true(rejects(reflect.Type.of<bigint>()))
}

test "reject string array" {
    assert.is_true(rejects(reflect.Type.of<string[]>()))
}

test "reject enum array" {
    assert.is_true(rejects(reflect.Type.of<Team[]>()))
}

test "reject map" {
    assert.is_true(rejects(reflect.Type.of<map<string, Team>>()))
}

test "reject optional bool" {
    assert.is_true(rejects(reflect.Type.of<bool?>()))
}

test "reject optional float" {
    assert.is_true(rejects(reflect.Type.of<float?>()))
}

test "reject optional class" {
    assert.is_true(rejects(reflect.Type.of<Report?>()))
}

test "reject class union" {
    assert.is_true(rejects(reflect.Type.of<Report | Sentiment>()))
}

test "reject mixed structural union" {
    assert.is_true(rejects(reflect.Type.of<Team | Sentiment>()))
}

test "reject unsupported leaf" {
    assert.is_true(rejects(reflect.Type.of<WithNotes>()))
}

test "reject recursive class" {
    assert.is_true(rejects(reflect.Type.of<Node>()))
}

test "reject direct recursion by identity" {
    assert.is_true(rejects(reflect.Type.of<DirectNode>()))
}

test "reject recursive alias" {
    assert.is_true(rejects(reflect.Type.of<Tree>()))
}

test "reject image output" {
    assert.is_true(rejects(reflect.Type.of<image>()))
}

test "reject audio output" {
    assert.is_true(rejects(reflect.Type.of<audio>()))
}

test "reject pdf output" {
    assert.is_true(rejects(reflect.Type.of<pdf>()))
}

test "reject empty instructions" {
    assert.is_true(rejects(reflect.Type.of<bool>(), instructions = false))
}

test "union ignores member description" {
    assert.is_true(rejects(reflect.Type.of<Team?>(), instructions = false))
}

test "reject singleton" {
    assert.is_true(rejects(reflect.Type.of<"approved">()))
}

test "reject empty class" {
    assert.is_true(rejects(reflect.Type.of<Empty>()))
}

function malformed(body: string, ty: reflect.Type? = null) -> bool {
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(body)], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let cl = typesafeai.Client.new(api_key = "k", base_url = mock.base_url());
        let result = cl.invoke(input(ty ?? reflect.Type.of<bool>())).terminal_text() catch_all (e) {
            let failed: ai.errors.ParseFailed => {
                return true;
            },
            let invalid: ai.errors.InvalidResponse => {
                return true;
            },
            _ => {
                return false;
            },
        };
        false
    })
}

test "reject missing answer" {
    assert.is_true(malformed(`{"answers":{}}`))
}

test "reject wrong answer kind" {
    assert.is_true(malformed(`{"answers":{"result":{"type":"choice","noul":0.9}}}`))
}

test "reject out-of-range probability" {
    assert.is_true(malformed(noul_body(1.1)))
}

test "reject malformed response" {
    assert.is_true(malformed("not json"))
}

test "reject unoffered choice" {
    assert.is_true(malformed(choice_body("not-offered"), ty = reflect.Type.of<Team>()))
}

test "reject missing choice" {
    assert.is_true(
        malformed(`{"answers":{"result":{"type":"choice"}}}`, ty = reflect.Type.of<Team>()),
    )
}

function preview_metadata() -> bool {
    let cl = typesafeai.Client.new(api_key = "secret");
    let req = cl.render(input(reflect.Type.of<Override>(), instructions = false));
    assert.equal(req.headers.has("authorization"), false);
    assert.equal(req.url, "https://api.typesafe.ai/v1/systemone");
    assert.equal(
        baml.json.path<string>(baml.json.parse(req.body), ".questions.route.instructions"),
        "Who owns the escalation?",
    );
    assert.equal(Urgent@spec("x").client_id(), "typesafeai/jev-latest");
    true
}

test "preview and description precedence" {
    assert.is_true(preview_metadata())
}

function boolean_union() -> string {
    let cl = typesafeai.Client.new();
    baml.json.path<string>(
        baml.json.parse(cl.render(input(reflect.Type.of<true | false>())).body),
        ".questions.result.type",
    )
}

test "true or false normalizes to noul" {
    assert.equal(boolean_union(), "noul")
}

function skipped_subset() -> bool {
    let req = typesafeai.Client.new().render(
        input(reflect.Type.of<Team.Billing | Team.Technical | Team.Hidden>()),
    );
    let choices = baml.json.path<map<string, baml.json.json>>(
        baml.json.parse(req.body),
        ".questions.result.criteria",
    );
    choices.keys().length() == 2 && !choices.has("Hidden")
}

test "skip applies to enum variant subsets" {
    assert.is_true(skipped_subset())
}

function skipped_fields() -> bool {
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(`{"answers":{"route":{"type":"choice","choice":"tech"}}}`)], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let cl = typesafeai.Client.new(api_key = "k", base_url = mock.base_url());
        let value = ai.Agent.new(client = cl).run(SkipFields@spec("outage")).value;
        assert.equal(value.route, Team.Technical);
        assert.equal(value.notes, null);
        assert.equal(value.internal, null);
        assert.equal(value.flag, null);
        assert.equal(value.media, null);
        let questions = baml.json.path<map<string, baml.json.json>>(mock.last_request().json(), ".questions");
        assert.equal(questions.keys().length(), 1);
        assert.is_true(questions.has("route"));
        true
    })
}

test "skipped class fields are omitted and filled with null" {
    assert.is_true(skipped_fields())
}

test "reject nonnullable skipped fields before HTTP" {
    assert.is_true(rejects(reflect.Type.of<InvalidSkipped>()))
}

enum Shape {
    Circle,
    Square,
    Triangle,
    @@description("Which shape is in the image?")
}

function SeeShape(picture: image) -> Shape {
    client: "typesafeai/jev-latest"
    prompt: `
        ${picture}
    `
}

// A 288x288 PNG of one red circle on white. JEV rejects images under 280x280.
function circle_png() -> image {
    baml.media.Image.from_base64(
        "iVBORw0KGgoAAAANSUhEUgAAASAAAAEgAQMAAAAuTTzDAAAABlBMVEX////cAADRUR8qAAAB8ElEQVR42u1YS27CMBAlTiSyYMGSHelJGm7CUXq0HKFHyBG8jFCU6QJoMmM/x3YFtNW8DQ7zGL/5AfZmo1AoFAqFQqFQPANEH6ucd6JpjVMSEfXrjlZdFURERF2QtL2SLH/X8MfD9WUXJJ1v8iMkhUVt7yQb2O5wX+wCpPN3TnmG2dMspeiQp2Jetuu6hXLmqfYuBWk/LytIOoOASn9wPDzjD46HZ9DWDTBUIAhGqkE6GGn5YebWABlQbAvqKNsXDCPIJcumAblkmxu4c5OoyYCEs6wZkHD2aEDCmeNUTQ20pHoSo1bk105+3czFS9RUSFObG51Db3I9VdK0z/VUS1OdXbsY0l6aqoduF0VqEir2LE3OT07xek0/JZVwNhfT+WejU9Ivbzp3pk7/KAXOSNHrNTknmun1mqw0jQ/dbpCmIdfTKE0215NzhuxzPZE0dfl/tVswm8ll6dAcJHvqUfcme7Ko55akAXXKkjSiJkjWNKEmWJII1ZefOVt/6dJHqgOlY6QeVIWRLEg4Iw0g4Yw0goQz0gRyyUiEzuklOkKeUNKO883DCL+fLAjCgLAHSLrMy09IIlDHEpyfTrgnjt7gcm6NLl7dzp3VTdRbqFGP65Iib9cuPkmbnBu/uLvDqFvIuPtMhUKhUCgUCoUijC+3xLdAox4YzgAAAABJRU5ErkJggg==",
        "image/png",
    )
}

function SameShape(door: image, badge: image) -> bool {
    client: "typesafeai/jev-latest"
    prompt: `Is the shape in ${door} the same shape as in ${badge}?`
}

// A 288x288 PNG of one blue square on white.
function square_png() -> image {
    baml.media.Image.from_base64(
        "iVBORw0KGgoAAAANSUhEUgAAASAAAAEgAQMAAAAuTTzDAAAABlBMVEX///8AANzap02BAAAAP0lEQVR42u3LIQ4AIAwEwaYv5+dUoTCtIEHM2NuLAACAl/ZlnSk7f5FIJBKJRCKRSCQSiUQikUgk+iACAIChAqIoE48Ld+fOAAAAAElFTkSuQmCC",
        "image/png",
    )
}

testset "live" with testing.Sequential() {
    test "preconditions" {
        assert.is_true(true)
    }
    if (baml.env.get("TYPESAFE_API_KEY") != null) {
        test "shorthand bool" {
            assert.is_true(
                Urgent(
                    "Production is down and all customers are unable to use the service. Fix this immediately.",
                ),
            )
        }
        test "explicit client float" {
            assert.is_true(Probability("Production is down. This is an emergency!") >= 0.5)
        }
        test "enum" {
            assert.equal(
                Route("The API is returning 500 errors and our production integration is down."),
                Team.Technical,
            )
        }
        test "image" {
            assert.equal(SeeShape(circle_png()), Shape.Circle)
        }
        test "named images" {
            assert.equal(SameShape(circle_png(), circle_png()), true);
            assert.equal(SameShape(circle_png(), square_png()), false)
        }
    } else {
        log.warn("live typesafeai tests not registered: TYPESAFE_API_KEY is unset")
    }
}
