// End-to-end tests for rigid-typevar patterns whose type is NOT statically a
// member of the scrutinee type. Such a pattern is *potentially* unifiable with
// a scrutinee member — it matches exactly when the enclosing frame realizes the
// variable accordingly (`Box<T>` matches a `Box<int>` value iff `T = int`).
// Before the overlap-oracle narrowing fix, TIR intersected these pairs to
// `never` and MIR emitted a constant-false test — the arm silently never
// matched regardless of the realized type.
//
// Static coverage models these rigid arms as possible-but-not-covering rows:
// they never statically cover a concrete scrutinee member, so every match here
// needs (and keeps) its trailing `_` arm, which stays reachable.

// ─── 1. class-generic pattern in a method body (match + `is`) ─────────────────

class RigidBox<T> {
    v: T

    // `Box<T>` with the class's own `T`, against a scrutinee that mentions
    // only concrete types.
    function match_own(self, other: RigidBox<int> | int) -> string {
        match (other) {
            let b: RigidBox<T> => "box",
            _ => "other",
        }
    }

    function is_own(self, other: RigidBox<int> | int) -> bool {
        other is RigidBox<T>
    }
}

function box_match_hit_fn() -> string {
    RigidBox<int> { v: 1 }.match_own(RigidBox<int> { v: 2 })
}

test "class_generic_match_matches_realized_instantiation" {
    assert.equal(box_match_hit_fn(), "box")
}

function box_match_miss_fn() -> string {
    RigidBox<int> { v: 1 }.match_own(5)
}

test "class_generic_match_rejects_non_member" {
    assert.equal(box_match_miss_fn(), "other")
}

function box_is_hit_fn() -> bool {
    RigidBox<int> { v: 1 }.is_own(RigidBox<int> { v: 2 })
}

test "class_generic_is_matches_realized_instantiation" {
    assert.is_true(box_is_hit_fn())
}

function box_is_miss_fn() -> bool {
    RigidBox<int> { v: 1 }.is_own(5)
}

test "class_generic_is_rejects_non_member" {
    assert.is_true(!box_is_miss_fn())
}

// ─── 2. function-generic pattern vs a concrete-membered scrutinee ─────────────

function is_rigid_box<T>(x: RigidBox<int> | int) -> bool {
    x is RigidBox<T>
}

function fn_generic_is_hit_fn() -> bool {
    is_rigid_box<int>(RigidBox<int> { v: 1 })
}

test "fn_generic_is_matches_at_realized_arg" {
    assert.is_true(fn_generic_is_hit_fn())
}

function fn_generic_is_value_miss_fn() -> bool {
    is_rigid_box<int>(5)
}

test "fn_generic_is_rejects_non_class_value" {
    assert.is_true(!fn_generic_is_value_miss_fn())
}

function fn_generic_is_arg_miss_fn() -> bool {
    // The value is a `RigidBox<int>` but `T = string`: invariant generics, no match.
    is_rigid_box<string>(RigidBox<int> { v: 1 })
}

test "fn_generic_is_invariant_on_realized_arg" {
    assert.is_true(!fn_generic_is_arg_miss_fn())
}

function pick_rigid<T>(x: int | string) -> string {
    // Bare `T` against a scrutinee with only concrete members: the arm is
    // reachable (some realization matches each member), decided per-frame.
    match (x) {
        let t: T => "T",
        _ => "fallthrough",
    }
}

function pick_rigid_hit_fn() -> string {
    pick_rigid<int>(42)
}

test "fn_generic_arm_matches_realized_member" {
    assert.equal(pick_rigid_hit_fn(), "T")
}

function pick_rigid_other_member_fn() -> string {
    pick_rigid<int>("hi")
}

test "fn_generic_arm_rejects_other_member" {
    assert.equal(pick_rigid_other_member_fn(), "fallthrough")
}

function pick_rigid_foreign_fn() -> string {
    // `T = bool` matches neither member at runtime.
    pick_rigid<bool>(42)
}

test "fn_generic_arm_rejects_foreign_realization" {
    assert.equal(pick_rigid_foreign_fn(), "fallthrough")
}

// ─── 3. concrete arms over rigid scrutinees (the mirror direction) ────────────

// A concrete pattern over a bare-`T` scrutinee is possible-but-not-covering:
// it matches exactly the realizations that make `T` its subtype, so the `_`
// arm is statically required (omitting it is a non-exhaustive error, pinned in
// projects/diagnostic_errors/generic_rigid_coverage) and stays reachable.
function pick_concrete<T>(x: T) -> string {
    match (x) {
        let s: string => s + "!",
        _ => "other",
    }
}

function pick_concrete_miss_fn() -> string {
    pick_concrete<int>(5)
}

test "concrete_arm_rejects_foreign_realization" {
    assert.equal(pick_concrete_miss_fn(), "other")
}

function pick_concrete_hit_fn() -> string {
    pick_concrete<string>("hi")
}

test "concrete_arm_matches_own_realization" {
    assert.equal(pick_concrete_hit_fn(), "hi!")
}

// A union arm whose alternatives only partially overlap each scrutinee member
// must still match the full written union: `let p: T | string` over `T | bool`
// leaves the boolean arms reachable (they fire when `T` is not `bool`).
function pick_mixed_union<T>(x: T | bool) -> string {
    match (x) {
        let p: T | string => "p",
        true => "t",
        false => "f",
    }
}

function pick_mixed_union_bool_fn() -> string {
    pick_mixed_union<int>(true)
}

test "concrete_arms_after_open_union_arm_reachable" {
    assert.equal(pick_mixed_union_bool_fn(), "t")
}

