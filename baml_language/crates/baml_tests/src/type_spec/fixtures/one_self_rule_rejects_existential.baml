//
// The one-Self rule: `eq(self, other: Self)` is uncallable through an
// interface EXISTENTIAL - two existentials may hold different concrete
// types, so a Self-typed parameter is unsound. hir_ty types the call
// as the Error sentinel (S17 renders the diagnostic); TIR rejects with
// its own diagnostic, which the harness counts as a TIR failure.
function f(a: baml.ops.Equals, b: baml.ops.Equals) -> bool {
    a.eq(b)
 // ^^^^^^^ !error
}
