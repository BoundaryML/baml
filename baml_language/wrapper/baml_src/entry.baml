function fail(message: string) -> never throws baml.errors.InvalidArgument {
    throw baml.errors.InvalidArgument { message: message }
}

/// Parameterless packed entry point owns its complete argv. The pack host adds
/// the entry identifier at argv[1]; forward every subsequent token unchanged.
function error_message(error: unknown) -> string {
    match (error) {
        baml.errors.InvalidArgument { message }
            | baml.errors.Io { message }
            | baml.errors.ParseError { message }
            | baml.toml.ParseError { message }
            | baml.json.DecodeError { message }
            | baml.json.SerializationError { message }
            | baml.errors.Timeout { message } =>
        {
            message
        },
        _ => error.to_string(),
    }
}

// Exit statuses, as the toolchain's CLI uses them.
const EXIT_FAILURE = 1;

const EXIT_USAGE = 2;

function main() -> void throws never {
    let argv = baml.sys.argv();
    let args = argv.slice(2, argv.length());
    let telemetry: WrapperTelemetry? = null;
    let flush_executable: string? = null;
    let internal_helper = args == [FLUSH_TELEMETRY_COMMAND] || args.at(0) == "--replace";
    if (!internal_helper) {
        let directory = baml.sys.current_dir() catch (error) {
            _ => ""
        };
        telemetry = WrapperTelemetry.begin(args, directory);
    }
    defer {
        // A rejected drain must retain its queue for a later invocation, without
        // scheduling another helper that immediately retries the same rejection.
        if (!internal_helper) { WrapperTelemetry.finish(telemetry, executable = flush_executable); }
    }
    // Retain command outcomes without recording arguments or error payloads.
    flush_executable
        = dispatch(telemetry, $trace = trace.span(inputs = false, output = false, error = false)) catch (error) {
            _ => {
                telemetry_event(telemetry, "wrapper_unhandled_error", fields = { "event_kind": "unhandled_error" });
                baml.io.eprintln(`error: ${error_message(error)}`);
                baml.sys.exit(
                    if (error is root.clap.ParseError) {
                        EXIT_USAGE
                    } else {
                        EXIT_FAILURE
                    },
                )
            },
        };
    baml.sys.exit(0);
}

// Self-update returns a copy safe for deferred telemetry delivery.
function dispatch(telemetry: WrapperTelemetry?) -> string? {
    let argv = baml.sys.argv();
    let forwarded = argv.slice(2, argv.length());
    if (forwarded == [FLUSH_TELEMETRY_COMMAND]) {
        TelemetryQueue.new().drain();
        return null;
    }
    if (forwarded.at(0) == "--replace") {
        WrapperUpdater.replace(forwarded.slice(1, forwarded.length()));
        return null;
    }
    let parsed = WrapperInvocation.parse(
        forwarded,
        $trace = trace.span(inputs = false, output = false, error = false),
    );
    let invocation = match (parsed) {
        let help: root.clap.DisplayHelp => {
            baml.io.println(help.text);
            return null;
        },
        let invocation: WrapperInvocation => invocation,
    };
    invocation.telemetry = telemetry;
    let matches = invocation.matches;
    match (matches.sub_match) {
        null => {
            if (!matches.is_present("version")) {
                baml.io.println(ROOT_HELP);
                return null;
            }
            // Forward the spelling the user typed: the CLI answers to both.
            invocation.print_version(
                forwarded.find((arg) -> bool {
                    arg == "-V"
                })
                    ?? "--version",
            );
            null
        },
        let sub: root.clap.SubMatch => {
            if (sub.is_external) {
                return invocation.launch(forwarded);
            }
            match (sub.matches.sub_match) {
                // `baml toolchain` or `baml self` alone: say what it can do.
                null => {
                    baml.io.println(WrapperCli.spec().help_for([sub.name], "baml"));
                    null
                },
                let action: root.clap.SubMatch => match (sub.name) {
                    "toolchain" => {
                        invocation.manage(management_command(action.name, action.matches))
                    },
                    _ => {
                        WrapperUpdater.update(
                            telemetry = invocation.telemetry,
                            $trace = trace.span(inputs = false, output = false, error = false),
                        )
                    },
                },
            }
        },
    }
}

function describe_selector(value: string) -> string {
    if (channel(value)) {
        let active = ChannelState.new().recorded(value) catch (error) {
            WrapperFailure => null
        };
        if let active: string = active {
            return `${value} (${active})`;
        }
    }
    value
}

function previous_selector(original: string, global: bool) -> string {
    let doc = baml.toml.Table.parse(original);
    if (global) {
        if let defaults: baml.toml.Table = table_at(doc, "default", config_path()) {
            return describe_selector(string_at(defaults, "selector", config_path()) ?? "canary");
        }
        return describe_selector("canary");
    }
    if let toolchain: baml.toml.Table = table_at(doc, "toolchain", "baml.toml") {
        for (let key in ["version", "channel", "path"]) {
            if let value: string = string_at(toolchain, key, "baml.toml") {
                return describe_selector(value);
            }
        }
    }
    "not set"
}
