// Tests for new pattern features: chains, unions, generic destructure, array
// destructure, deep class destructure, and or-patterns.
//
// Converted from crates/baml_tests/tests/patterns_new_runtime.rs.
//
// Functions inside test blocks are not valid BAML: every function is top-level
// with a unique name per test. Classes are shared when structurally identical,
// or prefixed to avoid name collisions.
//
// Not converted (left in Rust, goes to diagnostic_errors separately):
//   - generic_class_destructure_without_type_args_is_rejected
//     (compile-error assertion)

// ─── Shared generic Box class ─────────────────────────────────────────────────

class BoxT<T> {
    value T
}

class OuterT<T> {
    inner BoxT<T>
}

// ─── Shared Foo3 class (rustc port tests) ────────────────────────────────────

class Foo3 {
    first bool
    second int[]?
}

// ─── Shared FooUnion class ────────────────────────────────────────────────────

class FooUnion {
    value int | float | bool
}

// ─── Shared PairAB class ─────────────────────────────────────────────────────

class PairAB {
    a int
    b int
}

// ─── Shared Direction enum ───────────────────────────────────────────────────

enum Direction {
    North
    East
    South
    West
}

// ─── Coord/Addr/UserCoord hierarchy (zip-only, shared by many tests) ─────────

class Coord {
    zip int
}

class Addr {
    coordinate Coord
}

class UserCoord {
    address Addr
}

// ─── Deep User hierarchy (zip+plus4+city+score+active) ───────────────────────
// Used by: let_deep_class_destructure_binds_inner_field

class Coordinate2 {
    zip int
    plus4 int
}

class Address2 {
    coordinate Coordinate2
    city string
}

class Profile2 {
    address Address2
    score int
}

class UserDeep {
    profile Profile2
    active bool
}

// ─── Coordinate3/Address3/User3 (zip+plus4, no city/score) ───────────────────
// Used by: match_or_deep_class_destructure_binds_when_second_alt_matches

class Coordinate3 {
    zip int
    plus4 int
}

class Address3 {
    coordinate Coordinate3
}

class UserOr {
    address Address3
}

// ─── Coordinate4/Address4/Profile4/User4 (nested, zip only) ──────────────────
// Used by: match_deep_class_destructure_falls_through_to_binding_arm

class Coordinate4 {
    zip int
}

class Address4 {
    coordinate Coordinate4
}

class Profile4 {
    address Address4
}

class UserNested {
    profile Profile4
}

// ─── Person classes ───────────────────────────────────────────────────────────

// let_class_destructure_binds_shorthand_and_ignores_omitted_fields
class PersonANS {
    name string
    age int
    score int
}

// let_class_destructure_renames_field_binding
class PersonNA {
    name string
    age int
}

// let_class_destructure_nested_class
class Address6 {
    zip int
    city string
}

class PersonAddr6 {
    age int
    address Address6
}

// match_class_destructure_tests_field_literal_and_falls_through
class PersonAS {
    age int
    score int
}

// match_top_level_class_destructure_tests_union_scrutinee
class PersonAge {
    age int
}

// match_class_destructure_binds_nested_field
class Address5 {
    zip int
}

class PersonAddr5 {
    address Address5
}

// ─── NestedInner/NestedOuter (same field name at different depths) ─────────────

class NestedInner {
    field int
}

class NestedOuter {
    field NestedInner
    other int
}

// ─── BoxUnion (non-generic Box with union field) ──────────────────────────────

class BoxUnion {
    value int | string
}

// ─── Team classes (all have `scores int[]`) ───────────────────────────────────

class Team1 {
    scores int[]
}

class Team2 {
    scores int[]
}

class Team3 {
    scores int[]
}

class Team4 {
    scores int[]
}

// ─── UserScore (name + score) ─────────────────────────────────────────────────

class UserScore {
    name string
    score int
}

// ─── Or-pattern class pairs ───────────────────────────────────────────────────

// match_or_binding_from_class_field_or_nested_array_position
class OPat_A {
    x int
}

class OPat_B {
    y int[]
}

// match_or_class_destructure_same_binding_name_compatible_types
class PersonIdA {
    id int
}

class PersonIdB {
    id int
}

// match_or_class_destructure_same_binding_name_different_depths
class DepthClass {
    field int
}

class DepthClass3 {
    field int
}

class DepthClass2 {
    c DepthClass3
}

// match_class_field_pattern_should_refine_parent_union_scrutinee
class RefineA {
    field int
}

class RefineB {
    field string
}

// match_or_class_patterns_should_refine_parent_union_scrutinee
// `ABCDE_A..D` share an `int field`; accessing it on the refined union scrutinee
// (`(ABCDE_A | ABCDE_B | ABCDE_C | ABCDE_D).field`) goes through the interface
// they implement. `ABCDE_E` has a `string field`, so it stays out of the union.
interface HasIntField { field int }

class ABCDE_A {
    field int
    implements HasIntField {}
}

class ABCDE_B {
    field int
    implements HasIntField {}
}

class ABCDE_C {
    field int
    implements HasIntField {}
}

class ABCDE_D {
    field int
    implements HasIntField {}
}

class ABCDE_E {
    field string
}

// ─── UserFieldUser (shorthand field same name as class) ───────────────────────

class UserFieldUser {
    User int
    other int
}

// ─── Wrap/Pair/Cell/Grid/Atom/Slot/Row/Sheet ─────────────────────────────────

class Wrap1 {
    inner int[]
}

class Pair1 {
    left Wrap1
    right int[]
}

class Atom1 {
    value int
}

class Slot1 {
    atoms Atom1[]
}

class Row1 {
    slots Slot1[]
}

class Sheet1 {
    rows Row1[]
}

class Cell1 {
    row int[]
}

class Grid1 {
    cells Cell1[]
}

class Wrap2 {
    grid int[][]
}

// ─── Helpers: top-level functions used by multiple tests ─────────────────────

