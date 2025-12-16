

function MyFunction4(flag: bool) -> string {
  //# Check
  let result = if (flag) {
    "then-branch"
  } else {
    "else-branch"
  };

  watch let tagged = "before";
  
  for (let item in [
    "lorem",
    "ipsum",
    "dolor"
  ]) {
    //# Extract1
    LlmEcho(item);

    tagged = item;
  
    //# Extract2
    LlmEcho(item);
  }


  tagged
}

test default {
  functions [MyFunction4]
  args {
    flag true
  }
}

