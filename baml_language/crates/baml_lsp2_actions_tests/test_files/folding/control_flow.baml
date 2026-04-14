function test_if(x: int) -> string {
  if (x > 0) {
    "positive"
  } else if (x < 0) {
    "negative"
  } else {
    "zero"
  }
}

function test_match(x: int) -> string {
  match (x) {
    1 => "one",
    2 => "two",
    _ => "other",
  }
}

function test_for() -> int {
  let sum = 0;
  for (i in [1, 2, 3]) {
    sum = sum + i;
  }
  sum
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- folding_ranges
// control_flow.baml:1:1-9:2
// control_flow.baml:1:17-1:25
// control_flow.baml:1:36-9:2
// control_flow.baml:2:3-8:4
// control_flow.baml:2:14-4:4
// control_flow.baml:4:10-8:4
// control_flow.baml:4:21-6:4
// control_flow.baml:6:10-8:4
// control_flow.baml:11:1-17:2
// control_flow.baml:11:20-11:28
// control_flow.baml:11:39-17:2
// control_flow.baml:12:3-16:4
// control_flow.baml:19:1-25:2
// control_flow.baml:19:18-19:20
// control_flow.baml:19:28-25:2
// control_flow.baml:21:3-23:4
// control_flow.baml:21:24-23:4
