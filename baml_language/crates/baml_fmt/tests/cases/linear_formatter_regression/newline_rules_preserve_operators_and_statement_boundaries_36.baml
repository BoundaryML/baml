function example() -> int {
 consume(() -> int { if (true) { return 3; }
 -1 })
 -3
}
