// Builtin type names resolve in a LAYER of the resolution chain (the builtin
// scope), not in an intercept ahead of it, and `ShadowMode::PreferBuiltin`
// decides scope-versus-builtin precedence once. So a user declaration may
// spell a builtin name: the spelling keeps denoting the builtin in EVERY
// position - annotation and static qualifier alike - and the declaration stays
// reachable through `root.`.
//
// Regression for B-849. `class int` used to steal `int.max_value()` from the
// primitive (E0003 `unresolved name: max_value`) while `x: int` still meant
// the primitive, because the two positions consulted different mechanisms in
// different orders.
//
// These declarations sit in the package ROOT namespace deliberately: that is
// the scope a bare builtin name competes in. A namespaced file cannot see root
// items unqualified, so no other test file is affected.

class int {
    shadowed bool
}

class string {
    shadowed int
}

// Static-qualifier position: the primitive's companion statics still resolve.
function shadowed_int_static() -> bool {
    int.max_value() > 0
}

function shadowed_string_static() -> string {
    string.from(7)
}

// Annotation position: `string` is the primitive.
function shadowed_string_annotation(s: string) -> int {
    s.length()
}

// The shadowed declarations stay reachable when explicitly qualified.
function shadowing_declarations_are_reachable() -> int {
    let i = root.int { shadowed: true }
    let s = root.string { shadowed: 5 }
    if i.shadowed { s.shadowed } else { 0 }
}

test "builtin_statics_survive_a_shadowing_declaration" {
    assert.equal(shadowed_int_static(), true)
    assert.equal(shadowed_string_static(), "7")
}

test "annotation_position_denotes_the_builtin" {
    assert.equal(shadowed_string_annotation("hi"), 2)
}

test "shadowing_declarations_stay_reachable_qualified" {
    assert.equal(shadowing_declarations_are_reachable(), 5)
}
