function MyFunction4(flag: bool) -> string {
  //# Check
  let result = if (flag) {
    "then-branch"
  } else {
    "else-branch"
  };

  watch let tagged = "branch:" + result;

  tagged
}

function Main() -> string {
  MyFunction4(true)
}

test default {
  functions [Main]
  args { }
}
