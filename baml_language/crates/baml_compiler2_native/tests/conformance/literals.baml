// A value of a union of literals is held as their one type, and a test for
// one of them compares values.

function literal_op(i: int) -> "add" | "sub" | "mul" {
  if (i % 3 == 0) { return "add"; }
  if (i % 3 == 1) { return "sub"; }
  "mul"
}
function literal_level(i: int) -> 1 | 2 | 30 {
  if (i % 3 == 0) { return 1; }
  if (i % 3 == 1) { return 2; }
  30
}
class LiteralStep { op: "add" | "sub" | "mul", level: 1 | 2 | 30 }

function literal_strings() -> string {
  let out = "";
  for (let i = 0; i < 7; i += 1) {
    out = out + match (literal_op(count(i))) {
      "add" => "a",
      "sub" => "s",
      "mul" => "m",
    };
  }
  out
}
function literal_ints() -> int {
  let total = 0;
  for (let i = 0; i < 7; i += 1) {
    total = total * 3 + match (literal_level(count(i))) {
      1 => 5,
      2 => 7,
      30 => 11,
    };
  }
  total
}
function literal_fields() -> string {
  let steps: LiteralStep[] = [];
  for (let i = 0; i < 5; i += 1) {
    steps.push(LiteralStep { op: literal_op(count(i)), level: literal_level(count(i + 1)) });
  }
  let acc = 100;
  let out = "";
  for (let step in steps) {
    acc = match (step.op) {
      "add" => acc + step.level,
      "sub" => acc - step.level,
      "mul" => acc * step.level,
    };
    out = out + `${step.op}${step.level}=${acc},`;
  }
  out
}
// A wildcard arm takes what no literal arm did.
function literal_wildcard() -> string {
  let out = "";
  for (let i = 0; i < 4; i += 1) {
    out = out + match (literal_op(count(i))) {
      "sub" => "sub,",
      _ => "other,",
    };
  }
  out
}
