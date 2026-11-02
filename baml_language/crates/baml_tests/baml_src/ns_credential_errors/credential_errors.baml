// Credential material never reaches an error's text. Users put credential
// documents where a path is expected, and errors reach application logs, so
// each case asserts what an error must NOT contain. Offline: no case gets as
// far as a request.

function CredentialSpec() -> string {
    client: "openai/gpt-4o-mini"
    prompt: `Say pong. ${ctx.output_format()}`
}

function credential_input() -> ai.ModelTurnInput {
    let spec = CredentialSpec@spec();
    ai.ModelTurnInput {
        prompt: spec.prompt_template,
        journal: ai.Journal.new(spec),
        toolbox: spec.tools(),
        output_type: spec.output_type(),
    }
}

// A service-account document whose key is not a key: minting fails before
// any request is sent.
function fake_service_account() -> string {
    let document: map<string, string> = {
        "type": "service_account",
        "project_id": "fake-project",
        "private_key_id": "fake-key-id-1234",
        "private_key": "-----BEGIN PRIVATE KEY-----\nFAKEKEYMATERIALDONOTUSE\n-----END PRIVATE KEY-----\n",
        "client_email": "fake@fake-project.iam.gserviceaccount.com",
        "token_uri": "https://oauth2.googleapis.com/token",
    };
    baml.json.stringify(document)
}

// Every part of `fake_service_account()` that `text` carries.
function leaked(text: string) -> string[] {
    let secrets = ["PRIVATE KEY", "FAKEKEYMATERIALDONOTUSE", "fake-key-id-1234", "fake@fake-project"];
    secrets.filter((secret: string) -> bool {
        text.includes(secret)
    })
}

function failure_text(error: unknown) -> string {
    match (error) {
        let invalid: ai.errors.InvalidRequest => `InvalidRequest:${invalid.detail}`,
        _ => `other:${error.to_string()}`,
    }
}

function vertex_failure(client: google.VertexClient) -> string {
    let outcome = client.invoke(credential_input()) catch_all (e) {
        _ => failure_text(e),
    };
    match (outcome) {
        let text: string => text,
        let turn: ai.ModelTurn => "unexpected success",
    }
}

test "vertex_credentials_file_holding_a_document_is_described_not_quoted" {
    let text = vertex_failure(
        google.VertexClient.new(
            location = "us-central1",
            project_id = "p",
            credentials_file = fake_service_account(),
        ),
    );
    assert.equal(leaked(text), []);
    assert.is_true(text.includes("holds a service_account credential document"))
}

test "vertex_credentials_file_path_is_still_shown" {
    let text = vertex_failure(
        google.VertexClient.new(
            location = "us-central1",
            project_id = "p",
            credentials_file = "/nonexistent/creds/key.json",
        ),
    );
    assert.is_true(text.includes("/nonexistent/creds/key.json"))
}

test "vertex_broken_credentials_json_stays_out_of_the_error" {
    let text = vertex_failure(google.VertexClient.new(location = "us-central1", credentials_json = fake_service_account()));
    assert.is_true(text.starts_with("InvalidRequest:Vertex AI authentication failed"));
    assert.equal(leaked(text), [])
}

class CannedClient {
    implements ai.Client {
        function requested_model(self) -> string {
            "canned"
        }
        function provider(self) -> string {
            "test"
        }

        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            let _ = input;
            baml.http.Request {
                method: "POST",
                url: "https://example.invalid/canned",
                headers: {},
                body: "{}",
            }
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            let _ = input;
            ai.ModelTurn {
                calls: [],
                child_turns: [],
                content: [ai.content.Text { text: "pong" }],
                stop_reason: ai.content.StopReason.Complete,
            }
        }
    }
}

// A Fallback that moves past a misconfigured member still returns that
// member's failure on the successful turn, where a host may log it.
test "fallback_keeps_a_skipped_members_failure_free_of_credentials" {
    let vertex = google.VertexClient.new(location = "us-central1", credentials_json = fake_service_account());
    let fallback = ai.clients.Fallback { members: [vertex, CannedClient {}] };
    let turn = fallback.invoke(credential_input());
    assert.equal(turn.terminal_text(), "pong");
    let skipped = turn.child_turns.at(0) ?? baml.sys.panic("the skipped member's turn");
    let text = match (skipped.error) {
        null => "no error recorded",
        let error: ai.errors.Failure | baml.errors.Timeout => failure_text(error),
    };
    assert.is_true(text.includes("Vertex AI authentication failed"));
    assert.equal(leaked(text), [])
}

test "captured_requests_redact_credential_headers" {
    let redacted = ai.wire.redact_request_headers(
        {
            "Authorization": "Bearer a",
            "ocp-apim-subscription-key": "b",
            "x-vendor-token": "c",
            "content-type": "application/json",
            "x-goog-user-project": "my-project",
        },
    );
    assert.equal(
        redacted,
        {
            "Authorization": "REDACTED",
            "ocp-apim-subscription-key": "REDACTED",
            "x-vendor-token": "REDACTED",
            "content-type": "application/json",
            "x-goog-user-project": "my-project",
        },
    )
}

function sse_failure_label(message: string) -> string {
    match (ai.wire.sse_failure("test", baml.errors.Io { message: message })) {
        let invalid: ai.errors.InvalidRequest => `InvalidRequest/${invalid.status_code ?? 0}`,
        let network: ai.errors.NetworkFailure => `NetworkFailure:${network.detail}`,
        _ => "other",
    }
}

// One classifier for every provider's stream opener: a rejected request keeps
// its status, and a transport error loses any credential in its URL.
test "sse_failures_classify_the_status_and_redact_the_url" {
    assert.equal(
        sse_failure_label("SSE request failed with status 401: {\"error\":\"bad key\"}"),
        "InvalidRequest/401",
    );
    assert.equal(
        sse_failure_label("error sending request for url (https://x/v1/m?alt=sse&key=SEKRIT)"),
        "NetworkFailure:transport failed before a response arrived: error sending request for url (https://x/v1/m?alt=sse&key=REDACTED)",
    )
}

// A document wrapped in a JSON string is still recognized, and a `type`
// google-auth does not define is not quoted either.
test "vertex_credentials_file_wrapped_or_unknown_documents_are_not_quoted" {
    let wrapped = vertex_failure(
        google.VertexClient.new(
            location = "us-central1",
            project_id = "p",
            credentials_file = baml.json.stringify(fake_service_account()),
        ),
    );
    assert.equal(leaked(wrapped), []);
    assert.is_true(wrapped.includes("holds a service_account credential document"));
    let unknown = vertex_failure(
        google.VertexClient.new(
            location = "us-central1",
            project_id = "p",
            credentials_file = "{\"type\":\"sekrit-type-value\",\"private_key\":\"-----BEGIN PRIVATE KEY-----\"}",
        ),
    );
    assert.equal(leaked(unknown), []);
    assert.is_true(!unknown.includes("sekrit-type-value"));
    assert.is_true(unknown.includes("holds a JSON document"))
}
