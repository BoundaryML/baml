//! Regression test for the racing-Array-mutation crash that motivated the
//! lazy biased mutex on `Object::Array`.
//!
//! Before this PR: two BAML fibers each pushing to the same shared array
//! could corrupt `Vec`'s internal `(ptr, len, cap)` state — lost pushes,
//! `Vec` `debug_assert!` SIGTRAP, or use-after-free if a grow happened
//! mid-write.
//!
//! After this PR: the per-container `LazyBiasedMutex` serializes mutators.
//! User-visible logic races (lost-push semantics) are still permitted —
//! same contract as JVM `ArrayList`. The runtime itself does not crash.
//!
//! This test does not assert exact final contents (logic races are
//! accepted); it only asserts that no panic / SIGTRAP / SIGSEGV occurred
//! and that the array is in a consistent state at the end.

use std::sync::Arc;

use baml_tests::engine::{OptLevel, compile_source_with_opt};
use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use sys_native::SysOpsExt;

/// Each of N fibers pushes M values to the same shared array. With the
/// mutex in place, the run completes without crashes. The final length is
/// between 0 and N*M (logic-race: a push reading the same `len` as a peer
/// may be clobbered, but the structural invariants of `Vec` always hold).
#[tokio::test]
async fn racing_array_push_does_not_crash() {
    let program = compile_source_with_opt(
        r#"
        function push_n(arr: int[], n: int) -> int {
            let i = 0;
            while i < n {
                arr.push(i);
                i = i + 1;
            };
            arr.length()
        }

        function main() -> int {
            let arr = [];
            let a = spawn { push_n(arr, 200) };
            let b = spawn { push_n(arr, 200) };
            let c = spawn { push_n(arr, 200) };
            let d = spawn { push_n(arr, 200) };
            (await a) + (await b) + (await c) + (await d);
            arr.length()
        }
        "#,
        OptLevel::One,
    );
    let engine = Arc::new(
        BexEngine::new(program, Arc::new(sys_ops::SysOps::native()), Vec::new()).expect("engine"),
    );

    let result = engine
        .call_function_bound_args(
            "user.main",
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;

    // We don't assert exact length — logic races may drop pushes. We do
    // assert the program completed and the result is a valid bounded
    // integer (0 ≤ len ≤ 4 * 200 = 800).
    match result {
        Ok(BexExternalValue::Int(len)) => {
            assert!(
                (0..=800).contains(&len),
                "expected array length in 0..=800, got {len}"
            );
        }
        other => panic!("expected Int result, got {other:?}"),
    }
}

/// Racing push vs pop. After both threads finish, the array is in a valid
/// state (no torn `(ptr, len, cap)` from concurrent grow + remove).
#[tokio::test]
async fn racing_array_push_pop_does_not_crash() {
    let program = compile_source_with_opt(
        r#"
        function push_n(arr: int[], n: int) -> int {
            let i = 0;
            while i < n {
                arr.push(i);
                i = i + 1;
            };
            0
        }

        function pop_n(arr: int[], n: int) -> int {
            let i = 0;
            while i < n {
                arr.pop();
                i = i + 1;
            };
            0
        }

        function main() -> int {
            let arr = [1, 2, 3, 4, 5];
            let p = spawn { push_n(arr, 500) };
            let q = spawn { pop_n(arr, 500) };
            (await p) + (await q);
            arr.length()
        }
        "#,
        OptLevel::One,
    );
    let engine = Arc::new(
        BexEngine::new(program, Arc::new(sys_ops::SysOps::native()), Vec::new()).expect("engine"),
    );

    let result = engine
        .call_function_bound_args(
            "user.main",
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;

    // Initial length 5, 500 pushes, 500 pops. Final length is bounded but
    // not deterministic under racing pops.
    match result {
        Ok(BexExternalValue::Int(len)) => {
            assert!(
                (0..=505).contains(&len),
                "expected array length in 0..=505, got {len}"
            );
        }
        other => panic!("expected Int result, got {other:?}"),
    }
}

/// Racing direct index access (`arr[i]`) against grow-triggering pushes.
/// Exercises the `LoadArrayElement` opcode path (not the codegen'd
/// `Array.at(i)` builtin) — that opcode acquires the container lock so a
/// racing grow can't tear the `(ptr, len, cap)` triple under the reader.
#[tokio::test]
async fn racing_array_index_read_vs_grow_does_not_crash() {
    let program = compile_source_with_opt(
        r#"
        function grow_n(arr: int[], n: int) -> int {
            let i = 0;
            while i < n {
                arr.push(i);
                i = i + 1;
            };
            0
        }

        function read_n(arr: int[], n: int) -> int {
            let i = 0;
            let acc = 0;
            while i < n {
                let len = arr.length();
                if len > 0 {
                    acc = acc + arr[0];
                };
                i = i + 1;
            };
            acc
        }

        function main() -> int {
            let arr = [1];
            let g = spawn { grow_n(arr, 1000) };
            let r1 = spawn { read_n(arr, 500) };
            let r2 = spawn { read_n(arr, 500) };
            (await g) + (await r1) + (await r2);
            arr.length()
        }
        "#,
        OptLevel::One,
    );
    let engine = Arc::new(
        BexEngine::new(program, Arc::new(sys_ops::SysOps::native()), Vec::new()).expect("engine"),
    );

    let result = engine
        .call_function_bound_args(
            "user.main",
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;

    match result {
        Ok(BexExternalValue::Int(len)) => {
            assert!(
                (1..=1001).contains(&len),
                "expected array length in 1..=1001, got {len}"
            );
        }
        other => panic!("expected Int result, got {other:?}"),
    }
}

/// Racing `map.set` and `map.delete`. The map's internal hash chains
/// should not form cycles or torn pointers under the lazy biased mutex.
#[tokio::test]
async fn racing_map_set_delete_does_not_crash() {
    let program = compile_source_with_opt(
        r#"
        function set_n(m: map<string, int>, base: string, n: int) -> int {
            let i = 0;
            while i < n {
                m.set(base, i);
                i = i + 1;
            };
            0
        }

        function del_n(m: map<string, int>, base: string, n: int) -> int {
            let i = 0;
            while i < n {
                m.delete(base);
                i = i + 1;
            };
            0
        }

        function main() -> int {
            let m: map<string, int> = {};
            let s = spawn { set_n(m, "a", 200) };
            let t = spawn { set_n(m, "b", 200) };
            let d = spawn { del_n(m, "a", 200) };
            (await s) + (await t) + (await d);
            m.length()
        }
        "#,
        OptLevel::One,
    );
    let engine = Arc::new(
        BexEngine::new(program, Arc::new(sys_ops::SysOps::native()), Vec::new()).expect("engine"),
    );

    let result = engine
        .call_function_bound_args(
            "user.main",
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;

    // Only two distinct keys ("a" and "b") are ever inserted, so the final
    // size is bounded by the key cardinality regardless of how the
    // racing inserts/deletes interleave.
    match result {
        Ok(BexExternalValue::Int(len)) => {
            assert!(
                (0..=2).contains(&len),
                "expected map length in 0..=2, got {len}"
            );
        }
        other => panic!("expected Int result, got {other:?}"),
    }
}

/// Racing byte-array pushes have the same structural risk as normal arrays:
/// `Vec<u8>` can reallocate while another fiber is reading or mutating it.
#[tokio::test]
async fn racing_uint8array_push_does_not_crash() {
    let program = compile_source_with_opt(
        r#"
        function push_n(data: uint8array, n: int) -> int {
            let i = 0;
            while i < n {
                data.push(i);
                i = i + 1;
            };
            data.length()
        }

        function main() -> int {
            let data = b"";
            let a = spawn { push_n(data, 200) };
            let b = spawn { push_n(data, 200) };
            let c = spawn { push_n(data, 200) };
            let d = spawn { push_n(data, 200) };
            (await a) + (await b) + (await c) + (await d);
            data.length()
        }
        "#,
        OptLevel::One,
    );
    let engine = Arc::new(
        BexEngine::new(program, Arc::new(sys_ops::SysOps::native()), Vec::new()).expect("engine"),
    );

    let result = engine
        .call_function_bound_args(
            "user.main",
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;

    match result {
        Ok(BexExternalValue::Int(len)) => {
            assert!(
                (0..=800).contains(&len),
                "expected uint8array length in 0..=800, got {len}"
            );
        }
        other => panic!("expected Int result, got {other:?}"),
    }
}

#[tokio::test]
async fn racing_uint8array_push_pop_does_not_crash() {
    let program = compile_source_with_opt(
        r#"
        function push_n(data: uint8array, n: int) -> int {
            let i = 0;
            while i < n {
                data.push(i);
                i = i + 1;
            };
            0
        }

        function pop_n(data: uint8array, n: int) -> int {
            let i = 0;
            while i < n {
                data.pop();
                i = i + 1;
            };
            0
        }

        function main() -> int {
            let data = b"\x01\x02\x03\x04\x05";
            let p = spawn { push_n(data, 500) };
            let q = spawn { pop_n(data, 500) };
            (await p) + (await q);
            data.length()
        }
        "#,
        OptLevel::One,
    );
    let engine = Arc::new(
        BexEngine::new(program, Arc::new(sys_ops::SysOps::native()), Vec::new()).expect("engine"),
    );

    let result = engine
        .call_function_bound_args(
            "user.main",
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;

    match result {
        Ok(BexExternalValue::Int(len)) => {
            assert!(
                (0..=505).contains(&len),
                "expected uint8array length in 0..=505, got {len}"
            );
        }
        other => panic!("expected Int result, got {other:?}"),
    }
}

#[tokio::test]
async fn racing_uint8array_index_read_vs_grow_does_not_crash() {
    let program = compile_source_with_opt(
        r#"
        function grow_n(data: uint8array, n: int) -> int {
            let i = 0;
            while i < n {
                data.push(i);
                i = i + 1;
            };
            0
        }

        function read_n(data: uint8array, n: int) -> int {
            let i = 0;
            let acc = 0;
            while i < n {
                let len = data.length();
                if len > 0 {
                    acc = acc + data[0];
                };
                i = i + 1;
            };
            acc
        }

        function main() -> int {
            let data = b"\x01";
            let g = spawn { grow_n(data, 1000) };
            let r1 = spawn { read_n(data, 500) };
            let r2 = spawn { read_n(data, 500) };
            (await g) + (await r1) + (await r2);
            data.length()
        }
        "#,
        OptLevel::One,
    );
    let engine = Arc::new(
        BexEngine::new(program, Arc::new(sys_ops::SysOps::native()), Vec::new()).expect("engine"),
    );

    let result = engine
        .call_function_bound_args(
            "user.main",
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;

    match result {
        Ok(BexExternalValue::Int(len)) => {
            assert!(
                (1..=1001).contains(&len),
                "expected uint8array length in 1..=1001, got {len}"
            );
        }
        other => panic!("expected Int result, got {other:?}"),
    }
}

#[tokio::test]
async fn racing_uint8array_sort_vs_grow_does_not_crash() {
    let program = compile_source_with_opt(
        r#"
        function push_n(data: uint8array, n: int) -> int {
            let i = 0;
            while i < n {
                data.push(i);
                i = i + 1;
            };
            0
        }

        function sort_n(data: uint8array, n: int) -> int {
            let i = 0;
            while i < n {
                data.sort();
                i = i + 1;
            };
            0
        }

        function main() -> int {
            let data = b"\x03\x02\x01";
            let p = spawn { push_n(data, 300) };
            let s = spawn { sort_n(data, 100) };
            (await p) + (await s);
            data.length()
        }
        "#,
        OptLevel::One,
    );
    let engine = Arc::new(
        BexEngine::new(program, Arc::new(sys_ops::SysOps::native()), Vec::new()).expect("engine"),
    );

    let result = engine
        .call_function_bound_args(
            "user.main",
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;

    match result {
        Ok(BexExternalValue::Int(len)) => {
            assert!(
                (3..=303).contains(&len),
                "expected uint8array length in 3..=303, got {len}"
            );
        }
        other => panic!("expected Int result, got {other:?}"),
    }
}

#[tokio::test]
async fn racing_captured_local_increment_does_not_crash() {
    let program = compile_source_with_opt(
        r#"
        function main() -> int {
            let value = 0;
            let a = spawn {
                let i = 0;
                while i < 400 {
                    value = value + 1;
                    i = i + 1;
                };
                0
            };
            let b = spawn {
                let i = 0;
                while i < 400 {
                    value = value + 1;
                    i = i + 1;
                };
                0
            };
            (await a) + (await b);
            value
        }
        "#,
        OptLevel::One,
    );
    let engine = Arc::new(
        BexEngine::new(program, Arc::new(sys_ops::SysOps::native()), Vec::new()).expect("engine"),
    );

    let result = engine
        .call_function_bound_args(
            "user.main",
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;

    match result {
        Ok(BexExternalValue::Int(value)) => {
            assert!(
                (0..=800).contains(&value),
                "expected captured local value in 0..=800, got {value}"
            );
        }
        other => panic!("expected Int result, got {other:?}"),
    }
}

#[tokio::test]
async fn racing_class_field_increment_does_not_crash() {
    let program = compile_source_with_opt(
        r#"
        class Counter {
            value int
        }

        function bump(c: Counter, n: int) -> int {
            let i = 0;
            while i < n {
                c.value = c.value + 1;
                i = i + 1;
            };
            c.value
        }

        function main() -> int {
            let c = Counter { value: 0 };
            let a = spawn { bump(c, 400) };
            let b = spawn { bump(c, 400) };
            (await a) + (await b);
            c.value
        }
        "#,
        OptLevel::One,
    );
    let engine = Arc::new(
        BexEngine::new(program, Arc::new(sys_ops::SysOps::native()), Vec::new()).expect("engine"),
    );

    let result = engine
        .call_function_bound_args(
            "user.main",
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;

    match result {
        Ok(BexExternalValue::Int(value)) => {
            assert!(
                (0..=800).contains(&value),
                "expected class field value in 0..=800, got {value}"
            );
        }
        other => panic!("expected Int result, got {other:?}"),
    }
}
