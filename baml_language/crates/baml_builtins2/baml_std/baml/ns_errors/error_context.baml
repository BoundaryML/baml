/// The temporal context of a thrown error: the error value itself, where it
/// was thrown, and the error it superseded while the scope was unwinding (its
/// `cause`), if any. Bound by the second parameter of a `catch (e, ctx)`
/// handler.
///
/// The chain runs newest → oldest through `cause`; `root_cause` walks to the
/// original failure at the tail, and `to_string` renders the whole chain
/// Python-style ("During handling of the above error, another error occurred").
class Context {
    error: unknown,
    stack_trace: StackTrace,
    cause: Context?,

    /// The original error at the tail of the cause chain — walks `cause` to the
    /// deepest link. Returns `self` when nothing was superseded.
    function root_cause(self) -> Context throws never {
        match (self.cause) {
            null => self,
            let c: Context => c.root_cause(),
        }
    }

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
