/// Universal wrapper for errors that do not implement a known capability channel.
class UnknownError {
    data: unknown,
    message: string[],

    /// Preserve a known `T`, recover a wrapped `T`, pass an existing
    /// `UnknownError` through, or wrap an otherwise unknown value.
    function from<T>(data: unknown) -> T | UnknownError throws never {
        match (data) {
            let value: T => {
                UnknownError._preserve_context(data, value);
                value
            },
            UnknownError { data: let inner: T } => {
                UnknownError._preserve_context(data, inner);
                inner
            },
            let error: UnknownError => {
                UnknownError._preserve_context(data, error);
                error
            },
            _ => {
                let error = UnknownError { data: data, message: [] };
                UnknownError._preserve_context(data, error);
                error
            },
        }
    }

    /// As with `from`, but append a breadcrumb while the value remains
    /// unknown.
    function with_message<T>(data: unknown, message: string) -> T | UnknownError throws never {
        match (data) {
            let value: T => {
                UnknownError._preserve_context(data, value);
                value
            },
            UnknownError { data: let inner: T } => {
                UnknownError._preserve_context(data, inner);
                inner
            },
            let error: UnknownError => {
                error.message = error.message.concat([message]);
                UnknownError._preserve_context(data, error);
                error
            },
            _ => {
                let error = UnknownError { data: data, message: [message] };
                UnknownError._preserve_context(data, error);
                error
            },
        }
    }

    //baml:mut_vm
    function _preserve_context(source: unknown, target: unknown) -> null throws never {
        $rust_function
    }

    implements baml.ToString {
        function to_string(self) -> string throws never {
            let detail = self.data.to_string();
            if (self.message.length() == 0) {
                `UnknownError: ${detail}`
            } else {
                `UnknownError: ${self.message.join(": ")}: ${detail}`
            }
        }
    }
}
