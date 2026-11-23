function f() -> int {
    let v = foo.bar(aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa).baz(bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb).qux().0 catch (e) {
        _ => 1,
    };
    v
}
