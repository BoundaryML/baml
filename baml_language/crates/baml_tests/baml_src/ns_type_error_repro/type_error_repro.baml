// Minimal reproduction tests for "type error: expected int, got any".
//
// Runtime regression tests — every test compiles AND runs.
//
// Tests that mutate locals or use multi-step indexed writes go through a
// top-level `function` helper so they aren't affected by the test-block
// local-boxing VM behavior (a `let` local compared against a literal in a test
// block can mismatch; code inside a function is fine).
//
// Bytecode-snapshot-only tests are structured with test-running functions so
// the test passes if the body compiles and runs without error.
//
// Cannot express in BAML corpus (require compile-error diagnostic assertions):
// - array_index_with_null_is_rejected_at_compile_time  (type mismatch: int | null subscript)
// - int_literal_above_max_is_rejected_at_compile_time  (E0150 oversize int literal)
// - negated_int_literal_below_min_is_rejected_at_compile_time  (E0150)
// - parenthesized_oversized_literal_is_rejected  (E0150)
// - map_with_int_key_is_rejected_at_compile_time  (map keys must be `string`)
// - generic_map_key_annotation_is_rejected  (map keys must be `string`; got `K`)
// - map_with_int_alias_key_is_rejected_at_compile_time  (map keys must be `string`; got alias)

// ============================================================================
// §0 — Bytecode-snapshot tests (is_ok: compile-and-run only)
// ============================================================================

// Bytecode snapshot test: 1D array write on a local variable.
function bytecode_1d_array_write_local_body() -> string {
    let arr = ["a", "b", "c"];
    arr[1] = "z";
    arr[1]
}

test "bytecode_1d_array_write_local" {
    let _ = bytecode_1d_array_write_local_body();
    null
}

// Bytecode-only: 1D array write through a field on self.
class BytecodeContainer {
    items string[]

    function set(self, idx: int, val: string) -> null {
        self.items[idx] = val;
        null
    }
}

function bytecode_1d_array_write_through_field_body() -> null {
    let c = BytecodeContainer { items: ["a", "b", "c"] };
    c.set(1, "z");
    null
}

test "bytecode_1d_array_write_through_field" {
    let _ = bytecode_1d_array_write_through_field_body();
    null
}

// Bytecode-only: 1D array READ through a field on self.
class BytecodeContainerRead {
    items string[]

    function get(self, idx: int) -> string {
        self.items[idx]
    }
}

function bytecode_1d_array_read_through_field_body() -> string {
    let c = BytecodeContainerRead { items: ["a", "b", "c"] };
    c.get(1)
}

test "bytecode_1d_array_read_through_field" {
    let _ = bytecode_1d_array_read_through_field_body();
    null
}

// Bytecode-only: 2D array write on a local variable.
function bytecode_2d_array_write_local_body() -> string {
    let cells = [["a", "b"], ["c", "d"]];
    cells[1][0] = "x";
    cells[1][0]
}

test "bytecode_2d_array_write_local" {
    let _ = bytecode_2d_array_write_local_body();
    null
}

// Bytecode-only: 2D array write through a field on self.
class BytecodeGrid {
    cells string[][]

    function set(self, row: int, col: int, val: string) -> null {
        self.cells[row][col] = val;
        null
    }
}

function bytecode_2d_array_write_through_field_body() -> null {
    let g = BytecodeGrid { cells: [["a", "b"], ["c", "d"]] };
    g.set(1, 0, "x");
    null
}

test "bytecode_2d_array_write_through_field" {
    let _ = bytecode_2d_array_write_through_field_body();
    null
}

// Bytecode-only: map write through a field on self.
class BytecodeScores {
    data map<string, int>

    function set(self, key: string, val: int) -> null {
        self.data[key] = val;
        null
    }
}

function bytecode_map_write_through_field_body() -> null {
    let s = BytecodeScores { data: {} };
    s.set("alice", 100);
    null
}

test "bytecode_map_write_through_field" {
    let _ = bytecode_map_write_through_field_body();
    null
}

// Bytecode-only: map write on a local variable.
function bytecode_map_write_local_body() -> int {
    let scores: map<string, int> = {};
    scores["alice"] = 100;
    scores["alice"]
}

test "bytecode_map_write_local" {
    let _ = bytecode_map_write_local_body();
    null
}

