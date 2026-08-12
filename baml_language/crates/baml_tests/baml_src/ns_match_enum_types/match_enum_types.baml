// End-to-end tests for enum-*type* discrimination in `match` and `is`.
//
// Before the enum-identity fix, a bare enum type test lowered to the single
// shared `ENUM` type tag (`type_tag_for_ty` / `realized_type_tag`), which cannot
// tell one enum from another. So `match (x: Color | Status)` routed a `Status`
// value to a `Color` arm — on the chain (coarse tag), and worse on the TypeTag
// switch (both arms deduped onto the one `ENUM` tag, dropping the second arm).
//
// The fix tests enum-pointer identity (`is Color` compares the value's enum
// object) so the chain discriminates `Color` from `Status`, and gates the switch
// (`switch_member_tag_sufficient`) so an enum arm bails to the precise chain when
// a second enum type shares the tag — while a lone enum type keeps the fast tag.
//
// (Enum *variant* matching — `Status.Active` — is unaffected: it dispatches on
// the enum discriminant, a separate precise path.)

enum Color { Red, Green }
enum Status { Active, Inactive }

// ─── 1. chain path: 2-arm enum-type discrimination ────────────────────────────

function classify(x: Color | Status) -> string {
    match (x) {
        let c: Color => "color",
        let s: Status => "status",
    }
}

function classify_color_fn() -> string { classify(Color.Red) }
function classify_status_fn() -> string { classify(Status.Active) }

test "match_enum_type_color" { assert.equal(classify_color_fn(), "color") }
test "match_enum_type_status" { assert.equal(classify_status_fn(), "status") }

// ─── 2. `is` enum-type test ───────────────────────────────────────────────────

function status_is_color_fn() -> bool {
    let x: Color | Status = Status.Active;
    x is Color
}
test "is_status_is_not_color" { assert.is_true(!status_is_color_fn()) }

// ─── 3. TypeTag switch path: enum collision bails to the precise chain ────────
// With enough tag-eligible arms the lowering prefers the TypeTag jump-table
// switch. `Color` and `Status` share the one `ENUM` tag, so the switch gate
// detects the collision and falls back to the enum-precise chain rather than
// deduping the second enum arm away.

function classify_switch(x: Color | Status | int | string | bool) -> string {
    match (x) {
        let c: Color => "color",
        let s: Status => "status",
        let n: int => "int",
        let st: string => "string",
        let b: bool => "bool",
    }
}

function classify_switch_status_fn() -> string { classify_switch(Status.Inactive) }
function classify_switch_int_fn() -> string { classify_switch(7) }

test "match_switch_enum_status" { assert.equal(classify_switch_status_fn(), "status") }
test "match_switch_enum_prims_still_dispatch" { assert.equal(classify_switch_int_fn(), "int") }

// ─── 4. single enum type keeps the fast tag (no regression) ───────────────────
// Only one enum type reaches this match, so the coarse `ENUM` tag has nothing to
// conflate with and the switch stays a jump table.

function classify_one_enum(x: Color | int | string | bool) -> string {
    match (x) {
        let c: Color => "color",
        let n: int => "int",
        let st: string => "string",
        let b: bool => "bool",
    }
}

function classify_one_enum_color_fn() -> string { classify_one_enum(Color.Green) }
test "match_single_enum_switch_color" { assert.equal(classify_one_enum_color_fn(), "color") }
