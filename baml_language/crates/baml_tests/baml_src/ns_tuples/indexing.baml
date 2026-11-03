// Tuple bracket access with static or integer-literal-typed indices, union
// receivers of different arities, optional receivers, postfix `?` on a
// nullable element, and the one-way expansion of a product of unions into a
// union of products.

// ─── Static and literal-typed indices ────────────────────────────────────────

function tpi_literal_typed(t: (int, string, bool), i: 1) -> string {
    t[i]
}

function tpi_folded(t: (int, string, bool), i: 1) -> bool {
    t[i + 1]
}

function tpi_constant_expr(t: (int, string, bool)) -> int {
    t[(2 - 2)]
}

function tpi_negated_zero(t: (int, string, bool)) -> int {
    t[-0]
}

// The index is an ordinary operand: a variable whose type is a literal.
function tpi_literal_variable(t: (int, string, bool)) -> string {
    let i: 1 = 1;
    t[i]
}

test "tuple_static_and_literal_typed_indices" {
    assert.equal(tpi_negated_zero((1, "a", true)), 1)
    assert.equal(tpi_literal_variable((1, "a", true)), "a")
    let t = (1, "a", true);
    assert.equal(tpi_literal_typed(t, 1), "a")
    assert.is_true(tpi_folded(t, 1))
    assert.equal(tpi_constant_expr(t), 1)
}

// ─── Union receivers ─────────────────────────────────────────────────────────

// Every variant has slots 0 and 1; the arities differ.
function tpi_common_prefix(t: (int, bool) | (string, bool, int)) -> int | string {
    t[0]
}

function tpi_common_second(t: (int, bool) | (string, bool, int)) -> bool {
    t[1]
}

test "tuple_union_common_prefix" {
    assert.equal(tpi_common_prefix((1, true)), 1)
    assert.equal(tpi_common_prefix(("s", false, 3)), "s")
    assert.is_true(tpi_common_second((1, true)))
    assert.equal(tpi_common_second(("s", false, 3)), false)
}

// ─── Optional receivers and nullable elements ────────────────────────────────

function tpi_optional_receiver(t: (int, string) | (string, string) | null) -> string? {
    t?.[1]
}

class TpiName {
    name: string,
}

// A nullable ELEMENT in a non-optional tuple: postfix `?` on the element.
function tpi_nullable_element(t: (TpiName?, int)) -> string? {
    t[0]?.name
}

// A nullable tuple element is itself an optional receiver.
function tpi_nullable_tuple_element(t: ((int,)?, int)) -> int? {
    t[0]?.[0]
}

// A slot read through a literal-typed index is the base of a field store.
function tpi_assign_through_literal_index() -> string {
    let t = (TpiName { name: "a" }, 1);
    let i: 0 = 0;
    t[i].name = "b";
    t[i].name
}

class TpiCounter {
    n: int,
}

function tpi_counted(counter: TpiCounter) -> (int, string)? {
    counter.n = counter.n + 1;
    (counter.n, "x")
}

test "tuple_optional_access" {
    assert.equal(tpi_assign_through_literal_index(), "b")
    // `?.[` evaluates its receiver once.
    let counter = TpiCounter { n: 0 };
    assert.equal(tpi_counted(counter)?.[0], 1)
    assert.equal(counter.n, 1)
    assert.equal(tpi_optional_receiver((1, "a")), "a")
    assert.equal(tpi_optional_receiver(("x", "b")), "b")
    assert.equal(tpi_optional_receiver(null), null)
    assert.equal(tpi_nullable_element((TpiName { name: "n" }, 1)), "n")
    assert.equal(tpi_nullable_element((null, 1)), null)
    assert.equal(tpi_nullable_tuple_element(((7,), 1)), 7)
    assert.equal(tpi_nullable_tuple_element((null, 1)), null)
}

// ─── One-way expansion ───────────────────────────────────────────────────────

// `(int | string, bool)` is assignable to `(int, bool) | (string, bool)`.
function tpi_expand(t: (int | string, bool)) -> (int, bool) | (string, bool) {
    t
}

// The reverse holds by covariance.
function tpi_collapse(t: (int, bool) | (string, bool)) -> (int | string, bool) {
    t
}

test "tuple_product_expands_into_union" {
    assert.equal(tpi_expand((1, true))[0], 1)
    assert.equal(tpi_collapse(("s", false))[0], "s")
}

// ─── `const` indices ─────────────────────────────────────────────────────────

// An unannotated `const` initialized to an integer literal has a statically
// known value, so it names a slot; a `let` widens to `int` and does not.
function tpi_const_local(t: (int, string, bool)) -> string {
    const i = 1;
    t[i]
}

// The binding itself is still an `int`: it widens wherever it flows.
function tpi_const_local_widens(t: (int, string, bool)) -> int {
    const i = 1;
    let j = i;
    j = 7;
    j + i
}

// Reassigning the `const` (which BAML does not prevent yet) re-narrows it to
// the new value, like any assignment.
function tpi_const_reassigned(t: (int, string, bool)) -> bool {
    const i = 1;
    i = 2;
    t[i]
}

// A global initialized to an integer literal, or a fold of one, is never
// reassigned, so it names a slot too.
const TPI_SECOND = 1;
const TPI_LAST = 3 - 1;

function tpi_const_global(t: (int, string, bool)) -> string {
    t[TPI_SECOND]
}

function tpi_const_global_folded(t: (int, string, bool)) -> bool {
    t[TPI_LAST]
}

test "tuple_const_indices" {
    let t = (1, "a", true);
    assert.equal(tpi_const_local(t), "a")
    assert.equal(tpi_const_local_widens(t), 8)
    assert.is_true(tpi_const_reassigned(t))
    assert.equal(tpi_const_global(t), "a")
    assert.is_true(tpi_const_global_folded(t))
}
