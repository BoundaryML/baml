// A host-callable parameter whose callable type tries to give one of its own
// arguments a default (`y: int = 10`). A default is a property of a function
// *declaration*, not a function *type*, so the parser rejects it — the same
// `E0010` rule that rejects `type T = (a: int = 1) -> int`, here exercised in
// the host-callback parameter position. This pins that an optional callback
// argument must be written with the `?` marker (`y?: int`), never a default.
function call_with_defaulted_callback(callback: (x: int, y: int = 10) -> string, x: int) -> string {
    callback(x)
}
