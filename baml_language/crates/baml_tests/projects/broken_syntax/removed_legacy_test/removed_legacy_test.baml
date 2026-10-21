function Greet(name: string) -> string {
  "Hello, {name}!"
}

test FunctionsFirst {
  functions [Greet]
  args {
    name "world"
  }
}

test ArgsFirst {
  args {
    name "world"
  }
  functions [Greet]
}
