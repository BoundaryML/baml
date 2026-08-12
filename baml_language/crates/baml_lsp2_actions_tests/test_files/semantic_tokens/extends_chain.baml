// BEP-044: a class implementing the leaf interface in a three-level requires
// chain should satisfy every contract in the chain. All fields and methods
// flow down transitively.

interface HasName {
  name: string
}

interface Greeter requires HasName {
  function greet(self) -> string throws never
}

interface Polite requires Greeter {
  function farewell(self) -> string throws never
}

class Robot {
  name: string

  implements HasName {}

  implements Greeter {
    function greet(self) -> string { return "hello" }
  }

  implements Polite {
    function farewell(self) -> string { return "bye" }
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// extends_chain.baml:1:1 (comment) len=77 "// BEP-044: a class implementing the leaf interface in a three-level requires"
// extends_chain.baml:2:1 (comment) len=75 "// chain should satisfy every contract in the chain. All fields and methods"
// extends_chain.baml:3:1 (comment) len=26 "// flow down transitively."
// extends_chain.baml:5:1 (keyword) len=9 "interface"
// extends_chain.baml:5:11 (interface) [declaration] len=7 "HasName"
// extends_chain.baml:6:3 (property) [declaration] len=4 "name"
// extends_chain.baml:6:9 (type) [defaultLibrary] len=6 "string"
// extends_chain.baml:9:1 (keyword) len=9 "interface"
// extends_chain.baml:9:11 (interface) [declaration] len=7 "Greeter"
// extends_chain.baml:9:19 (keyword) len=8 "requires"
// extends_chain.baml:9:28 (interface) len=7 "HasName"
// extends_chain.baml:10:3 (keyword) len=8 "function"
// extends_chain.baml:10:12 (method) [declaration] len=5 "greet"
// extends_chain.baml:10:18 (parameter) [declaration] len=4 "self"
// extends_chain.baml:10:27 (type) [defaultLibrary] len=6 "string"
// extends_chain.baml:10:34 (keyword) len=6 "throws"
// extends_chain.baml:10:41 (type) [defaultLibrary] len=5 "never"
// extends_chain.baml:13:1 (keyword) len=9 "interface"
// extends_chain.baml:13:11 (interface) [declaration] len=6 "Polite"
// extends_chain.baml:13:18 (keyword) len=8 "requires"
// extends_chain.baml:13:27 (interface) len=7 "Greeter"
// extends_chain.baml:14:3 (keyword) len=8 "function"
// extends_chain.baml:14:12 (method) [declaration] len=8 "farewell"
// extends_chain.baml:14:21 (parameter) [declaration] len=4 "self"
// extends_chain.baml:14:30 (type) [defaultLibrary] len=6 "string"
// extends_chain.baml:14:37 (keyword) len=6 "throws"
// extends_chain.baml:14:44 (type) [defaultLibrary] len=5 "never"
// extends_chain.baml:17:1 (keyword) len=5 "class"
// extends_chain.baml:17:7 (class) [declaration] len=5 "Robot"
// extends_chain.baml:18:3 (property) [declaration] len=4 "name"
// extends_chain.baml:18:9 (type) [defaultLibrary] len=6 "string"
// extends_chain.baml:20:3 (keyword) len=10 "implements"
// extends_chain.baml:20:14 (interface) len=7 "HasName"
// extends_chain.baml:22:3 (keyword) len=10 "implements"
// extends_chain.baml:22:14 (interface) len=7 "Greeter"
// extends_chain.baml:23:5 (keyword) len=8 "function"
// extends_chain.baml:23:14 (method) [declaration] len=5 "greet"
// extends_chain.baml:23:20 (parameter) [declaration] len=4 "self"
// extends_chain.baml:23:29 (type) [defaultLibrary] len=6 "string"
// extends_chain.baml:23:38 (keyword) len=6 "return"
// extends_chain.baml:23:45 (string) len=7 "\"hello\""
// extends_chain.baml:26:3 (keyword) len=10 "implements"
// extends_chain.baml:26:14 (interface) len=6 "Polite"
// extends_chain.baml:27:5 (keyword) len=8 "function"
// extends_chain.baml:27:14 (method) [declaration] len=8 "farewell"
// extends_chain.baml:27:23 (parameter) [declaration] len=4 "self"
// extends_chain.baml:27:32 (type) [defaultLibrary] len=6 "string"
// extends_chain.baml:27:41 (keyword) len=6 "return"
// extends_chain.baml:27:48 (string) len=5 "\"bye\""