// Bytecode-only: non-self param array write.
class BytecodeContainerNonSelf {
    items string[]
}

function bytecode_set_item(c: BytecodeContainerNonSelf, idx: int, val: string) -> null {
    c.items[idx] = val;
    null
}

function bytecode_non_self_param_array_write_body() -> null {
    let c = BytecodeContainerNonSelf { items: ["a", "b", "c"] };
    bytecode_set_item(c, 1, "z");
    null
}

test "bytecode_non_self_param_array_write" {
    let _ = bytecode_non_self_param_array_write_body();
    null
}

// ============================================================================
// §1 — Array indexing with null produces a type error
// ============================================================================

// BUG: VM type errors (raised inside the VM) are not catchable via a BAML
// catch expression — only user-thrown values (throw "..." / throw Class { … })
// are catchable. There is no BAML-level way to assert this expression raises
// a VM type error, so this test cannot be expressed in the corpus.
//
// test "array_index_with_null_says_got_any" {
//     {
//         let arr = ["a", "b", "c"];
//         let idx: int? = null;
//         arr[idx]
//     } catch (e) {
//         _ => null
//     }
// }

// ============================================================================
// §2 — Array indexing with correct int works fine
// ============================================================================

// Baseline: array indexing with a proper int works.
function array_index_with_int_works_body() -> string {
    let arr = ["a", "b", "c"];
    arr[1]
}

test "array_index_with_int_works" {
    assert.equal(array_index_with_int_works_body(), "b")
}

// Passing int through a function call and using it as index works.
function get_element_for_int_through_call(arr: string[], idx: int) -> string {
    arr[idx]
}

function int_through_function_call_as_index_works_body() -> string {
    let arr = ["a", "b", "c"];
    get_element_for_int_through_call(arr, 1)
}

test "int_through_function_call_as_index_works" {
    assert.equal(int_through_function_call_as_index_works_body(), "b")
}

// ============================================================================
// §3 — Nested array indexing (like cells[row][col])
// ============================================================================

// 2D array indexed with int literals — works fine.
function get_cell_for_nested_test(cells: string[][], row: int, col: int) -> string {
    cells[row][col]
}

function nested_array_index_with_int_works_body() -> string {
    let cells = [
        ["a", "b", "c"],
        ["d", "e", "f"],
        ["g", "h", "i"]
    ];
    get_cell_for_nested_test(cells, 1, 2)
}

test "nested_array_index_with_int_works" {
    assert.equal(nested_array_index_with_int_works_body(), "f")
}

// ============================================================================
// §4 — Array element assignment
// ============================================================================

// 2D array write with literal indices.
function array_element_assignment_2d_type_error_body() -> string {
    let cells = [
        ["", "", ""],
        ["", "", ""],
        ["", "", ""]
    ];
    cells[1][2] = "x";
    cells[1][2]
}

test "array_element_assignment_2d_type_error" {
    assert.equal(array_element_assignment_2d_type_error_body(), "x")
}

// 1D array assignment.
function array_element_assignment_1d_body() -> string {
    let arr = ["a", "b", "c"];
    arr[1] = "z";
    arr[1]
}

test "array_element_assignment_1d" {
    assert.equal(array_element_assignment_1d_body(), "z")
}

// Class method with array READ — this should work.
class BoardRead {
    cells string[][]

