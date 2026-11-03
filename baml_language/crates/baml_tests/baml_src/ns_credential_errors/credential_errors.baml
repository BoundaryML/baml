// Credential material never reaches an error's text. Users put credential
// documents where a path is expected, and errors reach application logs, so
// each case asserts what an error must NOT contain. Offline: a case that sends
// a request sends it to a local mock server.

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
    assert.is_true(text.includes("Vertex AI credentials_file holds a service_account credential document (") && text.includes("pass the document through credentials_json instead"))
}

// A plain path to a `.json` file is named, with why it could not be read, as
// GOOGLE_APPLICATION_CREDENTIALS is; anything else is shown by its length only.
test "vertex_unreadable_credentials_file_names_only_a_path" {
    let path = vertex_failure(
        google.VertexClient.new(
            location = "us-central1",
            project_id = "p",
            credentials_file = "/nonexistent/creds/key.json",
        ),
    );
    assert.is_true(path.includes("Vertex AI credentials_file points to '/nonexistent/creds/key.json' but the file could not be read: "));
    let token = vertex_failure(
        google.VertexClient.new(location = "us-central1", project_id = "p", credentials_file = "ya29.a0-secret-token"),
    );
    assert.is_true(token.includes("credentials_file does not name a readable file (the value, 20 characters, is not shown"));
    assert.is_true(!token.includes("secret-token"))
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
    assert.is_true(!unknown.includes("PRIVATE KEY"));
    assert.is_true(unknown.includes("holds a JSON document"))
}

// ─── late-bound credential options ──────────────────────────────────────────
// These options are `ai.Credential`s: `env.NAME` is read when a request is
// built, and one whose variable is unset counts as not configured. PATH stands
// in for a variable that is always set; its value is not a credential, so
// each case fails before any request, and deterministically.

test "vertex_credentials_json_reads_its_env_ref_at_request_time" {
    let text = vertex_failure(
        google.VertexClient.new(
            location = "us-central1",
            project_id = "p",
            credentials_json = env.PATH,
        ),
    );
    // The error reports the length of the value it read, which is PATH's.
    let path = baml.env.get("PATH") ?? baml.sys.panic("PATH is unset");
    assert.is_true(text.includes(`is not a credential document: it is not a JSON object (${path.length()} characters, not shown)`));
    assert.is_true(!text.includes(path))
}

// An unset `credentials_json` moves on to `credentials_file`; the error from
// there says which option was skipped.
test "vertex_unset_credential_ref_is_named_in_the_next_sources_error" {
    let text = vertex_failure(
        google.VertexClient.new(
            location = "us-central1",
            project_id = "p",
            credentials_json = env.BAML_TEST_UNSET_GCP_SA_JSON,
            credentials_file = "/nonexistent/creds/key.json",
        ),
    );
    assert.is_true(text.includes("credentials_file points to '/nonexistent/creds/key.json' but the file could not be read"));
    assert.is_true(
        text.includes("(unset or blank, so not used: credentials_json (env.BAML_TEST_UNSET_GCP_SA_JSON))"),
    )
}

test "unset_credentials_note_names_only_unset_refs" {
    assert.equal(
        ai.wire.unset_credentials_note(
            { "a": env.BAML_TEST_UNSET_CREDENTIAL, "b": "literal", "c": null, "d": env.PATH },
        ),
        " (unset or blank, so not used: a (env.BAML_TEST_UNSET_CREDENTIAL))",
    );
    assert.equal(ai.wire.unset_credentials_note({ "b": "literal" }), "")
}

// A path put in `credentials_json` is named, with where it goes.
test "vertex_path_in_credentials_json_says_where_paths_go" {
    let text = vertex_failure(
        google.VertexClient.new(
            location = "us-central1",
            project_id = "p",
            credentials_json = "/creds/key.json",
        ),
    );
    assert.is_true(text.includes("it looks like the path '/creds/key.json', which goes in credentials_file"))
}

// With no project configured, the project lookup meets an unusable credential
// first and reports it as such, not as a missing project.
test "vertex_unusable_credentials_file_is_reported_before_the_project" {
    let text = vertex_failure(google.VertexClient.new(location = "us-central1", credentials_file = fake_service_account()));
    assert.is_true(text.includes("Vertex AI credentials_file holds a service_account credential document (") && text.includes("pass the document through credentials_json instead"));
    assert.equal(leaked(text), [])
}

// A region is not a secret, so a preview shows a configured one, from a
// literal or `env.NAME` alike; an unset one previews as the marker region.
test "bedrock_preview_shows_a_configured_region" {
    let unset = aws.BedrockClient.new(model = "m", region = env.BAML_TEST_UNSET_REGION).render(credential_input());
    let literal = aws.BedrockClient.new(model = "m", region = "eu-west-1").render(credential_input());
    assert.is_true(unset.url.includes("bedrock-runtime.preview-region.amazonaws.com"));
    assert.is_true(literal.url.includes("bedrock-runtime.eu-west-1.amazonaws.com"))
}

function bedrock_static_keys_error(client: aws.BedrockClient) -> string {
    let request = baml.http.Request { method: "POST", url: "https://example.invalid/model", headers: {}, body: "{}" };
    let outcome = aws.internal.sign(client, request) catch_all (e) {
        _ => failure_text(e),
    };
    match (outcome) {
        let text: string => text,
        let signed: baml.http.Request => "signed",
    }
}

