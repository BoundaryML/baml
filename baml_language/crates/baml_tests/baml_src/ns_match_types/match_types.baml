// Unified tests for match types: instanceof/typed patterns, enums, classes,
// guards (both typed and int literal guards), typetag switch, and mixed patterns
// (literals + types + guards).
//
// Converted from crates/baml_tests/tests/match_types.rs.
//
// SKIPPED (compile-error bug):
//   - match_typed_pattern_first_arm: inline scrutinee ascription triggers
//     "type mismatch / unreachable arm" in the typechecker.
//
// Previously skipped but now active:
//   - match_typed_pattern_with_field_access: passes when using a type alias
//     for the scrutinee (same pattern as match_typed_pattern_second_arm).

// ============================================================================
// Typed Pattern Tests (instanceof)
// ============================================================================

class Success {
    data string
}

class Failure {
    reason string
}

type Res = Success | Failure

// match_typed_pattern_second_arm: scrutinee is a local instance → helper.
function typed_pattern_second_arm() -> string {
    let result: Res = Failure { reason: "error" };
    match (result) {
        let s: Success => "success: " + s.data,
        let f: Failure => "failure: " + f.reason,
    }
}

test "match_typed_pattern_second_arm" {
    assert.equal(typed_pattern_second_arm(), "failure: error")
}

// match_typed_pattern_first_arm: scrutinee has inline type ascription in match.
// Previously ignored; passes when using a type alias, but the original RS test
// uses `match (result: Success | Failure)` inline ascription syntax.
//
// BUG: compile error — "type mismatch: expected Success, got Failure" and
// "unreachable arm" when the scrutinee carries inline type ascription.
// The typechecker narrows the ascribed type to the concrete value type, making
// the second arm an unreachable type-error.
//
// class MtpSuccess {
//     data string
// }
//
// class MtpFailure {
//     reason string
// }
//
// function match_typed_pattern_first_arm() -> string {
//     let result = MtpSuccess { data: "hello" };
//     match (result: MtpSuccess | MtpFailure) {
//         let s: MtpSuccess => "success: " + s.data,
//         MtpFailure => "failure",
//     }
// }
//
// test "match_typed_pattern_first_arm" {
//     assert.equal(match_typed_pattern_first_arm(), "success: hello")
// }

// match_typed_pattern_with_field_access: previously ignored; passes now when
// using a type alias for the scrutinee (same conversion pattern as second_arm).
class MtpPoint {
    x int
    y int
}

class MtpCircle {
    radius int
}

type MtpShape = MtpPoint | MtpCircle

function match_typed_pattern_with_field_access() -> int {
    let shape: MtpShape = MtpPoint { x: 10, y: 20 };
    match (shape) {
        let p: MtpPoint => p.x + p.y,
        let c: MtpCircle => c.radius,
    }
}

test "match_typed_pattern_with_field_access" {
    assert.equal(match_typed_pattern_with_field_access(), 30)
}

// ============================================================================
// TypeTag Switch Tests (discard patterns path)
// ============================================================================

function classify_primitives(x: int | string | bool | float) -> int {
    match (x) {
        int => 1,
        string => 2,
        bool => 3,
        float => 4,
    }
}

test "match_typed_discard_patterns_typetag_switch_path" {
    assert.equal(classify_primitives(true), 3)
}

// ============================================================================
// Guard Tests
// ============================================================================

class Score {
    value int
}

// match_guard_true: scrutinee is a local instance → helper.
function guard_true() -> string {
    let s = Score { value: 95 };
    match (s) {
        let x: Score if x.value >= 90 => "excellent",
        let x: Score if x.value >= 70 => "good",
        Score => "needs work",
    }
}

test "match_guard_true" {
    assert.equal(guard_true(), "excellent")
}

function guard_fallthrough() -> string {
    let s = Score { value: 75 };
    match (s) {
        let x: Score if x.value >= 90 => "excellent",
        let x: Score if x.value >= 70 => "good",
        Score => "needs work",
    }
}

test "match_guard_fallthrough" {
    assert.equal(guard_fallthrough(), "good")
}

function guard_all_fail() -> string {
    let s = Score { value: 50 };
    match (s) {
        let x: Score if x.value >= 90 => "excellent",
        let x: Score if x.value >= 70 => "good",
        Score => "needs work",
    }
}

test "match_guard_all_fail" {
    assert.equal(guard_all_fail(), "needs work")
}

