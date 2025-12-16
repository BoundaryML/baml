
function LlmEcho(input: string) -> string {
    input
}



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

function Main() -> string {
  MyFunction4(true)
}

test default {
  functions [Main]
  args { }
}
