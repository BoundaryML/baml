// Test combining multiple string methods

function StringMethodsChain() -> string {
  let s = "  HELLO world  ";
  let trimmed = s.trim();
  let lower = trimmed.toLowerCase();
  let replaced = lower.replace("world", "baml");
  replaced
}

//> StringMethodsChain()
//
// "hello baml"

function StringSplitAndCheck() -> bool {
  let sentence = "apple,banana,cherry";
  let parts = sentence.split(",");
  let first = parts[0];
  first.startsWith("app") && first.endsWith("le")
}

//> StringSplitAndCheck()
//
// true
