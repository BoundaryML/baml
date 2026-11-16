//! The dispatch loop's fast path must not change what a program computes.
//!
//! Every program here runs with the fast path on and off
//! (`BexVm::dispatch_fast_path`), with telemetry present and absent (the
//! fast path runs plain calls and returns only without it, as with
//! `BAML_TELEMETRY=off`), and at several early-yield intervals with a park
//! request pending, so execution hands off between the fast path, the full
//! dispatch and the engine loop at many different instructions. The result
//! and the number of early yields must be identical in every mode: the yield
//! count also checks that both paths tick the early-yield counter at the
//! same instructions.
//!
//! The programs cover what the fast path runs (local and constant traffic,
//! int arithmetic and comparisons, truthiness, branches, field loads, exact
//! calls and returns) together with what it hands back (overflow, heap
//! truthiness, generic, indirect and native calls, closures, allocation,
//! array and map operations, strings, exceptions, deep recursion).

use std::sync::{Arc, atomic::AtomicBool};

use baml_test_support::compile_source;
use bex_vm::{BexVm, VmExecState};

const PROGRAMS: &[(&str, &str)] = &[
    (
        "scalar loops",
        r#"
        function fib(n: int) -> int {
            let a = 0;
            let b = 1;
            for (let i = 0; i < n; i += 1) { let t = b; b = a + b; a = t; }
            a
        }
        function main() -> int {
            let s = 0;
            for (let i = 0; i < 30; i += 1) {
                for (let j = 0; j < 20; j += 1) {
                    if (i * j > 100 && i != j) { s += i - j; } else { s -= 1; }
                    if (i <= j || j >= 19) { s += 2; }
                }
            }
            s + fib(40)
        }
        "#,
    ),
    (
        "collatz and modulo",
        r#"
        function steps(n: int) -> int {
            let x = n;
            let k = 0;
            while (x != 1) {
                if (x % 2 == 0) { x = x / 2; } else { x = 3 * x + 1; }
                k += 1;
            }
            k
        }
        function main() -> int {
            let total = 0;
            for (let n = 1; n < 300; n += 1) { total += steps(n); }
            total
        }
        "#,
    ),
    (
        "recursion and plain calls",
        r#"
        function fib(n: int) -> int { if (n <= 1) { n } else { fib(n - 1) + fib(n - 2) } }
        function leaf(n: int) -> int { n }
        function twice(n: int) -> int { leaf(n) + leaf(n) }
        function main() -> int {
            let s = 0;
            for (let i = 0; i < 200; i += 1) { s += twice(i); }
            s + fib(16)
        }
        "#,
    ),
    (
        "classes, fields and methods",
        r#"
        class Vec2 {
            x: int
            y: int
            function norm2(self) -> int { self.x * self.x + self.y * self.y }
        }
        class Node { left: Node?, right: Node? }
        function build(depth: int) -> Node {
            if (depth <= 0) { return Node { left: null, right: null }; }
            Node { left: build(depth - 1), right: build(depth - 1) }
        }
        function count(node: Node) -> int {
            let l = node.left;
            let r = node.right;
            if (l == null || r == null) { return 1; }
            1 + count(l) + count(r)
        }
        function main() -> int {
            let acc = 0;
            for (let i = 0; i < 100; i += 1) {
                let v = Vec2 { x: i, y: i + 1 };
                acc += v.norm2();
            }
            acc + count(build(8))
        }
        "#,
    ),
    (
        "arrays and maps",
        r#"
        function sort(xs: int[]) -> int[] {
            let n = xs.length();
            for (let i = 0; i < n; i += 1) {
                for (let j = 0; j < n - 1 - i; j += 1) {
                    if (xs[j] > xs[j + 1]) { let t = xs[j]; xs[j] = xs[j + 1]; xs[j + 1] = t; }
                }
            }
            xs
        }
        function main() -> int {
            let xs: int[] = [];
            for (let i = 0; i < 60; i += 1) { xs.push((i * 37) % 61); }
            let sorted = sort(xs);
            let m: map<string, int> = {};
            for (let i = 0; i < 40; i += 1) { m[`k${i % 7}`] = i; }
            let s = 0;
            for (let i = 0; i < sorted.length(); i += 1) { s += sorted[i] * i; }
            s + m["k3"] + m.length()
        }
        "#,
    ),
    (
        "closures, indirect, generic and native calls",
        r#"
        function leaf(n: int) -> int { n }
        function add(a: int, b: int = 10, c: int = 20) -> int { a + b + c }
        function invoke(f: (int) -> int, n: int) -> int { f(n) }
        function id<T>(x: T) -> T { x }
        function main() -> int {
            let k = 3;
            let plus = (x: int) -> int { x + k };
            let values = [1, 2, 3].map((n: int) -> int { leaf(n) * 2 });
            let s = 0;
            for (let i = 0; i < 50; i += 1) { s += invoke(plus, i) + id<int>(i) + add(i); }
            s + values[0] + values[1] + values[2] + invoke(leaf, 4)
        }
        "#,
    ),
    (
        "truthiness and short circuits",
        r#"
        function score(flag: bool, n: int, s: string, xs: int[]) -> int {
            let r = 0;
            if (!flag) { r += 1; }
            if (!n) { r += 10; }
            if (!s) { r += 100; }
            if (!xs) { r += 1000; }
            r
        }
        function pick(a: int?, b: int?) -> int { a ?? b ?? -1 }
        function main() -> int {
            let s = score(false, 0, "", []) + 10000 * score(true, 5, "x", [1]);
            for (let i = 0; i < 40; i += 1) {
                let a: int? = null;
                if (i % 3 == 0) { a = i; }
                let b: int? = null;
                if (i % 5 == 0) { b = 100; }
                s += pick(a, b);
                if (i > 10 && i < 20) { s += 1000; }
                if (i < 2 || i > 37) { s += 7; }
            }
            s
        }
        "#,
    ),
    (
        "overflow and exceptions",
        r#"
        function grow(seed: int) -> int {
            let x = seed;
            while (true) { x = x * 3; }
            x
        }
        function inc(seed: int) -> int {
            let x = seed;
            while (true) { x = x + 4611686018427387; }
            x
        }
        function dec(seed: int) -> int {
            let x = seed;
            while (true) { x = x - 4611686018427387; }
            x
        }
        function recurse(n: int) -> int { recurse(n + 1) }
        function main() -> int {
            let a = grow(1) catch (e) { baml.panics.IntegerOverflow => 1 };
            let b = inc(0) catch (e) { baml.panics.IntegerOverflow => 20 };
            let c = dec(0) catch (e) { baml.panics.IntegerOverflow => 300 };
            let d = recurse(0) catch (e) { baml.panics.StackOverflow => 4000 };
            a + b + c + d
        }
        "#,
    ),
    (
        "strings",
        r#"
        function main() -> int {
            let s = "";
            for (let i = 0; i < 30; i += 1) { s = s + `${i},`; }
            let parts = s.split(",");
            let total = 0;
            for (let i = 0; i < parts.length(); i += 1) { total += parts[i].length(); }
            total
        }
        "#,
    ),
];

