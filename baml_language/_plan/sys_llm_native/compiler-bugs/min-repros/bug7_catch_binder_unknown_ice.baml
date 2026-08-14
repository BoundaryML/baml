// BUG 7: `.to_string()` (checker sugar, no real callee) charged an
// UNACCOUNTED throw fact, silently typing the catch binder `Io | Unknown`;
// bytecode lowering then ICE'd:
//   internal error: `Unknown` is not a valid `RuntimeTy`
// Pre-fix: the PANIC at check/lowering time is the repro (no diagnostic first).
function may_throw() -> string throws baml.errors.Io {
    throw baml.errors.Io { message: "boom" }
}

function bug7() -> string {
    may_throw().to_string() catch_all (e) {
        _ => "caught",
    }
}

test "bug7_no_ice_and_catch_works" {
    assert.equal(bug7(), "caught")
}
