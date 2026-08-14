// Property shorthand (`{ key }`) versus a written entry whose quoted key text
// happens to equal the value identifier (`{ "key": key }`), across every
// binding form.
//
// Two invariants, both regressions of the hir_ty inference walk:
//   * shorthand-ness is the PARSER's fact (the source map's shorthand marker),
//     never re-derived from key text — `{ "key": key }` is NOT shorthand;
//   * a shorthand value is an ordinary path expression, so it is in scope
//     exactly when a plain use of the same name is — through every binder
//     (`if let`, match arms, `for`, `catch`, class destructures) and at every
//     block-nesting depth, not just top-level `let`s and parameters.
//
// Helpers are prefixed `psb_` and classes `Psb*` to stay unique in the shared
// corpus namespace. Everything lives in a top-level function; test blocks only
// assert.

class PsbBox {
    key string
}

function psb_lookup(m: map<string, string>) -> string {
    m.get("key") ?? "MISSING"
}

// ─── quoted `{ "key": key }` under every binder ──────────────────────────────

// The original repro: quoted key, `if let` binder of the same name.
function psb_quoted_if_let(v: string?) -> string {
    if let key: string = v {
        let params: map<string, string> = { "key": key };
        psb_lookup(params)
    } else {
        "NONE"
    }
}

test "quoted_key_sees_if_let_binder" {
    assert.equal(psb_quoted_if_let("a"), "a");
    assert.equal(psb_quoted_if_let(null), "NONE")
}

function psb_quoted_match_arm(v: string | int) -> string {
    match (v) {
        let key: string => {
            let params: map<string, string> = { "key": key };
            psb_lookup(params)
        },
        _ => "NONE",
    }
}

test "quoted_key_sees_match_arm_binder" {
    assert.equal(psb_quoted_match_arm("b"), "b");
    assert.equal(psb_quoted_match_arm(1), "NONE")
}

function psb_quoted_for_binder(xs: string[]) -> string {
    let out = "";
    for (let key in xs) {
        let params: map<string, string> = { "key": key };
        out += psb_lookup(params);
    }
    out
}

test "quoted_key_sees_for_binder" {
    assert.equal(psb_quoted_for_binder(["x", "y"]), "xy")
}

function psb_quoted_catch_binder(s: string) -> string {
    baml.Int.parse(s).to_string() catch (key) {
        baml.errors.ParseError => {
            // The quoted entry's VALUE is the catch binder; the key text is a
            // coincidence with the binder name, not a shorthand marker.
            let params: map<string, baml.errors.ParseError> = { "key": key };
            (params.get("key") != null).to_string()
        }
    }
}

test "quoted_key_sees_catch_binder" {
    assert.equal(psb_quoted_catch_binder("12"), "12");
    assert.equal(psb_quoted_catch_binder("nope"), "true")
}

function psb_quoted_destructure(b: PsbBox) -> string {
    match (b) {
        PsbBox { key: let key } => {
            let params: map<string, string> = { "key": key };
            psb_lookup(params)
        },
    }
}

test "quoted_key_sees_class_destructure_binder" {
    assert.equal(psb_quoted_destructure(PsbBox { key: "d" }), "d")
}

// A plain `let` nested inside a block — the body's own scope is not the
// expression's scope, so this exercises the same lookup path.
function psb_quoted_nested_block_let() -> string {
    if (true) {
        let key = "n";
        let params: map<string, string> = { "key": key };
        psb_lookup(params)
    } else {
        "NONE"
    }
}

test "quoted_key_sees_nested_block_let" {
    assert.equal(psb_quoted_nested_block_let(), "n")
}

// ─── genuine `{ key }` shorthand under every binder ──────────────────────────

function psb_shorthand_if_let(v: string?) -> string {
    if let key: string = v {
        let params: map<string, string> = { key };
        psb_lookup(params)
    } else {
        "NONE"
    }
}

test "shorthand_sees_if_let_binder" {
    assert.equal(psb_shorthand_if_let("a"), "a");
    assert.equal(psb_shorthand_if_let(null), "NONE")
}

