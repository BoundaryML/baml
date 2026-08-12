function RenderViolations(violations: string[]) -> string {
  let result = ""
  for (let violation in violations) {
    result = result + ` - ${viola<[CURSOR]tion}`
  }
  result
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- on_hover expressions
// hover at b966_template_interpolation_hover_highlight.baml:4:34
// ```baml
// violation: string
// ```
//
//- semantic_tokens
// b966_template_interpolation_hover_highlight.baml:1:1 (keyword) len=8 "function"
// b966_template_interpolation_hover_highlight.baml:1:10 (function) [declaration] len=16 "RenderViolations"
// b966_template_interpolation_hover_highlight.baml:1:27 (parameter) [declaration] len=10 "violations"
// b966_template_interpolation_hover_highlight.baml:1:39 (type) [defaultLibrary] len=6 "string"
// b966_template_interpolation_hover_highlight.baml:1:52 (type) [defaultLibrary] len=6 "string"
// b966_template_interpolation_hover_highlight.baml:2:3 (keyword) len=3 "let"
// b966_template_interpolation_hover_highlight.baml:2:7 (variable) [declaration] len=6 "result"
// b966_template_interpolation_hover_highlight.baml:2:14 (operator) len=1 "="
// b966_template_interpolation_hover_highlight.baml:2:16 (string) len=2 "\"\""
// b966_template_interpolation_hover_highlight.baml:3:3 (keyword) len=3 "for"
// b966_template_interpolation_hover_highlight.baml:3:8 (keyword) len=3 "let"
// b966_template_interpolation_hover_highlight.baml:3:12 (variable) [declaration] len=9 "violation"
// b966_template_interpolation_hover_highlight.baml:3:22 (keyword) len=2 "in"
// b966_template_interpolation_hover_highlight.baml:3:25 (parameter) len=10 "violations"
// b966_template_interpolation_hover_highlight.baml:4:5 (variable) len=6 "result"
// b966_template_interpolation_hover_highlight.baml:4:12 (operator) len=1 "="
// b966_template_interpolation_hover_highlight.baml:4:14 (variable) len=6 "result"
// b966_template_interpolation_hover_highlight.baml:4:21 (operator) len=1 "+"
// b966_template_interpolation_hover_highlight.baml:4:23 (string) len=1 "`"
// b966_template_interpolation_hover_highlight.baml:4:25 (string) len=1 "-"
// b966_template_interpolation_hover_highlight.baml:4:29 (variable) len=9 "violation"
// b966_template_interpolation_hover_highlight.baml:4:39 (string) len=1 "`"
// b966_template_interpolation_hover_highlight.baml:6:3 (variable) len=6 "result"
