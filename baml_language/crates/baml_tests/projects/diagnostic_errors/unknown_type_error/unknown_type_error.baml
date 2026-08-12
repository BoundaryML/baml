// Test case: unknown type in return position should report error
// and suppress downstream type mismatch errors
function Foo() -> blah {
  1
}

// Test case: unknown type in parameter position
function Bar(x: unknown_param_type) -> int {
  1
}

// Test case: unknown type in optional wrapper
function Baz() -> unknown_optional? {
  null
}

// Test case: enum-variant path with a typo'd variant name. `Status.Active` is
// a real singleton type, but `Status.Typo` doesn't exist on the enum and must
// be reported as `UnresolvedType` rather than silently producing a bogus
// `Ty::EnumVariant`.
enum Status {
  Active
  Inactive
}

function StatusTypoInTypePosition(s: Status.Typo) -> int {
  1
}

function UnknownFieldAccess(raw: unknown) -> string {
  raw.email.to_lower_case()
}

class Envelope {
  payload unknown
}

function UnknownNestedFieldAccess(envelope: Envelope) -> string {
  envelope.payload.email
}
