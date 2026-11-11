function f(r: int | string) -> int {
    let v: int = r else {
        return 0;
    };
    v
}