function risky_throws_string() -> string throws string {
    throw "boom"
}

function score_empty_prefix_rest(xs: int[]) -> int {
    match (xs) {
        [] => 0,
        [let first, ..] => first
    }
}

function score_rest_copy(xs: int[]) -> int {
    match (xs) {
        [let first, ..] => first * 10 + (xs.length() - 1),
        [] => 0
    }
}

function score_suffix_rest(xs: int[]) -> int {
    match (xs) {
        [.., let last] => (xs.length() - 1) * 10 + last,
        _ => 0
    }
}

function score_opat(input: OPat_A | OPat_B) -> int {
    match (input) {
        OPat_A { x: let value } | OPat_B { y: [_, let value] } => value,
        _ => 0
    }
}

function score_ints(xs: int[]) -> int {
    match (xs) {
        [1, 2, ..] => 12,
        [1, ..] => 10,
        _ => 0
    }
}

function score_strings_len(xs: string[]) -> int {
    match (xs) {
        [string, ..] => xs.length(),
        [] => 0
    }
}

function score_or_share(xs: int[]) -> int {
    match (xs) {
        [let x] | [let x, ..] => x,
        _ => 0
    }
}

function score_or_nested_2d(rows: int[][]) -> int {
    match (rows) {
        [[let x, ..]] | [[let x, ..], ..] => x,
        _ => 0
    }
}

function score_team2(teams: Team2[]) -> int {
    match (teams) {
        [Team2 { scores: [let x, ..] }] |
        [Team2 { scores: [_, let x, ..] }, ..] => x,
        _ => 0
    }
}

function score_outer_chain(xs: int[][]) -> int {
    match (xs) {
        [.., let second_last, let last]: int[][] =>
            (xs.length() - 2) * 100 + second_last[0] * 10 + last[0],
        _ => 0
    }
}

function pick_sheet(s: Sheet1) -> int {
    match (s) {
        Sheet1 { rows: [_, Row1 { slots: [_, Slot1 { atoms: [_, Atom1 { value: let x: int }, ..] }, ..] }, ..] }
        | Sheet1 { rows: [.., Row1 { slots: [.., Slot1 { atoms: [.., Atom1 { value: let x: int }, _] }] }] } => x,
        _ => 0
    }
}

function pick_pair(p: Pair1 | Wrap1 | int[][] | int[]) -> int {
    match (p) {
        Pair1 { left: Wrap1 { inner: [_, let v: int, ..] }, right: _ }
        | Pair1 { left: _, right: [_, _, let v: int, ..] }
        | Wrap1 { inner: [.., let v: int, _, _] }
        | [_, [.., let v: int], ..] => v,
        _ => 0
    }
}

function pick_grid(g: Grid1) -> int {
    match (g) {
        Grid1 { cells: [_, Cell1 { row: [_, let x, ..] }, ..] }
        | Grid1 { cells: [.., Cell1 { row: [.., let x, _] }] } => x,
        _ => 0
    }
}

function score_wrap2(v: int[][] | Wrap2) -> int {
    match (v) {
        [[let x, ..], ..]
        | Wrap2 { grid: [.., [_, let x, ..]] } => x,
        _ => 0
    }
}

function score_two_prefix(xs: int[]) -> int {
    match (xs) {
        [let x, let y, .., let z] => x * 100 + y * 10 + z,
        _ => 0
    }
}

function score_prefix_two_suffix(xs: int[]) -> int {
    match (xs) {
        [let x, .., let y, let z] => x * 100 + y * 10 + z,
        _ => 0
    }
}

function score_team3(team: Team3) -> int {
    match (team) {
        Team3 { scores: [] } => 10,
        Team3 { scores: [let first, ..] } => first
    }
}

function score_user_score(users: UserScore[]) -> int {
    match (users) {
        [] => 0,
        [UserScore { score: let first }, ..] =>
            first * 10 + (users.length() - 1)
    }
}

function score_wildcard_second(xs: int[]) -> int {
    match (xs) {
        [_, let second, ..] => second,
        _ => 0
    }
}

function exact_length(xs: int[]) -> int {
    match (xs) {
        [] => 0,
        [_] => 1,
        [_, _] => 2,
        [_, ..] => 3
    }
}

function prefix_length(xs: int[]) -> int {
    match (xs) {
        [_, ..] => 1,
        [] => 0
    }
}

function rest_any(xs: int[]) -> int {
    match (xs) {
        [..] => 7
    }
}

function score_team4(team: Team4) -> int {
    match (team) {
        Team4 { scores: [_, let second, ..] } => second,
        _ => 0
    }
}

function score_wildcard_user(users: UserScore[]) -> int {
    match (users) {
        [UserScore { name: _, score: let first }, ..] => first,
        _ => 0
    }
}

function score_depth(v: DepthClass | DepthClass2) -> int {
    match (v) {
        DepthClass { field } |
        DepthClass2 { c: DepthClass3 { field } } => field
    }
}

function score_personid(v: PersonIdA | PersonIdB) -> int {
    match (v) {
        PersonIdA { id } | PersonIdB { id } => id
    }
}

function score_person_age(v: PersonAge | int) -> int {
    match (v) {
        PersonAge { age } => age,
        int => 0
    }
}

function score_foo3(f: Foo3) -> int {
    match (f) {
        Foo3 { first: true, second: null } => 1,
        Foo3 { first: true, second: int[] } => 2,
        Foo3 { first: false, second: null } => 3,
        Foo3 { first: false, second: int[] } => 4
    }
}

function score_bool_slice(xs: bool[]) -> int {
    match (xs) {
        [true, ..] => 1,
        [.., false] => 2,
        [] => 3,
        _ => 4
    }
}

function describe_direction(d: Direction) -> string {
    match (d) {
        Direction.North => "up",
        Direction.East => "right",
        Direction.South => "down",
        Direction.West => "left"
    }
}

