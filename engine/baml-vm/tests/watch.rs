//! VM tests for watch functionality.

mod common;
use common::{assert_vm_emits, EmitProgram, Notification};

#[test]
fn notify_primitive_on_change() -> anyhow::Result<()> {
    assert_vm_emits(EmitProgram {
        source: r#"
            function primitive() -> int {
                let value = 0 @watch;

                value = 1;

                value
            }
        "#,
        function: "primitive",
        expected: vec![vec![Notification::on_channel("value")]],
    })
}

#[test]
fn notify_primitive_on_nested_scope() -> anyhow::Result<()> {
    assert_vm_emits(EmitProgram {
        source: r#"
            function primitive() -> int {
                let value = 0 @watch;

                if (true) {
                    value = 1;
                }

                value
            }
        "#,
        function: "primitive",
        expected: vec![vec![Notification::on_channel("value")]],
    })
}

#[test]
fn stop_notifying_on_scope_exit() -> anyhow::Result<()> {
    assert_vm_emits(EmitProgram {
        source: r#"
            class Point {
                x int
                y int
            }

            function scope_exit() -> Point {
                let outter_point =  {
                    let point = Point { x: 0, y: 0 } @watch;
                    point.x = 1; // Expect only one notification here.
                    point
                };

                outter_point.x = 2; // No notify

                outter_point
            }
        "#,
        function: "scope_exit",
        expected: vec![vec![Notification::on_channel("point")]],
    })
}

#[test]
fn notify_on_function_call_modifications() -> anyhow::Result<()> {
    assert_vm_emits(EmitProgram {
        source: r#"
            class Point {
                x int
                y int

                function set(self, x: int, y: int) -> Point {
                    // Expect two notifications here.
                    self.x = x;
                    self.y = y;
                    self
                }
            }

            function call_function() -> Point {
                let point = Point { x: 0, y: 0 } @watch;
                point.set(1, 2);
                point
            }
        "#,
        function: "call_function",
        expected: vec![
            vec![Notification::on_channel("point")],
            vec![Notification::on_channel("point")],
        ],
    })
}
