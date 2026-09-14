// Bitwise ops type through a hardcoded table in BOTH engines until the
// stdlib grows baml.ops bitwise interfaces (hir_ty: ops.rs HACK note;
// TIR: infer_bitwise). Decision 3B routes them through dispatch_operator
// once the interfaces land.
function bits(a: int, b: int) -> int {
    let masked = a & b;
     // ^^^^^^ int
    let shifted = a << 2;
     // ^^^^^^^ int
    masked | shifted
 // ^^^^^^^^^^^^^^^^ int
}
