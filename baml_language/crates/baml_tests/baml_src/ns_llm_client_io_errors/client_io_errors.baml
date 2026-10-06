// A custom `ai.Client` may throw a raw `baml.errors.Io`.
//
// `ai.errors.InvokeError` is the error channel of a client, and it has
// `baml.errors.Io` as a member. So a client that calls `baml.sys.sleep`, reads
// a file, or starts a process needs no `catch` around the call. The stack
// above the client classifies the raw `Io` as `ai.errors.normalize` does, into
// an `ai.errors.NetworkFailure` from the provider "client", before it judges
// the failure:
//
//   - a direct caller of `client.invoke` gets the raw `Io`;
//   - the agent loop, `build_request` and `ai.stream.from_spec` throw the
//     `NetworkFailure`, so the error set of an LLM function has no `Io`;
//   - `Retry` tries again and `Fallback` moves to the next member, as for
//     every safe transport failure, and `retry_if` judges the `NetworkFailure`.
//
// Every case body is a top-level function; a `test` block only asserts.

function CioAnswer() -> string {
    client: "openai/gpt-4o-mini"
    prompt: `Return one string. ${ctx.output_format()}`
}

class CioProbe {
    invocations: int,
    judged: string[],
}

/// Sleeps before each answer, and throws a raw `Io` on its first `io_failures`
/// invocations. `render` throws a raw `Io` when `render_fails` is set.
class CioClient {
    client_id: string,
    io_failures: int,
    render_fails: bool,
    probe: CioProbe,

    implements ai.Client {
        function id(self) -> string {
            self.client_id
        }

        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            let _ = input;
            if (self.render_fails) {
                throw baml.errors.Io { message: `${self.client_id} cannot read its template` }
            }
            baml.http.Request {
                method: "POST",
                url: `https://example.invalid/${self.client_id}`,
                headers: {},
                body: "{}",
            }
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            let _ = input;
            baml.sys.sleep(baml.time.Duration.from_milliseconds(0));
            self.probe.invocations += 1;
            if (self.probe.invocations <= self.io_failures) {
                throw baml.errors.Io { message: `${self.client_id} lost its file` }
            }
            ai.ModelTurn {
                calls: [],
                content: [ai.content.Text { text: `"answer from ${self.client_id}"` }],
                stop_reason: ai.content.StopReason.Complete,
                usage: null,
            }
        }
    }

    implements ai.stream.StreamingClient {
        function invoke_stream(self, input: ai.ModelTurnInput) -> ai.stream.TurnStream {
            let _ = input;
            baml.sys.sleep(baml.time.Duration.from_milliseconds(0));
            self.probe.invocations += 1;
            if (self.probe.invocations <= self.io_failures) {
                throw baml.errors.Io { message: `${self.client_id} lost its file` }
            }
            ai.stream.TurnStream.from_chunks([`"answer from ${self.client_id}"`])
        }
    }
}

/// A client that waits before it hands the turn to another client. The error
/// channel is written out: it is the interface's, and the sleep needs no
/// `catch`.
class CioDelayedClient {
    inner: ai.Client,

