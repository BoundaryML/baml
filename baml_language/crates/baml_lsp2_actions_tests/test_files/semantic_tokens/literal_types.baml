class TestLiterals {
  a "SingleLiteral"
  b "Field" | "With" | "Multiple" | "Literals"
  c "Field" | "With" | "Some" @description("Description")
  d 2 | 3
  e "Optional"?
  f 2 | "SomeString"
  g "boolean" | true | false
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// literal_types.baml:1:1 (keyword) len=5 "class"
// literal_types.baml:1:7 (class) [declaration] len=12 "TestLiterals"
// literal_types.baml:2:3 (property) [declaration] len=1 "a"
// literal_types.baml:2:5 (string) len=15 "\"SingleLiteral\""
// literal_types.baml:3:3 (property) [declaration] len=1 "b"
// literal_types.baml:3:5 (string) len=7 "\"Field\""
// literal_types.baml:3:13 (operator) len=1 "|"
// literal_types.baml:3:15 (string) len=6 "\"With\""
// literal_types.baml:3:22 (operator) len=1 "|"
// literal_types.baml:3:24 (string) len=10 "\"Multiple\""
// literal_types.baml:3:35 (operator) len=1 "|"
// literal_types.baml:3:37 (string) len=10 "\"Literals\""
// literal_types.baml:4:3 (property) [declaration] len=1 "c"
// literal_types.baml:4:5 (string) len=7 "\"Field\""
// literal_types.baml:4:13 (operator) len=1 "|"
// literal_types.baml:4:15 (string) len=6 "\"With\""
// literal_types.baml:4:22 (operator) len=1 "|"
// literal_types.baml:4:24 (string) len=6 "\"Some\""
// literal_types.baml:4:31 (decorator) len=1 "@"
// literal_types.baml:4:32 (decorator) len=11 "description"
// literal_types.baml:4:44 (string) len=13 "\"Description\""
// literal_types.baml:5:3 (property) [declaration] len=1 "d"
// literal_types.baml:5:5 (number) len=1 "2"
// literal_types.baml:5:7 (operator) len=1 "|"
// literal_types.baml:5:9 (number) len=1 "3"
// literal_types.baml:6:3 (property) [declaration] len=1 "e"
// literal_types.baml:6:5 (string) len=10 "\"Optional\""
// literal_types.baml:7:3 (property) [declaration] len=1 "f"
// literal_types.baml:7:5 (number) len=1 "2"
// literal_types.baml:7:7 (operator) len=1 "|"
// literal_types.baml:7:9 (string) len=12 "\"SomeString\""
// literal_types.baml:8:3 (property) [declaration] len=1 "g"
// literal_types.baml:8:5 (string) len=9 "\"boolean\""
// literal_types.baml:8:15 (operator) len=1 "|"
// literal_types.baml:8:17 (type) len=4 "true"
// literal_types.baml:8:22 (operator) len=1 "|"
// literal_types.baml:8:24 (type) len=5 "false"
