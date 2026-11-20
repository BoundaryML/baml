// fmt: line_width=40
function example() -> int {
    consume(() -> int { if (true) { return 3; }
    -/*C5*/1 })
    -3
}
