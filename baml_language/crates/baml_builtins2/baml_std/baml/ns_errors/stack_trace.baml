/// A single frame in a call stack, identifying a source location.
class StackFrame {
    file: string,
    line: int,
    function_name: string,
}

/// A captured call stack, consisting of one or more `StackFrame` entries.
class StackTrace {
    frames: StackFrame[],

    implements baml.ToString {
        function to_string(self) -> string throws never {
            self._to_string_impl()
        }
    }

    //baml:vm
    function _to_string_impl(self) -> string throws never {
        $rust_function
    }
}
