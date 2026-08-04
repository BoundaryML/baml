// pending: A7 byte-string literals (S15 ledger, 87 nodes)
//
// `b"..."` is a `uint8array` literal - a missing literal-kind arm.
function f() -> int throws never {
    let x = b"hi";
     // ^ uint8array
    0
}
