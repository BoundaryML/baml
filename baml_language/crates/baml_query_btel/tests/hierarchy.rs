//! Futures, span ancestry, recursion, the profiler and value SQL on real
//! engine recordings.
mod support;

use baml_query_btel::Status;
use bex_engine::BexExternalValue;
use serde_json::json;
use support::*;

const PROGRAM: &str = r"
function Inner(n: int) -> int { n * 2 }

function Outer(n: int) -> int {
    let child = spawn { Inner(n) };
    Inner(n + 1) + (await child)
}

function Fib(n: int) -> int {
    if (n < 2) { n } else { Fib(n - 1) + Fib(n - 2) }
}

function Leaf(n: int) -> int { n + 1 }

function main(n: int) -> int {
    let a = Leaf(n);
    let i = 0;
    while (i < 3) {
        a = a + Leaf(i);
        i = i + 1;
    }
    Outer(n) + Fib(6) + a
}
";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn spans_profiler_and_recursion_from_a_real_recording() {
    let project = tempfile::tempdir().unwrap();
    let results =
        record_program(project.path(), PROGRAM, &["Outer", "Inner"], &[("main", 4)]).await;
    // Outer(4) = 10 + 8; Fib(6) = 8; Leaf(4) + Leaf(0..3) = 5 + 6.
    assert_eq!(results[0], Ok(BexExternalValue::Int(18 + 8 + 11)));
    let mut index = index(project.path());

    // The host's call is a root future.
    let root = sql(
        &mut index,
        "SELECT span_id, span_name, status, future_id FROM spans
         WHERE span_type = 'future' AND parent_span_id IS NULL AND span_name = 'user.main'",
    );
    assert_eq!(root.rows.len(), 1);
    let root_id = root.rows[0][0].as_str().unwrap().to_owned();
    assert_eq!(
        &root.rows[0][1..],
        &[json!("user.main"), json!("return"), json!(null)]
    );
    let process = sql(&mut index, "SELECT process_id, status FROM processes");
    assert_eq!(process.rows[0][1], json!("success"));

    // Outer ran on the root future; the future it spawned hangs off it.
    let outer = sql(
        &mut index,
        "SELECT span_id, future_id, parent_span_id FROM spans WHERE span_name = 'user.Outer'",
    );
    let outer_id = outer.rows[0][0].as_str().unwrap().to_owned();
    assert_eq!(outer.rows[0][1], json!(root_id));
    assert_eq!(outer.rows[0][2], json!(root_id));
    let spawned = sql(
        &mut index,
        &format!(
            "SELECT span_id, span_name, future_id, status FROM spans
             WHERE span_type = 'future' AND parent_span_id = '{outer_id}'"
        ),
    );
    assert_eq!(spawned.rows.len(), 1);
    let spawned_id = spawned.rows[0][0].as_str().unwrap().to_owned();
    assert_eq!(
        &spawned.rows[0][1..],
        &[
            json!(".<lambda(Outer, 0)>"),
            json!(root_id),
            json!("return")
        ]
    );

    // Inner: once nested directly in Outer, once on the spawned future.
    let inner = sql(
        &mut index,
        "SELECT parent_span_id, future_id, input_args['n'], output_value FROM spans
         WHERE span_name = 'user.Inner' ORDER BY input_args['n']",
    );
    assert_eq!(
        inner.rows,
        vec![
            vec![json!(spawned_id), json!(spawned_id), json!(4), json!(8)],
            vec![json!(outer_id), json!(root_id), json!(5), json!(10)],
        ]
    );
    // The whole tree under the root call, with depths and values.
    let tree = sql(
        &mut index,
        &format!(
            "WITH RECURSIVE tree(id, depth, n) AS (
               SELECT span_id, 0, input_args FROM spans WHERE span_id = '{root_id}'
               UNION ALL
               SELECT s.span_id, t.depth + 1, s.input_args FROM spans s
               JOIN tree t ON s.parent_span_id = t.id)
             SELECT depth, COUNT(*), SUM(n['n']) FROM tree GROUP BY depth ORDER BY depth"
        ),
    );
    assert_eq!(
        tree.rows,
        vec![
            vec![json!(0), json!(1), json!(null)],
            vec![json!(1), json!(1), json!(4)],
            vec![json!(2), json!(2), json!(5)],
            vec![json!(3), json!(1), json!(4)],
        ],
        "main, Outer(4), Inner(5) and the spawned future, then Inner(4)"
    );
    // Each span's profiler node is in the process's profiler.
    let noded = sql(
        &mut index,
        "SELECT SUM(s.span_name != 'baml.gc'), COUNT(*) = (SELECT COUNT(*) FROM spans)
         FROM spans s JOIN profiler p ON p.profiler_node_id = s.profiler_node_id",
    );
    assert_eq!(noded.rows, vec![vec![json!(5), json!(1)]]);

    // Recursion: Fib(6) is 25 invocations of one node.
    let fib = sql(
        &mut index,
        "SELECT invocation_count, return_count, error_count, self_time = total_time,
           io_total_time
         FROM profiler WHERE function_name = 'user.Fib'",
    );
    assert_eq!(
        fib.rows,
        vec![vec![json!(25), json!(25), json!(0), json!(1), json!(0)]]
    );
    // One callee from two call sites of main: one node.
    let leaf = sql(
        &mut index,
        "SELECT l.invocation_count, m.function_name FROM profiler l
         JOIN profiler m ON m.profiler_node_id = l.parent_profiler_node_id
         WHERE l.function_name = 'user.Leaf'",
    );
    assert_eq!(leaf.rows, vec![vec![json!(4), json!("user.main")]]);
    // Inner runs under two nodes: in Outer, and in the spawned lambda.
    let inner = sql(
        &mut index,
        "SELECT p.function_name, i.invocation_count FROM profiler i
         JOIN profiler p ON p.profiler_node_id = i.parent_profiler_node_id
         WHERE i.function_name = 'user.Inner' ORDER BY 1",
    );
    assert_eq!(
        inner.rows,
        vec![
            vec![json!(".<lambda(Outer, 0)>"), json!(1)],
            vec![json!("user.Outer"), json!(1)],
        ]
    );
    // main's self time is its total without its synchronous callees'.
    let main = sql(
        &mut index,
        "SELECT m.self_time = m.total_time - SUM(c.total_time), m.error_count
         FROM profiler m JOIN profiler c ON c.parent_profiler_node_id = m.profiler_node_id
         WHERE m.function_name = 'user.main'",
    );
    assert_eq!(main.rows, vec![vec![json!(1), json!(0)]]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn value_sql_through_unions_states_and_unsupported_comparisons() {
    let project = tempfile::tempdir().unwrap();
    record(project.path(), &[(1, "ann"), (7, "bob")]).await;
    let mut index = index(project.path());

    // UNION keeps BAML values typed to the output.
    let union = sql(
        &mut index,
        "SELECT span_name, output_value FROM spans
           WHERE span_name = 'user.Extract' AND input_args['customer']['name'] = 'bob'
         UNION ALL SELECT span_name, output_value FROM spans
           WHERE span_name = 'user.Classify' AND input_args['customer']['name'] = 'bob'
         ORDER BY 1",
    );
    assert_eq!(union.columns[1].column_type, "baml_value");
    assert_eq!(union.rows[0][0], json!("user.Classify"));
    assert_eq!(union.rows[0][1], json!("Premium"));
    assert_eq!(union.rows[1][1]["$class"], json!("user.Order"));
    assert_eq!(union.rows[1][1]["customer"]["name"], json!("bob"));

    // Distinct states, one call each.
    let states = sql(
        &mut index,
        "SELECT baml_value_state(input_args['threshold']),
           baml_value_state(input_args['customer']['age']),
           baml_value_state(input_args['customer']['nope']), baml_value_state(error_value),
           baml_kind(output_value), baml_kind(context_metadata), temporary_projections IS NULL
         FROM spans WHERE span_name = 'user.Classify' AND input_args['customer']['name'] = 'bob'",
    );
    assert_eq!(
        states.rows,
        vec![vec![
            json!("omitted"),
            json!("present"),
            json!("missing"),
            json!("no_value"),
            json!("enum"),
            json!("json"),
            json!(1)
        ]]
    );
    assert_eq!(states.outcome.status, Status::Complete);

    // Different class types compare unequal, even with shared nested content.
    let compared = sql(
        &mut index,
        "SELECT COUNT(*) FROM spans WHERE span_name = 'user.Extract'
           AND output_value = input_args['customer']",
    );
    assert_eq!(compared.rows, vec![vec![json!(0)]]);
    assert_eq!(compared.outcome.status, Status::Complete);
    let ordered = sql(
        &mut index,
        "SELECT COUNT(*) FROM spans WHERE span_name = 'user.Extract'
           AND output_value < input_args['customer']",
    );
    assert_eq!(ordered.outcome.status, Status::Incomplete);
    assert_eq!(
        ordered.outcome.diagnostics[0].code,
        "comparison_unsupported"
    );
    // A structured value is unequal to a scalar: a defined answer.
    let scalar = sql(
        &mut index,
        "SELECT COUNT(*) FROM spans WHERE span_name = 'user.Extract' AND output_value = 'x'",
    );
    assert_eq!(scalar.outcome.status, Status::Complete);

    // Failed calls, including one caught inside main: the error value is the
    // context a handler would see, and the root call's status tells them apart.
    let errors = sql(
        &mut index,
        "SELECT e.span_name, e.error_value['error']['code'], baml_kind(e.error_value['stack_trace']),
           x.status
         FROM spans e JOIN spans x ON x.span_id = e.parent_span_id
         WHERE e.status = 'user_error' ORDER BY x.status",
    );
    assert_eq!(
        errors.rows,
        vec![
            vec![
                json!("user.Validate"),
                json!(42),
                json!("json"),
                json!("return")
            ],
            vec![
                json!("user.Validate"),
                json!(42),
                json!("json"),
                json!("user_error")
            ],
        ],
        "a caught error leaves its root call returning"
    );
}
