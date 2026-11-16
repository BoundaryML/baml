function test_throws() -> int {
    let risky = (x: int) -> int throws string { x }
    risky(42)
}
