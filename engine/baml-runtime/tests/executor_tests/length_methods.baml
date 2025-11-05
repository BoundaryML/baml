// Test both and length() methods

function ArrayLength() -> int {
  let arr: int[] = [10, 20, 30];
  arr.length()
}

//> ArrayLength()
//
// 3

function StringLength() -> int {
  let s = "hello";
  s.length()
}

//> StringLength()
//
// 5

function MapLength() -> int {
  let m = {"a": 1, "b": 2};
  m.length()
}

//> MapLength()
//
// 2
