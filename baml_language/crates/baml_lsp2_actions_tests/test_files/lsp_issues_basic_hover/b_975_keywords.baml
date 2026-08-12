cl<[CURSOR]ass KeywordExample {
  value string
}

fun<[CURSOR]ction UseKeywordExample(value: KeywordExample) -> string {
  re<[CURSOR]turn value.value
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- on_hover expressions
// hover at b_975_keywords.baml:1:3
// ```baml
// class
// ```
// Defines a structured data type with named fields.
// hover at b_975_keywords.baml:5:4
// ```baml
// function
// ```
// Declares a named callable with typed parameters and a return type.
// hover at b_975_keywords.baml:6:5
// ```baml
// return
// ```
// Returns a value from the current function.
