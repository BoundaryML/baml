// Type mismatch: lambda return type vs actual body type
function test_return_mismatch() -> int {
    let bad = (x: int) -> string { x * 2 }
    0
}

// Lambda param with no annotation and no expected type context
function test_no_annotation_no_context() -> int {
    let bad = (x) -> { x * 2 }
    0
}

// Arity mismatch: lambda has wrong number of params for expected type
function test_arity_mismatch() -> int[] {
    let items: int[] = [1, 2, 3]
    items.map((x, y) -> { x })
}

// Wrong param type annotation vs expected
function test_param_type_mismatch() -> int[] {
    let items: int[] = [1, 2, 3]
    items.map((x: string) -> { x })
}
