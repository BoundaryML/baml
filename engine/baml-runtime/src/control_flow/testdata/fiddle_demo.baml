
function LlmEcho(input: string) -> string {
  // see clients.baml
  client "openai/gpt-4o-mini"

  // The prompt uses Jinja syntax. Change the models or this text and watch the prompt preview change!
  prompt #"
    Repeat the following text back to me: {{ input }}
  "#
}



function fiddle_demo(flag: bool) -> string {
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
  fiddle_demo(true)
}