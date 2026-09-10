//! all_complete must observe every input before returning the first input error.

use baml_tests::engine::{IndexMap, OptLevel, compile_source_with_opt, run_compiled};
use bex_engine::BexExternalValue;

#[tokio::test]
async fn failure_waits_for_later_inputs_without_cancelling_them() {
    let program = compile_source_with_opt(
        r#"
        class Failed { key: string }
        function value(n: int) -> int throws Failed {
            if (n < 0) { throw Failed { key: n.to_string() }; }
            n
        }
        function main() -> string {
            let group = baml.spawn.TaskGroup.new(0);
            let failed = spawn { value(-1) };
            let later = spawn with baml.spawn.options(group = group) { value(2) };
            let release = spawn {
                baml.sys.sleep(baml.time.Duration.from_milliseconds(100));
                group.set_limit(1);
            };
            defer { group.set_limit(1); }
            let result = {
                await baml.future.all_complete([failed, later]);
                "unexpected success"
            } catch (e) {
                Failed => {
                    if (!later.is_settled()) { "later input still pending" }
                    else if (later.is_cancelled()) { "later input cancelled" }
                    else { `${e.key}:${await later}` }
                },
            };
            await release;
            result
        }
        "#,
        OptLevel::One,
    );
    let output = run_compiled(program, "main", IndexMap::new(), false).await;
    assert_eq!(
        output.result.expect("recovery succeeds"),
        BexExternalValue::String("-1:2".into()),
    );
}

#[tokio::test]
async fn results_and_first_error_follow_input_order() {
    let program = compile_source_with_opt(
        r#"
        class Failed { key: string }
        function value(n: int) -> int throws Failed {
            if (n < 0) { throw Failed { key: n.to_string() }; }
            n
        }
        function main() -> string {
            let values = await baml.future.all_complete([spawn { 3 }, spawn { 1 }]);
            let group = baml.spawn.TaskGroup.new(0);
            let failed = spawn { value(-1) };
            let second = spawn with baml.spawn.options(group = group) { value(-2) };
            let release = spawn {
                baml.sys.sleep(baml.time.Duration.from_milliseconds(100));
                group.set_limit(1);
            };
            defer { group.set_limit(1); }
            let message = {
                await baml.future.all_complete([failed, second]);
                "unexpected success"
            } catch (e) {
                Failed => {
                    if (!second.is_settled()) { "second error still pending" }
                    else { e.key }
                },
            };
            await release;
            `${values[0]}:${values[1]}:${message}`
        }
        "#,
        OptLevel::One,
    );
    let output = run_compiled(program, "main", IndexMap::new(), false).await;
    assert_eq!(
        output.result.expect("recovery succeeds"),
        BexExternalValue::String("3:1:-1".into()),
    );
}
