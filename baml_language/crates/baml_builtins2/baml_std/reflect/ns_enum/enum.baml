class Value {
  name string
  meta reflect.Meta
}

/// Construct one enum row with optional schema metadata.
//baml:mut_vm
function value(
  name: string,
  alias: string? = null,
  description: string? = null,
  docstring: string? = null,
  other: map<string, string>? = null,
) -> Value throws never { $rust_function }

/// Mint a fresh runtime enum definition. Every evaluation has distinct type
/// identity even when its name and rows are byte-identical.
//baml:mut_vm
//baml:fallible
function new(name: string, values: (string | Value)[]) -> Type throws reflect.errors.CompilationError {
  $rust_function
}

/// Recover the source variant name from a runtime or static enum value.
//baml:mut_vm
//baml:fallible
function get_value(value: unknown) -> string throws never { $rust_function }

/// An enum-kind view of a `type` value. Users never construct this class.
class Type {
  implements reflect.TypeView {
    //baml:vm
    function as_type(self) -> reflect.Type throws never { $rust_function }
  }
  //baml:mut_vm
  function values(self) -> Value[] throws never { $rust_function }
  //baml:mut_vm
  function meta(self) -> reflect.Meta throws never { $rust_function }
}