function psb_shorthand_match_arm(v: string | int) -> string {
    match (v) {
        let key: string => {
            let params: map<string, string> = { key };
            psb_lookup(params)
        },
        _ => "NONE",
    }
}

test "shorthand_sees_match_arm_binder" {
    assert.equal(psb_shorthand_match_arm("b"), "b");
    assert.equal(psb_shorthand_match_arm(2), "NONE")
}

function psb_shorthand_for_binder(xs: string[]) -> string {
    let out = "";
    for (let key in xs) {
        let params: map<string, string> = { key };
        out += psb_lookup(params);
    }
    out
}

test "shorthand_sees_for_binder" {
    assert.equal(psb_shorthand_for_binder(["p", "q"]), "pq")
}

function psb_shorthand_catch_binder(s: string) -> string {
    baml.Int.parse(s).to_string() catch (key) {
        baml.errors.ParseError => {
            let params: map<string, baml.errors.ParseError> = { key };
            (params.get("key") != null).to_string()
        }
    }
}

test "shorthand_sees_catch_binder" {
    assert.equal(psb_shorthand_catch_binder("7"), "7");
    assert.equal(psb_shorthand_catch_binder("nope"), "true")
}

function psb_shorthand_destructure(b: PsbBox) -> string {
    match (b) {
        PsbBox { key: let key } => {
            let params: map<string, string> = { key };
            psb_lookup(params)
        },
    }
}

test "shorthand_sees_class_destructure_binder" {
    assert.equal(psb_shorthand_destructure(PsbBox { key: "d" }), "d")
}

function psb_shorthand_nested_block_let() -> string {
    if (true) {
        let key = "n";
        let params: map<string, string> = { key };
        psb_lookup(params)
    } else {
        "NONE"
    }
}

test "shorthand_sees_nested_block_let" {
    assert.equal(psb_shorthand_nested_block_let(), "n")
}

// Shorthand still resolves through the forms that already worked: a parameter
// and a body-level `let`.
function psb_shorthand_param(key: string) -> string {
    let params: map<string, string> = { key };
    psb_lookup(params)
}

function psb_shorthand_body_let() -> string {
    let key = "l";
    let params: map<string, string> = { key };
    psb_lookup(params)
}

test "shorthand_sees_param_and_body_let" {
    assert.equal(psb_shorthand_param("p"), "p");
    assert.equal(psb_shorthand_body_let(), "l")
}

// A shorthand whose name is bound by a closure parameter, inside a lambda body
// (expressions there key under the LAMBDA's scope).
function psb_shorthand_closure_param(xs: string[]) -> string {
    xs.map((key) -> {
        let params: map<string, string> = { key };
        psb_lookup(params)
    }).join("")
}

test "shorthand_sees_closure_param" {
    assert.equal(psb_shorthand_closure_param(["m", "n"]), "mn")
}

// ─── the binder is genuinely used, not the key text ──────────────────────────

// Shadowing: the innermost binder wins, exactly as a plain path use would.
function psb_shorthand_uses_innermost_binder(v: string?) -> string {
    let key = "outer";
    if let key: string = v {
        let params: map<string, string> = { key };
        psb_lookup(params)
    } else {
        let params: map<string, string> = { key };
        psb_lookup(params)
    }
}

test "shorthand_uses_innermost_binder" {
    assert.equal(psb_shorthand_uses_innermost_binder("inner"), "inner");
    assert.equal(psb_shorthand_uses_innermost_binder(null), "outer")
}

// The quoted form is a normal entry: the key is the literal text and the value
// is the binder, so a key that does NOT match the binder name still works.
function psb_quoted_key_differs_from_binder(v: string?) -> string {
    if let key: string = v {
        let params: map<string, string> = { "other": key };
        params.get("other") ?? "MISSING"
    } else {
        "NONE"
    }
}

test "quoted_key_differs_from_binder" {
    assert.equal(psb_quoted_key_differs_from_binder("z"), "z")
}
