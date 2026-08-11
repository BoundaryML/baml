class Value {
  name string
  meta baml.reflect.Meta
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
function new(name: string, values: (string | Value)[]) -> Type throws baml.reflect.errors.CompilationError {
  $rust_function
}

/// Recover the source variant name from a runtime or static enum value.
//baml:mut_vm
//baml:fallible
function get_value(value: unknown) -> string throws never { $rust_function }

/// An enum-kind view of a `type` value. Users never construct this class.
class Type {
  implements baml.reflect.TypeView {
    //baml:vm
    function as_type(self) -> type throws never { $rust_function }
  }
  //baml:mut_vm
  function values(self) -> Value[] throws never { $rust_function }
  //baml:mut_vm
  function meta(self) -> baml.reflect.Meta throws never { $rust_function }
}
