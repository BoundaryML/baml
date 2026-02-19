//! VM tests for field assignments and complex assignment scenarios.

use baml_tests::bytecode::{
    ExecState, FailingProgram, Program, Value, assert_vm_executes, assert_vm_fails,
};
use bex_vm::RuntimeError;

// Block expressions
#[test]
fn block_expr() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: "
            function main() -> int {
                let a = {
                    let b = 1;
                    b
                };

                a
            }
        ",
        function: "main",
        expected: ExecState::Complete(Value::Int(1)),
    })
}

// Variable mutation
#[test]
fn mutable_var_in_function() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main() -> int {
                let y = 3;
                y = 5;
                y
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(5)),
    })
}

#[test]
fn mutable_param() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function MutableInArg(x: int) -> int {
                x = 3;
                x
            }

            function main() -> int {
                let r = MutableInArg(42);
                r
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(3)),
    })
}

// Regression: Virtual inlining must not re-evaluate rvalue when a dependency
// is modified in an intermediate block between def and use.
#[test]
fn virtual_cross_block_soundness() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main(c: bool) -> int {
                let a = 1;
                let b = a;
                if (c) {
                    a = 2;
                }
                b
            }

            function entry() -> int {
                main(true)
            }
        "#,
        function: "entry",
        expected: ExecState::Complete(Value::Int(1)),
    })
}

// Regression: Virtual inlining of a rvalue that reads a parameter must not
// re-evaluate when the parameter is reassigned on an intermediate path.
// Parameters have an implicit entry definition NOT tracked in `all_defs`,
// so even a single explicit reassignment means multiple definitions.
#[test]
fn virtual_cross_block_param_mutation_soundness() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main(c: bool, p: int) -> int {
                let x = p;
                if (c) {
                    p = 2;
                }
                x
            }

            function entry() -> int {
                main(true, 42)
            }
        "#,
        function: "entry",
        expected: ExecState::Complete(Value::Int(42)),
    })
}

// Regression: CopyOf must not propagate through mutable parameters.
#[test]
fn copy_of_mutable_param_soundness() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main(x: int) -> int {
                let y = x;
                x = 2;
                y
            }

            function entry() -> int {
                main(42)
            }
        "#,
        function: "entry",
        expected: ExecState::Complete(Value::Int(42)),
    })
}

// Regression: cross-block virtualization must account for transitive
// dependencies; `x = t` where `t = p` must not be re-evaluated as latest `p`.
#[test]
fn virtual_cross_block_transitive_param_mutation_soundness() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function main(c: bool, p: int) -> int {
                let t = p;
                let x = t;
                if (c) {
                    p = 2;
                }
                x
            }

            function entry() -> int {
                main(true, 42)
            }
        "#,
        function: "entry",
        expected: ExecState::Complete(Value::Int(42)),
    })
}

// Regression: multiple definitions of a local must not allow inlining that
// skips side-effecting call evaluation.
#[test]
fn virtual_multiple_defs_preserve_side_effects() -> anyhow::Result<()> {
    assert_vm_fails(FailingProgram {
        source: r#"
            function fail() -> int {
                assert(false);
                1
            }

            function main() -> int {
                let x = fail();
                x = 2;
                x
            }
        "#,
        function: "main",
        expected: RuntimeError::AssertionError.into(),
    })
}

// Field assignment operations
#[test]
fn field_assignment_add_assign() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Counter {
                value int
            }
            function main() -> int {
                let c = Counter { value: 10 };
                c.value += 5;
                c.value
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(15)),
    })
}

#[test]
fn field_assignment_sub_assign() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Counter {
                value int
            }
            function main() -> int {
                let c = Counter { value: 20 };
                c.value -= 8;
                c.value
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(12)),
    })
}

#[test]
fn field_assignment_mul_assign() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Counter {
                value int
            }
            function main() -> int {
                let c = Counter { value: 7 };
                c.value *= 3;
                c.value
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(21)),
    })
}

#[test]
fn field_assignment_div_assign() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Counter {
                value int
            }
            function main() -> int {
                let c = Counter { value: 24 };
                c.value /= 4;
                c.value
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(6)),
    })
}

