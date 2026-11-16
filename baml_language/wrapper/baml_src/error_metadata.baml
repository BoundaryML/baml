// These labels are the entire wrapper error schema. Never derive a label from
// an error message, serialized exception, command argument, path, or class name.
type WrapperErrorStage = "dispatch"
    | "parse"
    | "management_command"
    | "preflight"
    | "execute"
    | "selection"
    | "acquisition"
    | "status_acquisition"
    | "freshness"
    | "refresh"
    | "version"
    | "self_update"
    | "child_launch";

type WrapperErrorCategory = "usage"
    | "invalid_selection"
    | "project_not_found"
    | "configuration"
    | "io"
    | "timeout"
    | "invalid_argument"
    | "other";

type WrapperErrorDisposition = "propagated" | "recovered" | "terminal";

// Call hidden: classification inspects only built-in/known wrapper types and
// does not put the exception into a trace, even when automatic spans are on.
function wrapper_error_category(error: unknown) -> WrapperErrorCategory throws never {
    match (error) {
        root.clap.Error => "usage",
        InvalidToolchainVersion => "invalid_selection",
        ProjectNotFound => "project_not_found",
        MissingConfigurationField
            | baml.errors.ParseError
            | baml.toml.ParseError
            | baml.json.ParseError
            | baml.json.DecodeError
            | baml.json.SerializationError =>
        {
            "configuration"
        },
        baml.errors.Io => "io",
        baml.errors.Timeout => "timeout",
        baml.errors.InvalidArgument | baml.regex.Error => "invalid_argument",
        _ => "other",
    }
}

function record_wrapper_error(
    stage: WrapperErrorStage,
    category: WrapperErrorCategory,
    disposition: WrapperErrorDisposition,
) -> null throws never {
    wrapper_error(
        $trace = trace.empty_span().context(
            metadata = {
                "wrapper.error.stage": stage,
                "wrapper.error.category": category,
                "wrapper.error.disposition": disposition,
            },
        ),
    )
}

// A parameterless diagnostic marker, with no input/output/error value capture.
// It inherits existing command/version/caller context, adding only the labels.
function wrapper_error() -> null throws never {
    null
}
