class StringToClassEntry {
  word string
}
// test string
function TestFnNamedArgsSingleMapStringToClass(myMap: map<string, StringToClassEntry>) -> map<string, StringToClassEntry> {
  client GPT35
  prompt #"
    Return this value back to me: {{myMap}}
  "#
}

function TestFnNamedArgsSingleMapStringToClassExprFunc(myMap: map<string, StringToClassEntry>) -> map<string, StringToClassEntry> {
  return myMap;
}

test TestFnNamedArgsSingleMapStringToClass {
  functions [TestFnNamedArgsSingleMapStringToClass]
  args {
    myMap {
      "key" {
        word "lorem ipsum"
      }
    }
  }
}
