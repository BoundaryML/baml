function long_params(
  name: string,
  age: int,
  address: string,
  email: string
) -> string {
  name
}

function multiline_if(x: int, y: int) -> string {
  if (x > 0
      && y > 0
      && x + y > 10) {
    "both positive and sum > 10"
  } else {
    "other"
  }
}

function multiline_for(items: int[]) -> int {
  let sum = 0;
  for (item in [
    1,
    2,
    3,
    4,
    5
  ]) {
    sum = sum + item;
  }
  sum
}

function multiline_match(x: int) -> string {
  match (x > 0
         && x < 100) {
    true => "in range",
    false => "out of range",
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- folding_ranges
// multiline.baml:1:1-8:2
// multiline.baml:1:21-6:2
// multiline.baml:6:13-8:2
// multiline.baml:10:1-18:2
// multiline.baml:10:22-10:38
// multiline.baml:10:49-18:2
// multiline.baml:11:3-17:4
// multiline.baml:13:22-15:4
// multiline.baml:15:10-17:4
// multiline.baml:20:1-32:2
// multiline.baml:20:23-20:37
// multiline.baml:20:45-32:2
// multiline.baml:22:3-30:4
// multiline.baml:28:6-30:4
// multiline.baml:34:1-40:2
// multiline.baml:34:25-34:33
// multiline.baml:34:44-40:2
// multiline.baml:35:3-39:4
