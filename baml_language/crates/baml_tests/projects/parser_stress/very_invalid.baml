// Invalid BAML
// Invariants:
//   - Doesn't crash the parser
//   - Doesn't break the StringTorture class above
function BodyTorture() -> int {
  let x = ¿;
  
  while \ {
      x += 1;
  }
}