// ============================================================================
// Guards with Integer Literals
// ============================================================================

function classify_guarded_int(x: int, flag: bool) -> string {
    match (x) {
        1 if flag => "one with flag",
        1 => "one without flag",
        2 => "two",
        _ => "other"
    }
}

function guarded_main_with_flag() -> string {
    classify_guarded_int(1, true)
}

function guarded_main_without_flag() -> string {
    classify_guarded_int(1, false)
}

test "match_guarded_int_literal_guard_true" {
    assert.equal(guarded_main_with_flag(), "one with flag")
}

test "match_guarded_int_literal_guard_false" {
    assert.equal(guarded_main_without_flag(), "one without flag")
}

function classify_all_guarded(x: int, flag: bool) -> string {
    match (x) {
        0 if flag => "zero",
        1 if flag => "one",
        2 if flag => "two",
        _ => "fallback"
    }
}

function all_guarded_main() -> string {
    classify_all_guarded(1, false)
}

test "match_all_arms_guarded_all_fail" {
    assert.equal(all_guarded_main(), "fallback")
}

// ============================================================================
// Mixed Patterns: Literals + Types + Guards
// ============================================================================

function classify_mixed_lit_typed_guard(x: int, flag: bool) -> string {
    match (x) {
        0 => "zero",
        1 if flag => "one with flag",
        let n: int => "other int"
    }
}

test "match_mixed_literal_typed_guard" {
    assert.equal(classify_mixed_lit_typed_guard(1, true), "one with flag")
}

test "match_mixed_literal_typed_guard_fallthrough" {
    assert.equal(classify_mixed_lit_typed_guard(1, false), "other int")
}

function classify_guard_on_typed_field(result: Success | Failure) -> string {
    match (result) {
        let s: Success if s.data != "" => "success with data",
        let s: Success => "empty success",
        let f: Failure => "failure"
    }
}

test "match_guard_on_typed_pattern_field_access" {
    assert.equal(classify_guard_on_typed_field(Success { data: "hello" }), "success with data")
}

test "match_guard_on_typed_pattern_field_access_fails" {
    assert.equal(classify_guard_on_typed_field(Success { data: "" }), "empty success")
}

// ============================================================================
// Enum Variant Patterns
// ============================================================================

enum Status {
    Active
    Inactive
    Pending
}

function classify_status(s: Status) -> string {
    match (s) {
        Status.Active => "active",
        Status.Inactive => "inactive",
        Status.Pending => "pending"
    }
}

test "match_enum_variant_first" {
    assert.equal(classify_status(Status.Active), "active")
}

test "match_enum_variant_last" {
    assert.equal(classify_status(Status.Pending), "pending")
}

function classify_status_wildcard(s: Status) -> string {
    match (s) {
        Status.Active => "active",
        Status.Inactive => "inactive",
        _ => "other"
    }
}

test "match_enum_variant_with_wildcard" {
    assert.equal(classify_status_wildcard(Status.Pending), "other")
}

test "match_enum_variant_with_wildcard_matched" {
    assert.equal(classify_status_wildcard(Status.Active), "active")
}

// ============================================================================
// Enum Variant Patterns with 4+ Arms (jump table)
// ============================================================================

enum Direction {
    North
    East
    South
    West
}

function compass(d: Direction) -> string {
    match (d) {
        Direction.North => "N",
        Direction.East => "E",
        Direction.South => "S",
        Direction.West => "W"
    }
}

test "match_enum_four_variants_jump_table" {
    assert.equal(compass(Direction.South), "S")
}

// ============================================================================
// Exhaustive and Non-Exhaustive Class Type Tests
// ============================================================================

class Cat { name string }
class Dog { name string }
class Bird { name string }

function classify_animal(animal: Cat | Dog | Bird) -> string {
    match (animal) {
        let c: Cat => "cat: " + c.name,
        let d: Dog => "dog: " + d.name,
        let b: Bird => "bird: " + b.name
    }
}

test "match_class_types_exhaustive_first" {
    assert.equal(classify_animal(Cat { name: "Whiskers" }), "cat: Whiskers")
}

test "match_class_types_exhaustive_last" {
    assert.equal(classify_animal(Bird { name: "Tweety" }), "bird: Tweety")
}

