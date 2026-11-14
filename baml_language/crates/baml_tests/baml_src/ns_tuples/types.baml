
function tpy_never_collapses() -> bool {
    reflect.Type.of<(never, string)>() == reflect.Type.of<never>()
}

function tpy_never_arm_dropped() -> bool {
    reflect.Type.of<(never, string) | int>() == reflect.Type.of<int>()
}

function tpy_never_arm_typechecks(x: (never, string) | int) -> int {
    x
}

test "tuple_never_slot" {
    assert.is_true(tpy_never_collapses())
    assert.is_true(tpy_never_arm_dropped())
    assert.equal(tpy_never_arm_typechecks(4), 4)
}

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

function tpy_distinct_arms_kept() -> bool {
    reflect.Type.of<(int,) | (int?,)>() == reflect.Type.of<(int?,)>()
}

function tpy_distinct_arms_kept_other_way() -> bool {
    reflect.Type.of<(int,) | (int?,)>() == reflect.Type.of<(int,)>()
}

function tpy_equivalent_spellings_dedupe() -> bool {
    reflect.Type.of<(int,) | (5 | int,)>() == reflect.Type.of<(int,)>()
}

test "tuple_reflected_identity" {
    assert.equal(tpy_distinct_arms_kept(), false)
    assert.equal(tpy_distinct_arms_kept_other_way(), false)
    assert.is_true(tpy_equivalent_spellings_dedupe())
    assert.equal(tpy_product_is_not_union(), false)
    assert.equal(tpy_nested_product_is_not_union(), false)
    assert.equal(tpy_singleton_is_not_element(), false)
    assert.equal(tpy_singleton_value_is_not_element(), false)
    assert.is_true(reflect.Type.of<(int,)>() == reflect.Type.of<(int,)>())
}
