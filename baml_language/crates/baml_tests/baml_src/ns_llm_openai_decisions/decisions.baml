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
    client: "openai-decisions/gpt-6-luna"
    prompt: `
        ${message}
    `
}

type Tree = Tree[] | Team;

function Urgent(message: string) -> bool {
    client: "openai-decisions/gpt-6-luna"
    prompt: `
        ${role("instructions")}

        Does this ticket require immediate attention?

        ${role("user")}

        ${message}
    `
}

function Probability(message: string) -> float {
    client: openai.DecisionsClient.new()
    prompt: `
        ${role("instructions")}

        How likely is urgency?

        ${role("user")}

        ${message}
    `
}

function Route(message: string) -> Team {
    client: "openai-decisions/gpt-6-luna"
    prompt: `
        ${message}
    `
}

function Assess(message: string) -> Report {
    client: "openai-decisions/gpt-6-luna"
    prompt: `
        ${role("instructions")}

        You are triaging support tickets.

        ${role("user")}

        ${message}
    `
}

function Generic(message: string) -> Judged<Team> {
    client: "openai-decisions/gpt-6-luna"
    prompt: `
        ${message}
    `
}

function Optional(message: string) -> Team? {
    client: "openai-decisions/gpt-6-luna"
    prompt: `
        ${role("instructions")}

        Pick a team, if any.

        ${role("user")}

        ${message}
    `
}

function Collide(message: string) -> Team | "tech" {
    client: "openai-decisions/gpt-6-luna"
    prompt: `
        ${role("instructions")}

        Pick an option.

        ${role("user")}

        ${message}
    `
}

function CollideReversed(message: string) -> "tech" | Team {
    client: "openai-decisions/gpt-6-luna"
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
    client: "openai-decisions/gpt-6-luna"
    prompt: `
        ${message}
    `
}

function rejects(ty: reflect.Type, instructions: bool = true) -> bool {
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok("{}")], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let cl = openai.DecisionsClient.new(api_key = "k", base_url = mock.base_url());
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

function predicate_body(n: float) -> string {
    baml.json.stringify(
        {
            "model": "gpt-6-luna",
            "answers": [{ "name": "result", "type": "predicate", "probability": n }],
            "usage": { "input_tokens": 142, "output_tokens": 0 },
        },
    )
}

function choice_body(key: string) -> string {
    baml.json.stringify(
        {
            "answers": [
                {
                    "name": "result",
                    "type": "choice",
                    "choice": key,
                    "probabilities": [{ "value": "wrong", "probability": 1.0 }],
                    "confidence": 0.8,
                },
            ],
        },
    )
}

function predicate(n: float, ty: reflect.Type) -> unknown {
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(predicate_body(n))], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let cl = openai.DecisionsClient.new(api_key = "test-key", base_url = mock.base_url());
        let turn = cl.invoke(input(ty));
        assert.equal(
            turn.usage.to_json(),
            {
                "input_tokens": 142,
                "output_tokens": 0,
                "cached_input_tokens": null,
                "reasoning_tokens": null,
                "cache_write_input_tokens": null,
                "uncached_input_tokens": 142,
            },
        );
        assert.equal(turn.model, "gpt-6-luna");
        let sent = mock.last_request();
        assert.equal(sent.path(), "/decisions");
        assert.equal(sent.header("authorization"), "Bearer test-key");
        assert.equal(
            sent.json(),
            {
                "model": "gpt-6-luna",
                "input": "Production is down.",
                "questions": [
                    {
                        "type": "predicate",
                        "name": "result",
                        "instructions": "Does this ticket require immediate attention?",
                    },
                ],
            },
        );
        turn.parsed_output?.value
    })
}

test "predicate request, usage, thresholds and probability outputs" {
    assert.equal(predicate(0.0, reflect.Type.of<bool>()), false);
    assert.equal(predicate(0.49, reflect.Type.of<bool>()), false);
    assert.equal(predicate(0.5, reflect.Type.of<bool>()), true);
    assert.equal(predicate(1.0, reflect.Type.of<bool>()), true);
    assert.equal(predicate(0.72, reflect.Type.of<float>()), 0.72)
}

function choose(ty: reflect.Type, key: string) -> unknown {
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(choice_body(key))], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        openai.DecisionsClient.new(api_key = "k", base_url = mock.base_url())
            .invoke(input(ty))
            .parsed_output?.value
    })
}

