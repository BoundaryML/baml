// BEP-044 §"Diamond inheritance": each `implements` block resolves its own
// interface's method chain independently. No ambiguity on the class itself —
// callers must qualify (see method disambiguation tests).

interface Base {
  function foo(self) -> string throws never {
    return "Base"
  }
}

interface Left requires Base {
  function foo(self) -> string throws never {
    return "Left"
  }
}

interface Right requires Base {
  function foo(self) -> string throws never {
    return "Right"
  }
}

class Diamond {
  implements Base {}
  implements Left {}
  implements Right {}
}

//----
//- diagnostics
// <no-diagnostics-expected>
