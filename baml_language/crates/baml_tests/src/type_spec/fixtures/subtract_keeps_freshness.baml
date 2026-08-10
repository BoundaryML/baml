// flow subtraction filters members without rewriting them
function skf(c: bool) -> int throws never {
    let x: int | null = 0
    x = if c { 7 } else { null }
    if x == null {
        return 0
    }
    let y = x
     // ^ int
    y
}
