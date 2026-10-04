/// Feeds a value into a `baml.hash.Hasher`.
///
/// Equal values must feed equivalent hash input into the same initial hasher
/// state. A custom implementation must agree with `baml.ops.Equals`; neither
/// the compiler nor the runtime can verify arbitrary implementations.
///
/// Data types with structural equality have an implicit implementation when
/// their fields or elements support `Hash`. A custom `Equals.eq` requires a
/// compatible explicit `Hash` implementation, including when the matching
/// equality declaration has generic bounds. Cyclic structural values panic;
/// repeated references to an acyclic value are hashed by value.
///
/// Keep the hash and equality of a stored map key stable. Hash values and their
/// input encoding are not portable identifiers or a serialization format.
interface Hash {
    function hash(self, state: root.hash.Hasher) -> void throws never;
}

/// The native structural implementation selected by a `Hash` witness.
//baml:mut_vm
//baml:may_yield
function _hash_structural_default(value: unknown, state: unknown) -> void throws never {
    $rust_function
}
