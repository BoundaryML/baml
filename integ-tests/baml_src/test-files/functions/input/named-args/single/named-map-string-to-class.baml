class StringToClassEntry {
  word string
}
// test string
function TestFnNamedArgsSingleMapStringToClass(myMap: int) -> map<string, StringToClassEntry> {
  client GPT35
  prompt #"
    Return this value back to me: {{myMap}}
  "#
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
