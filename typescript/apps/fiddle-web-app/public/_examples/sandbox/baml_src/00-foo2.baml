

function MyFunction5(flag: bool) -> string {
  //# Check
  let result = if (flag) {
    "then-branch"
  } else {
    "else-branch"
  };

  //# Extract1
  watch let item = "before";
  LlmEcho(item);

  //# Extract2
  item = "extract2";
  LlmEcho(item);

  //# Extract3
  item = "extract3";
  LlmEcho(item);

  //# Extract4
  item = "extract4";
  LlmEcho(item);

  item
}

test default {
  functions [MyFunction5]
  args {
    flag true
  }
}