    function new() -> BoardRead {
        BoardRead {
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

function class_method_array_read_works_body() -> string {
    let board = BoardRead.new();
    board.get(1, 2)
}

test "class_method_array_read_works" {
    assert.equal(class_method_array_read_works_body(), "")
}

// Class method with array WRITE — the tic-tac-toe bug.
class BoardWrite {
    cells string[][]

    function new() -> BoardWrite {
        BoardWrite {
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

function class_method_array_write_type_error_body() -> string {
    let board = BoardWrite.new();
    board.play("x", 1, 2);
    board.get(1, 2)
}

test "class_method_array_write_type_error" {
    assert.equal(class_method_array_write_type_error_body(), "x")
}

// ============================================================================
// §5 — Isolate: is it the assignment or the nested index?
// ============================================================================

// 2D array read with variables (not literals).
function nested_array_read_with_variables_body() -> string {
    let cells = [
        ["a", "b", "c"],
        ["d", "e", "f"],
        ["g", "h", "i"]
    ];
    let row = 1;
    let col = 2;
    cells[row][col]
}

test "nested_array_read_with_variables" {
    assert.equal(nested_array_read_with_variables_body(), "f")
}

// 2D array write with variables — isolates the assignment path.
function nested_array_write_with_variables_body() -> string {
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

test "nested_array_write_with_variables" {
    assert.equal(nested_array_write_with_variables_body(), "x")
}

// Simplest possible: 1D array assignment with a variable index.
function array_write_with_variable_index_body() -> string {
    let arr = ["a", "b", "c"];
    let idx = 1;
    arr[idx] = "z";
    arr[idx]
}

test "array_write_with_variable_index" {
    assert.equal(array_write_with_variable_index_body(), "z")
}

// ============================================================================
// §6 — Isolate: self.field assignment vs local variable assignment
// ============================================================================

// 1D array assignment through self — is `self` the issue?
class ContainerSelf {
    items string[]

    function new() -> ContainerSelf {
        ContainerSelf { items: ["a", "b", "c"] }
    }

    function set(self, idx: int, val: string) -> null {
        self.items[idx] = val;
        null
    }

    function get(self, idx: int) -> string {
        self.items[idx]
    }
}

function class_method_1d_array_write_through_self_body() -> string {
    let c = ContainerSelf.new();
    c.set(1, "z");
    c.get(1)
}

test "class_method_1d_array_write_through_self" {
    assert.equal(class_method_1d_array_write_through_self_body(), "z")
}

// 2D array assignment through self with literal indices — is it self + 2D?
class GridSelf {
    cells string[][]

    function new() -> GridSelf {
        GridSelf {
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

function class_method_2d_array_write_through_self_literals_body() -> string {
    let g = GridSelf.new();
    g.set(1, 2, "x");
    g.get(1, 2)
}

test "class_method_2d_array_write_through_self_literals" {
    assert.equal(class_method_2d_array_write_through_self_literals_body(), "x")
}

// Same as Board.play but without if/else guards — minimal class method.
class BoardMinimal {
    cells string[][]

    function new() -> BoardMinimal {
        BoardMinimal {
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

function class_method_2d_array_write_minimal_body() -> null {
    let board = BoardMinimal.new();
    board.play("x", 1, 2);
    null
}

test "class_method_2d_array_write_minimal" {
    assert.equal(class_method_2d_array_write_minimal_body(), null)
}

// ============================================================================
// §7 — Map element assignment through self
// ============================================================================

// Does self.map[key] = val hit the same bug?
class ScoresSelf {
    data map<string, int>

    function new() -> ScoresSelf {
        ScoresSelf { data: {} }
    }

    function set(self, key: string, val: int) -> null {
        self.data[key] = val;
        null
    }

    function get(self, key: string) -> int {
        self.data[key]
    }
}

function class_method_map_write_through_self_body() -> int {
    let s = ScoresSelf.new();
    s.set("alice", 100);
    s.get("alice")
}

test "class_method_map_write_through_self" {
    assert.equal(class_method_map_write_through_self_body(), 100)
}

// Map write on a local variable — control case.
function map_write_local_variable_body() -> int {
    let scores: map<string, int> = {};
    scores["alice"] = 100;
    scores["alice"]
}

test "map_write_local_variable" {
    assert.equal(map_write_local_variable_body(), 100)
}

// ============================================================================
// §8 — Nested field access + array assignment
// ============================================================================

// Does self.inner.items[idx] = val break? (nested object field)
class InnerItems {
    items string[]
}

class OuterItems {
    inner InnerItems

    function new() -> OuterItems {
        OuterItems { inner: InnerItems { items: ["a", "b", "c"] } }
    }

    function set(self, idx: int, val: string) -> null {
        self.inner.items[idx] = val;
        null
    }

    function get(self, idx: int) -> string {
        self.inner.items[idx]
    }
}

function class_method_nested_field_array_write_body() -> string {
    let o = OuterItems.new();
    o.set(1, "z");
    o.get(1)
}

test "class_method_nested_field_array_write" {
    assert.equal(class_method_nested_field_array_write_body(), "z")
}

// ============================================================================
// §9 — Mixed map + array through self
// ============================================================================

// self.data[key] where data is map<string, int[]> — map read then array write.
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

function class_method_map_then_array_write_through_self_body() -> int {
    let r = Registry.new();
    r.set("nums", 0, 99);
    r.get("nums", 0)
}

test "class_method_map_then_array_write_through_self" {
    assert.equal(class_method_map_then_array_write_through_self_body(), 99)
}

// ============================================================================
// §10 — 3D array through self
// ============================================================================

// 3D array write through self — does more nesting make it worse?
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

function class_method_3d_array_write_through_self_body() -> int {
    let c = Cube.new();
    c.set(1, 0, 1, 42);
    c.get(1, 0, 1)
}

test "class_method_3d_array_write_through_self" {
    assert.equal(class_method_3d_array_write_through_self_body(), 42)
}

// ============================================================================
// §11 — Array push through self (method call, not index assignment)
// ============================================================================

// Array push through self — uses a method call, not index assignment.
// If this works, it confirms the bug is specific to index-based assignment.
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

function class_method_array_push_through_self_body() -> int {
    let s = Stack.new();
    s.push(42);
    s.peek()
}

test "class_method_array_push_through_self" {
    assert.equal(class_method_array_push_through_self_body(), 42)
}

// ============================================================================
// §12 — Non-self class field array write (static-like function)
// ============================================================================

// Pass a class instance as a regular param (not self) and write to its array.
class ContainerNonSelf {
    items string[]
}

function set_item_non_self(c: ContainerNonSelf, idx: int, val: string) -> null {
    c.items[idx] = val;
    null
}

function non_self_param_array_write_body() -> string {
    let c = ContainerNonSelf { items: ["a", "b", "c"] };
    set_item_non_self(c, 1, "z");
    c.items[1]
}

test "non_self_param_array_write" {
    assert.equal(non_self_param_array_write_body(), "z")
}

// ============================================================================
// §13 — The optional index `?.[]` is null-safe in the *index* too
// ============================================================================

// `?.[]` is the null-safe index operator, so a null subscript short-circuits
// the whole expression to null instead of aborting the VM (it used to crash
// with `got any`). The base guard and the index guard are symmetric.
function optional_index_with_null_index_body() -> int? {
    let arr: int[]? = [10, 20, 30];
    let i: int? = null;
    arr?.[i]
}

test "optional_index_with_null_index_returns_null" {
    assert.equal(optional_index_with_null_index_body(), null)
}

// A valid (non-null) index through `?.[]` still returns the element.
function optional_index_with_valid_index_body() -> int? {
    let arr: int[]? = [10, 20, 30];
    let i: int? = 1;
    arr?.[i]
}

test "optional_index_with_valid_index_returns_element" {
    assert.equal(optional_index_with_valid_index_body(), 20)
}

// ============================================================================
// §14 — Integer literals at the i63 `int` range boundary (B-266)
// ============================================================================

// `int.min_value()` (`-2^62`) must remain writable as a negated literal even
// though `+2^62` is not a legal `int` literal: the leading `-` forms a single
// negative literal. (Mirrors the i64::MIN literal rule in Rust/Java/C#.)
function negated_int_min_literal_body() -> int {
    -4611686018427387904
}

test "negated_int_min_literal_is_valid" {
    assert.equal(negated_int_min_literal_body(), -4611686018427387904)
}

// ============================================================================
// §15 — String-denoting alias map keys are accepted (B-533)
// ============================================================================

type TypeErrReproStrKey = string

// A `map` keyed by an alias that denotes `string` type-checks and runs.
function map_with_string_alias_key_body() -> int {
    let counts: map<TypeErrReproStrKey, int> = {};
    let _ = counts.set("x", 1);
    counts.get("x") ?? 0
}

test "map_with_string_alias_key_is_accepted" {
    assert.equal(map_with_string_alias_key_body(), 1)
}

// A union of string-denoting aliases is also a legal key type.
function map_with_repeated_string_alias_union_key_body() -> int {
    let counts: map<TypeErrReproStrKey | TypeErrReproStrKey, int> = {};
    let _ = counts.set("x", 1);
    counts.get("x") ?? 0
}

test "map_with_repeated_string_alias_union_key_is_accepted" {
    assert.equal(map_with_repeated_string_alias_union_key_body(), 1)
}