function score_pair_or(p: PairAB) -> int {
    match (p) {
        PairAB { a: 0 | 1, b: 2 | 3 } => 1,
        _ => 0
    }
}

function score_int_optional(x: int?) -> int {
    match (x) {
        null | 0 | 1 => 1,
        _ => 0
    }
}

function score_refine_a(v: RefineA | RefineB) -> int {
    match (v) {
        RefineA { field: int } => v.field,
        _ => 0
    }
}

function score_foo_union(f: FooUnion) -> int {
    match (f) {
        FooUnion { value: int } => 1,
        FooUnion { value: float } => 2,
        FooUnion { value: bool } => 3
    }
}

function score_int_string_array(v: int[] | string[]) -> int {
    match (v) {
        [let x: int] => v[0] + x,
        _ => 0
    }
}

function score_abcde(v: ABCDE_A | ABCDE_B | ABCDE_C | ABCDE_D | ABCDE_E) -> int {
    match (v) {
        ABCDE_A { field: int } | ABCDE_B { field: int } | ABCDE_C { field: int } | ABCDE_D { field: int } => v.field,
        _ => 0
    }
}

function score_or_array_refine(v: int[] | string[]) -> int {
    match (v) {
        [let x: int] | [let x: int, ..] => v[0] + x,
        _ => 0
    }
}

function classify_box_int(boxed: BoxT<int>) -> int {
    match (boxed) {
        BoxT<int> {} => 1
    }
}

function classify_box_string(boxed: BoxT<string>) -> int {
    match (boxed) {
        BoxT<string> {} => 20
    }
}

function classify_box_union_by_type(box: BoxT<int> | BoxT<string>) -> int {
    match (box) {
        BoxT<int> => 1,
        BoxT<string> => 2
    }
}

function classify_box_union_by_field(box: BoxT<int> | BoxT<string>) -> int {
    match (box) {
        BoxT<int> { value: let value: int } => value,
        BoxT<string> { value: let value: string } => value.length()
    }
}

function from_optional_box(box: BoxT<int>?) -> int {
    match (box) {
        BoxT<int> { value: let value: int } => value,
        null => 0
    }
}

function from_union_box(box: BoxT<int> | null) -> int {
    match (box) {
        BoxT<int> { value: let value: int } => value,
        null => 0
    }
}

function from_array_boxes(boxes: BoxT<int>[]) -> int {
    let total = 0;
    for (let BoxT<int> { value: let value: int } in boxes) {
        total += value;
    }
    total
}

function classify_box_either(box: BoxT<int> | BoxT<string>) -> int {
    match (box) {
        BoxT<int> {} | BoxT<string> {} => 7
    }
}

function deep_destruct_box() -> int {
    let outer: OuterT<int> = OuterT {
        inner: BoxT<int> { value: 123 }
    };
    let OuterT<int> {
        inner: BoxT<int> { value }
    } = outer;
    value
}

function match_person_as(p: PersonAS) -> int {
    match (p) {
        PersonAS { age: 7 } => 70,
        PersonAS { age: let actual } => actual
    }
}

function match_box_union(b: BoxUnion) -> int {
    match (b) {
        BoxUnion { value: int } => 1,
        BoxUnion { value: string } => 2
    }
}

function match_coord_user() -> int {
    match (UserNested {
        profile: Profile4 {
            address: Address4 {
                coordinate: Coordinate4 { zip: 22222 }
            }
        }
    }) {
        UserNested {
            profile: Profile4 {
                address: Address4 {
                    coordinate: Coordinate4 { zip: 11111 }
                }
            }
        } => 1,
        UserNested {
            profile: Profile4 {
                address: Address4 {
                    coordinate: Coordinate4 { zip: let zip }
                }
            }
        } => zip
    }
}

function or_deep_class() -> int {
    match (UserOr { address: Address3 { coordinate: Coordinate3 { zip: 7, plus4: 2 } } }) {
        (
            UserOr { address: Address3 { coordinate: Coordinate3 { zip: let zip: int, plus4: 1 } } }
        ) | (
            UserOr { address: Address3 { coordinate: Coordinate3 { zip: let zip: int, plus4: 2 } } }
        ) => zip,
        _ => 0
    }
}

// ─── dispatch by function signature ──────────────────────────────────────────

// A function-typed arm is a real, signature-precise test: the value matcher
// applies the canonical function relation (contravariant parameters, covariant
// return and throws) to the type the callable value reconstructs. Every
// callable reconstructs faithfully — a closure, generic function, or bound
// method materializes its stored signature templates against the realized
// frame it carries — so which arm a callable takes is decided by its actual
// signature, whatever flavor of callable it is.

function dispatch_cb(cb: ((int) -> int throws never) | ((string) -> string throws never)) -> string {
    match (cb) {
        let f: (int) -> int throws never => "int:" + f(2).to_string(),
        let g: (string) -> string throws never => "str:" + g("x")
    }
}

function test_dispatches_int_callback() -> string {
    dispatch_cb((n: int) -> int { n * 10 })
}

function test_dispatches_string_callback() -> string {
    dispatch_cb((s: string) -> string { s + "!" })
}

class DispatchCounter {
    n: int
    function bump(self, d: int) -> int throws never { self.n + d }
}

// A bound method carries its complete curried frame, and its reconstructed
// type drops the applied receiver — so it dispatches as the `(int) -> int` it
// is from the caller's side.
function test_dispatches_bound_method() -> string {
    let c = DispatchCounter { n: 100 }
    dispatch_cb(c.bump)
}

// A closure minted inside a generic frame: its stored signature references
// that frame's slots, and substituting the args the value captured yields
// `(int) -> int` — not a coarsened `unknown`.
function make_generic_cb<T>(v: T) -> (int) -> T throws never {
    (n: int) -> T { v }
}

function test_dispatches_generic_frame_closure() -> string {
    dispatch_cb(make_generic_cb<int>(7))
}

