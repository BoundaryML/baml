// The right side of `&&` and `||` is checked with what the left side proved.
//
// `&&` and `||` short-circuit: the right side runs only after the left side
// was true (`&&`) or false (`||`). So `x is A && x.a == 1` reads `x.a` on an
// `A`, and `s == null || s.length() == 0` reads the length of a string.
//
// The rule narrows a bare local binding (a `let` or a parameter) that no
// closure captures. Such a binding belongs to this call: no other task and no
// alias can change it between the test and the read. A captured binding, a
// field path and an index path are not narrowed. The compile errors for them
// are in the diagnostics project `diagnostic_errors/short_circuit_narrowing`.
//
// Every case body is a top-level function; a `test` block only asserts.

class ScnA {
    a: int,
}

class ScnB {
    b: string,
}

class ScnBox {
    v: int | string,
}

class ScnHolder {
    v: ScnA | ScnB,
}

class ScnProbe {
    ran: string[],
}

function scn_a(n: int) -> ScnA | ScnB {
    ScnA { a: n }
}

function scn_b(text: string) -> ScnA | ScnB {
    ScnB { b: text }
}

function scn_ran(probe: ScnProbe, name: string, value: bool) -> bool {
    probe.ran.push(name);
    value
}

// --- The operators short-circuit: the narrowing is sound only for that. ---

function scn_and_skips_the_right_side() -> string {
    let probe = ScnProbe { ran: [] };
    let result = scn_ran(probe, "left", false) && scn_ran(probe, "right", true);
    `${result}:${probe.ran.join(",")}`
}

function scn_or_skips_the_right_side() -> string {
    let probe = ScnProbe { ran: [] };
    let result = scn_ran(probe, "left", true) || scn_ran(probe, "right", false);
    `${result}:${probe.ran.join(",")}`
}

function scn_and_runs_the_right_side() -> string {
    let probe = ScnProbe { ran: [] };
    let result = scn_ran(probe, "left", true) && scn_ran(probe, "right", true);
    `${result}:${probe.ran.join(",")}`
}

function scn_or_runs_the_right_side() -> string {
    let probe = ScnProbe { ran: [] };
    let result = scn_ran(probe, "left", false) || scn_ran(probe, "right", false);
    `${result}:${probe.ran.join(",")}`
}

function scn_chain_stops_at_the_first_decision() -> string {
    let probe = ScnProbe { ran: [] };
    let result = scn_ran(probe, "a", true) && scn_ran(probe, "b", false) && scn_ran(probe, "c", true)
        || scn_ran(probe, "d", true)
        || scn_ran(probe, "e", true);
    `${result}:${probe.ran.join(",")}`
}

// The right side would panic on a `ScnB`: it has no `a`. It does not run.
function scn_skipped_read(x: ScnA | ScnB) -> bool {
    x is ScnA && x.a == 1
}

// --- `&&`: the right side has the left side's true facts. ---

function scn_and_in_if(x: ScnA | ScnB) -> int {
    if (x is ScnA && x.a > 0) {
        x.a
    } else {
        -1
    }
}

function scn_null_guard(text: string?) -> bool {
    text != null && text.length() > 0
}

function scn_truthy_guard(text: string?) -> bool {
    text && text.starts_with("a")
}

// A function value is narrowed like every other value: the call is on the
// function type, not on the optional.
function scn_optional_hook(hook: ((int) -> bool throws never)?, n: int) -> bool {
    hook != null && hook(n)
}

function scn_is_positive(n: int) -> bool throws never {
    n > 0
}

function scn_chain(x: ScnA | ScnB, y: ScnA | ScnB) -> bool {
    x is ScnA && y is ScnA && x.a == y.a
}

function scn_right_nested(x: ScnA | ScnB | null) -> bool {
    x != null && (x is ScnA && x.a == 1)
}

// --- `||`: the right side has the left side's false facts. ---

function scn_or_null(text: string?) -> bool {
    text == null || text.length() == 0
}

function scn_not_or(x: ScnA | ScnB) -> bool {
    !(x is ScnA) || x.a == 1
}

function scn_or_then_and(x: ScnA | ScnB | null) -> bool {
    x == null || x is ScnB || x.a == 1
}

function scn_mixed(x: ScnA | ScnB | null) -> bool {
    x != null && (x is ScnB || x.a == 1)
}

function scn_de_morgan(x: ScnA | ScnB | null) -> bool {
    !(x == null || x is ScnB) && x.a == 1
}

// The early exit keeps the facts of the whole condition.
function scn_early_exit(x: ScnA | ScnB | null) -> int {
    if (x == null || x is ScnB) {
        return -1;
    }
    x.a
}

// --- An `&&` that is not a condition. ---

function scn_let_value(x: ScnA | ScnB) -> bool {
    let ok = x is ScnA && x.a == 1;
    ok
}

