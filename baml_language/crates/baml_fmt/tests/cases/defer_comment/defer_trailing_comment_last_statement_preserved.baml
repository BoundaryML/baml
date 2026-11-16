function f() -> int {
    defer { cleanup() } // bye
    42
}