test "enum aliases, finite unions, optional null and bigint choices" {
    assert.equal(choose(reflect.Type.of<Team>(), "tech"), Team.Technical);
    assert.equal(choose(reflect.Type.of<Team?>(), "<null>"), null);
    assert.equal(choose(reflect.Type.of<1 | 2 | 3>(), "2"), 2);
    assert.equal(choose(reflect.Type.of<"calm" | "angry">(), "angry"), "angry");
    assert.equal(choose(reflect.Type.of<true | "unsure">(), "true"), true);
    assert.equal(
        choose(reflect.Type.of<9007199254740993n | "unsure">(), "9007199254740993"),
        9007199254740993n,
    );
    assert.equal(choose(reflect.Type.of<Team.Billing | Team.Technical>(), "tech"), Team.Technical);
    assert.equal(choose(reflect.Type.of<Team | "tech">(), "tech"), Team.Technical)
}

test "defaults, lazy credentials, preview and both shorthand paths" {
    let cl = openai.DecisionsClient.new();
    assert.is_type<baml.env.Ref>(cl.api_key);
    assert.equal(
        cl.to_json(),
        {
            "model": "gpt-6-luna",
            "api_key": { "name": "OPENAI_API_KEY" },
            "base_url": "https://api.openai.com/v1",
            "timeout": null,
            "capture_wire": true,
        },
    );
    assert.equal(cl.id(), "openai-decisions/gpt-6-luna");
    assert.equal(Urgent@spec("x").client_id(), cl.id());
    let provider = "openai-decisions";
    assert.equal(ai.clients.resolve(`${provider}/gpt-6-luna`).id(), cl.id());
    let req = cl.render(input(reflect.Type.of<Override>(), instructions = false));
    assert.equal(
        req.to_json(),
        {
            "method": "POST",
            "url": "https://api.openai.com/v1/decisions",
            "headers": { "content-type": "application/json" },
            "body": baml.json.stringify(
                {
                    "model": "gpt-6-luna",
                    "input": "Production is down.",
                    "questions": [
                        {
                            "type": "choice",
                            "name": "route",
                            "instructions": "Who owns the escalation?",
                            "choices": [
                                { "value": "Billing", "description": "Payments and refunds" },
                                { "value": "tech", "description": "Bugs and outages" },
                                { "value": "Account" },
                            ],
                        },
                    ],
                },
            ),
            "timeout": null,
            "connect_timeout": null,
        },
    );
    let custom = openai.DecisionsClient.new(
        model = "custom-model",
        api_key = "secret",
        base_url = "http://localhost:1234",
    );
    let custom_req = custom.render(input(reflect.Type.of<bool>()));
    assert.equal(
        custom_req.to_json(),
        {
            "method": "POST",
            "url": "http://localhost:1234/decisions",
            "headers": { "content-type": "application/json" },
            "body": baml.json.stringify(
                {
                    "model": "custom-model",
                    "input": "Production is down.",
                    "questions": [
                        {
                            "type": "predicate",
                            "name": "result",
                            "instructions": "Does this ticket require immediate attention?",
                        },
                    ],
                },
            ),
            "timeout": null,
            "connect_timeout": null,
        },
    )
}

test "enum descriptions, aliases and skips become choice options" {
    let req = openai.DecisionsClient.new().render(input(reflect.Type.of<Team>(), instructions = false));
    assert.equal(
        baml.json.parse(req.body),
        {
            "model": "gpt-6-luna",
            "input": "Production is down.",
            "questions": [
                {
                    "type": "choice",
                    "name": "result",
                    "instructions": "Which team should handle this ticket?",
                    "choices": [
                        { "value": "Billing", "description": "Payments and refunds" },
                        { "value": "tech", "description": "Bugs and outages" },
                        { "value": "Account" },
                    ],
                },
            ],
        },
    )
}