function pick_mixed_union_open_fn() -> string {
    // T = bool: the value is a member of the `T` alternative, so the union
    // arm captures it first (set-membership semantics, like the dynamic
    // first-match capture in ns_generic_match_typevar).
    pick_mixed_union<bool>(true)
}

test "open_union_arm_dynamic_first_match_capture" {
    assert.equal(pick_mixed_union_open_fn(), "p")
}

// An arm matches and binds exactly its WRITTEN pattern type: alternatives
// overlapping only a rigid scrutinee member are not dropped from the runtime
// test. `let v: int | string` over `T | int` matches a `T = string` value.
function pick_written_union<T>(x: T | int) -> string {
    match (x) {
        let v: int | string => "int-or-string",
        _ => "other",
    }
}

function pick_written_union_rigid_hit_fn() -> string {
    pick_written_union<string>("hi")
}

test "written_union_arm_matches_rigid_member_realization" {
    assert.equal(pick_written_union_rigid_hit_fn(), "int-or-string")
}

function pick_written_union_concrete_hit_fn() -> string {
    pick_written_union<string>(5)
}

test "written_union_arm_matches_concrete_member" {
    assert.equal(pick_written_union_concrete_hit_fn(), "int-or-string")
}

function pick_written_union_miss_fn() -> string {
    pick_written_union<bool>(true)
}

test "written_union_arm_rejects_foreign_realization" {
    assert.equal(pick_written_union_miss_fn(), "other")
}

// ─── 4. bounded rigid arm, satisfiable bound (positive polarity) ──────────────
// The negative polarity — a bound that provably excludes every scrutinee
// member — is a compile error, pinned in
// projects/diagnostic_errors/generic_bound_patterns.

interface RigidMarked {
    function tag(self) -> string throws never
}

class RigidTagged {
    v: int
    implements RigidMarked {
        function tag(self) -> string {
            return "tagged"
        }
    }
}

function pick_bounded<T extends RigidMarked>(x: RigidTagged | string) -> string {
    match (x) {
        let t: T => "bounded",
        _ => "other",
    }
}

function pick_bounded_hit_fn() -> string {
    pick_bounded<RigidTagged>(RigidTagged { v: 1 })
}

test "bounded_rigid_arm_matches_implementing_member" {
    assert.equal(pick_bounded_hit_fn(), "bounded")
}

function pick_bounded_miss_fn() -> string {
    pick_bounded<RigidTagged>("hi")
}

test "bounded_rigid_arm_rejects_other_member" {
    assert.equal(pick_bounded_miss_fn(), "other")
}

// ─── 5. a refutable rigid arm ahead of a concrete cover ───────────────────────
// A rigid arm sits outside its column's alphabet, so it is specialized in a
// branch of its own purely to keep it reachable. That branch excludes the
// concrete covering arm (`RigidCell<T>` and `RigidCell<int>` are distinct
// invariant ctors), so when the rigid arm's own fields are refutable the
// branch looks non-exhaustive from the inside — but the values it describes
// are a subset of the concrete arm's, which covers them all. These matches are
// exhaustive and must compile without a trailing `_`.

class RigidCell<T> {
    v: T
}

function rigid_cell_split<T>(c: RigidCell<int>) -> string {
    match (c) {
        RigidCell<T> { v: 1 } => "rigid-one",
        RigidCell<int> { v: _ } => "concrete",
    }
}

function rigid_cell_hit_fn() -> string {
    rigid_cell_split<int>(RigidCell<int> { v: 1 })
}

test "refutable_rigid_arm_matches_at_realized_arg" {
    assert.equal(rigid_cell_hit_fn(), "rigid-one")
}

function rigid_cell_field_miss_fn() -> string {
    // Right instantiation, wrong field value: falls through to the cover.
    rigid_cell_split<int>(RigidCell<int> { v: 2 })
}

test "refutable_rigid_arm_field_test_still_applies" {
    assert.equal(rigid_cell_field_miss_fn(), "concrete")
}

function rigid_cell_arg_miss_fn() -> string {
    // `T = string`: invariant generics, so the rigid arm cannot match at all.
    rigid_cell_split<string>(RigidCell<int> { v: 1 })
}

test "refutable_rigid_arm_rejects_foreign_realization" {
    assert.equal(rigid_cell_arg_miss_fn(), "concrete")
}

// The same shape with the rigid row in a *list* column: a slice arm covers
// every length class, and the rigid `T[]` head — which no slice ctor covers —
// must not fabricate a missing case out of its own refutable sibling field.

class RigidRow {
    xs: string[]
    n: int
}

function rigid_row_split<T>(r: RigidRow) -> string {
    match (r) {
        RigidRow { xs: let t: T[], n: 1 } => "rigid-one",
        RigidRow { xs: [..], n: _ } => "slice",
    }
}

function rigid_row_hit_fn() -> string {
    rigid_row_split<string>(RigidRow { xs: ["a"], n: 1 })
}

test "refutable_rigid_list_arm_matches_at_realized_arg" {
    assert.equal(rigid_row_hit_fn(), "rigid-one")
}

function rigid_row_field_miss_fn() -> string {
    rigid_row_split<string>(RigidRow { xs: ["a"], n: 2 })
}

test "refutable_rigid_list_arm_field_test_still_applies" {
    assert.equal(rigid_row_field_miss_fn(), "slice")
}

function rigid_row_arg_miss_fn() -> string {
    rigid_row_split<int>(RigidRow { xs: ["a"], n: 1 })
}

test "refutable_rigid_list_arm_rejects_foreign_realization" {
    assert.equal(rigid_row_arg_miss_fn(), "slice")
}
