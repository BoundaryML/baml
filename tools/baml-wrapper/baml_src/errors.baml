// Recoverable errors crossing the wrapper's internal operation interfaces.
// Entry points report them as diagnostics; passive checks and telemetry handle
// only the failures they can recover from.
type WrapperFailure = baml.errors.Io
    | baml.errors.Timeout
    | baml.errors.InvalidArgument
    | baml.errors.ParseError
    | baml.toml.ParseError
    | baml.json.ParseError
    | baml.json.DecodeError
    | baml.json.SerializationError
    | baml.regex.Error
    | InvalidToolchainVersion
    | UnexpectedManagementArgument
    | UnknownManagementCommand
    | MissingConfigurationField
    | ProjectNotFound;

// Expected filesystem, configuration, serialization, and transport failures
// are best effort for telemetry. Other failures are not silently discarded.
type TelemetryFailure = baml.errors.Io
    | baml.errors.Timeout
    | baml.errors.InvalidArgument
    | baml.errors.ParseError
    | baml.toml.ParseError
    | baml.json.ParseError
    | baml.json.DecodeError
    | baml.json.SerializationError
    | baml.regex.Error;

class MissingConfigurationField {
    path: string,
    fields: string[],

    implements baml.ToString {
        function to_string(self) -> string {
            `${self.path}: ${self.fields.join(".")} is required`
        }
    }
}

class ProjectNotFound {
    directory: string,

    implements baml.ToString {
        function to_string(self) -> string {
            "no baml.toml found; run inside a BAML project or use: baml toolchain use --global <selector>"
        }
    }
}
