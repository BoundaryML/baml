function f(r: int | string) -> int {
    let v: int = r /* a */ else /* b */ {
        return 0;
    };
    v
}
