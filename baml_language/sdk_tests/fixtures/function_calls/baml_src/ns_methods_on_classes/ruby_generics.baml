class BindingField {
  __baml_type_args string
}

class GenericBindingField<T> {
  __baml_type_args string
  value T
}

function binding_field(value: BindingField) -> BindingField { value }
function generic_binding_field<T>(value: GenericBindingField<T>) -> GenericBindingField<T> { value }

interface RubyNamed<X> {
  function label(self, other: X) -> string throws never;
}

implement RubyNamed<int> for int {
  function label(self, other: int) -> string throws never { "int" }
}

class RubyTagged<T extends RubyNamed<T>> {
  value T
  function zero() -> int { 0 }
}
