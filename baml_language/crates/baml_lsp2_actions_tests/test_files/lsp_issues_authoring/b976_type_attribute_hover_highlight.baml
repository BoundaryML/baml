class Detection {
  finding string @descrip<[CURSOR]tion("The detected finding")
  confidence float @description("Confidence in the finding")
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- on_hover expressions
// hover at b976_type_attribute_hover_highlight.baml:2:26
// ```baml
// @description("text")
// ```
// Attaches a human-readable description to a class, enum, field, or enum variant. Expects exactly one string argument.
//
//- semantic_tokens
// b976_type_attribute_hover_highlight.baml:1:1 (keyword) len=5 "class"
// b976_type_attribute_hover_highlight.baml:1:7 (class) [declaration] len=9 "Detection"
// b976_type_attribute_hover_highlight.baml:2:3 (property) [declaration] len=7 "finding"
// b976_type_attribute_hover_highlight.baml:2:11 (type) [defaultLibrary] len=6 "string"
// b976_type_attribute_hover_highlight.baml:2:18 (decorator) len=1 "@"
// b976_type_attribute_hover_highlight.baml:2:19 (decorator) len=11 "description"
// b976_type_attribute_hover_highlight.baml:2:31 (string) len=22 "\"The detected finding\""
// b976_type_attribute_hover_highlight.baml:3:3 (property) [declaration] len=10 "confidence"
// b976_type_attribute_hover_highlight.baml:3:14 (type) [defaultLibrary] len=5 "float"
// b976_type_attribute_hover_highlight.baml:3:20 (decorator) len=1 "@"
// b976_type_attribute_hover_highlight.baml:3:21 (decorator) len=11 "description"
// b976_type_attribute_hover_highlight.baml:3:33 (string) len=27 "\"Confidence in the finding\""
