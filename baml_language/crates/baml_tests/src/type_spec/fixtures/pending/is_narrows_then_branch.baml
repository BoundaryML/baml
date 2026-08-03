// pending: S10b flow narrowing - `is` conditions refine the tested local
function f(v: int | string) -> string {
    if (v is string) { v } else { "x" }
                    // ^ string
}
