//! `ai.Retry` attempts and `ai.Fallback` member tries are spans of their own,
//! recorded by the real engine against canned providers on 127.0.0.1.
mod support;

use serde_json::{Value as Json, json};
use support::*;

/// A provider that fails its first requests with the given statuses, then
/// answers like Anthropic: a whole message, or a stream when asked for one.
const PRELUDE: &str = r##"
class Flaky {
    server: baml.http.Server,
    failures: int[],
    served: int,

    function handle(self, req: baml.http.ServerRequest) -> baml.http.Response {
        let status = self.failures.at(self.served) ?? 200;
        self.served += 1;
        if (status != 200) {
            return baml.http.Response.new(status, { "content-type": "application/json" }, `{"type":"error","error":{"type":"overloaded_error","message":"try again"}}`.to_utf8());
        }
        if (req.body.includes("\"stream\":true")) {
            let resp = baml.http.Response.new_streaming(200, { "content-type": "text/event-stream" });
            spawn {
                for (let event in sse_events()) {
                    resp.write((event + "\n\n").to_utf8());
                }
                resp.end();
                null
            };
            return resp;
        }
        baml.http.Response.new(200, { "content-type": "application/json" }, `{"id":"m","type":"message","role":"assistant","model":"claude-opus-5-5","content":[{"type":"text","text":"short"}],"stop_reason":"end_turn","usage":{"input_tokens":900,"output_tokens":30,"cache_read_input_tokens":200,"cache_creation_input_tokens":100}}`.to_utf8())
    }
}

function sse_events() -> string[] {
    [
        "event: message_start\ndata: " + `{"type":"message_start","message":{"id":"m","model":"claude-opus-5-5","usage":{"input_tokens":900,"cache_read_input_tokens":200,"cache_creation_input_tokens":100}}}`,
        "event: content_block_start\ndata: " + `{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}`,
        "event: content_block_delta\ndata: " + `{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"short"}}`,
        "event: message_delta\ndata: " + `{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":30}}`,
        "event: message_stop\ndata: " + `{"type":"message_stop"}`,
    ]
}

function flaky(failures: int[]) -> Flaky {
    Flaky { server: baml.http.Server.bind("127.0.0.1:0"), failures: failures, served: 0 }
}

function Summarize(text: string) -> string {
    client: "anthropic/claude-opus-5-5"
    prompt: `Summarize ${text}`
}

function at(base_url: string, model: string) -> anthropic.Client {
    anthropic.Client.new(model = model, api_key = "test-key", base_url = base_url)
}

function retrying(base_url: string) -> ai.clients.Retry {
    ai.clients.Retry.new(
        inner = at(base_url, "claude-opus-5-5"),
        max_attempts = 3,
        backoff = ai.clients.Backoff.new(initial_ms = 1),
    )
}

function Ask(base_url: string) -> string {
    ai.Agent.new(client = retrying(base_url)).run(Summarize@spec("facts")).value
}
"##;

/// Each network span with its nearest attempt-span ancestor, whatever spans
/// lie between them (the stdlib's per-request span, for now).
const NEAREST_ATTEMPT: &str = "WITH RECURSIVE up(network, span) AS (
    SELECT span_id, parent_span_id FROM spans WHERE span_type = 'network_span'
    UNION ALL
    SELECT u.network, s.parent_span_id FROM up u JOIN spans s ON s.span_id = u.span
    WHERE s.span_name NOT IN ('ai.clients.retry_attempt', 'ai.clients.fallback_attempt')
  ),
  nearest AS (
    SELECT u.network, a.span_id AS attempt FROM up u JOIN spans a ON a.span_id = u.span
    WHERE a.span_name IN ('ai.clients.retry_attempt', 'ai.clients.fallback_attempt')
  )";

async fn record(project: &std::path::Path, main: &str) {
    let source = format!("{PRELUDE}\n{main}");
    let results = record_program(project, &source, &[], &[("main", 0)]).await;
    assert_eq!(results, vec![Ok(bex_engine::BexExternalValue::Int(5))]);
}

