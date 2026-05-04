//! Minimal reproduction tests for "type error: expected int, got any".
//!
//! These tests isolate the components of the tic-tac-toe bug:
//! - Array indexing with non-int values
//! - Null values used as array indices
//! - User-defined function argument type checking gaps
//! - The confusing "any" error message for null values

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

// ============================================================================
// Bytecode snapshots — compare working vs broken emission
// ============================================================================

/// WORKS: 1D array write on a local variable.
/// Compare this bytecode with `bytecode_1d_array_write_through_field` to see
/// the stack ordering difference.
#[tokio::test]
async fn bytecode_1d_array_write_local() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let arr = ["a", "b", "c"];
            arr[1] = "z";
            arr[1]
        }
    "#
    );
    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "a"
        load_const "b"
        load_const "c"
        alloc_array 3
        store_var arr
        load_var arr
        load_const 1
        load_const "z"
        store_array_element
        load_var arr
        load_const 1
        load_array_element
        return
    }
    "#);
    assert!(output.result.is_ok());
}

/// BROKEN: 1D array write through a field on self.
/// The bytecode here should reveal the stack ordering bug.
#[tokio::test]
async fn bytecode_1d_array_write_through_field() {
    let output = baml_test!(
        r#"
        class Container {
            items string[]

            function set(self, idx: int, val: string) -> null {
                self.items[idx] = val;
                null
            }
        }

        function main() -> null {
            let c = Container { items: ["a", "b", "c"] };
            c.set(1, "z");
            null
        }
    "#
    );
    insta::assert_snapshot!(output.bytecode, @r#"
    function Container.set(self: null, idx: int, val: string) -> null {
        load_var self
        load_field .items
        load_var idx
        load_var val
        store_array_element
        load_const null
        return
    }

    function main() -> null {
        alloc_instance user.Container
        load_const "a"
        load_const "b"
        load_const "c"
        alloc_array 3
        init_field .items
        load_const 1
        load_const "z"
        call user.Container.set
        pop 1
        load_const null
        return
    }
    "#);
}

/// WORKS: 1D array READ through a field on self (control — same path, no assignment).
#[tokio::test]
async fn bytecode_1d_array_read_through_field() {
    let output = baml_test!(
        r#"
        class Container {
            items string[]

            function get(self, idx: int) -> string {
                self.items[idx]
            }
        }

        function main() -> string {
            let c = Container { items: ["a", "b", "c"] };
            c.get(1)
        }
    "#
    );
    insta::assert_snapshot!(output.bytecode, @r#"
    function Container.get(self: null, idx: int) -> string {
        load_var self
        load_field .items
        load_var idx
        load_array_element
        return
    }

    function main() -> string {
        alloc_instance user.Container
        load_const "a"
        load_const "b"
        load_const "c"
        alloc_array 3
        init_field .items
        load_const 1
        call user.Container.get
        return
    }
    "#);
    assert!(output.result.is_ok());
}

/// WORKS: 2D array write on a local variable.
#[tokio::test]
async fn bytecode_2d_array_write_local() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let cells = [["a", "b"], ["c", "d"]];
            cells[1][0] = "x";
            cells[1][0]
        }
    "#
    );
    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "a"
        load_const "b"
        alloc_array 2
        load_const "c"
        load_const "d"
        alloc_array 2
        alloc_array 2
        store_var cells
        load_var cells
        load_const 1
        load_array_element
        load_const 0
        load_const "x"
        store_array_element
        load_var cells
        load_const 1
        load_array_element
        load_const 0
        load_array_element
        return
    }
    "#);
    assert!(output.result.is_ok());
}

