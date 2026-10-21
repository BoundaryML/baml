// BEP-079: the `cleanup` finalizer returns the unit type, however spelled.

type Nothing = void

// --- finalizers ---

class ViaVoid {
  function cleanup(self) -> void {}
}

class ViaNull {
  function cleanup(self) -> null {}
}

class ViaAlias {
  function cleanup(self) -> Nothing {}
}

// --- not finalizers ---

// Reported once, at the signature: the body is not then held to the return
// type the signature should not have.
class ReturnsValue {
  function cleanup(self) -> int { 5 }
}

class ReturnsOptional {
  function cleanup(self) -> int? {}
}
