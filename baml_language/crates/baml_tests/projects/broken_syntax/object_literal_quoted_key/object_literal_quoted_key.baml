// A field of a constructed object is named by an identifier; a quoted key
// is reported at the key, and the constructor is not left behind as a bare
// path followed by a map.
class Foo {
    a: int,
    b: int,
}
function f() -> Foo {
    let s = Foo { "a": 1, b: 2 };
    s
}
