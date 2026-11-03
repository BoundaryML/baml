// Type identities: a `never` slot empties the tuple, a product of unions is
// not the union of products, and a singleton is not its element.

// `(never, string)` has no values, so it is `never`, and a union drops it.
function tpy_never_collapses() -> bool {
    reflect.Type.of<(never, string)>() == reflect.Type.of<never>()
}

function tpy_never_arm_dropped() -> bool {
    reflect.Type.of<(never, string) | int>() == reflect.Type.of<int>()
}

// The dropped arm needs no handling: the parameter is just an `int`.
function tpy_never_arm_typechecks(x: (never, string) | int) -> int {
    x
}

test "tuple_never_slot" {
    assert.is_true(tpy_never_collapses())
    assert.is_true(tpy_never_arm_dropped())
    assert.equal(tpy_never_arm_typechecks(4), 4)
}

// A product of unions expands one way into a union of products for
// assignability, but reflection keeps the two types distinct.
function tpy_product_is_not_union() -> bool {
    reflect.Type.of<(int | string, bool)>() == reflect.Type.of<(int, bool) | (string, bool)>()
}

function tpy_nested_product_is_not_union() -> bool {
    reflect.Type.of<((int | string,), bool)>() == reflect.Type.of<((int,), bool) | ((string,), bool)>()
}

function tpy_singleton_is_not_element() -> bool {
    reflect.Type.of<(int,)>() == reflect.Type.of<int>()
}

function tpy_singleton_value_is_not_element() -> bool {
    reflect.Type.of_value((1,)) == reflect.Type.of<int>()
}

test "tuple_reflected_identity" {
    assert.equal(tpy_product_is_not_union(), false)
    assert.equal(tpy_nested_product_is_not_union(), false)
    assert.equal(tpy_singleton_is_not_element(), false)
    assert.equal(tpy_singleton_value_is_not_element(), false)
    assert.is_true(reflect.Type.of<(int,)>() == reflect.Type.of<(int,)>())
}
