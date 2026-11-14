function f() -> int {
    let a = spawn { let x = 1; x + 1 };
    await a
}