// Static credentials are one identity: a partly configured set fails naming
// what is missing, instead of signing as whatever the environment holds.
test "bedrock_partial_static_credentials_name_what_is_missing" {
    let unset_secret = bedrock_static_keys_error(
        aws.BedrockClient.new(
            model = "m",
            region = "us-east-1",
            access_key_id = "AKIAIOSFODNN7EXAMPLE",
            secret_access_key = env.BAML_TEST_UNSET_AWS_SECRET,
        ),
    );
    assert.is_true(unset_secret.includes("AWS static credentials are incomplete: secret_access_key (env.BAML_TEST_UNSET_AWS_SECRET is unset or blank)"));
    let lone_key = bedrock_static_keys_error(
        aws.BedrockClient.new(model = "m", region = "us-east-1", access_key_id = "AKIAIOSFODNN7EXAMPLE"),
    );
    assert.is_true(lone_key.includes("incomplete: secret_access_key (not set)"));
    let lone_token = bedrock_static_keys_error(
        aws.BedrockClient.new(model = "m", region = "us-east-1", session_token = "example-token"),
    );
    assert.is_true(lone_token.includes("incomplete: access_key_id (not set), secret_access_key (not set)"))
}

// Options that read nothing are not configured: long-lived keys sign without
// the session token, and with nothing read the provider chain runs, so one
// `env.AWS_*`-style client works wherever the chain supplies credentials.
test "bedrock_static_credentials_that_read_nothing_are_not_set" {
    let without_token = bedrock_static_keys_error(
        aws.BedrockClient.new(
            model = "m",
            region = "us-east-1",
            access_key_id = "AKIAIOSFODNN7EXAMPLE",
            secret_access_key = "example-secret",
            session_token = env.BAML_TEST_UNSET_AWS_TOKEN,
        ),
    );
    assert.equal(without_token, "signed");
    let nothing_read = bedrock_static_keys_error(
        aws.BedrockClient.new(
            model = "m",
            region = "us-east-1",
            access_key_id = env.BAML_TEST_UNSET_AWS_KEY,
            secret_access_key = "  ",
            session_token = env.BAML_TEST_UNSET_AWS_TOKEN,
        ),
    );
    // Signed by the chain, or its own failure: never the static-keys error.
    assert.is_true(!nothing_read.includes("static credentials are incomplete"))
}

// ─── inline documents authenticate ──────────────────────────────────────────
// Each route that takes a credential document mints a token with it. The
// document is an `authorized_user` one, whose refresh grant needs no key,
// and its `token_uri` is the mock server, which answers the token request and
// then the model call. `tag` keeps the document unique: minted tokens are
// cached process-wide by credential material.

function authorized_user_document(token_uri: string, tag: string) -> string {
    let document: map<string, string> = {
        "type": "authorized_user",
        "client_id": `${tag}-client-id`,
        "client_secret": `${tag}-client-secret`,
        "refresh_token": `${tag}-refresh-token`,
        "token_uri": token_uri,
    };
    baml.json.stringify(document)
}

// The token request's path, the bearer the model call carried, and the turn.
function minted_turn(client_for: (string) -> google.VertexClient) -> string {
    let token = `{"access_token":"ya29.mocked","token_type":"Bearer","expires_in":3600}`;
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(token), root.llm_mock.MockResponse.ok(root.llm_google._canned_turn())], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let turn = client_for(mock.base_url()).invoke(credential_input());
        let token_request = mock.requests.at(0) ?? baml.sys.panic("the token request");
        [
            token_request.path(),
            mock.last_request().header("authorization") ?? "no authorization",
            turn.terminal_text() ?? "",
        ]
            .join("|")
    })
}

test "vertex_credentials_json_document_authenticates" {
    assert.equal(
        minted_turn((base: string) -> google.VertexClient {
            google.VertexClient.new(
                location = "us-central1",
                project_id = "p",
                base_url = base,
                credentials_json = authorized_user_document(`${base}/token`, "inline-json"),
            )
        }),
        "/token|Bearer ya29.mocked|pong",
    )
}

test "vertex_credentials_file_document_authenticates" {
    let directory = baml.env.get("TMPDIR") ?? baml.env.get("TEMP") ?? "/tmp";
    // Named for the mock server's port, which no concurrent run shares.
    let written: string[] = [];
    let outcome = minted_turn((base: string) -> google.VertexClient {
        let port = base.split(":").at(-1) ?? "0";
        let path = `${directory}/baml-credential-errors-${port}.json`;
        let _ = baml.fs.write(path, authorized_user_document(`${base}/token`, "inline-file"));
        written.push(path);
        google.VertexClient.new(
            location = "us-central1",
            project_id = "p",
            base_url = base,
            credentials_file = path,
        )
    }) catch_all (e) {
        // Caught so the file is removed on a failure too.
        _ => `failed: ${e.to_string()}`,
    };
    for (let path in written) {
        let _ = baml.fs.remove(path);
    }
    assert.equal(outcome, "/token|Bearer ya29.mocked|pong")
}
