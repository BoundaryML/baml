class Answer {
    count: int,
}

function Extract() -> Answer {
    client: "openai/gpt-4o-mini"
    prompt: `Return a count. ${ctx.output_format()}`
}

function input() -> ai.ModelTurnInput {
    let spec = Extract@spec();
    ai.ModelTurnInput {
        prompt: spec.prompt_template,
        journal: ai.Journal.new(spec),
        toolbox: spec.tools(),
        output_type: spec.output_type(),
    }
}

function completed(text: string) -> string {
    baml.json.stringify(
        {
            "status": "completed",
            "output": [{ "type": "message", "content": [{ "type": "output_text", "text": text }] }],
        },
    )
}

function stream_schema_failure(drain_partials: bool) -> bool {
    let text = `{"count":"not a number"}`;
    root.llm_mock.mock_sse_serve([root.llm_mock.sse_data(baml.json.stringify({ "type": "response.output_text.delta", "delta": text })), root.llm_mock.sse_data(`{"type":"response.completed","response":{"status":"completed","output":[]}}`)], (
        mock: root.llm_mock.MockSseServer,
    ) -> {
        let cl = openai.ResponsesClient.new(api_key = "test", base_url = mock.base_url());
        let stream = ai.stream.from_spec(Extract@spec(), client = cl);
        let result = {
            if (drain_partials) {
                while (!(stream.next() is ai.stream.Done)) {}
            }
            stream.final();
            false
        } catch_all (e) {
            let failed: ai.errors.ParseFailed => {
                failed.provider == cl.id()
                    && failed.raw_output == text
                    && (failed.message ?? "") != ""
            },
            _ => false,
        };
        result
    })
}

test "stream final schema failure has AI classification and diagnostics" {
    assert.is_true(stream_schema_failure(false))
}

test "schema failure remains classified after partial consumption" {
    assert.is_true(stream_schema_failure(true))
}

test "direct SAP parsing retains its general parser error" {
    let result = baml.sap.parse<Answer>(`{"count":"not a number"}`) catch (e) {
        let parse: baml.errors.ParseError => {
            assert.is_true(parse.message != "");
            return;
        },
    };
    let _ = result;
    assert.is_true(false)
}

test "malformed HTTP 200 envelope retains body and actual status" {
    let raw = "not JSON";
    let preserved = root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(raw)], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let cl = openai.ResponsesClient.new(api_key = "test", base_url = mock.base_url());
        cl.invoke(input()) catch_all (e) {
            let invalid: ai.errors.InvalidResponse => {
                return invalid.raw_body == raw && invalid.status_code == 200 && invalid.message != "";
            },
            _ => {
                return false;
            },
        };
        false
    });
    assert.is_true(preserved)
}

function retry_reported_failure(code: string, expected_attempts: int) -> bool {
    let failure = baml.json.stringify(
        {
            "status": "failed",
            "error": { "code": code, "type": "provider_error", "message": "provider diagnostic" },
        },
    );
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(failure)], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let cl = ai.clients.Retry.new(
            openai.ResponsesClient.new(api_key = "test", base_url = mock.base_url()),
            max_attempts = 2,
            backoff = ai.clients.Backoff.new(initial_ms = 0, max_ms = 0),
        );
        let preserved = cl.invoke(input()) catch_all (e) {
            let reported: ai.errors.ProviderError => {
                return reported.error_code == code
                    && reported.error_type == "provider_error"
                    && reported.message == "provider diagnostic"
                    && reported.raw_body != null
                    && reported.status_code == null
                    && mock.request_count() == expected_attempts;
            },
            _ => {
                return false;
            },
        };
        let _ = preserved;
        false
    })
}

test "transient in-band provider failure retries and retains metadata" {
    assert.is_true(retry_reported_failure("server_error", 2))
}

test "permanent in-band provider failure is not retried" {
    assert.is_true(retry_reported_failure("invalid_prompt", 1))
}

test "normalization retains new error identities" {
    let invalid = ai.errors.InvalidResponse {
        provider: "test",
        message: "before",
        raw_body: "bad",
        status_code: 200,
    };
    let reported = ai.errors.ProviderError {
        provider: "test",
        message: "before",
        error_type: "overloaded",
        error_code: "server_error",
    };
    let normalized_invalid = ai.errors.normalize(invalid);
    let normalized_reported = ai.errors.normalize(reported);
    invalid.message = "after";
    reported.message = "after";
    match (normalized_invalid) {
        let same: ai.errors.InvalidResponse => assert.equal(same.message, "after"),
        _ => assert.is_true(false),
    };
    match (normalized_reported) {
        let same: ai.errors.ProviderError => assert.equal(same.message, "after"),
        _ => assert.is_true(false),
    }
}
