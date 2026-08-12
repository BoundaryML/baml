// BEP-044 §"self Access": `self.as<InterfaceName>.method()` works from inside an
// `implements` block to reach a sibling interface's method even when names
// are flat.

interface Greeter {
  function greet(self) -> string throws never
}

interface Farewell {
  function bye(self) -> string throws never
}

class Polite {
  name: string

  implements Greeter {
    function greet(self) -> string {
      return "Hello, I'm " + self.name
    }
  }

  implements Farewell {
    function bye(self) -> string {
      return self.as<Greeter>.greet() + " — and goodbye!"
    }
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// self_qualified_call.baml:1:1 (comment) len=82 "// BEP-044 §\"self Access\": `self.as<InterfaceName>.method()` works from inside an"
// self_qualified_call.baml:2:1 (comment) len=75 "// `implements` block to reach a sibling interface's method even when names"
// self_qualified_call.baml:3:1 (comment) len=12 "// are flat."
// self_qualified_call.baml:5:1 (keyword) len=9 "interface"
// self_qualified_call.baml:5:11 (interface) [declaration] len=7 "Greeter"
// self_qualified_call.baml:6:3 (keyword) len=8 "function"
// self_qualified_call.baml:6:12 (method) [declaration] len=5 "greet"
// self_qualified_call.baml:6:18 (parameter) [declaration] len=4 "self"
// self_qualified_call.baml:6:27 (type) [defaultLibrary] len=6 "string"
// self_qualified_call.baml:6:34 (keyword) len=6 "throws"
// self_qualified_call.baml:6:41 (type) [defaultLibrary] len=5 "never"
// self_qualified_call.baml:9:1 (keyword) len=9 "interface"
// self_qualified_call.baml:9:11 (interface) [declaration] len=8 "Farewell"
// self_qualified_call.baml:10:3 (keyword) len=8 "function"
// self_qualified_call.baml:10:12 (method) [declaration] len=3 "bye"
// self_qualified_call.baml:10:16 (parameter) [declaration] len=4 "self"
// self_qualified_call.baml:10:25 (type) [defaultLibrary] len=6 "string"
// self_qualified_call.baml:10:32 (keyword) len=6 "throws"
// self_qualified_call.baml:10:39 (type) [defaultLibrary] len=5 "never"
// self_qualified_call.baml:13:1 (keyword) len=5 "class"
// self_qualified_call.baml:13:7 (class) [declaration] len=6 "Polite"
// self_qualified_call.baml:14:3 (property) [declaration] len=4 "name"
// self_qualified_call.baml:14:9 (type) [defaultLibrary] len=6 "string"
// self_qualified_call.baml:16:3 (keyword) len=10 "implements"
// self_qualified_call.baml:16:14 (interface) len=7 "Greeter"
// self_qualified_call.baml:17:5 (keyword) len=8 "function"
// self_qualified_call.baml:17:14 (method) [declaration] len=5 "greet"
// self_qualified_call.baml:17:20 (parameter) [declaration] len=4 "self"
// self_qualified_call.baml:17:29 (type) [defaultLibrary] len=6 "string"
// self_qualified_call.baml:18:7 (keyword) len=6 "return"
// self_qualified_call.baml:18:14 (string) len=13 "\"Hello, I'm \""
// self_qualified_call.baml:18:28 (operator) len=1 "+"
// self_qualified_call.baml:18:30 (parameter) len=4 "self"
// self_qualified_call.baml:18:35 (property) len=4 "name"
// self_qualified_call.baml:22:3 (keyword) len=10 "implements"
// self_qualified_call.baml:22:14 (interface) len=8 "Farewell"
// self_qualified_call.baml:23:5 (keyword) len=8 "function"
// self_qualified_call.baml:23:14 (method) [declaration] len=3 "bye"
// self_qualified_call.baml:23:18 (parameter) [declaration] len=4 "self"
// self_qualified_call.baml:23:27 (type) [defaultLibrary] len=6 "string"
// self_qualified_call.baml:24:7 (keyword) len=6 "return"
// self_qualified_call.baml:24:14 (parameter) len=4 "self"
// self_qualified_call.baml:24:19 (keyword) len=2 "as"
// self_qualified_call.baml:24:21 (operator) len=1 "<"
// self_qualified_call.baml:24:22 (interface) len=7 "Greeter"
// self_qualified_call.baml:24:29 (operator) len=1 ">"
// self_qualified_call.baml:24:31 (method) len=5 "greet"
// self_qualified_call.baml:24:39 (operator) len=1 "+"
// self_qualified_call.baml:24:41 (string) len=19 "\" — and goodbye!\""