function nested_result() -> bool {
    let body = baml.json.stringify(
        {
            "answers": [
                { "name": "mood", "type": "choice", "choice": "angry" },
                { "name": "agent.churn", "type": "predicate", "probability": 0.2 },
                { "name": "route", "type": "choice", "choice": "tech" },
                { "name": "customer.angry", "type": "predicate", "probability": 0.9 },
                { "name": "customer.churn", "type": "predicate", "probability": 0.8 },
                { "name": "agent.angry", "type": "predicate", "probability": 0.1 },
            ],
        },
    );
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(body)], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let cl = openai.DecisionsClient.new(api_key = "k", base_url = mock.base_url());
        let result = ai.Agent.new(client = cl).run(Assess@spec("outage")).value;
        assert.equal(
            result.to_json(),
            {
                "route": "Technical",
                "customer": { "angry": true, "churn": 0.8 },
                "agent": { "angry": false, "churn": 0.2 },
                "mood": "angry",
            },
        );
        assert.equal(
            mock.last_request().json(),
            {
                "model": "gpt-6-luna",
                "input": "outage",
                "questions": [
                    {
                        "type": "choice",
                        "name": "route",
                        "instructions": "You are triaging support tickets.\n\ndepartment: Which team should handle this ticket?",
                        "choices": [
                            { "value": "Billing", "description": "Payments and refunds" },
                            { "value": "tech", "description": "Bugs and outages" },
                            { "value": "Account" },
                        ],
                    },
                    {
                        "type": "predicate",
                        "name": "customer.angry",
                        "instructions": "You are triaging support tickets.\n\nthe customer.angry: Is the customer angry?",
                    },
                    {
                        "type": "predicate",
                        "name": "customer.churn",
                        "instructions": "You are triaging support tickets.\n\nthe customer.churn: How likely is cancellation?",
                    },
                    {
                        "type": "predicate",
                        "name": "agent.angry",
                        "instructions": "You are triaging support tickets.\n\nthe agent.angry: Is the customer angry?",
                    },
                    {
                        "type": "predicate",
                        "name": "agent.churn",
                        "instructions": "You are triaging support tickets.\n\nthe agent.churn: How likely is cancellation?",
                    },
                    {
                        "type": "choice",
                        "name": "mood",
                        "instructions": "You are triaging support tickets.\n\nWhat is the mood?",
                        "choices": [{ "value": "calm" }, { "value": "angry" }],
                    },
                ],
            },
        );
        true
    })
}

test "nested typed outputs reconstruct by answer name and preserve metadata" {
    assert.is_true(nested_result())
}

function skipped_result() -> bool {
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(`{"answers":[{"name":"route","type":"choice","choice":"tech"}]}`)], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let cl = openai.DecisionsClient.new(api_key = "k", base_url = mock.base_url());
        let value = ai.Agent.new(client = cl).run(SkipFields@spec("outage")).value;
        assert.equal(
            value.to_json(),
            { "route": "Technical", "notes": null, "internal": null, "flag": null, "media": null },
        );
        assert.equal(
            mock.last_request().json(),
            {
                "model": "gpt-6-luna",
                "input": "outage",
                "questions": [
                    {
                        "type": "choice",
                        "name": "route",
                        "instructions": "Which team should handle this ticket?",
                        "choices": [
                            { "value": "Billing", "description": "Payments and refunds" },
                            { "value": "tech", "description": "Bugs and outages" },
                            { "value": "Account" },
                        ],
                    },
                ],
            },
        );
        true
    })
}

test "skipped nullable fields are omitted and reconstructed as null" {
    assert.is_true(skipped_result())
}

function malformed(body: string, ty: reflect.Type? = null) -> bool {
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(body)], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let cl = openai.DecisionsClient.new(api_key = "k", base_url = mock.base_url());
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

test "malformed answers fail without inventing outputs" {
    assert.is_true(malformed(`{"answers":[]}`));
    assert.is_true(malformed(`{"answers":[{"type":"predicate","probability":0.9}]}`));
    assert.is_true(malformed(`{"answers":[{"name":"wrong","type":"predicate","probability":0.9}]}`));
    assert.is_true(malformed(`{"answers":[{"name":"result","type":"choice","probability":0.9}]}`));
    assert.is_true(malformed(`{"answers":[{"name":"result","type":"predicate"}]}`));
    assert.is_true(malformed(predicate_body(1.1)));
    assert.is_true(malformed(predicate_body(-0.1)));
    assert.is_true(malformed(choice_body("not-offered"), ty = reflect.Type.of<Team>()));
    assert.is_true(
        malformed(
            `{"answers":[{"name":"route","type":"choice","choice":"tech"},{"name":"route","type":"choice","choice":"tech"}]}`,
            ty = reflect.Type.of<Judged<Team>>(),
        ),
    );
    assert.is_true(malformed(`{"answers":{}}`))
}

