// test string
function TestFnNamedArgsSingleMapStringToString(myMap: map<string, string>) -> map<string, string> {
  client GPT35
  prompt #"
    Return this value back to me: {{myMap}}
  "#
}

function TestFnNamedArgsSingleMapStringToStringExprFunc(myMap: map<string, string>) -> map<string, string> {
  return myMap;
}

test TestFnNamedArgsSingleMapStringToString {
  functions [TestFnNamedArgsSingleMapStringToString]
  args {
    myMap {
      "key" "example string"
    }
  }
}
