// Membership in a recursive alias is decided structurally: `Tree` denotes the
// equirecursive set `int | Tree[]`, so a type test against it unfolds the alias
// (the pooled alias object the head binds to) instead of looking for a class
// named `Tree`. A non-recursive alias never reaches a type test as a head — it
// was expanded at lowering — so the recursive case is the only one that exercises
// this road; it used to compile to constant false.
//
// Arrays are invariant, so a list is a `Tree[]` only when it was CREATED with
// element type `int | Tree[]`: an unannotated `[1, [2, 3]]` has element type
// `int | int[]`, which is not that set, and is correctly rejected.

type Tree = int | Tree[]

function is_tree(v: unknown) -> bool { v is Tree }

test "int_is_a_tree" { assert.is_true(is_tree(1)) }

test "list_created_as_tree_is_a_tree" {
    let t: Tree = [1, [2, 3]];
    assert.is_true(is_tree(t))
}

test "string_is_not_a_tree" { assert.is_true(!is_tree("x")) }

test "list_with_a_different_element_type_is_not_a_tree" {
    assert.is_true(!is_tree([1, [2, 3]]))
}
