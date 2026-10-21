// A7 (S15 ledger): `b"..."` is a `uint8array` value - its own expr
// kind, not a `Literal` (no literal type per byte-string).
function f() -> int throws never {
    let x = b"hi";
     // ^ uint8array
    0
}