// A non-final function-typed arm ahead of a wildcard: its test is emitted and
// decides, rather than being elided or answering constant-false.
function test_fn_arm_before_wildcard(cb: ((int) -> int throws never) | int) -> string {
    match (cb) {
        let f: (int) -> int throws never => "fn:" + f(3).to_string(),
        _ => "other"
    }
}

function test_fn_arm_before_wildcard_fn() -> string {
    test_fn_arm_before_wildcard((n: int) -> int { n + 1 })
}

function test_fn_arm_before_wildcard_int() -> string {
    test_fn_arm_before_wildcard(5)
}

// `is` against a function type: a coarser `throws` accepts (covariant), a
// different parameter type rejects.
function test_fn_is_matching() -> bool {
    ((n: int) -> int { n }) is (int) -> int throws unknown
}

function test_fn_is_mismatched() -> bool {
    ((n: int) -> int { n }) is (string) -> string throws unknown
}

test "dispatches_int_callback" {
    assert.equal(test_dispatches_int_callback(), "int:20")
}

test "dispatches_string_callback" {
    assert.equal(test_dispatches_string_callback(), "str:x!")
}

test "dispatches_bound_method_by_receiver_dropped_signature" {
    assert.equal(test_dispatches_bound_method(), "int:102")
}

test "dispatches_closure_minted_in_generic_frame" {
    assert.equal(test_dispatches_generic_frame_closure(), "int:7")
}

test "non_final_fn_arm_decides_against_wildcard" {
    assert.equal(test_fn_arm_before_wildcard_fn(), "fn:4")
    assert.equal(test_fn_arm_before_wildcard_int(), "other")
}

test "fn_is_test_discriminates_by_signature" {
    assert.is_true(test_fn_is_matching())
    assert.is_true(!test_fn_is_mismatched())
}

// Per-test helpers that need unique names
function test_let_or_same_binding() -> int {
    let x | let x = 1;
    x
}

function test_let_or_six_same_bindings() -> int {
    let x | let x | let x | let x | let x | let x = 1;
    x
}

function test_match_or_bare_bindings() -> int {
    match (5) {
        let x | let x | let x => x + 1
    }
}

function test_match_or_chain_narrows() -> int {
    match (10) {
        (let x: int) | (let x: int) => x * 2
    }
}

function test_match_chain_of_bindings() -> int {
    match (7) {
        let a: let b: let c => a + b + c
    }
}

function test_match_chain_trailing_binding() -> int {
    match ([[9]]) {
        let rows: [[let x]]: int[][] => rows[0][0] * 100 + x * 10,
        _ => 0
    }
}

function test_match_chain_binding_guard() -> int {
    match (4) {
        let n: int if n > 2 => n * 10,
        _ => 0
    }
}

function test_for_let_chain_of_bindings() -> int {
    let total = 0;
    for (let a: let b in [1, 2, 3]) {
        total += a + b
    };
    total
}

function test_for_let_chain_trailing_type() -> int {
    let total = 0;
    for (let row: int[] in [[4, 5], [6, 7, 8]]) {
        total += row.length() * 100 + row[0] * 10 + row.length()
    };
    total
}

function test_for_let_chain_alias_capture() -> int {
    let funcs: (() -> int throws never)[] = [];
    for (let a: let b in [1, 2, 3]) {
        funcs.push(() -> int { b })
    };
    funcs[0]() * 100 + funcs[1]() * 10 + funcs[2]()
}

function test_for_let_or_same_binding() -> int {
    let total = 0;
    for (let x | let x in [1, 2, 3, 4]) {
        total += x
    };
    total
}

function test_for_let_or_six_bindings() -> int {
    let total = 0;
    for (let x | let x | let x | let x | let x | let x in [1, 2, 3]) {
        total += x
    };
    total
}

function test_for_rest_sums_rows() -> int {
    let rows: int[][] = [[1, 2, 3], [], [4]];
    let total = 0;
    for (let row in rows) {
        total += row.length()
    };
    total
}

function test_for_rest_capture_fresh_cell() -> int {
    let rows = [[1], [2, 3], [4, 5, 6]];
    let funcs: (() -> int throws never)[] = [];
    for (let row in rows) {
        funcs.push(() -> int { row.length() })
    };
    funcs[0]() * 100 + funcs[1]() * 10 + funcs[2]()
}

function test_for_inside_class_destruct() -> int {
    let teams = [
        Team1 { scores: [1, 2] },
        Team1 { scores: [] },
        Team1 { scores: [3] }
    ];
    let total = 0;
    for (let Team1 { scores: let s } in teams) {
        total += s.length()
    };
    total
}

function test_let_wildcard_discards() -> int {
    let _ = 5;
    42
}

function test_match_bind_type_ascription() -> int {
    match (1) {
        let n: int => n + 100
    }
}

function test_match_or_chain_swapped() -> int {
    match (7) {
        (let x: let y) | (let y: let x) => x + y
    }
}

function test_chain_second_binding_capture() -> int {
    let x: let y = 1;
    y = 2;
    let f = () -> int {
        y
    };
    f()
}

function test_let_class_destruct_shorthand() -> int {
    let PersonANS { age } = PersonANS { name: "Ada", age: 41, score: 99 };
    age
}

function test_let_class_destruct_rename() -> int {
    let PersonNA { age: let years } = PersonNA { name: "Ada", age: 42 };
    years
}

function test_let_class_destruct_nested() -> int {
    let PersonAddr6 { address: Address6 { zip } } =
        PersonAddr6 { age: 1, address: Address6 { zip: 94107, city: "SF" } };
    zip
}

function test_let_nested_same_field_names() -> int {
    let NestedOuter {
        field: let whole_field: NestedInner { field: let inner_field }
    } = NestedOuter {
        field: NestedInner { field: 21 },
        other: 1
    };
    whole_field.field + inner_field
}

