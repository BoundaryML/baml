// Outermost frame: the host entry point. Calls ChainMiddle, which calls
// ChainInner, which throws — producing a three-`.baml`-file-deep traceback.

/// Outer step — the host calls this; it calls ChainMiddle.
function ChainOuter() -> string {
  ChainMiddle()
}
