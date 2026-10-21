// The element type of an empty `[]` is a real inference variable
// (`List(?e)`), the honest replacement for TIR's evolving-list sentinel. Here
// it resolves through EXPECTED-RETURN back-propagation: the tail use checks
// `List(?e)` against the declared `int[]`, invariance decays to `?e = int`.
// (Push-DRIVEN resolution, with no return type to help, is the pending
// `empty_list_element_from_push_only` fixture - it needs method calls.)
function f() -> int[] {
    let xs = [];
    xs.push(1);
    xs
 // ^^ int[]
}
