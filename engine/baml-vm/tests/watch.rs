//! VM tests for watch functionality.

mod common;
use common::{assert_vm_emits, Notification, WatchProgram};

#[test]
fn notify_primitive_on_change() -> anyhow::Result<()> {
    assert_vm_emits(WatchProgram {
        source: r#"
            function primitive() -> int {
                watch let value = 0;

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
    assert_vm_emits(WatchProgram {
        source: r#"
            function primitive() -> int {
                watch let value = 0;

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
    assert_vm_emits(WatchProgram {
        source: r#"
            class Point {
                x int
                y int
            }

            function scope_exit() -> Point {
                let outter_point =  {
                    watch let point = Point { x: 0, y: 0 };
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
    assert_vm_emits(WatchProgram {
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
                watch let point = Point { x: 0, y: 0 };
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

#[test]
fn notify_on_change_with_alias() -> anyhow::Result<()> {
    assert_vm_emits(WatchProgram {
        source: r#"
            class Point {
                x int
                y int
            }

            function alias() -> Point {
                watch let point = Point { x: 0, y: 0 };
                let alias = point;

                alias.x = 1; // Notify

                point
            }
        "#,
        function: "alias",
        expected: vec![vec![Notification::on_channel("point")]],
    })
}

#[test]
fn notify_on_change_with_alias_in_nested_scope() -> anyhow::Result<()> {
    assert_vm_emits(WatchProgram {
        source: r#"
            class Point {
                x int
                y int
            }

            function nested_alias() -> Point {
                watch let point = Point { x: 0, y: 0 };
                if (true) {
                    let alias = point;
                    alias.x = 1; // Notify
                }

                point
            }
        "#,
        function: "nested_alias",
        expected: vec![vec![Notification::on_channel("point")]],
    })
}

#[test]
fn notify_when_nested_object_is_modified_after_addtion() -> anyhow::Result<()> {
    assert_vm_emits(WatchProgram {
        source: r#"
            class Value {
                value int
            }

            class Point {
                x Value
                y Value
            }

            class Vec2D {
                p Point
                q Point
            }

            function nested_object_added() -> Vec2D {
                watch let vec = Vec2D {
                    p: Point { x: Value { value: 0 }, y: Value { value: 0 } },
                    q: Point { x: Value { value: 0 }, y: Value { value: 0 } },
                };

                let p = Point { x: Value { value: 1 }, y: Value { value: 1 } };

                vec.p = p; // Notify here.
                p.x.value = 2; // Notify here too.

                vec
            }
        "#,
        function: "nested_object_added",
        expected: vec![
            vec![Notification::on_channel("vec")],
            vec![Notification::on_channel("vec")],
        ],
    })
}

#[test]
fn dont_notify_when_nested_object_is_modified_after_removal() -> anyhow::Result<()> {
    assert_vm_emits(WatchProgram {
        source: r#"
            class Value {
                value int
            }

            class Point {
                x Value
                y Value
            }

            class Vec2D {
                p Point
                q Point
            }

            function nested_object_removed() -> Vec2D {
                watch let vec = Vec2D {
                    p: Point { x: Value { value: 0 }, y: Value { value: 0 } },
                    q: Point { x: Value { value: 0 }, y: Value { value: 0 } },
                };

                let p = vec.p;

                vec.p = Point { x: Value { value: 1 }, y: Value { value: 1 } }; // Notify once here

                p.x.value = 2; // No notify here

                vec
            }
        "#,
        function: "nested_object_removed",
        expected: vec![vec![Notification::on_channel("vec")]],
    })
}

// Complicated case from the edge cases doc.
#[test]
fn cyclic_graph() -> anyhow::Result<()> {
    assert_vm_emits(WatchProgram {
        source: r#"
            class Vertex {
                edges Vertex[]
                value int
            }

            function cycle() -> int {
                let v1 = Vertex { value: 1, edges: [] };
                watch let v2 = Vertex { value: 2, edges: [] };
                let v3 = Vertex { value: 3, edges: [] };
                watch let v4 = Vertex { value: 4, edges: [] };

                // NO EMIT (neither v2 nor v4 have changed)
                v1.edges = [v2];

                // EMIT v2
                v2.edges = [v3];

                // EMIT v2
                v3.edges = [v4];

                // EMIT [v2, v4]
                v4.edges = [v1];

                // EMIT [v2, v4]
                v2.value = 20;

                // EMIT [v2, v4]
                v1.value = 10;

                // EMIT [v2, v4]
                v3.value = 30;

                0
            }
        "#,
        function: "cycle",
        expected: vec![
            // v2.edges = [v3];
            vec![Notification::on_channel("v2")],
            // v3.edges = [v4];
            vec![Notification::on_channel("v2")],
            // v4.edges = [v1];
            vec![
                Notification::on_channel("v2"),
                Notification::on_channel("v4"),
            ],
            // v2.value = 20;
            vec![
                Notification::on_channel("v2"),
                Notification::on_channel("v4"),
            ],
            // v1.value = 10;
            vec![
                Notification::on_channel("v2"),
                Notification::on_channel("v4"),
            ],
            // v3.value = 30;
            vec![
                Notification::on_channel("v2"),
                Notification::on_channel("v4"),
            ],
        ],
    })
}

#[test]
fn run_user_filter() -> anyhow::Result<()> {
    assert_vm_emits(WatchProgram {
        source: r#"
            function greater_than_five(value: int) -> bool {
                value > 5
            }

            function primitive() -> int {
                watch let value = 0;
                value.$watch.options(baml.WatchOptions { when: greater_than_five });

                value = 1; // No notify
                value = 6; // Notify

                value
            }
        "#,
        function: "primitive",
        expected: vec![vec![Notification::on_channel("value")]],
    })
}

#[test]
fn run_default_filter() -> anyhow::Result<()> {
    assert_vm_emits(WatchProgram {
        source: r#"
            function primitive() -> int {
                watch let value = 0;

                value = 0; // No notify
                value = 6; // Notify

                value
            }
        "#,
        function: "primitive",
        expected: vec![vec![Notification::on_channel("value")]],
    })
}

#[test]
fn manual_notify() -> anyhow::Result<()> {
    assert_vm_emits(WatchProgram {
        source: r#"
            function primitive() -> int {
                watch let value = 0;
                value.$watch.options(baml.WatchOptions { when: "manual" });

                value = 1; // No notify
                value = 2; // No notify
                value = 3; // No notify

                value.$watch.notify(); // Notify

                value
            }
        "#,
        function: "primitive",
        expected: vec![vec![Notification::on_channel("value")]],
    })
}