/// BROKEN: 2D array write through a field on self.
#[tokio::test]
async fn bytecode_2d_array_write_through_field() {
    let output = baml_test!(
        r#"
        class Grid {
            cells string[][]

            function set(self, row: int, col: int, val: string) -> null {
                self.cells[row][col] = val;
                null
            }
        }

        function main() -> null {
            let g = Grid { cells: [["a", "b"], ["c", "d"]] };
            g.set(1, 0, "x");
            null
        }
    "#
    );
    insta::assert_snapshot!(output.bytecode, @r#"
    function Grid.set(self: null, row: int, col: int, val: string) -> null {
        load_var self
        load_field .cells
        load_var row
        load_array_element
        load_var col
        load_var val
        store_array_element
        load_const null
        return
    }

    function main() -> null {
        alloc_instance user.Grid
        load_const "a"
        load_const "b"
        alloc_array 2
        load_const "c"
        load_const "d"
        alloc_array 2
        alloc_array 2
        init_field .cells
        load_const 1
        load_const 0
        load_const "x"
        call user.Grid.set
        pop 1
        load_const null
        return
    }
    "#);
}

/// BROKEN: map write through a field on self.
#[tokio::test]
async fn bytecode_map_write_through_field() {
    let output = baml_test!(
        r#"
        class Scores {
            data map<string, int>

            function set(self, key: string, val: int) -> null {
                self.data[key] = val;
                null
            }
        }

        function main() -> null {
            let s = Scores { data: {} };
            s.set("alice", 100);
            null
        }
    "#
    );
    insta::assert_snapshot!(output.bytecode, @r#"
    function Scores.set(self: null, key: string, val: int) -> null {
        load_var self
        load_field .data
        load_var key
        load_var val
        store_map_element
        load_const null
        return
    }

    function main() -> null {
        alloc_instance user.Scores
        alloc_map 0
        init_field .data
        load_const "alice"
        load_const 100
        call user.Scores.set
        pop 1
        load_const null
        return
    }
    "#);
}

/// WORKS: map write on a local variable.
#[tokio::test]
async fn bytecode_map_write_local() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let scores: map<string, int> = {};
            scores["alice"] = 100;
            scores["alice"]
        }
    "#
    );
    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int {
        alloc_map 0
        store_var scores
        load_var scores
        load_const "alice"
        load_const 100
        store_map_element
        load_var scores
        load_const "alice"
        load_map_element
        return
    }
    "#);
    assert!(output.result.is_ok());
}

/// BROKEN: non-self param field array write (proves it's not self-specific).
#[tokio::test]
async fn bytecode_non_self_param_array_write() {
    let output = baml_test!(
        r#"
        class Container {
            items string[]
        }

        function set_item(c: Container, idx: int, val: string) -> null {
            c.items[idx] = val;
            null
        }

        function main() -> null {
            let c = Container { items: ["a", "b", "c"] };
            set_item(c, 1, "z");
            null
        }
    "#
    );
    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> null {
        alloc_instance user.Container
        load_const "a"
        load_const "b"
        load_const "c"
        alloc_array 3
        init_field .items
        load_const 1
        load_const "z"
        call user.set_item
        pop 1
        load_const null
        return
    }

    function set_item(c: Container, idx: int, val: string) -> null {
        load_var c
        load_field .items
        load_var idx
        load_var val
        store_array_element
        load_const null
        return
    }
    "#);
}

// ============================================================================
// §1 — Array indexing with null produces "got any" instead of "got null"
// ============================================================================

/// Direct reproduction: index an array with a null value.
/// This should produce a type error, but the error message says "got any"
/// instead of "got null".
#[tokio::test]
async fn array_index_with_null_says_got_any() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let arr = ["a", "b", "c"];
            let idx: int? = null;
            arr[idx]
        }
    "#
    );

    let err = output.result.unwrap_err();
    let msg = err.to_string();
    // This is the current (confusing) behavior: "got any" instead of "got null"
    assert!(
        msg.contains("type error"),
        "expected a type error, got: {msg}"
    );
}

// ============================================================================
// §2 — Array indexing with correct int works fine
// ============================================================================

/// Baseline: array indexing with a proper int works.
#[tokio::test]
async fn array_index_with_int_works() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let arr = ["a", "b", "c"];
            arr[1]
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::String("b".to_string())));
}

/// Baseline: passing int through a function call and using it as index works.
#[tokio::test]
async fn int_through_function_call_as_index_works() {
    let output = baml_test!(
        r#"
        function get_element(arr: string[], idx: int) -> string {
            arr[idx]
        }

        function main() -> string {
            let arr = ["a", "b", "c"];
            get_element(arr, 1)
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::String("b".to_string())));
}

// ============================================================================
// §3 — Nested array indexing (like cells[row][col])
// ============================================================================

/// 2D array indexed with int literals — works fine.
#[tokio::test]
async fn nested_array_index_with_int_works() {
    let output = baml_test!(
        r#"
        function get_cell(cells: string[][], row: int, col: int) -> string {
            cells[row][col]
        }

        function main() -> string {
            let cells = [
                ["a", "b", "c"],
                ["d", "e", "f"],
                ["g", "h", "i"]
            ];
            get_cell(cells, 1, 2)
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::String("f".to_string())));
}

// ============================================================================
// §4 — Array element assignment — THE BUG
// ============================================================================

/// Reading from a 2D array works, but WRITING produces "expected int, got any".
/// This is the core bug: `self.cells[row][col] = player` fails even with valid ints.
#[tokio::test]
async fn array_element_assignment_2d_type_error() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let cells = [
                ["", "", ""],
                ["", "", ""],
                ["", "", ""]
            ];
            cells[1][2] = "x";
            cells[1][2]
        }
    "#
    );

    // BUG: This should succeed but currently fails with
    // "type error: expected int, got any"
    assert_eq!(output.result, Ok(BexExternalValue::String("x".to_string())));
}