    implements ai.Client {
        function id(self) -> string {
            self.inner.id()
        }

        function render(self, input: ai.ModelTurnInput) -> baml.http.Request throws ai.errors.InvokeError {
            self.inner.render(input)
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn throws ai.errors.InvokeError {
            baml.sys.sleep(baml.time.Duration.from_milliseconds(0));
            self.inner.invoke(input)
        }
    }
}

function cio_client(client_id: string, io_failures: int, probe: CioProbe) -> CioClient {
    CioClient { client_id: client_id, io_failures: io_failures, render_fails: false, probe: probe }
}

function cio_probe() -> CioProbe {
    CioProbe { invocations: 0, judged: [] }
}

function cio_input() -> ai.ModelTurnInput {
    let spec = CioAnswer@spec();
    ai.ModelTurnInput {
        prompt: spec.prompt_template,
        journal: ai.Journal.new(spec),
        toolbox: spec.tools(),
        output_type: spec.output_type(),
    }
}

function cio_error_name(error: unknown) -> string {
    match (error) {
        let io: baml.errors.Io => `Io:${io.message}`,
        let network: ai.errors.NetworkFailure => `NetworkFailure:${network.provider}:${network.detail}`,
        _ => `other:${error.to_string()}`,
    }
}

function cio_turn_text(client: ai.Client) -> string {
    let turn = client.invoke(cio_input()) catch_all (e) {
        _ => {
            return cio_error_name(e);
        },
    };
    turn.terminal_text() ?? ""
}

function cio_failed_legs(client: ai.Client) -> string {
    let turn = client.invoke(cio_input()) catch_all (e) {
        _ => {
            return cio_error_name(e);
        },
    };
    let legs: string[] = [];
    for (let call in turn.calls) {
        if (!call.selected) {
            legs.push(`${call.client_name}:${cio_error_name(call.failure)}`);
        }
    }
    legs.join(",")
}

function cio_agent(client: ai.Client) -> string {
    ai.Agent.new(client = client).run(CioAnswer@spec()).value catch_all (e) {
        _ => cio_error_name(e),
    }
}

function cio_spec_call(client: ai.Client) -> string {
    CioAnswer@spec().call(client = client) catch_all (e) {
        _ => cio_error_name(e),
    }
}

function cio_build_request(client: ai.Client) -> string {
    let request = CioAnswer@spec().build_request(client = client) catch_all (e) {
        _ => {
            return cio_error_name(e);
        },
    };
    request.url
}

function cio_stream_final(client: ai.stream.StreamingClient) -> string {
    ai.stream.from_spec(CioAnswer@spec(), client = client).final() catch_all (e) {
        _ => cio_error_name(e),
    }
}

function cio_turn_stream_text(client: ai.stream.StreamingClient) -> string {
    let turn = client.invoke_stream(cio_input()).final_turn() catch_all (e) {
        _ => {
            return cio_error_name(e);
        },
    };
    turn.terminal_text() ?? ""
}

function cio_retry(inner: ai.Client, max_attempts: int) -> ai.clients.Retry {
    ai.clients.Retry.new(inner, max_attempts = max_attempts, backoff = ai.clients.Backoff.new(initial_ms = 0, max_ms = 0))
}

/// A `Retry` whose judgment refuses every failure and records what it judged.
function cio_refusing_retry(inner: ai.Client, probe: CioProbe) -> ai.clients.Retry {
    let refuse = (failure: ai.errors.Failure) -> bool throws never {
        probe.judged.push(cio_error_name(failure));
        false
    };
    ai.clients.Retry.new(
        inner,
        max_attempts = 3,
        retry_if = refuse,
        backoff = ai.clients.Backoff.new(initial_ms = 0, max_ms = 0),
    )
}

/// The result of `normalize` has no `Io` member: this match has no arm for it.
function cio_normalized(error: ai.errors.RaisedInClient) -> string {
    match (ai.errors.normalize(error)) {
        let failure: ai.errors.Failure => cio_error_name(failure),
        let timeout: baml.errors.Timeout => "Timeout",
        let unknown_error: baml.errors.UnknownError => "UnknownError",
        let compilation: reflect.errors.CompilationError => "CompilationError",
    }
}

// The error sets above a client have no `Io`: each clause below is the set
// that the call had before a client could throw `Io`.
function cio_call_error_set(client: ai.Client) -> string throws baml.errors.InvalidArgument | ai.errors.NormalizedError {
    CioAnswer@spec().call(client = client)
}

function cio_build_request_error_set(client: ai.Client) -> baml.http.Request throws baml.errors.InvalidArgument | ai.errors.NormalizedError {
    CioAnswer@spec().build_request(client = client)
}

function cio_stream_error_set(client: ai.stream.StreamingClient) -> string throws baml.errors.InvalidArgument | ai.errors.NormalizedError {
    ai.stream.from_spec(CioAnswer@spec(), client = client).final()
}

function cio_retry_error_set(client: ai.clients.Retry) -> ai.ModelTurn throws baml.errors.InvalidArgument | ai.errors.NormalizedError {
    client.invoke(cio_input())
}

function cio_direct_io() -> string {
    cio_turn_text(cio_client("direct", 1, cio_probe()))
}

function cio_sleeping_client_answers() -> string {
    cio_agent(cio_client("sleeper", 0, cio_probe()))
}

function cio_delayed_client_answers() -> string {
    cio_spec_call(CioDelayedClient { inner: cio_client("delayed", 0, cio_probe()) })
}

function cio_delayed_client_classifies() -> string {
    cio_spec_call(CioDelayedClient { inner: cio_client("delayed", 1, cio_probe()) })
}

function cio_agent_classifies() -> string {
    let probe = cio_probe();
    let ended = cio_agent(cio_client("agent", 1, probe));
    `${ended} after ${probe.invocations}`
}

function cio_retry_recovers() -> string {
    let probe = cio_probe();
    let answer = cio_turn_text(cio_retry(cio_client("flaky", 2, probe), 3));
    `${answer} after ${probe.invocations}`
}

function cio_retry_exhausts() -> string {
    let probe = cio_probe();
    let ended = cio_turn_text(cio_retry(cio_client("broken", 9, probe), 3));
    `${ended} after ${probe.invocations}`
}

function cio_retry_records_the_failed_legs() -> string {
    cio_failed_legs(cio_retry(cio_client("flaky", 1, cio_probe()), 3))
}

function cio_retry_judgment_sees_the_network_failure() -> string {
    let probe = cio_probe();
    let ended = cio_turn_text(cio_refusing_retry(cio_client("judged", 9, probe), probe));
    `${ended} after ${probe.invocations}; judged ${probe.judged.join(",")}`
}

function cio_fallback_moves_on() -> string {
    let first_probe = cio_probe();
    let backup_probe = cio_probe();
    let fallback = ai.clients.Fallback {
        members: [cio_client("first", 9, first_probe), cio_client("backup", 0, backup_probe)],
    };
    let answer = cio_turn_text(fallback);
    `${answer}; first ${first_probe.invocations}, backup ${backup_probe.invocations}`
}

function cio_round_robin_classifies() -> string {
    let probe = cio_probe();
    let ended = cio_turn_text(ai.clients.RoundRobin.new([cio_client("rotated", 9, probe)]));
    `${ended} after ${probe.invocations}`
}

function cio_stream_retry_recovers() -> string {
    let probe = cio_probe();
    let answer = cio_turn_stream_text(cio_retry(cio_client("stream-flaky", 2, probe), 3));
    `${answer} after ${probe.invocations}`
}

function cio_stream_classifies() -> string {
    cio_stream_final(cio_client("stream", 1, cio_probe()))
}

function cio_stream_answers() -> string {
    cio_stream_final(cio_client("stream", 0, cio_probe()))
}

function cio_render_classifies() -> string {
    cio_build_request(CioClient { client_id: "render", io_failures: 0, render_fails: true, probe: cio_probe() })
}

function cio_render_answers() -> string {
    cio_build_request(cio_client("render", 0, cio_probe()))
}

function cio_normalize_io() -> string {
    cio_normalized(baml.errors.Io { message: "the disk is gone" })
}

test "a client whose invoke sleeps needs no catch and answers" {
    assert.equal(cio_sleeping_client_answers(), "answer from sleeper");
    assert.equal(cio_delayed_client_answers(), "answer from delayed");
    assert.equal(cio_delayed_client_classifies(), "NetworkFailure:client:delayed lost its file");
}

test "a direct caller of a client gets its raw Io" {
    assert.equal(cio_direct_io(), "Io:direct lost its file");
}

test "the agent throws a client's Io as a NetworkFailure and does not try again" {
    assert.equal(cio_agent_classifies(), "NetworkFailure:client:agent lost its file after 1");
}

test "Retry tries again after a client's Io" {
    assert.equal(cio_retry_recovers(), "\"answer from flaky\" after 3");
}

test "Retry throws the NetworkFailure when every attempt ends with an Io" {
    assert.equal(cio_retry_exhausts(), "NetworkFailure:client:broken lost its file after 3");
}

test "Retry records an attempt that ended with an Io as a failed leg" {
    assert.equal(cio_retry_records_the_failed_legs(), "flaky:NetworkFailure:client:flaky lost its file");
}

test "retry_if judges a client's Io as a NetworkFailure" {
    assert.equal(
        cio_retry_judgment_sees_the_network_failure(),
        "NetworkFailure:client:judged lost its file after 1; judged NetworkFailure:client:judged lost its file",
    );
}

test "Fallback moves to the next member after a client's Io" {
    assert.equal(cio_fallback_moves_on(), "\"answer from backup\"; first 1, backup 1");
}

test "RoundRobin throws a member's Io as a NetworkFailure" {
    assert.equal(cio_round_robin_classifies(), "NetworkFailure:client:rotated lost its file after 1");
}

test "a streaming Retry opens the stream again after a client's Io" {
    assert.equal(cio_stream_retry_recovers(), "\"answer from stream-flaky\" after 3");
}

test "a stream from a spec throws a client's Io as a NetworkFailure" {
    assert.equal(cio_stream_classifies(), "NetworkFailure:client:stream lost its file");
    assert.equal(cio_stream_answers(), "answer from stream");
}

test "build_request throws a client's Io as a NetworkFailure" {
    assert.equal(cio_render_classifies(), "NetworkFailure:client:render cannot read its template");
    assert.equal(cio_render_answers(), "https://example.invalid/render");
}

test "normalize classifies an Io as a NetworkFailure and never returns one" {
    assert.equal(cio_normalize_io(), "NetworkFailure:client:the disk is gone");
}
