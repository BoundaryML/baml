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
        let preserved = {
            cl.invoke(input());
            false
        } catch_all (e) {
            let reported: ai.errors.ProviderError => {
                reported.error_code == code
                    && reported.error_type == "provider_error"
                    && reported.message == "provider diagnostic"
                    && reported.raw_body != null
                    && reported.status_code == null
                    && mock.request_count() == expected_attempts
            },
            _ => false,
        };
        preserved
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

// The EventStream transport turns :error-code/:error-message headers into a
// named event with plain-text data, not a modeled JSON exception payload.
test "Bedrock unmodeled error frame preserves plain text and native code" {
    let batch = baml.json.stringify([{ "event": "error:InternalFailure", "data": "stream failed" }]);
    aws.internal.decode_stream_batch(batch) catch_all (e) {
        let reported: ai.errors.ProviderError => {
            assert.equal(reported.message, "stream failed");
            assert.equal(reported.error_code, "InternalFailure");
            assert.equal(reported.error_type, "error:InternalFailure");
            assert.equal(reported.status_code, null);
            assert.is_true(ai.clients.default_retry_judgment(reported));
            return;
        },
        _ => assert.is_true(false),
    };
    assert.is_true(false)
}

function chat_reported_failure(streaming: bool) -> bool {
    let raw = `{"error":{"message":"overloaded","type":"server_error","code":503}}`;
    let result = {
        if (streaming) {
            let decoder = openai.internal.ChatStreamDecoder.new("openai-chat");
            decoder.decode(baml.json.stringify([{ "data": raw }]));
        } else {
            let response = baml.json.from_string<openai.internal.CcResponse>(raw);
            openai.internal.chat_parse(response, "openai-chat", raw);
        }
        false
    } catch_all (e) {
        let reported: ai.errors.ProviderError => {
            reported.provider == "openai-chat"
                && reported.message == "overloaded"
                && reported.error_type == "server_error"
                && reported.error_code == "503"
                && reported.raw_body == raw
                && reported.status_code == null
        },
        _ => false,
    };
    result
}

test "Chat unary in-band failure keeps provider metadata" {
    assert.is_true(chat_reported_failure(false))
}

test "Chat stream in-band failure keeps provider metadata" {
    assert.is_true(chat_reported_failure(true))
}

test "retry call retains status-less provider failure before success" {
    let raw = `{"status":"failed","error":{"code":"server_error","message":"overloaded"}}`;
    let preserved = root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(raw), root.llm_mock.MockResponse.ok(completed("ok"))], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let cl = ai.clients.Retry.new(
            openai.ResponsesClient.new(api_key = "test", base_url = mock.base_url()),
            max_attempts = 2,
            backoff = ai.clients.Backoff.new(initial_ms = 0, max_ms = 0),
        );
        let turn = cl.invoke(input());
        let failed = turn.calls.at(0);
        if let call: ai.events.LLMCall = failed {
            assert.is_true(!call.selected);
            assert.equal(call.http_response, null);
            match (call.failure) {
                let reported: ai.errors.ProviderError => {
                    return reported.message == "overloaded"
                        && reported.error_code == "server_error"
                        && reported.raw_body != null;
                },
                _ => {
                    return false;
                },
            };
        }
        false
    });
    assert.is_true(preserved)
}
