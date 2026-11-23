// Both `;` of a C-style `for` header are required; each missing one is
// reported where it should stand.
function f() -> int {
    for (let i = 0 i < 3; i += 1) {}
    for (let i = 0; i < 3 i += 1) {}
    0
}
