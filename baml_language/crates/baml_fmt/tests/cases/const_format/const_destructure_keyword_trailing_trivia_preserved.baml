class Foo {
    a: int,
}

function f(foo: Foo) -> int {
    const /*keep*/ Foo { a } = foo;
    a
}