function test_let_generic_destruct() -> int {
    let boxed: BoxT<int> = BoxT { value: 42 };
    let BoxT<int> { value } = boxed;
    value + 1
}

function test_empty_generic_destruct_contextual() -> int {
    let int_box: BoxT<int> = BoxT { value: 7 };
    let string_box: BoxT<string> = BoxT { value: "ready" };
    let BoxT<int> {} = int_box;
    let BoxT<string> {} = string_box;
    classify_box_int(int_box) + classify_box_string(string_box)
}

function test_match_generic_type_args_distinguish() -> int {
    let string_box: BoxT<string> = BoxT { value: "ready" };
    let int_box: BoxT<int> = BoxT { value: 7 };
    classify_box_union_by_type(string_box) * 10 + classify_box_union_by_type(int_box)
}

function test_match_generic_union_field() -> int {
    let string_box: BoxT<string> = BoxT { value: "ready" };
    let int_box: BoxT<int> = BoxT { value: 9 };
    classify_box_union_by_field(string_box) * 10 + classify_box_union_by_field(int_box)
}

function test_generic_backfills_wrapped() -> int {
    let boxed: BoxT<int> = BoxT { value: 40 };
    from_optional_box(boxed) + from_union_box(boxed) + from_array_boxes([boxed])
}

function test_empty_generic_covers_union() -> int {
    let string_box: BoxT<string> = BoxT { value: "ready" };
    let int_box: BoxT<int> = BoxT { value: 3 };
    classify_box_either(string_box) * 10 + classify_box_either(int_box)
}

function test_let_array_rest_binds() -> int {
    let [..] = [3, 4, 5];
    3
}

function test_let_array_rest_irrefutable() -> int {
    let [..] = [1, 2, 3];
    1
}

function test_match_wildcard_chained_array() -> int {
    let xs: [..]: int[] = [4, 5, 6];
    xs.length()
}

function test_match_array_shadows_outer() -> int {
    let x = 1;
    let inner = match ([4, 5]) {
        [let x, ..] => x,
        _ => 0
    };
    x * 10 + inner
}

function test_for_nested_rest_irrefutable() -> int {
    let total = 0;
    for (let [..] in [[1], [2, 3], []]) {
        total += 1;
    }
    total
}

function test_match_nested_array_projects_inner() -> int {
    match ([[4]]) {
        [[let x]] => x,
        [..] => 0
    }
}

function test_match_alphabet() -> int {
    match ([1, 2, 3, 4, 5, 6, 7, 8, 9, 10,
            11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
            21, 22, 23, 24, 25, 26]) {
        [let a, let b, let c, let d, let e, let f, let g, let h,
         let i, let j, let k, let l, let m, let n, let o, let p,
         let q, let r, let s, let t, let u, let v, let w, let x,
         let y, let z] =>
            a + b + c + d + e + f + g + h + i + j + k + l + m +
            n + o + p + q + r + s + t + u + v + w + x + y + z,
        _ => 0
    }
}

function test_for_rest_wildcard_irrefutable() -> int {
    let rows: int[][] = [[1], [], [2, 3]];
    let count = 0;
    for (let [..] in rows) {
        count += 1
    };
    count
}

function test_class_shorthand_same_name_as_class() -> int {
    let UserFieldUser { User } = UserFieldUser { User: 99, other: 1 };
    User
}

function test_match_class_binds_nested() -> int {
    match (PersonAddr5 { address: Address5 { zip: 12345 } }) {
        PersonAddr5 { address: Address5 { zip } } => zip
    }
}

function test_let_deep_class_inner_field() -> int {
    let UserDeep {
        profile: Profile2 {
            address: Address2 {
                coordinate: Coordinate2 {
                    zip: let zip
                }
            }
        }
    } = UserDeep {
        active: true,
        profile: Profile2 {
            score: 9,
            address: Address2 {
                city: "SF",
                coordinate: Coordinate2 { zip: 94107, plus4: 1200 }
            }
        }
    };
    zip
}

function test_for_deep_class_sums() -> int {
    let users = [
        UserCoord { address: Addr { coordinate: Coord { zip: 1 } } },
        UserCoord { address: Addr { coordinate: Coord { zip: 20 } } },
        UserCoord { address: Addr { coordinate: Coord { zip: 300 } } }
    ];
    let total = 0;
    for (let UserCoord {
        address: Addr {
            coordinate: Coord { zip: let zip }
        }
    } in users) {
        total += zip
    };
    total
}

function test_for_deep_class_capture_fresh() -> int {
    let users = [
        UserCoord { address: Addr { coordinate: Coord { zip: 1 } } },
        UserCoord { address: Addr { coordinate: Coord { zip: 2 } } },
        UserCoord { address: Addr { coordinate: Coord { zip: 3 } } }
    ];
    let funcs: (() -> int throws never)[] = [];
    for (let UserCoord {
        address: Addr {
            coordinate: Coord { zip: let zip }
        }
    } in users) {
        funcs.push(() -> int { zip })
    };
    funcs[0]() * 100 + funcs[1]() * 10 + funcs[2]()
}

function test_let_class_binds_inner_zip() -> int {
    let UserCoord {
        address: Addr {
            coordinate: Coord { zip: let zip }
        }
    } = UserCoord {
        address: Addr {
            coordinate: Coord { zip: 80808 }
        }
    };
    zip
}

function test_match_class_binds_inner_zip() -> int {
    match (UserCoord { address: Addr { coordinate: Coord { zip: 9 } } }) {
        UserCoord {
            address: Addr {
                coordinate: Coord { zip: 1 }
            }
        } => 1,
        UserCoord {
            address: Addr {
                coordinate: Coord { zip: let zip }
            }
        } => zip
    }
}

function test_for_class_binds_inner_zip() -> int {
    let users = [
        UserCoord { address: Addr { coordinate: Coord { zip: 4 } } },
        UserCoord { address: Addr { coordinate: Coord { zip: 5 } } },
        UserCoord { address: Addr { coordinate: Coord { zip: 6 } } }
    ];
    let product = 1;
    for (let UserCoord {
        address: Addr {
            coordinate: Coord { zip: let zip }
        }
    } in users) {
        product *= zip
    };
    product
}

