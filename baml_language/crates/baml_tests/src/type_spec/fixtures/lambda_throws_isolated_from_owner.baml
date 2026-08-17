// S12: a lambda's throw sites belong to the LAMBDA's effect channel -
// defining a throwing lambda throws nothing; the owner's effect stays
// never until something calls it.
function g() -> int {
      // ^ throws never
    let f = (x: int) -> int { if (x < 0) { throw "neg"; } x };
    1
}
