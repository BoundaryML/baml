/// A single frame in a call stack, identifying a source location.
class StackFrame {
  file string
  line int
  function_name string
}

/// A captured call stack, consisting of one or more `StackFrame` entries.
class StackTrace {
  frames StackFrame[]

  //baml:vm
  function to_string(self) -> string {
    $rust_function
  }
}