function classify_animal_wildcard(animal: Cat | Dog | Bird) -> string {
    match (animal) {
        let c: Cat => "cat: " + c.name,
        let d: Dog => "dog: " + d.name,
        _ => "other"
    }
}

test "match_class_types_non_exhaustive_wildcard" {
    assert.equal(classify_animal_wildcard(Bird { name: "Tweety" }), "other")
}

test "match_class_types_non_exhaustive_matched" {
    assert.equal(classify_animal_wildcard(Dog { name: "Rex" }), "dog: Rex")
}

// ============================================================================
// TypeTag Switch Tests (Union Types with Typed Patterns)
// ============================================================================

function identify(x: int | string | bool | float) -> string {
    match (x) {
        let n: int => "integer",
        let s: string => "text",
        let b: bool => "boolean",
        let f: float => "decimal"
    }
}

test "match_union_type_four_patterns_type_tag" {
    assert.equal(identify("hello"), "text")
}

// ============================================================================
// Multiple Typed Patterns with Guards
// ============================================================================

class SuccessCode { code int }
class FailureCode { code int }

function classify_result_guarded(result: SuccessCode | FailureCode, strict: bool) -> string {
    match (result) {
        let s: SuccessCode if s.code > 200 => "redirect",
        let s: SuccessCode if strict => "strict success",
        let s: SuccessCode => "success",
        let f: FailureCode => "failure"
    }
}

test "match_multiple_typed_patterns_with_guards" {
    assert.equal(classify_result_guarded(SuccessCode { code: 301 }, false), "redirect")
}

// ============================================================================
// Step 1.6 — Explicit TypeTag dispatch shape verification
// ============================================================================

class Fish { name string }

function describe_animal(animal: Cat | Dog | Bird | Fish) -> string {
    match (animal) {
        let c: Cat  => "cat",
        let d: Dog  => "dog",
        let b: Bird => "bird",
        let f: Fish => "fish"
    }
}

test "match_class_type_tag_jump_table" {
    assert.equal(describe_animal(Dog { name: "Rex" }), "dog")
}

function describe_two(animal: Cat | Dog) -> string {
    match (animal) {
        let c: Cat => "cat",
        let d: Dog => "dog"
    }
}

test "match_class_type_is_type_chain" {
    assert.equal(describe_two(Cat { name: "Whiskers" }), "cat")
}

class MyClass { value int }

function classify_mixed_class_primitive(x: MyClass | int | string | bool) -> string {
    match (x) {
        let c: MyClass => "class",
        let n: int     => "int",
        let s: string  => "string",
        let b: bool    => "bool"
    }
}

test "match_mixed_class_primitive_type_tag_switch" {
    assert.equal(classify_mixed_class_primitive(MyClass { value: 42 }), "class")
}

// ============================================================================
// Sparse Class Tags (Perfect Hash)
// ============================================================================

class Filler1 { x int }
class Filler2 { x int }
class Filler3 { x int }
class Filler4 { x int }
class Filler5 { x int }
class Filler6 { x int }
class Filler7 { x int }
class Filler8 { x int }
class Filler9 { x int }
class Filler10 { x int }
class Filler11 { x int }
class Filler12 { x int }

function describe_sparse(animal: Cat | Dog | Bird | Fish) -> string {
    match (animal) {
        let c: Cat  => "cat",
        let d: Dog  => "dog",
        let b: Bird => "bird",
        let f: Fish => "fish"
    }
}

test "match_sparse_class_type_tag_perfect_hash" {
    assert.equal(describe_sparse(Dog { name: "Rex" }), "dog")
}

// ============================================================================
// Null in TypeTag Switch
// ============================================================================

function classify_with_null(x: int | string | bool | null) -> string {
    match (x) {
        int    => "int",
        string => "string",
        bool   => "bool",
        null   => "null"
    }
}

test "match_null_in_type_tag_jump_table" {
    assert.equal(classify_with_null(null), "null")
}

function classify_null_with_class(x: Cat | Dog | null) -> string {
    match (x) {
        let c: Cat  => "cat",
        let d: Dog  => "dog",
        null => "nothing"
    }
}

test "match_null_with_class_types_type_tag" {
    assert.equal(classify_null_with_class(null), "nothing")
}

// Dense class tags still use direct JumpTable (same as match_class_type_tag_jump_table)
test "match_dense_class_still_uses_direct_jump_table" {
    assert.equal(describe_animal(Dog { name: "Rex" }), "dog")
}
