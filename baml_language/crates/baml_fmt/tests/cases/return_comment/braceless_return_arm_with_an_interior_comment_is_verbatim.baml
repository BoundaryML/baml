function f(x: int) -> string {
    match (x) {
        0 => "zero",
        _ => return x /* why */ .to_string(),
    }
}
