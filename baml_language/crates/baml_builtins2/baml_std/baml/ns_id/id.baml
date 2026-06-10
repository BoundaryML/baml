//baml:mut_vm
/// Returns the current BEX runtime ID for this function invocation.
function current() -> string throws never {
  $rust_function
}

/// Creates a fresh override runtime ID.
//baml:fallible
function new() -> string throws never {
  $rust_function
}

//baml:mut_vm
/// Sets the current BEX runtime ID to an override ID created by `baml.id.new()`.
//baml:fallible
function set(id: string) -> string throws never {
  $rust_function
}