function scn_takes_bool(flag: bool) -> string {
    if (flag) {
        "yes"
    } else {
        "no"
    }
}

function scn_argument(x: ScnA | ScnB) -> string {
    scn_takes_bool(x is ScnA && x.a == 1)
}

function scn_local(n: int) -> bool {
    let x = if (n > 0) {
        scn_a(n)
    } else {
        scn_b("bee")
    };
    x is ScnA && x.a == n
}

// --- Loops, guards. ---

function scn_while(x: ScnA | ScnB, limit: int) -> int {
    let count = 0;
    while (x is ScnA && x.a > count && count < limit) {
        count += 1;
    }
    count
}

// The loop assigns the binding: each turn tests the new value first.
function scn_while_reassigns(n: int) -> int {
    let x = scn_a(n);
    let turns = 0;
    while (x is ScnA && x.a > 0) {
        turns += 1;
        x = if (x.a > 1) {
            scn_a(x.a - 1)
        } else {
            scn_b("done")
        };
    }
    turns
}

function scn_guard(x: ScnA | ScnB, y: ScnA | ScnB) -> int {
    match (y) {
        let other: ScnA if x is ScnA && x.a == other.a => 1,
        _ => 0,
    }
}

// --- An assignment inside the right side. ---

// Inside the right side the binding has the assigned type from the assignment
// on. After the operator nothing is known about it: the test must be repeated.
function scn_assign_inside_the_right_side(x: ScnA | ScnB) -> string {
    let was_one = x is ScnA && {
        let before = x.a;
        x = ScnB { b: "replaced" };
        before == 1 && x.b == "replaced"
    };
    let now = if (x is ScnB) {
        x.b
    } else {
        "an A"
    };
    `${was_one}:${now}`
}

// --- A type parameter. ---

// `v is T` tests the realized type at run time. On the right side `v` is a
// `T`, so it can go into a `T[]`, which holds its element type.
function scn_collect<T>(v: unknown, items: T[]) -> int {
    let added = v is T && {
        items.push(v);
        true
    };
    if (added) {
        items.length()
    } else {
        -items.length()
    }
}

function scn_collect_int(v: unknown) -> int {
    let items: int[] = [5];
    scn_collect<int>(v, items)
}

// A `defer` body runs when the scope ends, after the operator. It does not
// capture the binding as a closure does, so the binding is narrowed.
function scn_defer_assigns_later(x: ScnA | ScnB) -> bool {
    defer {
        x = ScnB { b: "replaced" };
    }
    x is ScnA && x.a == 1
}

// --- What does not narrow: the value is still correct. ---

// A field path is read again on the right, so it is not narrowed. Bind the
// field first, or use `if let` or `match`.
function scn_field_bound_first(holder: ScnHolder) -> bool {
    let v = holder.v;
    v is ScnA && v.a == 1
}

function scn_field_if_let(holder: ScnHolder) -> bool {
    if let a: ScnA = holder.v {
        a.a == 1
    } else {
        false
    }
}

function scn_field_match(holder: ScnHolder) -> bool {
    match (holder.v) {
        let a: ScnA => a.a == 1,
        _ => false,
    }
}

// --- Facts that make the rule sound. ---

// A list keeps its declared element type at run time.
function scn_list_keeps_its_element_type() -> string {
    let mixed: (int | string)[] = [1, 2];
    let seen: unknown = mixed;
    let ints: int[] = [1, 2];
    let seen_ints: unknown = ints;
    `${seen is int[]}:${seen_ints is int[]}`
}

// A class pattern with a field constraint tests the field content, and narrows
// the binding to the class only.
function scn_class_pattern(value: ScnBox | ScnA) -> string {
    if (value is ScnBox { v: int }) {
        match (value.v) {
            let n: int => `int ${n}`,
            let text: string => `string ${text}`,
        }
    } else {
        "no match"
    }
}

test "the right side of && does not run after a false left side" {
    assert.equal(scn_and_skips_the_right_side(), "false:left");
    assert.equal(scn_and_runs_the_right_side(), "true:left,right");
    assert.equal(scn_skipped_read(scn_b("bee")), false);
    assert.equal(scn_skipped_read(scn_a(1)), true);
}

test "the right side of || does not run after a true left side" {
    assert.equal(scn_or_skips_the_right_side(), "true:left");
    assert.equal(scn_or_runs_the_right_side(), "false:left,right");
}

test "a chain stops at the first operand that decides it" {
    assert.equal(scn_chain_stops_at_the_first_decision(), "true:a,b,d");
}

test "the right side of && reads the type that the left side tested" {
    assert.equal(scn_and_in_if(scn_a(4)), 4);
    assert.equal(scn_and_in_if(scn_a(0)), -1);
    assert.equal(scn_and_in_if(scn_b("bee")), -1);
}

