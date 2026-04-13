class StackFrame {
  file string
  line int
  function_name string
}

class StackTrace {
  frames StackFrame[]

  //baml:vm
  function to_string(self) -> string {
    $rust_function
  }
}
