// Or-pattern where alternatives bind different variables
function test_or_mismatched_bindings(x: Cat | Fish) -> string {
    match (x) {
        Cat { name } | Fish { freshwater } => "matched"
    }
}

// Or-pattern where one side binds and the other doesn't
function test_or_missing_binding(x: Cat | Fish) -> string {
    match (x) {
        Cat { name } | Fish => "matched"
    }
}

// Bare type before class destructure in or-pattern
function test_or_type_then_destructure(x: int | Cat) -> string {
    match (x) {
        int | Cat { name } => name
    }
}

// Same binding name but different types across alternatives
class IntBox {
    val int
}

class StrBox {
    val string
}

function test_or_same_name_different_type(x: IntBox | StrBox) -> string {
    match (x) {
        IntBox { val } | StrBox { val } => "matched"
    }
}

// Or-pattern with rebind (valid — both n are string)
function test_or_rebind(x: Cat | Fish) -> string {
    match (x) {
        Cat { name: let n } | Fish { name: let n } => n
    }
}

// Nested destructure in or-pattern (valid — both name are string)
class CatBox {
    pet Cat
}

class FishBox {
    pet Fish
}

function test_or_nested_destructure(x: CatBox | FishBox) -> string {
    match (x) {
        CatBox { pet: Cat { name } } | FishBox { pet: Fish { name } } => name
    }
}

// --- Edge cases that should error ---

// 1. Non-existent field
function test_nonexistent_field(x: Cat) -> string {
    match (x) {
        Cat { bogus } => bogus
    }
}

// 2. Duplicate fields
function test_duplicate_field(x: Cat) -> string {
    match (x) {
        Cat { name, name } => name
    }
}

// 3. Destructure on a primitive
function test_destructure_primitive(x: int) -> int {
    match (x) {
        int { val } => val
    }
}

// 4. Wildcard in field position (field: _)
function test_wildcard_field(x: Cat) -> string {
    match (x) {
        Cat { name: _ } => "matched"
    }
}
