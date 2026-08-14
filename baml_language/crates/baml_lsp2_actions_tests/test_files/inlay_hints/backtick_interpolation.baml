// `${expr}` lowers to `string.from(expr)` (synthetic call) and `${ let … }`
// builds a concat-scope accumulator (`let " __m3_concat" = ""`, origin
// Compiler). Neither synthetic node should produce an inlay hint — only the
// user's real `let greeting` / `let counted` bindings get a type hint.
function Greet(name: string, items: int[]) -> string {
  let greeting = `Hi ${name}! you have ${items.length()} items`;
  let counted = `count: ${ let n = items.length() }${n} done`;
  greeting + counted
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- inlay_hints
// backtick_interpolation.baml:6:15 (Type) ": string"
// backtick_interpolation.baml:7:14 (Type) ": string"
// backtick_interpolation.baml:7:33 (Type) ": int"
