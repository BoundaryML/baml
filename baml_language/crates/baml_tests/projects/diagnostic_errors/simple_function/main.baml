function HelloWorld(name: string) -> string {
  client GPT4
  prompt #"
    Say hello to {{name}}
  "#
}

class User {
  name string
  age int
}// Added comment
// Test comment