function test_five_levels_or_flipped() -> int {
    let target = Atom1 { value: 42 };
    let stuffer_atom = Atom1 { value: 0 };
    let stuffer_slot = Slot1 { atoms: [stuffer_atom, stuffer_atom, stuffer_atom] };
    let stuffer_row = Row1 { slots: [stuffer_slot, stuffer_slot, stuffer_slot] };
    let target_slot = Slot1 { atoms: [stuffer_atom, target, stuffer_atom] };
    let target_row = Row1 { slots: [stuffer_slot, target_slot, stuffer_slot] };
    pick_sheet(Sheet1 { rows: [stuffer_row, target_row, stuffer_row] })
}

function test_or_four_alternates() -> int {
    let a = pick_pair(Pair1 { left: Wrap1 { inner: [99, 7, 100] }, right: [0] });
    let b = pick_pair(Pair1 { left: Wrap1 { inner: [99] }, right: [99, 99, 8, 99] });
    let c = pick_pair(Wrap1 { inner: [99, 9, 99, 99] });
    let d = pick_pair([[99], [4, 5, 6], [99]]);
    a * 1000 + b * 100 + c * 10 + d
}

function test_or_2d_vs_class() -> int {
    let bare = score_wrap2([[7, 1, 2], [8, 9]]);
    let wrapped = score_wrap2(Wrap2 { grid: [[1, 2], [3, 11, 5]] });
    bare * 100 + wrapped
}

// ─── Tests ────────────────────────────────────────────────────────────────────

test "let_or_of_same_binding" {
    assert.equal(test_let_or_same_binding(), 1)
}

test "let_or_of_six_same_bindings" {
    assert.equal(test_let_or_six_same_bindings(), 1)
}

test "match_or_of_bare_bindings_uses_value" {
    assert.equal(test_match_or_bare_bindings(), 6)
}

test "match_or_of_chain_narrows_uses_value" {
    assert.equal(test_match_or_chain_narrows(), 20)
}

test "match_chain_of_bindings_aliases_scrutinee" {
    assert.equal(test_match_chain_of_bindings(), 21)
}

test "match_chain_trailing_binding_after_array_type_aliases_value" {
    assert.equal(test_match_chain_trailing_binding(), 990)
}

test "match_chain_binding_used_in_guard" {
    assert.equal(test_match_chain_binding_guard(), 40)
}

test "for_let_chain_of_bindings" {
    assert.equal(test_for_let_chain_of_bindings(), 12)
}

test "for_let_chain_trailing_binding_after_type_aliases_iteration_value" {
    assert.equal(test_for_let_chain_trailing_type(), 605)
}

test "for_let_chain_alias_capture_per_iteration" {
    assert.equal(test_for_let_chain_alias_capture(), 123)
}

test "for_let_or_of_same_binding" {
    assert.equal(test_for_let_or_same_binding(), 10)
}

test "for_let_or_of_six_same_bindings" {
    assert.equal(test_for_let_or_six_bindings(), 6)
}

test "for_array_destructure_rest_sums_rows" {
    assert.equal(test_for_rest_sums_rows(), 4)
}

test "for_array_destructure_rest_capture_gets_fresh_cell_per_iteration" {
    assert.equal(test_for_rest_capture_fresh_cell(), 123)
}

test "for_array_destructure_inside_class_destructure_sums_fields" {
    assert.equal(test_for_inside_class_destruct(), 3)
}

test "catch_or_of_same_typed_binding" {
    assert.equal(risky_throws_string() catch (e) { (let s: string) | (let s: string) => s }, "boom")
}

test "let_wildcard_discards_value" {
    assert.equal(test_let_wildcard_discards(), 42)
}

test "match_bind_with_type_ascription" {
    assert.equal(test_match_bind_type_ascription(), 101)
}

test "match_or_of_chain_bindings_swapped" {
    assert.equal(test_match_or_chain_swapped(), 14)
}

test "chain_second_binding_capture_after_reassign" {
    assert.equal(test_chain_second_binding_capture(), 2)
}

test "let_class_destructure_binds_shorthand_and_ignores_omitted_fields" {
    assert.equal(test_let_class_destruct_shorthand(), 41)
}

test "let_class_destructure_renames_field_binding" {
    assert.equal(test_let_class_destruct_rename(), 42)
}

test "let_class_destructure_nested_class" {
    assert.equal(test_let_class_destruct_nested(), 94107)
}

test "let_nested_same_field_names_are_ok_with_distinct_bindings" {
    assert.equal(test_let_nested_same_field_names(), 42)
}

test "let_generic_class_destructure_substitutes_field_types" {
    assert.equal(test_let_generic_destruct(), 43)
}

test "empty_generic_class_destructure_uses_contextual_type" {
    assert.equal(test_empty_generic_destruct_contextual(), 21)
}

test "match_generic_class_type_patterns_distinguish_type_args" {
    assert.equal(test_match_generic_type_args_distinguish(), 21)
}

test "match_generic_class_destructure_union_tests_substituted_field_type" {
    assert.equal(test_match_generic_union_field(), 59)
}

test "generic_class_destructure_backfills_through_wrapped_flow_types" {
    assert.equal(test_generic_backfills_wrapped(), 120)
}

test "empty_generic_class_destructure_covers_union_of_same_class_instantiations" {
    assert.equal(test_empty_generic_covers_union(), 77)
}

test "deep_generic_class_destructure_substitutes_through_nested_fields" {
    assert.equal(deep_destruct_box(), 123)
}

test "match_class_destructure_tests_field_literal_and_falls_through" {
    assert.equal(match_person_as(PersonAS { age: 5, score: 99 }), 5)
}

