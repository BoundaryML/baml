enum Color {
    R
    G
    B
}

function foo() -> int {
    1 + 1
}

function bar() -> () {
    for (let i = 0; i < 10; i += 1) {
        foo();
    }
}

function baz() -> () {
    for (let i in [1, 2, 3]) {
        foo();
    }
}

function qux() -> () {
    for (let i in [[Color.R, Color.G, Color.B]]) {
        bar();
    }
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- inlay_hints
// for_loops.baml:12:15 (Type) ": int"
//   edit@12:15 ": int"
// for_loops.baml:18:15 (Type) ": int"
// for_loops.baml:24:15 (Type) ": Color[]"
//   target "Color" -> for_loops.baml:1:6
