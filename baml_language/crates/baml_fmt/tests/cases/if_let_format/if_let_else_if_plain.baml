function f(r: int | string) -> string {
    if let v: int = r {
        "int"
    } else if r is string {
        "str"
    } else {
        "other"
    }
}
