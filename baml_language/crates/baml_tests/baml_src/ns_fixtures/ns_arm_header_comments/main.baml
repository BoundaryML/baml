// Regression: `//#` header comments between match/catch arms.
//
// Header comments are not comment-trivia (they are graph anchors with their
// own production). The arm loops previously had no production for them: in
// match arms they produced a cascade of parse errors, and in catch arms the
// recovery path consumed zero tokens and the parser looped forever (28GB+).
//
// This project must parse and compile with ZERO diagnostics.

function _thrower() -> int throws baml.errors.ParseError {
  int.parse("nope")
}

function catch_with_headers() -> int throws never {
  _thrower() catch (e) {
    //# headers before the first catch arm are legal
    let p: baml.errors.ParseError => 1,
    //## and between arms, at any level
    _ => 0,
  }
}

function match_with_headers(x: int) -> int throws never {
  match (x) {
    //# headers before the first match arm are legal
    0 => 1,
    //## and between arms
    _ => 2,
  }
}
