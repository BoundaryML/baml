// fmt: line_width=60
function f() -> int {
    for (let very_long = 0; very_long < 1000 && flag; very_long += 1) {
        g();
    }
    0
}
