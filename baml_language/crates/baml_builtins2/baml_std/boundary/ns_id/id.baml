//baml:vm
/// Returns the current execution boundary ID for this function invocation.
///
/// For the root frame this is the host-created `BoundaryId` that keys
/// RunStore and boundary history. Child calls return their own explicit
/// boundary override when one is active; otherwise they return the default
/// structural `CallRef` for compatibility with the existing `$id` surface.
function current() -> string throws never {
    $rust_function
}