/// Simpler case: 1D array assignment.
#[tokio::test]
async fn array_element_assignment_1d() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let arr = ["a", "b", "c"];
            arr[1] = "z";
            arr[1]
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::String("z".to_string())));
}

/// Class method with array read — this works.
#[tokio::test]
async fn class_method_array_read_works() {
    let output = baml_test!(
        r#"
        class Board {
            cells string[][]

            function new() -> Board {
                Board {
                    cells: [
                        ["", "", ""],
                        ["", "", ""],
                        ["", "", ""]
                    ]
                }
            }

            function get(self, row: int, col: int) -> string {
                self.cells[row][col]
            }
        }

        function main() -> string {
            let board = Board.new();
            board.get(1, 2)
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::String("".to_string())));
}

/// Class method with array WRITE — this is the tic-tac-toe bug.
#[tokio::test]
async fn class_method_array_write_type_error() {
    let output = baml_test!(
        r#"
        class Board {
            cells string[][]

            function new() -> Board {
                Board {
                    cells: [
                        ["", "", ""],
                        ["", "", ""],
                        ["", "", ""]
                    ]
                }
            }

            function play(self, player: string, row: int, col: int) -> null {
                self.cells[row][col] = player;
                null
            }

            function get(self, row: int, col: int) -> string {
                self.cells[row][col]
            }
        }

        function main() -> string {
            let board = Board.new();
            board.play("x", 1, 2);
            board.get(1, 2)
        }
    "#
    );

    // BUG: This should return "x" but fails with:
    // TypeError { expected: Int, got: Object(Any) }
    // The error occurs in Board.play at `self.cells[row][col] = player`
    assert_eq!(output.result, Ok(BexExternalValue::String("x".to_string())));
}

// ============================================================================
// §5 — Isolate: is it the assignment or the nested index?
// ============================================================================

/// 2D array read with variables (not literals) — does this work?
#[tokio::test]
async fn nested_array_read_with_variables() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let cells = [
                ["a", "b", "c"],
                ["d", "e", "f"],
                ["g", "h", "i"]
            ];
            let row = 1;
            let col = 2;
            cells[row][col]
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::String("f".to_string())));
}

/// 2D array write with variables — isolates the assignment path.
#[tokio::test]
async fn nested_array_write_with_variables() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let cells = [
                ["a", "b", "c"],
                ["d", "e", "f"],
                ["g", "h", "i"]
            ];
            let row = 1;
            let col = 2;
            cells[row][col] = "x";
            cells[row][col]
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::String("x".to_string())));
}

/// Simplest possible: 1D array assignment with a variable index.
#[tokio::test]
async fn array_write_with_variable_index() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let arr = ["a", "b", "c"];
            let idx = 1;
            arr[idx] = "z";
            arr[idx]
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::String("z".to_string())));
}

// ============================================================================
// §6 — Isolate: self.field assignment vs local variable assignment
// ============================================================================

/// 1D array assignment through self — is `self` the issue?
#[tokio::test]
async fn class_method_1d_array_write_through_self() {
    let output = baml_test!(
        r#"
        class Container {
            items string[]

            function new() -> Container {
                Container { items: ["a", "b", "c"] }
            }

            function set(self, idx: int, val: string) -> null {
                self.items[idx] = val;
                null
            }

            function get(self, idx: int) -> string {
                self.items[idx]
            }
        }

        function main() -> string {
            let c = Container.new();
            c.set(1, "z");
            c.get(1)
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::String("z".to_string())));
}

/// 2D array assignment through self with literal indices — is it self + 2D?
#[tokio::test]
async fn class_method_2d_array_write_through_self_literals() {
    let output = baml_test!(
        r#"
        class Grid {
            cells string[][]

            function new() -> Grid {
                Grid {
                    cells: [
                        ["", "", ""],
                        ["", "", ""],
                        ["", "", ""]
                    ]
                }
            }

            function set(self, row: int, col: int, val: string) -> null {
                self.cells[row][col] = val;
                null
            }

            function get(self, row: int, col: int) -> string {
                self.cells[row][col]
            }
        }

        function main() -> string {
            let g = Grid.new();
            g.set(1, 2, "x");
            g.get(1, 2)
        }
    "#
    );

    // This isolates: is it self + 2D array + assignment?
    assert_eq!(output.result, Ok(BexExternalValue::String("x".to_string())));
}

