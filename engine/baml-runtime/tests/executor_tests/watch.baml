// Watch notification test

function WithWatch() -> int {
  watch let x = 5;
  x = 10;
  x * 2
}

//> WithWatch()
//
// [watch] x = 10
//
// 20