#[test]
fn field_assignment_mod_assign() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Counter {
                value int
            }
            function main() -> int {
                let c = Counter { value: 17 };
                c.value %= 5;
                c.value
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(2)),
    })
}

#[test]
fn field_assignment_simple() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Data {
                value int
                active bool
            }
            function main() -> int {
                let d = Data { value: 100, active: true };
                d.value = 42;
                d.value
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(42)),
    })
}

#[test]
fn field_assignment_multiple_ops() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Stats {
                score int
            }
            function main() -> int {
                let s = Stats { score: 10 };
                s.score += 5;   // 15
                s.score *= 2;   // 30
                s.score -= 10;  // 20
                s.score /= 4;   // 5
                s.score
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(5)),
    })
}

// Nested field assignments
#[test]
fn nested_field_assignment_simple() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Inner {
                value int
            }
            class Outer {
                inner Inner
            }
            function main() -> int {
                let i = Inner { value: 10 };
                let o = Outer { inner: i };
                o.inner.value = 42;
                o.inner.value
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(42)),
    })
}

#[test]
fn nested_field_assignment_compound() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Inner {
                value int
            }
            class Outer {
                inner Inner
            }
            function main() -> int {
                let i = Inner { value: 10 };
                let o = Outer { inner: i };
                o.inner.value += 32;
                o.inner.value
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(42)),
    })
}

#[test]
fn field_assignment_object_field() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Inner {
                value int
            }
            class Outer {
                inner Inner
            }
            function main() -> bool {
                let o = Outer { inner: Inner { value: 10 } };
                o.inner = Inner { value: 20 };
                // For now, test that assignment works, not nested field access
                true
            }"#,
        function: "main",
        expected: ExecState::Complete(Value::Bool(true)),
    })
}

// Array element field assignments
#[test]
fn array_element_field_assignment() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Item {
                count int
            }

            function main() -> int {
                let items = [
                    Item { count: 10 },
                    Item { count: 20 },
                    Item { count: 30 }
                ];

                // Modify field of array element
                items[1].count += 5;
                items[1].count
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(25)), // 20 + 5
    })
}

#[test]
fn array_element_method_field_assignment() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Data {
                value int

                function get_self(self) -> Data {
                    self
                }
            }

            class Container {
                data Data

                function get_data(self) -> Data {
                    self.data
                }
            }

            function main() -> int {
                let containers = [
                    Container { data: Data { value: 10 } },
                    Container { data: Data { value: 20 } },
                    Container { data: Data { value: 30 } }
                ];

                // First test: Can we modify array element's field?
                containers[1].data.value += 5;
                let result1 = containers[1].data.value; // Should be 25

                // Test method call assignment:
                containers[1].get_data().value += 10;
                let result2 = containers[1].data.value; // Should be 35 (25 + 10)

                result2
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(35)), // 20 + 5 + 10
    })
}

#[test]
fn method_call_then_array_access_assignment() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Item {
                value int
            }
            class Container {
                data Item[]
                function get_nested(self) -> Item[] {
                    self.data
                }
            }
            function main() -> int {
                let i1 = Item { value: 10 };
                let i2 = Item { value: 20 };
                let i3 = Item { value: 30 };
                let arr = [i1, i2, i3];
                let obj = Container { data: arr };
                obj.get_nested()[1].value += 5;
                obj.data[1].value
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(25)),
    })
}

#[test]
fn method_call_field_assignment() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Counter {
                value int
            }

            class Factory {
                counter Counter

                function get_counter(self) -> Counter {
                    self.counter
                }
            }

            function main() -> int {
                let f = Factory {
                    counter: Counter { value: 10 }
                };

                f.get_counter().value += 5;

                f.get_counter().value
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(15)),
    })
}

#[test]
fn method_call_field_assignment_with_copy() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Counter {
                value int
            }

            class Factory {
                counter Counter

                function get_counter(self) -> Counter {
                    self.counter
                }
            }

            function main() -> int {
                let f = Factory {
                    counter: Counter { value: 10 }
                };

                let c = f.get_counter();

                c.value += 5;

                c.value
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(15)),
    })
}