/// The cost summed over every span, and over network spans alone.
fn cost(index: &mut baml_query_btel::Index) -> (Json, Json) {
    let total = |index: &mut baml_query_btel::Index, filter: &str| {
        sql(
            index,
            &format!("SELECT round(sum(temporary_projections['cost']), 6) FROM spans {filter}"),
        )
        .rows[0][0]
            .clone()
    };
    (
        total(index, ""),
        total(index, "WHERE span_type = 'network_span'"),
    )
}

/// A 429 then a 200: two sibling attempt spans, each with its own request,
/// and the cost priced once, on the request that succeeded.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn each_retry_attempt_is_a_span_with_its_own_request() {
    let project = tempfile::tempdir().unwrap();
    record(
        project.path(),
        r#"
        function main(n: int) -> int {
            let server = flaky([429]);
            let task = spawn { server.server.serve(server.handle) };
            let answer = Ask("http://" + server.server.addr, $trace = trace.span());
            task.cancel();
            answer.length()
        }
        "#,
    )
    .await;
    let mut index = index(project.path());
    let attempts = sql(
        &mut index,
        "SELECT a.status, a.input_args, p.span_name
         FROM spans a JOIN spans p ON p.span_id = a.parent_span_id
         WHERE a.span_name = 'ai.clients.retry_attempt' ORDER BY a.start_time",
    );
    // Only the attempt number and the client's id are captured; the body
    // runs the attempt and is recorded as an opaque closure.
    let inputs = |attempt: i64| {
        json!({"attempt": attempt, "client": "anthropic/claude-opus-5-5",
            "body": {"$opaque": "closure"}})
    };
    assert_eq!(
        attempts.rows,
        vec![
            vec![json!("user_error"), inputs(1), json!("user.Ask")],
            vec![json!("return"), inputs(2), json!("user.Ask")],
        ]
    );
    assert_eq!(
        sql(
            &mut index,
            "SELECT count(DISTINCT parent_span_id) FROM spans
             WHERE span_name = 'ai.clients.retry_attempt'"
        )
        .rows,
        vec![vec![json!(1)]],
        "the attempts are siblings"
    );
    // Each request under its own attempt: a 429, then a 200 that carries
    // the cost.
    let requests = sql(
        &mut index,
        &format!(
            "{NEAREST_ATTEMPT}
             SELECT a.input_args['attempt'], n.network_event_values[0]['event_name'],
               n.network_event_values[0]['payload']['status'],
               round(n.temporary_projections['cost'], 6)
             FROM nearest x JOIN spans n ON n.span_id = x.network
             JOIN spans a ON a.span_id = x.attempt ORDER BY n.start_time"
        ),
    );
    assert_eq!(
        requests.rows,
        vec![
            vec![json!(1), json!("connection"), json!(429), Json::Null],
            vec![json!(2), json!("connection"), json!(200), json!(0.00474)],
        ]
    );
    assert_eq!(
        sql(
            &mut index,
            "SELECT count(*) FROM spans WHERE span_type = 'network_span'"
        )
        .rows,
        vec![vec![json!(2)]]
    );
    assert_eq!(cost(&mut index), (json!(0.00474), json!(0.00474)));
}

/// Fallback's members, tried in order: member 0 answers 500, member 1 200.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn each_fallback_member_try_is_a_span() {
    let project = tempfile::tempdir().unwrap();
    record(
        project.path(),
        r#"
        function Fall(base_url: string) -> string {
            let members: ai.Client[] = [at(base_url, "claude-opus-5"), at(base_url, "claude-opus-5-5")];
            let fallback = ai.clients.Fallback { members: members };
            ai.Agent.new(client = fallback).run(Summarize@spec("facts")).value
        }

        function main(n: int) -> int {
            let server = flaky([500]);
            let task = spawn { server.server.serve(server.handle) };
            let answer = Fall("http://" + server.server.addr, $trace = trace.span());
            task.cancel();
            answer.length()
        }
        "#,
    )
    .await;
    let mut index = index(project.path());
    let tries = sql(
        &mut index,
        &format!(
            "{NEAREST_ATTEMPT}
             SELECT a.status, a.input_args['member'], a.input_args['client'],
               n.network_event_values[0]['payload']['status']
             FROM nearest x JOIN spans n ON n.span_id = x.network
             JOIN spans a ON a.span_id = x.attempt ORDER BY a.start_time"
        ),
    );
    assert_eq!(
        tries.rows,
        vec![
            vec![
                json!("user_error"),
                json!(0),
                json!("anthropic/claude-opus-5"),
                json!(500)
            ],
            vec![
                json!("return"),
                json!(1),
                json!("anthropic/claude-opus-5-5"),
                json!(200)
            ],
        ]
    );
    assert_eq!(
        sql(
            &mut index,
            "SELECT count(DISTINCT parent_span_id) FROM spans
             WHERE span_name = 'ai.clients.fallback_attempt'"
        )
        .rows,
        vec![vec![json!(1)]],
        "the tries are siblings"
    );
}