function refuses() -> bool {
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(`{"answers":[{"name":"result","type":"refusal"}]}`)], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let value = openai.DecisionsClient.new(api_key = "k", base_url = mock.base_url()).invoke(
            input(reflect.Type.of<bool>()),
        ) catch (e) {
            ai.errors.Refused => {
                assert.equal(
                    e.to_json(),
                    {
                        "provider": "openai-decisions",
                        "reason": "Decisions refused question result",
                        "raw_body": null,
                    },
                );
                return true;
            },
        };
        false
    })
}

test "model refusals are typed" {
    assert.is_true(refuses())
}

function MediaInput(media: image | audio | video | pdf) -> bool {
    client: "openai-decisions/gpt-6-luna"
    prompt: `
        ${role("instructions")}
        Is there visible damage?
        ${role("user")}
        Inspect this product: ${media}
    `
}

function media_input(media: image | audio | video | pdf) -> ai.ModelTurnInput {
    let spec = MediaInput@spec(media);
    ai.ModelTurnInput {
        prompt: spec.prompt_template,
        output_type: reflect.Type.of<bool>(),
        journal: ai.Journal { log: [] },
        toolbox: ai.tools.Toolbox.new([]),
    }
}

function inline_image() -> bool {
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(predicate_body(0.9))], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let cl = openai.DecisionsClient.new(api_key = "k", base_url = mock.base_url());
        let media = baml.media.Image.from_base64("YWJj", "image/png");
        let turn = cl.invoke(media_input(media));
        assert.equal(turn.parsed_output?.value, true);
        assert.equal(
            mock.last_request().json(),
            {
                "model": "gpt-6-luna",
                "input": [
                    {
                        "role": "user",
                        "content": [
                            { "type": "input_text", "text": "Inspect this product:" },
                            { "type": "input_image", "image_url": "data:image/png;base64,YWJj" },
                        ],
                    },
                ],
                "questions": [
                    {
                        "type": "predicate",
                        "name": "result",
                        "instructions": "Is there visible damage?",
                    },
                ],
            },
        );
        assert.equal(cl.render(media_input(media)).body, mock.last_request().body);
        true
    })
}

test "inline image evidence and pure preview" {
    assert.is_true(inline_image())
}

function rejects_media(media: image | audio | video | pdf) -> bool {
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(predicate_body(0.9))], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let result = openai.DecisionsClient.new(api_key = "k", base_url = mock.base_url()).invoke(
            media_input(media),
        ) catch (e) {
            ai.errors.InvalidRequest => {
                assert.equal(mock.request_count(), 0);
                return true;
            },
        };
        false
    })
}

test "unsupported media is rejected before HTTP" {
    assert.is_true(rejects_media(baml.media.Audio.from_base64("QUJD", "audio/mpeg")));
    assert.is_true(rejects_media(baml.media.Video.from_base64("QUJD", "video/mp4")));
    assert.is_true(rejects_media(baml.media.Pdf.from_base64("QUJD", "application/pdf")))
}

test "remote image preview never downloads bytes" {
    let cl = openai.DecisionsClient.new();
    let result = cl.render(media_input(baml.media.Image.from_url("https://example.invalid/image.png", null))) catch (e) {
        ai.errors.PreviewUnsupported => {
            return;
        },
    };
    throw "expected PreviewUnsupported"
}

