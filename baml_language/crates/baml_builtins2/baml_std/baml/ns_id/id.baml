//baml:vm
/// Returns the current BEX runtime ID for this function invocation: the
/// override set via `baml.id.set` / `$id = ...` if one is active, otherwise
/// the call's default `CallRef`. Returns an empty string when no BEX
/// function is running (e.g. during `$init`).
function current() -> string throws never {
    $rust_function
}

/// Creates a fresh override runtime ID (a `baml_id_1_...` string), suitable
/// for `baml.id.set`.
//baml:fallible
function new() -> string throws never {
    $rust_function
}

//baml:mut_vm
/// Sets the current BEX runtime ID to an override ID created by
/// `baml.id.new()`.
///
/// Throws `InvalidArgument` if `id` is not a `baml_id_1_...` override string
/// (a default `baml_call_1_...` CallRef is rejected — calls cannot adopt
/// another call's identity), or if no BEX function is running.
//baml:fallible
function set(id: string) -> string throws root.errors.InvalidArgument {
    $rust_function
}
