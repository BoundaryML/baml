//! Regression tests for preserving mutable map identity across calls and loops.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

fn ok_string(value: &str) -> Result<BexExternalValue, String> {
    Ok(BexExternalValue::String(value.to_string().into()))
}

/// A single static use in a loop is many dynamic uses. The map binding must be
/// materialized once before the loop so callee mutations accumulate.
#[tokio::test]
async fn callee_map_mutations_persist_when_caller_does_not_read_map() {
    let output = baml_test!(
        r#"
        function put(m: map<string, int>, k: string) -> int throws unknown {
            m.set(k, 1)
            return m.length()
        }

        function main() -> string {
            let m: map<string, int> = {}
            let out = ""
            let i = 0
            while (i < 3) {
                let n = put(m, "k" + i.to_string()) catch (e) { _ => -1 }
                out = out + n.to_string() + ","
                i += 1
            }
            out
        }
        "#
    );

    assert_eq!(
        output.result.map_err(|error| format!("{error:?}")),
        ok_string("1,2,3,")
    );
}

/// Pin the formerly masking caller read as a positive control: observing the
/// map after the loop must retain the same alias and accumulated mutations.
#[tokio::test]
async fn callee_map_mutations_persist_when_caller_reads_map() {
    let output = baml_test!(
        r#"
        function put(m: map<string, int>, k: string) -> int throws unknown {
            m.set(k, 1)
            return m.length()
        }

        function main() -> string {
            let m: map<string, int> = {}
            let out = ""
            let i = 0
            while (i < 3) {
                let n = put(m, "k" + i.to_string()) catch (e) { _ => -1 }
                out = out + n.to_string() + ","
                i += 1
            }
            out + " caller=" + m.length().to_string()
        }
        "#
    );

    assert_eq!(
        output.result.map_err(|error| format!("{error:?}")),
        ok_string("1,2,3, caller=3")
    );
}
