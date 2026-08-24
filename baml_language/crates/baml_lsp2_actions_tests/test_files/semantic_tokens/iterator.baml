// Generic stdlib iterators (`baml.iter`), named type args, lambdas, `throws`.
function CollectStrings(it: baml.iter.Iterator<Item = string, Error = never>) -> string[] throws never {
  it.collect()
}

function DoubleAll() -> baml.iter.Iterator<Item = int, Error = never> throws never {
  baml.iter.ArrayIterator.new([1, 2, 3]).map((x: int) -> int { x * 2 })
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// iterator.baml:1:1 (comment) len=78 "// Generic stdlib iterators (`baml.iter`), named type args, lambdas, `throws`."
// iterator.baml:2:1 (keyword) len=8 "function"
// iterator.baml:2:10 (function) [declaration] len=14 "CollectStrings"
// iterator.baml:2:25 (parameter) [declaration] len=2 "it"
// iterator.baml:2:29 (namespace) [defaultLibrary] len=4 "baml"
// iterator.baml:2:33 (namespace) [defaultLibrary] len=1 "."
// iterator.baml:2:34 (namespace) [defaultLibrary] len=4 "iter"
// iterator.baml:2:38 (namespace) [defaultLibrary] len=1 "."
// iterator.baml:2:39 (interface) [defaultLibrary] len=8 "Iterator"
// iterator.baml:2:47 (operator) len=1 "<"
// iterator.baml:2:48 (type) len=4 "Item"
// iterator.baml:2:53 (operator) len=1 "="
// iterator.baml:2:55 (type) [defaultLibrary] len=6 "string"
// iterator.baml:2:63 (type) len=5 "Error"
// iterator.baml:2:69 (operator) len=1 "="
// iterator.baml:2:71 (type) [defaultLibrary] len=5 "never"
// iterator.baml:2:76 (operator) len=1 ">"
// iterator.baml:2:82 (type) [defaultLibrary] len=6 "string"
// iterator.baml:2:91 (keyword) len=6 "throws"
// iterator.baml:2:98 (type) [defaultLibrary] len=5 "never"
// iterator.baml:3:3 (parameter) len=2 "it"
// iterator.baml:3:6 (method) len=7 "collect"
// iterator.baml:6:1 (keyword) len=8 "function"
// iterator.baml:6:10 (function) [declaration] len=9 "DoubleAll"
// iterator.baml:6:25 (namespace) [defaultLibrary] len=4 "baml"
// iterator.baml:6:29 (namespace) [defaultLibrary] len=1 "."
// iterator.baml:6:30 (namespace) [defaultLibrary] len=4 "iter"
// iterator.baml:6:34 (namespace) [defaultLibrary] len=1 "."
// iterator.baml:6:35 (interface) [defaultLibrary] len=8 "Iterator"
// iterator.baml:6:43 (operator) len=1 "<"
// iterator.baml:6:44 (type) len=4 "Item"
// iterator.baml:6:49 (operator) len=1 "="
// iterator.baml:6:51 (type) [defaultLibrary] len=3 "int"
// iterator.baml:6:56 (type) len=5 "Error"
// iterator.baml:6:62 (operator) len=1 "="
// iterator.baml:6:64 (type) [defaultLibrary] len=5 "never"
// iterator.baml:6:69 (operator) len=1 ">"
// iterator.baml:6:71 (keyword) len=6 "throws"
// iterator.baml:6:78 (type) [defaultLibrary] len=5 "never"
// iterator.baml:7:3 (namespace) [defaultLibrary] len=4 "baml"
// iterator.baml:7:7 (namespace) [defaultLibrary] len=1 "."
// iterator.baml:7:8 (namespace) [defaultLibrary] len=4 "iter"
// iterator.baml:7:12 (namespace) [defaultLibrary] len=1 "."
// iterator.baml:7:13 (class) [defaultLibrary] len=13 "ArrayIterator"
// iterator.baml:7:27 (method) len=3 "new"
// iterator.baml:7:32 (number) len=1 "1"
// iterator.baml:7:35 (number) len=1 "2"
// iterator.baml:7:38 (number) len=1 "3"
// iterator.baml:7:47 (parameter) [declaration] len=1 "x"
// iterator.baml:7:50 (type) [defaultLibrary] len=3 "int"
// iterator.baml:7:58 (type) [defaultLibrary] len=3 "int"
// iterator.baml:7:64 (parameter) len=1 "x"
// iterator.baml:7:66 (operator) len=1 "*"
// iterator.baml:7:68 (number) len=1 "2"
