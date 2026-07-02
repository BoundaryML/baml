// Capability negotiation (scenario 36): the binary `match` answers "can it stream at all",
// but support is often graded. `Capabilities` is a behaviour-free descriptor returning a
// three-valued `Support` (the design's Yes/No/Maybe lattice) — the achievable-today answer
// to "a flag can't capture the gradient" (a library descriptor, not a static contract).

enum Support {
    Yes, // statically known to work
    No, // known to fail — don't bother sending
    Maybe, // depends on the payload — the runtime probe is the truth
}

interface Capabilities requires root.ai.Provider {
    function structured_output(self) -> Support throws never
    function image_input(self) -> Support throws never
    function parallel_tools(self) -> Support throws never
    function max_input_tokens(self) -> int throws never
}

// Usage: read the gradient up front instead of gambling on a throw.
function example_negotiate(p: root.ai.Provider) -> string throws never {
    match (p) {
        let cap: Capabilities => match (cap.structured_output()) {
            Support.Yes => "yes",
            Support.No => "no",
            Support.Maybe => "maybe",
        },
        _ => "unknown",
    }
}
