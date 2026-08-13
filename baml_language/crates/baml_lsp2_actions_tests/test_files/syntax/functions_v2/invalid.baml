function Foo() -> {
  client Bar
  prompt `fa`
}

function FooBar(arg) -> bar {

}

//----
//- diagnostics
// E0010
//
//   × unexpected token
//    ╭─[functions_v2_invalid.baml:1:19]
//  1 │ function Foo() -> {
//    ·                   ┬
//    ·                   ╰── expected `type`, found `'{'`
//    ╰────
// E0010
//
//   × unexpected token
//    ╭─[functions_v2_invalid.baml:6:20]
//  6 │ function FooBar(arg) -> bar {
//    ·                    ┬
//    ·                    ╰── expected `type annotation`, found `')'`
//    ╰────
// E0108
//
//   × could not parse type expression for return type of `Foo`
//    ╭─[functions_v2_invalid.baml:1:18]
//  1 │ function Foo() -> {
//    ·                  ▲
//    ·                  ╰── unparseable type
//    ╰────
// E0002
//
//   × unresolved type: bar
//    ╭─[functions_v2_invalid.baml:6:25]
//  6 │ function FooBar(arg) -> bar {
//    ·                         ───
//    ╰────