test "a null guard lets the right side of && use the value" {
    assert.equal(scn_null_guard("text"), true);
    assert.equal(scn_null_guard(""), false);
    assert.equal(scn_null_guard(null), false);
    assert.equal(scn_truthy_guard("apple"), true);
    assert.equal(scn_truthy_guard(null), false);
    assert.equal(scn_optional_hook(scn_is_positive, 3), true);
    assert.equal(scn_optional_hook(scn_is_positive, -3), false);
    assert.equal(scn_optional_hook(null, 3), false);
}

test "each operand of an && chain has the facts of the operands before it" {
    assert.equal(scn_chain(scn_a(2), scn_a(2)), true);
    assert.equal(scn_chain(scn_a(2), scn_a(3)), false);
    assert.equal(scn_chain(scn_a(2), scn_b("bee")), false);
    assert.equal(scn_right_nested(scn_a(1)), true);
    assert.equal(scn_right_nested(null), false);
}

test "the right side of || has the facts of a false left side" {
    assert.equal(scn_or_null(null), true);
    assert.equal(scn_or_null(""), true);
    assert.equal(scn_or_null("text"), false);
    assert.equal(scn_not_or(scn_b("bee")), true);
    assert.equal(scn_not_or(scn_a(1)), true);
    assert.equal(scn_not_or(scn_a(2)), false);
}

test "mixed && and || with ! keep the facts of each side" {
    assert.equal(scn_or_then_and(null), true);
    assert.equal(scn_or_then_and(scn_b("bee")), true);
    assert.equal(scn_or_then_and(scn_a(1)), true);
    assert.equal(scn_or_then_and(scn_a(2)), false);
    assert.equal(scn_mixed(scn_a(1)), true);
    assert.equal(scn_mixed(scn_b("bee")), true);
    assert.equal(scn_mixed(null), false);
    assert.equal(scn_de_morgan(scn_a(1)), true);
    assert.equal(scn_de_morgan(scn_b("bee")), false);
    assert.equal(scn_de_morgan(null), false);
}

test "an early exit on || keeps the facts of the whole condition" {
    assert.equal(scn_early_exit(scn_a(9)), 9);
    assert.equal(scn_early_exit(scn_b("bee")), -1);
    assert.equal(scn_early_exit(null), -1);
}

test "an && that is not a condition narrows its right side" {
    assert.equal(scn_let_value(scn_a(1)), true);
    assert.equal(scn_let_value(scn_b("bee")), false);
    assert.equal(scn_argument(scn_a(1)), "yes");
    assert.equal(scn_argument(scn_a(2)), "no");
    assert.equal(scn_local(3), true);
    assert.equal(scn_local(0), false);
}

test "a loop condition narrows its right side on each turn" {
    assert.equal(scn_while(scn_a(3), 10), 3);
    assert.equal(scn_while(scn_a(3), 2), 2);
    assert.equal(scn_while(scn_b("bee"), 10), 0);
    assert.equal(scn_while_reassigns(3), 3);
}

test "a match guard narrows its right side" {
    assert.equal(scn_guard(scn_a(5), scn_a(5)), 1);
    assert.equal(scn_guard(scn_b("bee"), scn_a(5)), 0);
}

test "an assignment inside the right side replaces the fact" {
    assert.equal(scn_assign_inside_the_right_side(scn_a(1)), "true:replaced");
    assert.equal(scn_assign_inside_the_right_side(scn_a(2)), "false:replaced");
    assert.equal(scn_assign_inside_the_right_side(scn_b("bee")), "false:bee");
}

test "the right side of && has a value of the type parameter that the left side tested" {
    assert.equal(scn_collect_int(1), 2);
    assert.equal(scn_collect_int("text"), -1);
}

test "a defer that assigns the binding later does not stop the narrowing" {
    assert.equal(scn_defer_assigns_later(scn_a(1)), true);
    assert.equal(scn_defer_assigns_later(scn_b("bee")), false);
}

test "a field is narrowed through a binding, if let or match" {
    assert.equal(scn_field_bound_first(ScnHolder { v: ScnA { a: 1 } }), true);
    assert.equal(scn_field_bound_first(ScnHolder { v: ScnB { b: "bee" } }), false);
    assert.equal(scn_field_if_let(ScnHolder { v: ScnA { a: 1 } }), true);
    assert.equal(scn_field_match(ScnHolder { v: ScnB { b: "bee" } }), false);
}

test "a list keeps its declared element type at run time" {
    assert.equal(scn_list_keeps_its_element_type(), "false:true");
}

test "a class pattern with a field constraint narrows the binding to the class only" {
    assert.equal(scn_class_pattern(ScnBox { v: 3 }), "int 3");
    assert.equal(scn_class_pattern(ScnBox { v: "text" }), "no match");
    assert.equal(scn_class_pattern(ScnA { a: 1 }), "no match");
}
