//! Interface calls the per-site dispatch cache answers: the VM enters the
//! callee directly, a generic impl's frame shares the cached type arguments,
//! and a site remembers the receiver object it last dispatched on. These run
//! with telemetry off, where that path is taken, and with the dispatch fast
//! path on and off.

use std::sync::{Arc, atomic::AtomicBool};

use baml_test_support::compile_source;
use bex_vm::{BexVm, VmExecState};

/// Run `main` with the fast path on and off; both must agree.
fn run(source: &str) -> i64 {
    let results = [true, false].map(|fast_path| {
        let program = compile_source(source);
        let entry = program.rendered_callables()["user.main"].object.raw();
        let mut vm =
            BexVm::from_program(program, Arc::new(AtomicBool::new(false))).expect("valid program");
        vm.dispatch_fast_path = fast_path;
        vm.disable_telemetry();
        vm.set_entry_point(vm.heap.compile_time_ptr(entry), &[]);
        loop {
            match vm.exec().expect("execution succeeds") {
                VmExecState::Complete(value) => break value.as_int().expect("integer result"),
                VmExecState::EarlyYield => {}
                other => panic!("expected completion, got {other:?}"),
            }
        }
    });
    assert_eq!(results[0], results[1], "fast path and full dispatch differ");
    results[0]
}

/// Adapter chains (`filter`, `map`, `reduce`, `take`) call their source's
/// `next` through the cache on every element, with new adapter objects
/// each round, and a `for` loop over a user iterator does the same.
#[test]
fn iterator_adapters_and_user_iterators_agree_with_plain_loops() {
    let source = r#"
        class Window {
            values: int[]
            pos: int
            end: int

            implements baml.iter.Iterable {
                type Item = int
                type Error = never

                function iter(self) -> baml.iter.Iterator<Item = int, Error = never> throws never {
                    self
                }
            }

            implements baml.iter.Iterator {
                type Item = int
                type Error = never

                function next(self) -> int | baml.iter.Done throws never {
                    if (self.pos >= self.end) {
                        return baml.iter.Done {};
                    }
                    let v = self.values[self.pos];
                    self.pos += 1;
                    v
                }
            }
        }

        function numbers() -> int[] {
            let xs: int[] = [];
            let seed = 5;
            for (let i = 0; i < 300; i += 1) {
                seed = seed * 16807 % 2147483647;
                xs.push(seed % 1000);
            }
            xs
        }

        function with_iterators(xs: int[]) -> int {
            let acc = 0;
            for (let round = 0; round < 4; round += 1) {
                let skip = round % 3;
                acc += xs.iter()
                    .filter((x: int) -> bool { x % 3 != skip })
                    .map((x: int) -> int { x * 2 + skip })
                    .reduce((a: int, x: int) -> int { a + x }, 0);
                acc += xs.iter().take(50 + round).map((x: int) -> int { x + round }).count();
                for (let v in Window { values: xs, pos: round * 10, end: round * 10 + 100 }) {
                    acc += v % 17;
                }
            }
            acc
        }

        function with_loops(xs: int[]) -> int {
            let acc = 0;
            for (let round = 0; round < 4; round += 1) {
                let skip = round % 3;
                for (let i = 0; i < xs.length(); i += 1) {
                    if (xs[i] % 3 != skip) {
                        acc += xs[i] * 2 + skip;
                    }
                }
                acc += 50 + round;
                for (let i = round * 10; i < round * 10 + 100; i += 1) {
                    acc += xs[i] % 17;
                }
            }
            acc
        }

        function main() -> int {
            let xs = numbers();
            let a = with_iterators(xs);
            let b = with_loops(xs);
            if (a == b && a > 0) { 1 } else { 0 }
        }
    "#;
    assert_eq!(run(source), 1);
}

/// One site calling a generic impl on two instantiations in turn: each call
/// reads its own receiver's type argument, and a `let type` in the body (a
/// write to the frame's type arguments) stays in that call.
#[test]
fn generic_impl_calls_keep_their_own_type_arguments() {
    let source = r#"
        interface DescribesWith {
            function describe_with(self, extra: reflect.Type) -> string throws never
        }

        class Tagged<T> {
            value: T

            implements DescribesWith {
                function describe_with(self, extra: reflect.Type) -> string {
                    let before = reflect.Type.of<T>().to_string();
                    let type X = extra;
                    before + "/" + reflect.Type.of<X>().to_string() + "/" + reflect.Type.of<T>().to_string()
                }
            }
        }

        function main() -> int {
            let items: DescribesWith[] = [Tagged<int> { value: 1 }, Tagged<string> { value: "a" }];
            let out = "";
            for (let i = 0; i < 6; i += 1) {
                out += items[i % 2].describe_with(bool.type()) + ";";
            }
            let same = items[0];
            for (let i = 0; i < 3; i += 1) {
                out += same.describe_with(float.type()) + ";";
            }
            let expected = "int/bool/int;string/bool/string;int/bool/int;string/bool/string;int/bool/int;string/bool/string;int/float/int;int/float/int;int/float/int;";
            if (out == expected) { 1 } else { 0 }
        }
    "#;
    assert_eq!(run(source), 1);
}
