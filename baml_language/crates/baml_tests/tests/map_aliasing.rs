//! Regression tests for preserving mutable map identity across calls and loops.

use baml_tests::{baml_test, baml_test_optimized};
use bex_engine::BexExternalValue;

fn ok_string(value: &str) -> Result<BexExternalValue, String> {
    Ok(BexExternalValue::String(value.to_string().into()))
}

macro_rules! check_output {
    ($name:ident, $source:expr, $expected:expr) => {
        #[tokio::test]
        async fn $name() {
            let output = baml_test!($source);
            assert_eq!(
                output.result.map_err(|error| format!("{error:?}")),
                ok_string($expected)
            );
        }
    };
}

macro_rules! check_optimized_output {
    ($name:ident, $source:expr, $expected:expr) => {
        #[tokio::test]
        async fn $name() {
            let output = baml_test_optimized!($source);
            assert_eq!(
                output.result.map_err(|error| format!("{error:?}")),
                ok_string($expected)
            );
        }
    };
}

const INT_LIST_SOURCE: &str = r#"
    function put(xs: int[]) -> int {
        xs.push(1)
        xs.length()
    }

    function main() -> string {
        let xs: int[] = []
        let out = ""
        let i = 0
        while (i < 3) {
            let n = put(xs)
            out = out + n.to_string() + ","
            i += 1
        }
        out
    }
"#;

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

// Pin the no-`return`, no-`catch` repro, whose tail `ReturnPhi` produces a
// different CFG from the original regression.
check_output!(
    map_identity_survives_return_phi_loop_cfg,
    r#"
    function put(m: map<string, int>, k: string) -> int {
        m.set(k, 1)
        m.length()
    }

    function main() -> string {
        let m: map<string, int> = {}
        let out = ""
        let i = 0
        while (i < 3) {
            let n = put(m, "k" + i.to_string())
            out = out + n.to_string() + ","
            i += 1
        }
        out
    }
    "#,
    "1,2,3,"
);

// Allocations authored inside the loop must remain fresh on every iteration.
check_output!(
    map_allocated_inside_loop_stays_fresh,
    r#"
    function put(m: map<string, int>) -> int {
        m.set("k", 1)
        m.length()
    }

    function main() -> string {
        let out = ""
        let i = 0
        while (i < 3) {
            let m: map<string, int> = {}
            let n = put(m)
            out = out + n.to_string() + ","
            i += 1
        }
        out
    }
    "#,
    "1,1,1,"
);

check_output!(
    int_list_identity_survives_callee_mutation_in_loop,
    INT_LIST_SOURCE,
    "1,2,3,"
);

check_optimized_output!(
    int_list_identity_survives_callee_mutation_in_loop_at_o2,
    INT_LIST_SOURCE,
    "1,2,3,"
);

check_output!(
    string_list_identity_survives_callee_mutation_in_loop,
    r#"
    function put(xs: string[], value: string) -> int {
        xs.push(value)
        xs.length()
    }

    function main() -> string {
        let xs: string[] = []
        let out = ""
        let i = 0
        while (i < 3) {
            let n = put(xs, "v" + i.to_string())
            out = out + n.to_string() + ","
            i += 1
        }
        out
    }
    "#,
    "1,2,3,"
);

check_output!(
    nested_list_identity_survives_callee_mutation_in_loop,
    r#"
    function put(xs: int[][], value: int) -> int {
        xs.push([value])
        xs.length()
    }

    function main() -> string {
        let xs: int[][] = []
        let out = ""
        let i = 0
        while (i < 3) {
            let n = put(xs, i)
            out = out + n.to_string() + ","
            i += 1
        }
        out
    }
    "#,
    "1,2,3,"
);

check_output!(
    class_identity_survives_callee_field_mutation_in_loop,
    r#"
    class Counter {
        n int
    }

    function bump(counter: Counter) -> int {
        counter.n = counter.n + 1
        counter.n
    }

    function main() -> string {
        let counter = Counter { n: 0 }
        let out = ""
        let i = 0
        while (i < 3) {
            let n = bump(counter)
            out = out + n.to_string() + ","
            i += 1
        }
        out
    }
    "#,
    "1,2,3,"
);

check_output!(
    class_with_map_identity_survives_callee_mutation_in_loop,
    r#"
    class Box {
        items map<string, int>
    }

    function put(box: Box, key: string) -> int {
        box.items.set(key, 1)
        box.items.length()
    }

    function main() -> string {
        let box = Box { items: {} }
        let out = ""
        let i = 0
        while (i < 3) {
            let n = put(box, "k" + i.to_string())
            out = out + n.to_string() + ","
            i += 1
        }
        out
    }
    "#,
    "1,2,3,"
);

check_output!(
    class_with_list_identity_survives_callee_mutation_in_loop,
    r#"
    class Acc {
        items string[]
    }

    function put(acc: Acc, value: string) -> int {
        acc.items.push(value)
        acc.items.length()
    }

    function main() -> string {
        let acc = Acc { items: [] }
        let out = ""
        let i = 0
        while (i < 3) {
            let n = put(acc, "v" + i.to_string())
            out = out + n.to_string() + ","
            i += 1
        }
        out
    }
    "#,
    "1,2,3,"
);