test "match_class_destructure_tests_union_field_type" {
    assert.equal(match_box_union(BoxUnion { value: "ready" }), 2)
}

test "match_top_level_class_destructure_tests_union_scrutinee" {
    assert.equal(score_person_age(PersonAge { age: 42 }) * 10 + score_person_age(7), 420)
}

test "match_or_class_destructure_same_binding_name_compatible_types" {
    assert.equal(score_personid(PersonIdB { id: 77 }), 77)
}

test "match_or_class_destructure_same_binding_name_different_depths" {
    assert.equal(
        score_depth(DepthClass { field: 4 }) * 10 + score_depth(DepthClass2 { c: DepthClass3 { field: 7 } }),
        47,
    )
}

test "match_array_destructure_empty_and_prefix_rest" {
    assert.equal(score_empty_prefix_rest([]) * 10 + score_empty_prefix_rest([7, 8, 9]), 7)
}

function score_exact_length_45() -> int {
    match ([4, 5]) {
        [let one] => one,
        [let first, let second] => first * 10 + second,
        _ => 0
    }
}

test "match_array_destructure_exact_length" {
    assert.equal(score_exact_length_45(), 45)
}

test "match_array_destructure_binds_rest_copy" {
    assert.equal(score_rest_copy([1, 2, 3]), 12)
}

test "match_array_destructure_suffix_rest" {
    assert.equal(score_suffix_rest([1, 2, 9]), 29)
}

test "let_array_destructure_binds_whole_rest" {
    assert.equal(test_let_array_rest_binds(), 3)
}

test "let_array_destructure_nested_rest_only_is_irrefutable" {
    assert.equal(test_let_array_rest_irrefutable(), 1)
}

test "match_array_destructure_entire_alphabet_of_bindings" {
    assert.equal(test_match_alphabet(), 351)
}

test "match_or_binding_from_class_field_or_nested_array_position" {
    assert.equal(score_opat(OPat_A { x: 4 }) * 10 + score_opat(OPat_B { y: [9, 7] }), 47)
}

test "match_wildcard_inside_chained_binding_type_array_pattern" {
    assert.equal(test_match_wildcard_chained_array(), 3)
}

test "match_array_destructure_shadows_outer_local_only_inside_arm" {
    assert.equal(test_match_array_shadows_outer(), 14)
}

test "for_array_destructure_nested_rest_only_is_irrefutable" {
    assert.equal(test_for_nested_rest_irrefutable(), 3)
}

test "match_array_destructure_literal_and_type_element_patterns" {
    assert.equal(
        score_ints([1, 2, 3]) * 100 + score_ints([1, 9]) * 10 + score_strings_len(["a", "b"]),
        1302,
    )
}

test "match_array_destructure_or_patterns_share_bindings" {
    assert.equal(score_or_share([4]) * 10 + score_or_share([5, 6]), 45)
}

test "match_array_destructure_or_patterns_nested_arrays_share_bindings" {
    assert.equal(score_or_nested_2d([[4, 5]]) * 10 + score_or_nested_2d([[7, 9], [2]]), 47)
}

test "match_array_destructure_nested_array_binding_projects_inner_element" {
    assert.equal(test_match_nested_array_projects_inner(), 4)
}

test "match_array_destructure_or_patterns_nested_class_array_fields" {
    assert.equal(
        score_team2([Team2 { scores: [3, 4] }]) * 10 + score_team2([ Team2 { scores: [8, 9] }, Team2 { scores: [1] } ]),
        39,
    )
}

test "match_array_destructure_outer_chain_applies_to_whole_array" {
    assert.equal(score_outer_chain([[1], [2], [3], [4]]), 234)
}

test "match_alternating_class_array_five_levels_or_flipped_traversal" {
    assert.equal(test_five_levels_or_flipped(), 42)
}

test "match_or_four_alternates_same_name_different_paths" {
    assert.equal(test_or_four_alternates(), 7896)
}

test "match_deep_class_array_or_picks_deep_cell" {
    assert.equal(
        pick_grid(Grid1 { cells: [ Cell1 { row: [1, 2, 3] }, Cell1 { row: [10, 42, 30] }, Cell1 { row: [100, 200, 300] } ] }),
        42,
    )
}

test "match_or_2d_array_vs_class_with_2d_field" {
    assert.equal(test_or_2d_vs_class(), 711)
}

test "match_array_destructure_two_prefix_rest_suffix_binding" {
    assert.equal(score_two_prefix([7, 8, 99, 99, 9]), 789)
}

test "match_array_destructure_prefix_rest_two_suffix_bindings" {
    assert.equal(score_prefix_two_suffix([7, 99, 99, 8, 9]), 789)
}

test "match_array_destructure_inside_class_destructure_empty_field" {
    assert.equal(score_team3(Team3 { scores: [] }) + score_team3(Team3 { scores: [7, 8] }), 17)
}

test "match_array_destructure_with_class_elements" {
    assert.equal(
        score_user_score([ UserScore { name: "a", score: 4 }, UserScore { name: "b", score: 9 }, UserScore { name: "c", score: 8 } ]),
        42,
    )
}

test "match_array_destructure_wildcard_elements_and_rest" {
    assert.equal(score_wildcard_second([5, 8, 13, 21]) * 10 + score_wildcard_second([1]), 80)
}

test "match_array_destructure_wildcard_length_shapes" {
    assert.equal(
        exact_length([]) * 1000000 + prefix_length([]) * 10000000 + exact_length([9]) * 100000 + exact_length([9, 8]) * 10000 + exact_length([9, 8, 7]) * 1000 + prefix_length([5]) * 100 + rest_any([]) * 10 + rest_any([1]),
        123177,
    )
}

test "match_array_destructure_wildcards_nested_in_class_fields" {
    assert.equal(score_team4(Team4 { scores: [4, 9, 16] }), 9)
}