function wire_capture() -> bool {
    let body = predicate_body(0.9);
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(body)], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let cl = openai.DecisionsClient.new(api_key = "private-key", base_url = mock.base_url());
        let turn = cl.invoke(input(reflect.Type.of<bool>()));
        assert.equal(turn.calls.at(0)?.http_response?.body, body);
        assert.equal(turn.calls.at(0)?.http_request?.headers.get("authorization"), "REDACTED");
        let quiet = openai.DecisionsClient.new(
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
        let cl = openai.DecisionsClient.new(api_key = "k", base_url = mock.base_url());
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

function decisions_tool() -> string {
    "unused"
}

test "journal evidence preserves prompt, user and assistant order" {
    let turn = input(reflect.Type.of<bool>());
    let history = ai.Journal {
        log: [
            ai.events.UserMessage.new("follow-up"),
            ai.events.AssistantMessage {
                content: [ai.content.Text { text: "earlier answer" }],
                client_id: "previous",
            },
        ],
    };
    let req = openai.DecisionsClient.new().render(
        ai.ModelTurnInput {
            prompt: turn.prompt,
            output_type: turn.output_type,
            journal: history,
            toolbox: turn.toolbox,
        },
    );
    assert.equal(
        baml.json.path<string>(baml.json.parse(req.body), ".input"),
        "Production is down.\n\nfollow-up\n\nearlier answer",
    )
}

test "tools are rejected before HTTP" {
    let turn = input(reflect.Type.of<bool>());
    let request = openai.DecisionsClient.new().render(
        ai.ModelTurnInput {
            prompt: turn.prompt,
            output_type: turn.output_type,
            journal: turn.journal,
            toolbox: ai.tools.Toolbox.new([ai.tools.tool(decisions_tool)]),
        },
    ) catch (e) {
        ai.errors.InvalidRequest => {
            return;
        },
    };
    throw "expected InvalidRequest"
}

test "tool history is rejected before HTTP" {
    let turn = input(reflect.Type.of<bool>());
    let request = openai.DecisionsClient.new().render(
        ai.ModelTurnInput {
            prompt: turn.prompt,
            output_type: turn.output_type,
            journal: ai.Journal { log: [ai.events.ToolCompleted.new("id", "result")] },
            toolbox: turn.toolbox,
        },
    ) catch (e) {
        ai.errors.InvalidRequest => {
            return;
        },
    };
    throw "expected InvalidRequest"
}

function fetched_image() -> bool {
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.with_headers(200, "abc", { "content-type": "image/png" }), root.llm_mock.MockResponse.ok(predicate_body(0.9))], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let cl = openai.DecisionsClient.new(api_key = "k", base_url = mock.base_url());
        let media = baml.media.Image.from_url(`${mock.base_url()}/photo.png`, null);
        let turn = cl.invoke(media_input(media));
        assert.equal(turn.parsed_output?.value, true);
        assert.equal(mock.request_count(), 2);
        let download = mock.requests.at(0);
        assert.equal(
            download.to_json(),
            {
                "method": "GET",
                "url": "/photo.png",
                "headers": { "accept": "*/*", "host": mock.server.addr },
                "body": "",
            },
        );
        assert.equal(
            mock.last_request().json(),
            {
                "model": "gpt-6-luna",
                "input": [
                    {
                        "role": "user",
                        "content": [
                            { "type": "input_text", "text": "Inspect this product:" },
                            { "type": "input_image", "image_url": "data:image/png;base64,YWJj" },
                        ],
                    },
                ],
                "questions": [
                    {
                        "type": "predicate",
                        "name": "result",
                        "instructions": "Is there visible damage?",
                    },
                ],
            },
        );
        true
    })
}

test "remote images are fetched and inlined on invocation" {
    assert.is_true(fetched_image())
}

function usage_breakdown() -> bool {
    let body = baml.json.stringify(
        {
            "answers": [{ "name": "result", "type": "predicate", "probability": 0.9 }],
            "usage": {
                "input_tokens": 100,
                "output_tokens": 10,
                "input_tokens_details": { "cached_tokens": 25, "cache_write_tokens": 15 },
                "output_tokens_details": { "reasoning_tokens": 7 },
            },
        },
    );
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(body)], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let turn = openai.DecisionsClient.new(api_key = "k", base_url = mock.base_url()).invoke(
            input(reflect.Type.of<bool>()),
        );
        assert.equal(
            turn.usage.to_json(),
            {
                "input_tokens": 100,
                "output_tokens": 10,
                "cached_input_tokens": 25,
                "reasoning_tokens": 7,
                "cache_write_input_tokens": 15,
                "uncached_input_tokens": 60,
            },
        );
        true
    })
}

test "reported token usage breakdown is preserved" {
    assert.is_true(usage_breakdown())
}

test "trailing slash base URL overrides use a single route separator" {
    let cl = openai.DecisionsClient.new(base_url = "https://gateway.example/v1/");
    assert.equal(
        cl.render(input(reflect.Type.of<bool>())).url,
        "https://gateway.example/v1/decisions",
    )
}
