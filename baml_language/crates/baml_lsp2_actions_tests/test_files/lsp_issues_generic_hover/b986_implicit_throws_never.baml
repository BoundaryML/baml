function <[CURSOR]bar() -> void {}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- on_hover expressions
// (B-986: a non-throwing function hover should omit implicit `throws never`.)
// hover at b986_implicit_throws_never.baml:1:10
// ```baml
// function bar() -> void
// ```
