//! Threads, retained-call ancestry, recursion and value SQL on real engine
//! recordings.
mod support;

use baml_query_btel::Status;
use bex_engine::BexExternalValue;
use serde_json::{Value as Json, json};
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
async fn spawn_tree_call_ancestry_and_recursion_from_a_real_recording() {
    let project = tempfile::tempdir().unwrap();
    let results =
        record_program(project.path(), PROGRAM, &["Outer", "Inner"], &[("main", 4)]).await;
    // Outer(4) = 10 + 8; Fib(6) = 8; Leaf(4) + Leaf(0..3) = 5 + 6.
    assert_eq!(results[0], Ok(BexExternalValue::Int(18 + 8 + 11)));
    let mut index = index(project.path());

    let execution = sql(
        &mut index,
        "SELECT execution_id, entry_fqn, status, threads_total, calls_retained,
           retained_ok_calls FROM executions WHERE entry_fqn = 'user.main'",
    );
    assert_eq!(execution.rows.len(), 1);
    let row = &execution.rows[0];
    let execution_id = row[0].as_str().unwrap().to_owned();
    assert_eq!(
        &row[1..],
        &[
            json!("user.main"),
            json!("ok"),
            json!(2),
            json!(3),
            json!(3)
        ]
    );

    // The spawned thread's recorded parent is the retained Outer call.
    let outer = sql(
        &mut index,
        "SELECT call_id, thread_id, parent_node_kind FROM calls WHERE fqn = 'user.Outer'",
    );
    let (outer_id, root) = (
        outer.rows[0][0].as_str().unwrap().to_owned(),
        outer.rows[0][1].as_str().unwrap().to_owned(),
    );
    assert_eq!(root, execution_id, "Outer ran on the root thread");
    assert_eq!(outer.rows[0][2], json!("thread"));
    let spawned = sql(
        &mut index,
        &format!(
            "SELECT kind, parent_node_kind, spawn_call_id, parent_thread_id, spawn_fqn, end_status
             FROM threads WHERE execution_id = '{execution_id}' AND kind = 'spawn'"
        ),
    );
    assert_eq!(
        spawned.rows,
        vec![vec![
            json!("spawn"),
            json!("call"),
            json!(outer_id),
            json!(root),
            json!(".<lambda(Outer, 0)>"),
            json!("ok")
        ]]
    );

    // Inner: once nested directly in Outer, once on the spawned thread.
    let inner = sql(
        &mut index,
        "SELECT c.parent_node_kind, c.parent_call_id, t.kind, c.args['n'], c.output
         FROM calls c JOIN threads t ON t.thread_id = c.thread_id
         WHERE c.fqn = 'user.Inner' ORDER BY t.kind",
    );
    assert_eq!(
        inner.rows,
        vec![
            vec![
                json!("call"),
                json!(outer_id),
                json!("root"),
                json!(5),
                json!(10)
            ],
            vec![
                json!("thread"),
                Json::Null,
                json!("spawn"),
                json!(4),
                json!(8)
            ],
        ]
    );

    // Recursion: Fib(6) is 25 invocations on one context.
    let fib = sql(
        &mut index,
        "SELECT normal_completed_calls, reentry_completed_calls, completed_calls,
           invocation_duration_sum_ns >= inclusive_ns, self_ns = inclusive_ns, direct_child_ns,
           self_time_state
         FROM call_path_stats WHERE fqn = 'user.Fib'",
    );
    assert_eq!(
        fib.rows,
        vec![vec![
            json!(1),
            json!(24),
            json!(25),
            json!(1),
            json!(1),
            json!(0),
            json!("valid")
        ]]
    );
    // One callee at two call sites: two contexts under main.
    let leaf = sql(
        &mut index,
        "SELECT s.depth, s.completed_calls, p.caller_fqn FROM call_path_stats s
         JOIN call_paths p ON p.call_path_id = s.call_path_id
         WHERE s.fqn = 'user.Leaf' ORDER BY s.completed_calls",
    );
    assert_eq!(
        leaf.rows,
        vec![
            vec![json!(1), json!(1), json!("user.main")],
            vec![json!(1), json!(3), json!("user.main")],
        ]
    );
    // Population counts agree across relations and never include retained
    // calls twice.
    let totals = sql(
        &mut index,
        &format!(
            "SELECT (SELECT completed_calls FROM executions WHERE execution_id = '{execution_id}'),
               (SELECT SUM(completed_calls) FROM function_stats WHERE execution_id = '{execution_id}'),
               (SELECT SUM(completed_calls) FROM call_path_nodes WHERE execution_id = '{execution_id}'),
               (SELECT completed_calls FROM function_stats WHERE fqn = 'user.Inner')"
        ),
    );
    let total = totals.rows[0][0].clone();
    assert_eq!(totals.rows[0][1], total);
    assert_eq!(totals.rows[0][2], total);
    assert_eq!(
        totals.rows[0][3],
        json!(2),
        "retained Inner counted once each"
    );
    // main's self time excludes its synchronous children's inclusive time.
    let main = sql(
        &mut index,
        "SELECT inclusive_ns >= direct_child_ns + await_ns + self_ns, self_time_state
         FROM call_path_stats WHERE fqn = 'user.main'",
    );
    assert_eq!(main.rows, vec![vec![json!(1), json!("valid")]]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn value_sql_through_unions_states_and_unsupported_comparisons() {
    let project = tempfile::tempdir().unwrap();
    record(project.path(), &[(1, "ann"), (7, "bob")]).await;
    let mut index = index(project.path());

    // UNION keeps BAML values typed to the output.
    let union = sql(
        &mut index,
        "SELECT fqn, output FROM calls
           WHERE fqn = 'user.Extract' AND args['customer']['name'] = 'bob'
         UNION ALL SELECT fqn, output FROM calls
           WHERE fqn = 'user.Classify' AND args['customer']['name'] = 'bob'
         ORDER BY 1",
    );
    assert_eq!(union.columns[1].column_type, "baml_value");
    assert_eq!(union.rows[0][0], json!("user.Classify"));
    assert_eq!(union.rows[0][1], json!("Premium"));
    assert_eq!(union.rows[1][1]["$class"], json!("Order"));
    assert_eq!(union.rows[1][1]["customer"]["name"], json!("bob"));

    // Distinct states, one call each.
    let states = sql(
        &mut index,
        "SELECT baml_value_state(args['threshold']), baml_value_state(args['customer']['age']),
           baml_value_state(args['customer']['nope']), baml_value_state(error), baml_kind(output),
           args_state, output_state, error_state
         FROM calls WHERE fqn = 'user.Classify' AND args['customer']['name'] = 'bob'",
    );
    assert_eq!(
        states.rows,
        vec![vec![
            json!("omitted"),
            json!("present"),
            json!("missing"),
            json!("no_value"),
            json!("enum"),
            json!("reference"),
            json!("reference"),
            json!("not_applicable")
        ]]
    );
    assert_eq!(states.outcome.status, Status::Complete);

    // Different class types compare unequal, even with shared nested content.
    let compared = sql(
        &mut index,
        "SELECT COUNT(*) FROM calls WHERE fqn = 'user.Extract' AND output = args['customer']",
    );
    assert_eq!(compared.rows, vec![vec![json!(0)]]);
    assert_eq!(compared.outcome.status, Status::Complete);
    let ordered = sql(
        &mut index,
        "SELECT COUNT(*) FROM calls WHERE fqn = 'user.Extract' AND output < args['customer']",
    );
    assert_eq!(ordered.outcome.status, Status::Incomplete);
    assert_eq!(
        ordered.outcome.diagnostics[0].code,
        "comparison_unsupported"
    );
    // A structured value is unequal to a scalar: a defined answer.
    let scalar = sql(
        &mut index,
        "SELECT COUNT(*) FROM calls WHERE fqn = 'user.Extract' AND output = 'x'",
    );
    assert_eq!(scalar.outcome.status, Status::Complete);

    // Errored calls, including one caught inside main.
    let errors = sql(
        &mut index,
        "SELECT e.fqn, e.error['code'], x.status FROM error_calls e
         JOIN executions x ON x.execution_id = e.execution_id ORDER BY x.status",
    );
    assert_eq!(
        errors.rows,
        vec![
            vec![json!("user.Validate"), json!(42), json!("errored")],
            vec![json!("user.Validate"), json!(42), json!("ok")],
        ],
        "a caught error leaves its execution ok"
    );
}
