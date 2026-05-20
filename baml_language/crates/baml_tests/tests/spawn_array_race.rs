//! Reproducer: BEP-034 spawn closes over a HeapPtr to a shared Array.
//! Both parent and child VMs can mutate the same `Vec<Value>` concurrently
//! → true Rust data race on `Vec::push`.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "reproducer for the open BEP-034 shared-heap-object data race; \
            captured arrays/maps/instances passed to spawn aliase the parent's \
            heap object, and concurrent mutation is a true Rust data race. \
            Empirically: 3/9 runs lose a push (length=1002 not 1003), 1/10 \
            runs hits SIGTRAP from a Vec internal debug_assert. Un-ignore \
            once we land deep-copy-on-spawn or a Send-check in the type system."]
async fn shared_array_concurrent_push_is_racy() {
    let output = baml_test!(
        r#"
        function nap() -> int {
            baml.sys.sleep(0) catch (e) { let e => 0 };
            1
        }
        function child_pushes(a: int[]) -> int {
            let i = 0;
            while (i < 500) {
                a.push(1000 + i);
                let _ = nap();
                i = i + 1;
            }
            0
        }
        function main() -> int {
            let array = [1, 2, 3];
            let f = spawn { child_pushes(array) };
            let i = 0;
            while (i < 500) {
                array.push(i);
                let _ = nap();
                i = i + 1;
            }
            let _ = (await f) catch (e) { let e => 0 };
            array.length()
        }
        "#
    );

    eprintln!("OUTCOME: {:?}", output.result);
    match output.result {
        Ok(BexExternalValue::Int(n)) => {
            eprintln!("final array length = {n} (expected 1003 if both pushed cleanly)");
        }
        other => eprintln!("unexpected: {other:?}"),
    }
}