test "match_array_destructure_wildcard_class_fields_and_rest" {
    assert.equal(
        score_wildcard_user([ UserScore { name: "hidden", score: 6 }, UserScore { name: "ignored", score: 7 } ]),
        6,
    )
}

test "for_array_destructure_rest_wildcard_is_irrefutable" {
    assert.equal(test_for_rest_wildcard_irrefutable(), 3)
}

test "class_destructure_shorthand_field_same_name_as_class_binds_field" {
    assert.equal(test_class_shorthand_same_name_as_class(), 99)
}

test "match_class_destructure_binds_nested_field" {
    assert.equal(test_match_class_binds_nested(), 12345)
}

test "let_deep_class_destructure_binds_inner_field" {
    assert.equal(test_let_deep_class_inner_field(), 94107)
}

test "match_deep_class_destructure_falls_through_to_binding_arm" {
    assert.equal(match_coord_user(), 22222)
}

test "match_or_deep_class_destructure_binds_when_second_alt_matches" {
    assert.equal(or_deep_class(), 7)
}

test "for_deep_class_destructure_sums_fields" {
    assert.equal(test_for_deep_class_sums(), 321)
}

test "for_deep_class_destructure_capture_gets_fresh_cell_per_iteration" {
    assert.equal(test_for_deep_class_capture_fresh(), 123)
}

test "let_class_destructure_binds_inner_zip" {
    assert.equal(test_let_class_binds_inner_zip(), 80808)
}

test "match_class_destructure_binds_inner_zip" {
    assert.equal(test_match_class_binds_inner_zip(), 9)
}

test "for_class_destructure_binds_inner_zip" {
    assert.equal(test_for_class_binds_inner_zip(), 120)
}

test "match_class_field_pattern_should_refine_parent_union_scrutinee" {
    assert.equal(score_refine_a(RefineA { field: 42 }), 42)
}

test "match_class_destructure_union_field_int_arm" {
    assert.equal(score_foo_union(FooUnion { value: 7 }), 1)
}

test "match_class_destructure_union_field_float_arm" {
    assert.equal(score_foo_union(FooUnion { value: 1.5 }), 2)
}

test "match_class_destructure_union_field_bool_arm" {
    assert.equal(score_foo_union(FooUnion { value: true }), 3)
}

test "match_array_element_pattern_should_refine_parent_union_scrutinee" {
    assert.equal(score_int_string_array([21]), 42)
}

test "match_or_class_patterns_should_refine_parent_union_scrutinee" {
    assert.equal(score_abcde(ABCDE_D { field: 42 }), 42)
}

test "match_or_array_patterns_should_refine_parent_union_scrutinee" {
    assert.equal(score_or_array_refine([21]), 42)
}

test "rustc_port_nested_class_optional_full_coverage_true_present" {
    assert.equal(score_foo3(Foo3 { first: true, second: [1, 2, 3] }), 2)
}

test "rustc_port_nested_class_optional_full_coverage_false_null" {
    assert.equal(score_foo3(Foo3 { first: false, second: null }), 3)
}

test "rustc_port_slice_prefix_dispatch" {
    assert.equal(score_bool_slice([true, false, false]), 1)
}

test "rustc_port_slice_suffix_dispatch" {
    assert.equal(score_bool_slice([false, true, false]), 2)
}

test "rustc_port_enum_full_variant_dispatch_north" {
    assert.equal(describe_direction(Direction.North), "up")
}

test "rustc_port_enum_full_variant_dispatch_west" {
    assert.equal(describe_direction(Direction.West), "left")
}

test "rustc_port_or_pattern_in_class_field_first_alt" {
    assert.equal(score_pair_or(PairAB { a: 0, b: 3 }), 1)
}

test "rustc_port_or_pattern_in_class_field_falls_to_wildcard" {
    assert.equal(score_pair_or(PairAB { a: 5, b: 5 }), 0)
}

test "rustc_port_optional_literal_or_arm_zero" {
    assert.equal(score_int_optional(0), 1)
}

test "rustc_port_optional_literal_or_arm_null" {
    assert.equal(score_int_optional(null), 1)
}

test "rustc_port_optional_literal_or_arm_other" {
    assert.equal(score_int_optional(99), 0)
}

// ─── `unknown` is the top type, including in a refutable position ────────────
// An exhaustive final `let v: unknown` arm has its test elided, so the bug this
// pins is only reachable when the arm must actually be tested — here a guard
// forces that. Emitting the tagless `unknown` leaf as a coarse type-tag check
// used to fall through to constant-FALSE, so every value fell past the arm.

class UnknownArmBox {
    v: int
}

function unknown_arm_tested(x: unknown, gate: bool) -> string {
    match (x) {
        let v: unknown if gate => "top",
        _ => "fell-through",
    }
}

function unknown_arm_int_fn() -> string {
    unknown_arm_tested(5, true)
}

test "guarded_unknown_arm_matches_int" {
    assert.equal(unknown_arm_int_fn(), "top")
}

function unknown_arm_class_fn() -> string {
    unknown_arm_tested(UnknownArmBox { v: 1 }, true)
}

test "guarded_unknown_arm_matches_class" {
    assert.equal(unknown_arm_class_fn(), "top")
}

function unknown_arm_closure_fn() -> string {
    unknown_arm_tested((n: int) -> int throws never { n }, true)
}

test "guarded_unknown_arm_matches_closure" {
    assert.equal(unknown_arm_closure_fn(), "top")
}

function unknown_arm_null_fn() -> string {
    unknown_arm_tested(null, true)
}

test "guarded_unknown_arm_matches_null" {
    assert.equal(unknown_arm_null_fn(), "top")
}

// The guard still decides: a false gate falls through, so the arm is a real
// test rather than an unconditional jump.
function unknown_arm_gate_false_fn() -> string {
    unknown_arm_tested(5, false)
}

test "guarded_unknown_arm_respects_guard" {
    assert.equal(unknown_arm_gate_false_fn(), "fell-through")
}
