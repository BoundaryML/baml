//! all_settled returns values, typed errors, and panics as ordered outcomes.

use std::time::Duration;

use baml_tests::engine::{IndexMap, OptLevel, compile_source_with_opt, run_compiled};
use bex_engine::BexExternalValue;

async fn check(body: &str, expected: &str) {
    let source = format!(
        r#"
        class Failed {{ key: string }}
        function value(n: int) -> int throws Failed {{
            if (n < 0) {{ throw Failed {{ key: n.to_string() }}; }}
            n
        }}
        function explode() -> int throws Failed {{
            baml.sys.panic("boom");
            value(2)
        }}
        function label(outcome: baml.future.Success<int> | baml.future.Failure<Failed> | baml.future.Panicked) -> string throws never {{
            match (outcome) {{
                let s: baml.future.Success<int> => `success:${{s.value}}`,
                let f: baml.future.Failure<Failed> => `failure:${{f.error.key}}`,
                let p: baml.future.Panicked => match (p.panic) {{
                    baml.panics.Cancelled => "cancelled",
                    baml.panics.UserPanic => "panicked",
                    _ => "unexpected panic",
                }},
            }}
        }}
        function main() -> string {{ {body} }}
        "#,
    );
    let program = compile_source_with_opt(&source, OptLevel::One);
    let output = tokio::time::timeout(
        Duration::from_secs(15),
        run_compiled(program, "main", IndexMap::new(), false),
    )
    .await
    .expect("collection must not hang");
    assert_eq!(
        output.result.expect("collection returns outcomes"),
        BexExternalValue::String(expected.into()),
    );
}

#[tokio::test]
async fn failure_preserves_successes_and_waits_for_later_inputs() {
    check(
        r#"
        let group = baml.spawn.TaskGroup.new(0);
        let failed = spawn { value(-1) };
        let later = spawn with baml.spawn.options(group = group) { value(2) };
        let release = spawn {
            baml.sys.sleep(baml.time.Duration.from_milliseconds(100));
            group.set_limit(1);
        };
        defer { group.set_limit(1); }
        let results = await baml.future.all_settled([failed, later, failed]);
        if (!later.is_settled() || later.is_cancelled()) { baml.sys.panic("later input must finish"); }
        await release;
        results.map((r) -> { label(r) }).join(",")
        "#,
        "failure:-1,success:2,failure:-1",
    )
    .await;
}

#[tokio::test]
async fn every_failure_is_returned_in_input_order() {
    check(
        r#"
        let group = baml.spawn.TaskGroup.new(0);
        let first = spawn with baml.spawn.options(group = group) { value(-1) };
        let second = spawn { value(-2) };
        let release = spawn {
            baml.sys.sleep(baml.time.Duration.from_milliseconds(100));
            group.set_limit(1);
        };
        defer { group.set_limit(1); }
        let results = await baml.future.all_settled([first, second]);
        await release;
        results.map((r) -> { label(r) }).join(",")
        "#,
        "failure:-1,failure:-2",
    )
    .await;
}

#[tokio::test]
async fn panic_and_input_cancellation_are_outcomes_with_context() {
    check(
        r#"
        let group = baml.spawn.TaskGroup.new(0);
        let cancelled = spawn with baml.spawn.options(group = group) { value(1) };
        cancelled.cancel();
        let panic = spawn { explode() };
        let results = await baml.future.all_settled([panic, cancelled, spawn { value(3) }]);
        match (results[0]) {
            let p: baml.future.Panicked => {
                let found_origin = false;
                for (let frame in p.context.stack_trace.frames) {
                    if (frame.file == "test.baml") { found_origin = true; }
                }
                if (!found_origin) { baml.sys.panic("missing original trace"); }
                if (!(p.context.error is baml.panics.UserPanic)) { baml.sys.panic("missing original panic"); }
                match (p.panic) {
                    let panic: baml.panics.UserPanic => {
                        if (panic.message != "boom") { baml.sys.panic("lost panic payload"); }
                    },
                    _ => baml.sys.panic("wrong panic"),
                }
            },
            _ => baml.sys.panic("missing panic outcome"),
        };
        results.map((r) -> { label(r) }).join(",")
        "#,
        "panicked,cancelled,success:3",
    )
    .await;
}

#[tokio::test]
async fn empty_input_has_no_outcomes_and_no_typed_errors() {
    check(
        r#"
        let inputs: baml.future.Future<int, Failed>[] = [];
        let collected: baml.future.Future<(baml.future.Success<int> | baml.future.Failure<Failed> | baml.future.Panicked)[], never> = baml.future.all_settled(inputs);
        (await collected).length().to_string()
        "#,
        "0",
    )
    .await;
}

#[tokio::test]
async fn returning_an_error_value_is_still_success() {
    check(
        r#"
        let results = await baml.future.all_settled([spawn { Failed { key: "returned" } }]);
        match (results[0]) {
            let s: baml.future.Success<Failed> => s.value.key,
            _ => "wrong outcome",
        }
        "#,
        "returned",
    )
    .await;
}

#[tokio::test]
async fn cancelling_the_collector_stops_waiting_without_cancelling_inputs() {
    check(
        r#"
        let group = baml.spawn.TaskGroup.new(0);
        let input = spawn with baml.spawn.options(group = group) { value(7) };
        defer { group.set_limit(1); }
        let collector = baml.future.all_settled([input]);
        baml.sys.sleep(baml.time.Duration.from_milliseconds(100));
        collector.cancel();
        let status = {
            await collector;
            "unexpected result"
        } catch (e) {
            baml.panics.Cancelled => "collector cancelled",
        };
        if (input.is_settled()) { baml.sys.panic("collector cancelled its input"); }
        group.set_limit(1);
        `${status}:${await input}`
        "#,
        "collector cancelled:7",
    )
    .await;
}
