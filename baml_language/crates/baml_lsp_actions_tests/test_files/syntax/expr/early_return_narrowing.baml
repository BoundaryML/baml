// Test 1: null check with early return narrows to non-null
function test_null_early_return(x: string?) -> string {
    if (x == null) {
        return "default";
    }
    // x should be narrowed to string here
    return x;
}

// Test 2: not-null check with early return
function test_not_null_early_return(x: string?) -> string? {
    if (x != null) {
        return x;
    }
    // x should be narrowed to null here
    return null;
}

// Test 3: instanceof with early return
class NarrowFoo {
    type "narrow_foo"
    field string
}

class NarrowBar {
    type "narrow_bar"
    other string
}

function test_instanceof_early_return(x: NarrowFoo | NarrowBar) -> string {
    if (x instanceof NarrowBar) {
        return "bar";
    }
    // x should be narrowed to NarrowFoo here
    return x.field;
}

// Test 4: null == x (reversed operand order)
function test_reversed_null_check(x: string?) -> string {
    if (null == x) {
        return "default";
    }
    // x should be narrowed to string here
    return x;
}

// Test 5: negated truthiness with early return
function test_negated_truthiness_early_return(x: string?) -> string {
    if (!x) {
        return "default";
    }
    // x should be narrowed to string here
    return x;
}

// Test 6: chained early returns
function test_chained(x: NarrowFoo | NarrowBar | null) -> string {
    if (x == null) {
        return "null";
    }
    // x is NarrowFoo | NarrowBar here
    if (x instanceof NarrowBar) {
        return "bar";
    }
    // x is NarrowFoo here
    return x.field;
}


function test_via_type_field(x: NarrowFoo | NarrowBar) -> string {
  if (x.type == "narrow_foo") {
    return x.field;
  }
  x.other
}

function test_simple_truthiness(x: int?) -> string {
  if (x) {
    return "x is not null";
  }
  "x was null"
}

function test_diverging_if_with_plain_else(x: int?) -> int {
  if (x == null) {
    return 0;
  } else {
  };
  x + 1
}

//----
//- diagnostics
// Error: `instanceof` is no longer supported. Use a `match` expression for type checking instead.
//     ╭─[ expr_early_return_narrowing.baml:62:9 ]
//     │
//  62 │     if (x instanceof NarrowBar) {
//     │         ───────────┬──────────  
//     │                    ╰──────────── `instanceof` is no longer supported. Use a `match` expression for type checking instead.
//     │ 
//     │ Note: Error code: E0098
// ────╯
// Error: Type `NarrowFoo | NarrowBar` has no field `field`
//     ╭─[ expr_early_return_narrowing.baml:66:12 ]
//     │
//  66 │     return x.field;
//     │            ───┬───  
//     │               ╰───── Type `NarrowFoo | NarrowBar` has no field `field`
//     │ 
//     │ Note: Error code: E0007
// ────╯
// Error: `instanceof` is no longer supported. Use a `match` expression for type checking instead.
//     ╭─[ expr_early_return_narrowing.baml:31:9 ]
//     │
//  31 │     if (x instanceof NarrowBar) {
//     │         ───────────┬──────────  
//     │                    ╰──────────── `instanceof` is no longer supported. Use a `match` expression for type checking instead.
//     │ 
//     │ Note: Error code: E0098
// ────╯
// Error: Type `NarrowFoo | NarrowBar` has no field `field`
//     ╭─[ expr_early_return_narrowing.baml:35:12 ]
//     │
//  35 │     return x.field;
//     │            ───┬───  
//     │               ╰───── Type `NarrowFoo | NarrowBar` has no field `field`
//     │ 
//     │ Note: Error code: E0007
// ────╯
