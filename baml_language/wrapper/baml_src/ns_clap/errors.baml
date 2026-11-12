// Parse failures for the clap-style parser: one class per failure
// kind, closed under the ParseError union so `match` stays
// exhaustive. Every member implements Error, so callers can read
// `argument`, `detail`, `command` and message() on the union directly.
// Constructors render `detail` from the structured fields, so all
// message wording lives here. DisplayHelp/DisplayVersion are not
// failures: they carry rendered text for --help/--version.

interface Error {
    argument: string,
    detail: string,
    command: string,

    function message(self) -> string throws never {
        self.detail
    }
}

class UnknownArgumentError {
    argument: string,
    detail: string,
    command: string,

    function new(argument: string, command: string) -> UnknownArgumentError {
        UnknownArgumentError {
            argument: argument,
            detail: `unexpected argument '${argument}' found`,
            command: command,
        }
    }

    implements Error {}

    implements baml.ToString {
        function to_string(self) -> string {
            self.detail
        }
    }
}

class UnknownCommandError {
    argument: string,
    detail: string,
    command: string,

    function new(argument: string, command: string) -> UnknownCommandError {
        UnknownCommandError {
            argument: argument,
            detail: `unrecognized command '${argument}'`,
            command: command,
        }
    }

    function missing(command: string) -> UnknownCommandError {
        UnknownCommandError { argument: "", detail: "no command given", command: command }
    }

    implements Error {}

    implements baml.ToString {
        function to_string(self) -> string {
            self.detail
        }
    }
}

class MissingValueError {
    argument: string,
    detail: string,
    command: string,

    function new(arg: Arg, command: string) -> MissingValueError {
        MissingValueError {
            argument: arg.name,
            detail: `a value is required for '${arg.display_name()}' but none was supplied`,
            command: command,
        }
    }

    implements Error {}

    implements baml.ToString {
        function to_string(self) -> string {
            self.detail
        }
    }
}

class MissingRequiredError {
    argument: string,
    detail: string,
    command: string,

    function new(arg: Arg, command: string) -> MissingRequiredError {
        MissingRequiredError {
            argument: arg.name,
            detail: `the following required arguments were not provided: ${arg.display_name()}`,
            command: command,
        }
    }

    function required_by(needed: string, by: Arg, command: string) -> MissingRequiredError {
        MissingRequiredError {
            argument: needed,
            detail: `the following required arguments were not provided: --${needed} (required by ${by.display_name()})`,
            command: command,
        }
    }

    implements Error {}

    implements baml.ToString {
        function to_string(self) -> string {
            self.detail
        }
    }
}

class InvalidValueError {
    argument: string,
    value: string,
    detail: string,
    command: string,

    function not_possible(arg: Arg, value: string, command: string) -> InvalidValueError {
        InvalidValueError {
            argument: arg.name,
            value: value,
            detail: `invalid value '${value}' for '${arg.display_name()}': expected one of ${arg.choices.join(", ")}`,
            command: command,
        }
    }

    function no_value_allowed(arg: Arg, value: string, command: string) -> InvalidValueError {
        InvalidValueError {
            argument: arg.name,
            value: value,
            detail: `unexpected value '${value}' for '--${arg.name}': no value is allowed`,
            command: command,
        }
    }

    function not_an_int(argument: string, value: string, command: string) -> InvalidValueError {
        InvalidValueError {
            argument: argument,
            value: value,
            detail: `invalid value '${value}' for '${argument}': expected an integer`,
            command: command,
        }
    }

    implements Error {}

    implements baml.ToString {
        function to_string(self) -> string {
            self.detail
        }
    }
}

class DuplicateArgumentError {
    argument: string,
    detail: string,
    command: string,

    function new(arg: Arg, command: string) -> DuplicateArgumentError {
        DuplicateArgumentError {
            argument: arg.name,
            detail: `the argument '${arg.display_name()}' was provided more than once`,
            command: command,
        }
    }

    implements Error {}

    implements baml.ToString {
        function to_string(self) -> string {
            self.detail
        }
    }
}

class ConflictError {
    argument: string,
    other: string,
    detail: string,
    command: string,

    // `other_arg` is the declared arg named `other`, used for its
    // display name; null falls back to the bare name.
    function new(arg: Arg, other: string, other_arg: Arg?, command: string) -> ConflictError {
        let other_name = match (other_arg) {
            null => other,
            let found: Arg => found.display_name(),
        };
        ConflictError {
            argument: arg.name,
            other: other,
            detail: `the argument '${arg.display_name()}' cannot be used with '${other_name}'`,
            command: command,
        }
    }

    implements Error {}

    implements baml.ToString {
        function to_string(self) -> string {
            self.detail
        }
    }
}

class UnexpectedPositionalError {
    argument: string,
    detail: string,
    command: string,

    function new(argument: string, command: string) -> UnexpectedPositionalError {
        UnexpectedPositionalError {
            argument: argument,
            detail: `unexpected argument '${argument}' found`,
            command: command,
        }
    }

    implements Error {}

    implements baml.ToString {
        function to_string(self) -> string {
            self.detail
        }
    }
}

// Closed union: `match` over ParseError is exhaustive. Each member
// carries structured fields plus pre-rendered detail text.
type ParseError = UnknownArgumentError
    | UnknownCommandError
    | MissingValueError
    | MissingRequiredError
    | InvalidValueError
    | DuplicateArgumentError
    | ConflictError
    | UnexpectedPositionalError;

class DisplayHelp {
    text: string,
}

class DisplayVersion {
    text: string,
}
