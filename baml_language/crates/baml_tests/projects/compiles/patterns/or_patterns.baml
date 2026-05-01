class Cat {
    name string
    indoor bool
}

class Fish {
    name string
    freshwater bool
}

// Or-pattern with same binding in both arms
function test_or_same_bindings(x: Cat | Fish) -> string {
    match (x) {
        Cat { name } | Fish { name } => name
    }
}
