// Named (optional) arguments at a call site: `name = value`. The argument
// name refers to the callee's parameter and should highlight as a parameter.

function Search(query: string, max_results: int = 10) -> int {
  query.length() + max_results
}

function UseIt() -> int {
  Search(query = "hello", max_results = 5)
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// call_named_args.baml:1:1 (comment) len=74 "// Named (optional) arguments at a call site: `name = value`. The argument"
// call_named_args.baml:2:1 (comment) len=77 "// name refers to the callee's parameter and should highlight as a parameter."
// call_named_args.baml:4:1 (keyword) len=8 "function"
// call_named_args.baml:4:10 (function) [declaration] len=6 "Search"
// call_named_args.baml:4:17 (parameter) [declaration] len=5 "query"
// call_named_args.baml:4:24 (type) [defaultLibrary] len=6 "string"
// call_named_args.baml:4:32 (parameter) [declaration] len=11 "max_results"
// call_named_args.baml:4:45 (type) [defaultLibrary] len=3 "int"
// call_named_args.baml:4:49 (operator) len=1 "="
// call_named_args.baml:4:51 (number) len=2 "10"
// call_named_args.baml:4:58 (type) [defaultLibrary] len=3 "int"
// call_named_args.baml:5:3 (parameter) len=5 "query"
// call_named_args.baml:5:9 (method) len=6 "length"
// call_named_args.baml:5:18 (operator) len=1 "+"
// call_named_args.baml:5:20 (parameter) len=11 "max_results"
// call_named_args.baml:8:1 (keyword) len=8 "function"
// call_named_args.baml:8:10 (function) [declaration] len=5 "UseIt"
// call_named_args.baml:8:21 (type) [defaultLibrary] len=3 "int"
// call_named_args.baml:9:3 (function) len=6 "Search"
// call_named_args.baml:9:10 (parameter) len=5 "query"
// call_named_args.baml:9:16 (operator) len=1 "="
// call_named_args.baml:9:18 (string) len=7 "\"hello\""
// call_named_args.baml:9:27 (parameter) len=11 "max_results"
// call_named_args.baml:9:39 (operator) len=1 "="
// call_named_args.baml:9:41 (number) len=1 "5"
