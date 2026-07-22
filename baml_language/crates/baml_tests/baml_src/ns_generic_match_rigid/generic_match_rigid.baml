// End-to-end tests for rigid-typevar patterns whose type is NOT statically a
// member of the scrutinee type. Such a pattern is *potentially* unifiable with
// a scrutinee member — it matches exactly when the enclosing frame realizes the
// variable accordingly (`Box<T>` matches a `Box<int>` value iff `T = int`).
// Before the overlap-oracle narrowing fix, TIR intersected these pairs to
// `never` and MIR emitted a constant-false test — the arm silently never
// matched regardless of the realized type.
//
// BUG(static-coverage): as in ns_generic_match_typevar, the coverage analysis
// treats a rigid arm as covering the whole scrutinee, so the `if true` guards
// below exist solely to keep the later arms statically reachable; remove them
// when coverage models rigid arms as possible-but-not-covering.

// ─── 1. class-generic pattern in a method body (match + `is`) ─────────────────

class RigidBox<T> {
    v: T

    // `Box<T>` with the class's own `T`, against a scrutinee that mentions
    // only concrete types.
    function match_own(self, other: RigidBox<int> | int) -> string {
        match (other) {
            let b: RigidBox<T> if true => "box",
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
        let t: T if true => "T",
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

// ─── 3. bounded rigid arm, satisfiable bound (positive polarity) ──────────────
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
        let t: T if true => "bounded",
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
