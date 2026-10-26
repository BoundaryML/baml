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

/// Seventeen leaves: one more than a Drex request carries.
class Wide {
    q1: bool @description("1"),
    q2: bool @description("2"),
    q3: bool @description("3"),
    q4: bool @description("4"),
    q5: bool @description("5"),
    q6: bool @description("6"),
    q7: bool @description("7"),
    q8: bool @description("8"),
    q9: bool @description("9"),
    q10: bool @description("10"),
    q11: bool @description("11"),
    q12: bool @description("12"),
    q13: bool @description("13"),
    q14: bool @description("14"),
    q15: bool @description("15"),
    q16: bool @description("16"),
    q17: bool @description("17"),
}

function SkipFields(message: string) -> WithSkipped {
    client: "naceai/drex-max"
    prompt: `
        ${message}
    `
}

function Urgent(message: string) -> bool {
    client: "naceai/drex-max"
    prompt: `
        ${role("instructions")}

        Does this ticket require immediate attention?

        ${role("user")}

        ${message}
    `
}

function Probability(message: string) -> float {
    client: naceai.Client.new(model = "drex-v2.0")
    prompt: `
        ${role("instructions")}

        How likely is urgency?

        ${role("user")}

        ${message}
    `
}

function Route(message: string) -> Team {
    client: "naceai/drex-max"
    prompt: `
        ${message}
    `
}

function Assess(message: string) -> Report {
    client: "naceai/drex-max"
    prompt: `
        ${role("instructions")}

        You are triaging support tickets.

        ${role("user")}

        ${message}
    `
}

function Generic(message: string) -> Judged<Team> {
    client: "naceai/drex-max"
    prompt: `
        ${message}
    `
}

function Optional(message: string) -> Team? {
    client: "naceai/drex-max"
    prompt: `
        ${role("instructions")}

        Pick a team, if any.

        ${role("user")}

        ${message}
    `
}

function Collide(message: string) -> Team | "tech" {
    client: "naceai/drex-max"
    prompt: `
        ${role("instructions")}

        Pick an option.

        ${role("user")}

        ${message}
    `
}

