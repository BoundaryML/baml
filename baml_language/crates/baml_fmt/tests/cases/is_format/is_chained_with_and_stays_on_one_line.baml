function both(a: int | string, b: int | string) -> bool {
    a is int && b is int
}