/// The modes every program runs in: the fast path on or off, and an
/// early-yield interval with a park request pending (`None`: no request).
const INTERVALS: &[Option<u64>] = &[None, Some(1), Some(3), Some(64)];

/// Run `main` to completion, returning its result (or the error's text) and
/// how many times it yielded.
fn run(
    source: &str,
    fast_path: bool,
    telemetry: bool,
    interval: Option<u64>,
) -> (Result<i64, String>, usize) {
    let program = compile_source(source);
    let entry = program.rendered_callables()["user.main"].object.raw();
    let flag = Arc::new(AtomicBool::new(interval.is_some()));
    let mut vm = BexVm::from_program(program, Arc::clone(&flag)).expect("valid program");
    vm.dispatch_fast_path = fast_path;
    if !telemetry {
        vm.disable_telemetry();
    }
    if let Some(interval) = interval {
        vm.early_yield = bex_vm_types::EarlyYieldCheck::with_interval(Arc::clone(&flag), interval);
    }
    vm.set_entry_point(vm.heap.compile_time_ptr(entry), &[]);
    let mut yields = 0;
    loop {
        match vm.exec() {
            Ok(VmExecState::EarlyYield) => {
                yields += 1;
                // Keep yielding at every poll, but bound the run.
                assert!(yields < 5_000_000, "runaway program");
            }
            Ok(VmExecState::Complete(value)) => {
                return (value.as_int().ok_or_else(|| format!("{value:?}")), yields);
            }
            Ok(other) => return (Err(format!("unexpected state {other:?}")), yields),
            Err(error) => return (Err(format!("{error}")), yields),
        }
    }
}

#[test]
fn fast_path_matches_full_dispatch() {
    for (name, source) in PROGRAMS {
        let expected = run(source, false, true, None).0;
        assert!(expected.is_ok(), "{name}: {expected:?}");
        for telemetry in [true, false] {
            for &interval in INTERVALS {
                let full = run(source, false, telemetry, interval);
                let fast = run(source, true, telemetry, interval);
                assert_eq!(
                    fast, full,
                    "{name}, telemetry {telemetry}, early-yield interval {interval:?}: \
                     (result, yields) with the fast path differ from the full dispatch"
                );
                // Neither telemetry nor a park request changes the result.
                assert_eq!(
                    full.0, expected,
                    "{name}, telemetry {telemetry}, {interval:?}"
                );
            }
        }
    }
}
