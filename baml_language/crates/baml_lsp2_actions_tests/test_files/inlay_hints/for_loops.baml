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
// <no-inlay-hints>
