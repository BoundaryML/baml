function deeply_nested(x: int) -> int {
  let result = 0;
  if (x > 0) {
    for (i in [1, 2, 3]) {
      if (i > 1) {
        result = match (i) {
          2 => x + 2,
          _ => x + 3,
        };
      } else {
        result = x + 1;
      }
    }
  }
  result
}

function with_lambda() -> int {
  let f = (x: int) -> int {
    x * 2
  };
  f(5)
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- folding_ranges
// nested.baml:1:1-16:2
// nested.baml:1:23-1:31
// nested.baml:1:39-16:2
// nested.baml:3:3-14:4
// nested.baml:3:14-14:4
// nested.baml:4:5-13:6
// nested.baml:4:26-13:6
// nested.baml:5:7-12:8
// nested.baml:5:18-10:8
// nested.baml:6:18-9:10
// nested.baml:10:14-12:8
// nested.baml:18:1-23:2
// nested.baml:18:21-18:23
// nested.baml:18:31-23:2
// nested.baml:19:11-21:4
// nested.baml:19:11-19:19
// nested.baml:19:27-21:4