function Bare(message: string) -> bool {
    client: "naceai/drex-max"
    prompt: `
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

function rejects(ty: reflect.Type, instructions: bool = true) -> bool {
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok("{}")], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let cl = naceai.Client.new(api_key = "k", base_url = mock.base_url());
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
    assert.is_true(rejects(string.type()))
}

test "reject int" {
    assert.is_true(rejects(int.type()))
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

test "reject optional class" {
    assert.is_true(rejects(reflect.Type.of<Report?>()))
}

test "reject class union" {
    assert.is_true(rejects(reflect.Type.of<Report | Sentiment>()))
}

test "reject unsupported leaf" {
    assert.is_true(rejects(WithNotes.type()))
}

test "reject recursive class" {
    assert.is_true(rejects(Node.type()))
}

test "reject image output" {
    assert.is_true(rejects(image.type()))
}

test "reject empty instructions" {
    assert.is_true(rejects(bool.type(), instructions = false))
}

test "reject singleton" {
    assert.is_true(rejects(reflect.Type.of<"approved">()))
}

test "reject empty class" {
    assert.is_true(rejects(Empty.type()))
}

test "reject more than 16 questions" {
    assert.is_true(rejects(Wide.type()))
}

function noul_body(n: float) -> string {
    baml.json.stringify(
        {
            "model": "drex-max",
            "answers": { "result": { "type": "noul", "noul": n } },
            "usage": { "input_tokens": 142, "output_tokens": 9 },
            "evaluation_time_ms": 180.8,
            "request_id": "req_test",
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

function noul(n: float, ty: reflect.Type) -> unknown {
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(noul_body(n))], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let cl = naceai.Client.new(api_key = "test-key", base_url = mock.base_url());
        let turn = cl.invoke(input(ty));
        assert.equal(turn.calls.length(), 1);
        assert.equal(
            turn.calls[0].resolved_model.to_json(),
            { "provider": "naceai", "model": "drex-max" },
        );
        assert.equal(
            turn.calls[0].usage.to_json(),
            {
                "input_tokens": 142,
                "output_tokens": 9,
                "cached_input_tokens": null,
                "reasoning_tokens": null,
                "cache_write_input_tokens": null,
                "uncached_input_tokens": 142,
            },
        );
        assert.equal(turn.resolved_model?.model, "drex-max");
        let sent = mock.last_request();
        assert.equal(sent.path(), "/systemone");
        assert.equal(sent.header("authorization"), "Bearer test-key");
        assert.equal(
            sent.json(),
            {
                "model": "drex-max",
                "state": "Production is down.",
                "questions": {
                    "result": {
                        "type": "noul",
                        "instructions": "Does this ticket require immediate attention?",
                    },
                },
            },
        );
        turn.parsed_output?.value
    })
}

test "noul request, usage, thresholds and probability outputs" {
    assert.equal(noul(0.0, bool.type()), false);
    assert.equal(noul(0.49, bool.type()), false);
    assert.equal(noul(0.5, bool.type()), true);
    assert.equal(noul(1.0, bool.type()), true);
    assert.equal(noul(0.72, float.type()), 0.72)
}

function choose(ty: reflect.Type, key: string) -> unknown {
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(choice_body(key))], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        naceai.Client.new(api_key = "k", base_url = mock.base_url())
            .invoke(input(ty))
            .parsed_output?.value
    })
}

test "enum aliases, finite unions, optional null and bigint choices" {
    assert.equal(choose(Team.type(), "tech"), Team.Technical);
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
    let cl = naceai.Client.new();
    assert.is_type<baml.env.Ref>(cl.api_key);
    assert.equal(
        cl.to_json(),
        {
            "model": "drex-max",
            "api_key": { "name": "NACE_API_KEY" },
            "base_url": "https://console.nace.ai/v1",
            "timeout": null,
            "capture_wire": true,
        },
    );
    assert.equal(cl.requested_model(), "drex-max");
    assert.equal(Urgent@spec("x").requested_model(), cl.requested_model());
    assert.equal(Probability@spec("x").requested_model(), "drex-v2.0");
    let provider = "naceai";
    assert.equal(ai.clients.resolve(`${provider}/drex-v2.0`).requested_model(), "drex-v2.0");
    let req = cl.render(input(Override.type(), instructions = false));
    assert.equal(
        req.to_json(),
        {
            "method": "POST",
            "url": "https://console.nace.ai/v1/systemone",
            "headers": { "content-type": "application/json" },
            "body": baml.json.stringify(
                {
                    "model": "drex-max",
                    "state": "Production is down.",
                    "questions": {
                        "route": {
                            "type": "choice",
                            "instructions": "Who owns the escalation?",
                            "criteria": {
                                "Billing": "Payments and refunds",
                                "tech": "Bugs and outages",
                                "Account": null,
                            },
                        },
                    },
                },
            ),
            "timeout": null,
            "connect_timeout": null,
        },
    );
    let custom = naceai.Client.new(model = "drex-v2.0", api_key = "secret", base_url = "http://localhost:1234");
    let custom_req = custom.render(input(bool.type()));
    assert.equal(
        custom_req.to_json(),
        {
            "method": "POST",
            "url": "http://localhost:1234/systemone",
            "headers": { "content-type": "application/json" },
            "body": baml.json.stringify(
                {
                    "model": "drex-v2.0",
                    "state": "Production is down.",
                    "questions": {
                        "result": {
                            "type": "noul",
                            "instructions": "Does this ticket require immediate attention?",
                        },
                    },
                },
            ),
            "timeout": null,
            "connect_timeout": null,
        },
    )
}

test "enum descriptions, aliases and skips become criteria" {
    let req = naceai.Client.new().render(input(Team.type(), instructions = false));
    assert.equal(
        baml.json.parse(req.body),
        {
            "model": "drex-max",
            "state": "Production is down.",
            "questions": {
                "result": {
                    "type": "choice",
                    "instructions": "Which team should handle this ticket?",
                    "criteria": {
                        "Billing": "Payments and refunds",
                        "tech": "Bugs and outages",
                        "Account": null,
                    },
                },
            },
        },
    )
}

function nested_result() -> bool {
    let body = baml.json.stringify(
        {
            "answers": {
                "mood": { "type": "choice", "choice": "angry" },
                "agent.churn": { "type": "noul", "noul": 0.2 },
                "route": { "type": "choice", "choice": "tech" },
                "customer.angry": { "type": "noul", "noul": 0.9 },
                "customer.churn": { "type": "noul", "noul": 0.8 },
                "agent.angry": { "type": "noul", "noul": 0.1 },
            },
        },
    );
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(body)], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let cl = naceai.Client.new(api_key = "k", base_url = mock.base_url());
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
                "model": "drex-max",
                "state": "outage",
                "questions": {
                    "route": {
                        "type": "choice",
                        "instructions": "You are triaging support tickets.\n\ndepartment: Which team should handle this ticket?",
                        "criteria": {
                            "Billing": "Payments and refunds",
                            "tech": "Bugs and outages",
                            "Account": null,
                        },
                    },
                    "customer.angry": {
                        "type": "noul",
                        "instructions": "You are triaging support tickets.\n\nthe customer.angry: Is the customer angry?",
                    },
                    "customer.churn": {
                        "type": "noul",
                        "instructions": "You are triaging support tickets.\n\nthe customer.churn: How likely is cancellation?",
                    },
                    "agent.angry": {
                        "type": "noul",
                        "instructions": "You are triaging support tickets.\n\nthe agent.angry: Is the customer angry?",
                    },
                    "agent.churn": {
                        "type": "noul",
                        "instructions": "You are triaging support tickets.\n\nthe agent.churn: How likely is cancellation?",
                    },
                    "mood": {
                        "type": "choice",
                        "instructions": "You are triaging support tickets.\n\nWhat is the mood?",
                        "criteria": { "calm": null, "angry": null },
                    },
                },
            },
        );
        true
    })
}

test "nested typed outputs reconstruct by answer id and preserve metadata" {
    assert.is_true(nested_result())
}

function skipped_result() -> bool {
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(`{"answers":{"route":{"type":"choice","choice":"tech"}}}`)], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let cl = naceai.Client.new(api_key = "k", base_url = mock.base_url());
        let value = ai.Agent.new(client = cl).run(SkipFields@spec("outage")).value;
        assert.equal(
            value.to_json(),
            { "route": "Technical", "notes": null, "internal": null, "flag": null, "media": null },
        );
        assert.equal(
            mock.last_request().json(),
            {
                "model": "drex-max",
                "state": "outage",
                "questions": {
                    "route": {
                        "type": "choice",
                        "instructions": "Which team should handle this ticket?",
                        "criteria": {
                            "Billing": "Payments and refunds",
                            "tech": "Bugs and outages",
                            "Account": null,
                        },
                    },
                },
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
        let cl = naceai.Client.new(api_key = "k", base_url = mock.base_url());
        let result = cl.invoke(input(ty ?? bool.type())).terminal_text() catch_all (e) {
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
    assert.is_true(malformed(`{"answers":{}}`));
    assert.is_true(malformed(`{"answers":[]}`));
    assert.is_true(malformed(`{"answers":{"wrong":{"type":"noul","noul":0.9}}}`));
    assert.is_true(malformed(`{"answers":{"result":{"type":"choice","noul":0.9}}}`));
    assert.is_true(malformed(`{"answers":{"result":{"type":"noul"}}}`));
    assert.is_true(malformed(noul_body(1.1)));
    assert.is_true(malformed(noul_body(-0.1)));
    assert.is_true(malformed(choice_body("not-offered"), ty = Team.type()));
    assert.is_true(
        malformed(`{"answers":{"route":{"type":"choice","choice":"tech"}}}`, ty = Judged<Team>.type()),
    )
}

function tolerates_extras() -> bool {
    // A live Drex reply: confidence and probabilities on every answer, an
    // array-valued legend on score answers, timing and request ids.
    let body = baml.json.stringify(
        {
            "model": "drex-max",
            "answers": {
                "result": { "type": "noul", "noul": 0.9, "confidence": 0.99 },
                "grade": {
                    "type": "score",
                    "score": 0.93,
                    "legend": ["unclear", "partly clear", "clear"],
                    "probabilities": { "0": 0.133, "1": 0.803, "2": 0.063 },
                    "confidence": 0.705,
                },
            },
            "usage": { "input_tokens": 307, "output_tokens": 311 },
            "evaluation_time_ms": 180.8,
            "request_id": "req_test",
        },
    );
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(body)], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let turn = naceai.Client.new(api_key = "k", base_url = mock.base_url()).invoke(input(bool.type()));
        assert.equal(turn.parsed_output?.value, true);
        assert.equal(turn.resolved_model?.model, "drex-max");
        assert.equal(turn.calls[0].usage?.input_tokens, 307);
        true
    })
}

test "extra answers, array legends and reply metadata are ignored" {
    assert.is_true(tolerates_extras())
}

function MediaInput(media: image | audio | video | pdf) -> bool {
    client: "naceai/drex-max"
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
        output_type: bool.type(),
        journal: ai.Journal { log: [] },
        toolbox: ai.tools.Toolbox.new([]),
    }
}

function inline_image() -> bool {
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(noul_body(0.9))], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let cl = naceai.Client.new(api_key = "k", base_url = mock.base_url());
        let media = baml.media.Image.from_base64("YWJj", "image/png");
        let turn = cl.invoke(media_input(media));
        assert.equal(turn.parsed_output?.value, true);
        assert.equal(
            mock.last_request().json(),
            {
                "model": "drex-max",
                "state": "Inspect this product: Image 1",
                "questions": { "result": { "type": "noul", "instructions": "Is there visible damage?" } },
                "images": ["data:image/png;base64,YWJj"],
            },
        );
        assert.equal(cl.render(media_input(media)).body, mock.last_request().body);
        true
    })
}

test "inline image evidence and pure preview" {
    assert.is_true(inline_image())
}

test "images are numbered in request order across prompt and journal" {
    let turn = media_input(baml.media.Image.from_base64("YWJj", "image/png"));
    let journal = ai.Journal {
        log: [
            ai.events.UserMessage.of(
                ["Compare with", baml.media.Image.from_base64("ZGVm", "image/jpeg")],
            ),
            ai.events.UserMessage.new("Which looks newer?"),
        ],
    };
    let req = naceai.Client.new().render(
        ai.ModelTurnInput {
            prompt: turn.prompt,
            output_type: turn.output_type,
            journal: journal,
            toolbox: turn.toolbox,
        },
    );
    assert.equal(
        baml.json.parse(req.body),
        {
            "model": "drex-max",
            "state": "Inspect this product: Image 1\n\nCompare with Image 2\n\nWhich looks newer?",
            "questions": { "result": { "type": "noul", "instructions": "Is there visible damage?" } },
            "images": ["data:image/png;base64,YWJj", "data:image/jpeg;base64,ZGVm"],
        },
    )
}

function rejects_media(media: image | audio | video | pdf) -> bool {
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(noul_body(0.9))], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let result = naceai.Client.new(api_key = "k", base_url = mock.base_url()).invoke(media_input(media)) catch (e) {
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

test "more than 8 images are rejected before HTTP" {
    let turn = media_input(baml.media.Image.from_base64("YWJj", "image/png"));
    let extra = ai.events.UserMessage.of(["Another angle:", baml.media.Image.from_base64("YWJj", "image/png")]);
    let journal = ai.Journal { log: [extra, extra, extra, extra, extra, extra, extra, extra] };
    let request = naceai.Client.new().render(
        ai.ModelTurnInput {
            prompt: turn.prompt,
            output_type: turn.output_type,
            journal: journal,
            toolbox: turn.toolbox,
        },
    ) catch (e) {
        ai.errors.InvalidRequest => {
            return;
        },
    };
    throw "expected InvalidRequest"
}

test "remote image preview never downloads bytes" {
    let cl = naceai.Client.new();
    let result = cl.render(media_input(baml.media.Image.from_url("https://example.invalid/image.png", null))) catch (e) {
        ai.errors.PreviewUnsupported => {
            return;
        },
    };
    throw "expected PreviewUnsupported"
}

function fetched_image() -> bool {
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.with_headers(200, "abc", { "content-type": "image/png" }), root.llm_mock.MockResponse.ok(noul_body(0.9))], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let cl = naceai.Client.new(api_key = "k", base_url = mock.base_url());
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
                "model": "drex-max",
                "state": "Inspect this product: Image 1",
                "questions": { "result": { "type": "noul", "instructions": "Is there visible damage?" } },
                "images": ["data:image/png;base64,YWJj"],
            },
        );
        true
    })
}

test "remote images are fetched and inlined on invocation" {
    assert.is_true(fetched_image())
}

function wire_capture() -> bool {
    let body = noul_body(0.9);
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(body)], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let cl = naceai.Client.new(api_key = "private-key", base_url = mock.base_url());
        let turn = cl.invoke(input(bool.type()));
        assert.equal(turn.calls.at(0)?.http_response?.body, body);
        assert.equal(turn.calls.at(0)?.http_request?.headers.get("authorization"), "REDACTED");
        let quiet = naceai.Client.new(api_key = "private-key", base_url = mock.base_url(), capture_wire = false);
        let no_body = quiet.invoke(input(bool.type()));
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
        let cl = naceai.Client.new(api_key = "k", base_url = mock.base_url());
        let result = cl.invoke(input(bool.type())).terminal_text() catch_all (e) {
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

function naceai_tool() -> string {
    "unused"
}

test "journal evidence preserves prompt, user and assistant order" {
    let turn = input(bool.type());
    let history = ai.Journal {
        log: [
            ai.events.UserMessage.new("follow-up"),
            ai.events.AssistantMessage {
                content: [ai.content.Text { text: "earlier answer" }],
                resolved_model: ai.events.ModelKey { provider: "naceai", model: "previous" },
                requested_model: "previous",
            },
        ],
    };
    let req = naceai.Client.new().render(
        ai.ModelTurnInput {
            prompt: turn.prompt,
            output_type: turn.output_type,
            journal: history,
            toolbox: turn.toolbox,
        },
    );
    assert.equal(
        baml.json.path<string>(baml.json.parse(req.body), ".state"),
        "Production is down.\n\nfollow-up\n\nearlier answer",
    )
}

test "tools are rejected before HTTP" {
    let turn = input(bool.type());
    let request = naceai.Client.new().render(
        ai.ModelTurnInput {
            prompt: turn.prompt,
            output_type: turn.output_type,
            journal: turn.journal,
            toolbox: ai.tools.Toolbox.new([ai.tools.tool(naceai_tool)]),
        },
    ) catch (e) {
        ai.errors.InvalidRequest => {
            return;
        },
    };
    throw "expected InvalidRequest"
}

test "tool history is rejected before HTTP" {
    let turn = input(bool.type());
    let request = naceai.Client.new().render(
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

test "trailing slash base URL overrides use a single route separator" {
    let cl = naceai.Client.new(base_url = "https://gateway.example/v1/");
    assert.equal(cl.render(input(bool.type())).url, "https://gateway.example/v1/systemone")
}
