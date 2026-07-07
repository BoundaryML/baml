// Capability introspection (scenario 36): the binary `match` answers "can it stream at
// all", but support is often graded. `Capabilities` is a behaviour-free descriptor
// returning a three-valued `Support` (the design's Yes/No/Maybe lattice) — the
// achievable-today answer to "a flag can't capture the gradient" (a library descriptor,
// not a static contract).

enum Support {
    Yes, // statically known to work
    No, // known to fail — don't bother sending
    Maybe, // depends on the payload — the runtime probe is the truth
}

/// Graded feature support, read up front instead of gambling on a throw:
/// `match (p) { let cap: Capabilities => cap.structured_output(), ... }`.
interface Capabilities requires root.ai.Provider {
    function structured_output(self) -> Support throws never
    function image_input(self) -> Support throws never
    function parallel_tools(self) -> Support throws never
    function max_input_tokens(self) -> int throws never
}
