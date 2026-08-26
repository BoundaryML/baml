// R7 (S15 residue): template expressions (untagged + prompt tag)
//
// Untagged backticks realize as the desugared `string.from` concat
// (result `string`, every `${expr}` typed in place). A tagged template
// types as the tag fn's RETURN (the `prompt` tag yields the
// `(Context) -> PromptAst` render closure), with the tag's body-lambda
// params (`role`, `ctx`) in scope inside the interpolations.
function untagged(name: string) -> string throws never {
    let s = `Hello ${name} ${1 + 2}`;
     // ^ string
    s
}

function tagged() -> int throws never {
    let render = prompt`${role("system")}You are helpful.${role("user")}Hi!`;
     // ^^^^^^ (ai.Context) -> ai.Prompt throws ai.errors.PromptRenderError
    0
}
