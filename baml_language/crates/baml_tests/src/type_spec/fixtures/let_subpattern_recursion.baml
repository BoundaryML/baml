// let sub-patterns lower recursively whatever their kind
function lsp_chain() -> int throws never {
    let x: let y = 1
    y = 2
    let f = () -> int { y }
    let r = f()
     // ^ int
    r
}
function lsp_ascribed_rest() -> int throws never {
    let xs: [..]: int[] = [4, 5, 6]
    let n = xs.length()
     // ^ int
    n
}
