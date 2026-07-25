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
// E0108
//
//   × could not parse type expression for return type of `bar`
//     ╭─[for_loops.baml:11:19]
//  11 │ function bar() -> () {
//     ·                   ─┬
//     ·                    ╰── unparseable type
//     ╰────
// E0108
//
//   × could not parse type expression for return type of `baz`
//     ╭─[for_loops.baml:17:19]
//  17 │ function baz() -> () {
//     ·                   ─┬
//     ·                    ╰── unparseable type
//     ╰────
// E0108
//
//   × could not parse type expression for return type of `qux`
//     ╭─[for_loops.baml:23:19]
//  23 │ function qux() -> () {
//     ·                   ─┬
//     ·                    ╰── unparseable type
//     ╰────
//
//- inlay_hints
// for_loops.baml:12:15 (Type) ": int"