/// Same as Board.play but without the if/else guards — minimal class method.
#[tokio::test]
async fn class_method_2d_array_write_minimal() {
    let output = baml_test!(
        r#"
        class Board {
            cells string[][]

            function new() -> Board {
                Board {
                    cells: [
                        ["", "", ""],
                        ["", "", ""],
                        ["", "", ""]
                    ]
                }
            }

            function play(self, player: string, row: int, col: int) -> null {
                self.cells[row][col] = player;
                null
            }
        }

        function main() -> null {
            let board = Board.new();
            board.play("x", 1, 2);
            null
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Null));
}

// ============================================================================
// §7 — Map element assignment through self
// ============================================================================

/// Does self.map[key] = val hit the same bug?
#[tokio::test]
async fn class_method_map_write_through_self() {
    let output = baml_test!(
        r#"
        class Scores {
            data map<string, int>

            function new() -> Scores {
                Scores { data: {} }
            }

            function set(self, key: string, val: int) -> null {
                self.data[key] = val;
                null
            }

            function get(self, key: string) -> int {
                self.data[key]
            }
        }

        function main() -> int {
            let s = Scores.new();
            s.set("alice", 100);
            s.get("alice")
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(100)));
}

/// Map write on a local variable — control case.
#[tokio::test]
async fn map_write_local_variable() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let scores: map<string, int> = {};
            scores["alice"] = 100;
            scores["alice"]
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(100)));
}

// ============================================================================
// §8 — Nested field access + array assignment
// ============================================================================

/// Does self.inner.items[idx] = val break? (nested object field)
#[tokio::test]
async fn class_method_nested_field_array_write() {
    let output = baml_test!(
        r#"
        class Inner {
            items string[]
        }

        class Outer {
            inner Inner

            function new() -> Outer {
                Outer { inner: Inner { items: ["a", "b", "c"] } }
            }

            function set(self, idx: int, val: string) -> null {
                self.inner.items[idx] = val;
                null
            }

            function get(self, idx: int) -> string {
                self.inner.items[idx]
            }
        }

        function main() -> string {
            let o = Outer.new();
            o.set(1, "z");
            o.get(1)
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::String("z".to_string())));
}

// ============================================================================
// §9 — Mixed map + array through self
// ============================================================================

/// self.data[key] where data is map<string, int[]> — map read then array write.
#[tokio::test]
async fn class_method_map_then_array_write_through_self() {
    let output = baml_test!(
        r#"
        class Registry {
            data map<string, int[]>

            function new() -> Registry {
                Registry { data: { "nums": [1, 2, 3] } }
            }

            function set(self, key: string, idx: int, val: int) -> null {
                self.data[key][idx] = val;
                null
            }

            function get(self, key: string, idx: int) -> int {
                self.data[key][idx]
            }
        }

        function main() -> int {
            let r = Registry.new();
            r.set("nums", 0, 99);
            r.get("nums", 0)
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(99)));
}

// ============================================================================
// §10 — 3D array through self
// ============================================================================

/// 3D array write through self — does more nesting make it worse?
#[tokio::test]
async fn class_method_3d_array_write_through_self() {
    let output = baml_test!(
        r#"
        class Cube {
            data int[][][]

            function new() -> Cube {
                Cube { data: [[[0, 0], [0, 0]], [[0, 0], [0, 0]]] }
            }

            function set(self, x: int, y: int, z: int, val: int) -> null {
                self.data[x][y][z] = val;
                null
            }

            function get(self, x: int, y: int, z: int) -> int {
                self.data[x][y][z]
            }
        }

        function main() -> int {
            let c = Cube.new();
            c.set(1, 0, 1, 42);
            c.get(1, 0, 1)
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

// ============================================================================
// §11 — Array push through self (method call, not index assignment)
// ============================================================================

/// Array push through self — this uses a method call, not index assignment.
/// If this works, it confirms the bug is specific to index-based assignment.
#[tokio::test]
async fn class_method_array_push_through_self() {
    let output = baml_test!(
        r#"
        class Stack {
            items int[]

            function new() -> Stack {
                Stack { items: [] }
            }

            function push(self, val: int) -> null {
                self.items.push(val);
                null
            }

            function peek(self) -> int {
                self.items[0]
            }
        }

        function main() -> int {
            let s = Stack.new();
            s.push(42);
            s.peek()
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

// ============================================================================
// §12 — Non-self class field array write (static-like function)
// ============================================================================

/// Pass a class instance as a regular param (not self) and write to its array.
/// If this works, the bug is specifically about the `self` parameter binding.
#[tokio::test]
async fn non_self_param_array_write() {
    let output = baml_test!(
        r#"
        class Container {
            items string[]
        }

        function set_item(c: Container, idx: int, val: string) -> null {
            c.items[idx] = val;
            null
        }

        function main() -> string {
            let c = Container { items: ["a", "b", "c"] };
            set_item(c, 1, "z");
            c.items[1]
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::String("z".to_string())));
}
