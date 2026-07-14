class Foo {
  tag string
  function zero<T>(self) -> T? {
    null
  }
}

function use_method_type_args() -> int {
  let f = Foo { tag: "x" };
  let v: int = f.zero<int>() ?? 5;
  v
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// method_explicit_type_args.baml:1:1 (keyword) len=5 "class"
// method_explicit_type_args.baml:1:7 (class) [declaration] len=3 "Foo"
// method_explicit_type_args.baml:2:3 (property) [declaration] len=3 "tag"
// method_explicit_type_args.baml:2:7 (type) [defaultLibrary] len=6 "string"
// method_explicit_type_args.baml:3:3 (keyword) len=8 "function"
// method_explicit_type_args.baml:3:12 (method) [declaration] len=4 "zero"
// method_explicit_type_args.baml:3:16 (operator) len=1 "<"
// method_explicit_type_args.baml:3:17 (typeParameter) [declaration] len=1 "T"
// method_explicit_type_args.baml:3:18 (operator) len=1 ">"
// method_explicit_type_args.baml:3:20 (parameter) [declaration] len=4 "self"
// method_explicit_type_args.baml:3:29 (type) len=1 "T"
// method_explicit_type_args.baml:4:5 (type) [defaultLibrary] len=4 "null"
// method_explicit_type_args.baml:8:1 (keyword) len=8 "function"
// method_explicit_type_args.baml:8:10 (function) [declaration] len=20 "use_method_type_args"
// method_explicit_type_args.baml:8:36 (type) [defaultLibrary] len=3 "int"
// method_explicit_type_args.baml:9:3 (keyword) len=3 "let"
// method_explicit_type_args.baml:9:7 (variable) [declaration] len=1 "f"
// method_explicit_type_args.baml:9:9 (operator) len=1 "="
// method_explicit_type_args.baml:9:11 (class) len=3 "Foo"
// method_explicit_type_args.baml:9:17 (property) len=3 "tag"
// method_explicit_type_args.baml:9:22 (string) len=3 "\"x\""
// method_explicit_type_args.baml:10:3 (keyword) len=3 "let"
// method_explicit_type_args.baml:10:7 (variable) [declaration] len=1 "v"
// method_explicit_type_args.baml:10:10 (type) [defaultLibrary] len=3 "int"
// method_explicit_type_args.baml:10:14 (operator) len=1 "="
// method_explicit_type_args.baml:10:16 (variable) len=1 "f"
// method_explicit_type_args.baml:10:18 (method) len=4 "zero"
// method_explicit_type_args.baml:10:22 (operator) len=1 "<"
// method_explicit_type_args.baml:10:23 (type) [defaultLibrary] len=3 "int"
// method_explicit_type_args.baml:10:26 (operator) len=1 ">"
// method_explicit_type_args.baml:10:33 (number) len=1 "5"
// method_explicit_type_args.baml:11:3 (variable) len=1 "v"