/// A streamed Retry whose first open is refused: an attempt span for each
/// open. The second attempt's span ends once its stream is open; its
/// request's network span ends when the stream has been read.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_streamed_retry_has_an_attempt_span_for_each_open() {
    let project = tempfile::tempdir().unwrap();
    record(
        project.path(),
        r#"
        function Stream(base_url: string) -> string {
            ai.stream.from_spec<string>(Summarize@spec("hi"), client = retrying(base_url)).final()
        }

        function main(n: int) -> int {
            let server = flaky([429]);
            let task = spawn { server.server.serve(server.handle) };
            let text = Stream("http://" + server.server.addr, $trace = trace.span());
            task.cancel();
            text.length()
        }
        "#,
    )
    .await;
    let mut index = index(project.path());
    let opens = sql(
        &mut index,
        &format!(
            "{NEAREST_ATTEMPT}
             SELECT a.input_args['attempt'], a.status,
               n.network_event_values[0]['payload']['status'], n.end_time > a.end_time,
               round(n.temporary_projections['cost'], 6)
             FROM nearest x JOIN spans n ON n.span_id = x.network
             JOIN spans a ON a.span_id = x.attempt ORDER BY a.start_time"
        ),
    );
    assert_eq!(
        opens.rows,
        vec![
            vec![
                json!(1),
                json!("user_error"),
                json!(429),
                json!(0),
                Json::Null
            ],
            vec![
                json!(2),
                json!("return"),
                json!(200),
                json!(1),
                json!(0.00474)
            ],
        ]
    );
    assert_eq!(cost(&mut index), (json!(0.00474), json!(0.00474)));
}

/// Two Retry calls in parallel futures: each future's attempts, and the
/// requests under them, stay in that future.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn parallel_retries_keep_their_attempts_apart() {
    let project = tempfile::tempdir().unwrap();
    let results = record_program(
        project.path(),
        &format!(
            "{PRELUDE}\n{}",
            r#"
            function main(n: int) -> int {
                let one = flaky([429]);
                let two = flaky([429]);
                let serving_one = spawn { one.server.serve(one.handle) };
                let serving_two = spawn { two.server.serve(two.handle) };
                let first = spawn "first" { Ask("http://" + one.server.addr) };
                let second = spawn "second" { Ask("http://" + two.server.addr) };
                let a = await first;
                let b = await second;
                serving_one.cancel();
                serving_two.cancel();
                a.length() + b.length()
            }
            "#
        ),
        &[],
        &[("main", 0)],
    )
    .await;
    assert_eq!(results, vec![Ok(bex_engine::BexExternalValue::Int(10))]);
    let mut index = index(project.path());
    let attempts = sql(
        &mut index,
        &format!(
            "{NEAREST_ATTEMPT}
             SELECT af.span_name, a.input_args['attempt'], nf.span_name,
               n.network_event_values[0]['payload']['status']
             FROM nearest x JOIN spans n ON n.span_id = x.network
             JOIN spans a ON a.span_id = x.attempt
             JOIN spans af ON af.span_id = a.future_id
             JOIN spans nf ON nf.span_id = n.future_id
             ORDER BY af.span_name, a.start_time"
        ),
    );
    let attempt = |future: &str, n: i64, status: i64| {
        vec![json!(future), json!(n), json!(future), json!(status)]
    };
    assert_eq!(
        attempts.rows,
        vec![
            attempt("first", 1, 429),
            attempt("first", 2, 200),
            attempt("second", 1, 429),
            attempt("second", 2, 200),
        ]
    );
}
