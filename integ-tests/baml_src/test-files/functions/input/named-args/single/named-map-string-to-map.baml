// test string
function TestFnNamedArgsSingleMapStringToMap(myMap: map<string, map<string, string>>) -> map<string, map<string, string>> {
  client GPT35
  prompt #"
    Return this value back to me: {{myMap}}
  "#
}

function TestFnNamedArgsSingleMapStringToMapExprFunc(myMap: map<string, map<string, string>>) -> map<string, map<string, string>> {
  return myMap;
}

test TestFnNamedArgsSingleMapStringToMap {
  functions [TestFnNamedArgsSingleMapStringToMap]
  args {
    myMap {
      "outer-key" {
        "key" "example string"
      }
    }
  }
}