function root_help() -> null {
    baml.io.println(
        `
            BAML

            Usage: baml [--project <path>] <command> [args...]

            Wrapper commands:
              toolchain     Manage language toolchains
              self-update   Update the wrapper executable

            CLI commands (provided by the selected toolchain):
              init, new, check, fmt, generate, test, run, pack, describe, query,
              auth, feedback, ide, agent, playground, lsp, telemetry, clean, update, upgrade

            Options:
              --project <path>  Select a filesystem project
              -h, --help       Show help
              -V, --version    Report wrapper and selected CLI versions

            Use baml help <command> for command help.
        `,
    );
    null
}

function help(command: string = "") -> null {
    let usage = match (command) {
        "use" => "use [-g|--global] <selector>",
        "install" => "install [<selector>] [--force]",
        "update" => "update",
        "status" => "status [--install] [--json]",
        "list" => "list",
        "uninstall" => "uninstall <version>",
        _ => null,
    };
    if let usage: string = usage {
        baml.io.println(`Usage: baml toolchain ${usage}`);
        baml.io.println("Options: -h, --help");
        if (command == "status") {
            baml.io.println(
                "Emit the active selector, concrete version, source, paths and installation state as JSON. --install installs an absent managed toolchain first.",
            );
        }
        return null;
    }
    baml.io.println(
        `
            BAML toolchain management

            Commands:
              use [-g|--global] <selector>  Install if needed and select in the closest baml.toml
                                       -g, --global selects in $BAML_HOME/config.toml instead
              install [<selector>] [--force]
              update
              status [--install] [--json]
              list
              uninstall <version>

            Selectors: canary, nightly, exact release version, or a local baml-cli path
            Wrapper updates: baml self-update
        `,
    );
    null
}

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

function main() -> void throws never {
    let argv = baml.sys.argv();
    let args = argv.slice(2, argv.length());
    let telemetry: WrapperTelemetry? = null;
    if (args != [FLUSH_TELEMETRY_COMMAND] && args.at(0) != "--replace") {
        let directory = baml.sys.current_dir() catch (error) {
            _ => ""
        };
        telemetry = WrapperTelemetry.begin(args, directory);
    }
    defer { WrapperTelemetry.finish(telemetry); }
    // Retain command outcomes without recording arguments or error payloads.
    dispatch(telemetry, $trace = trace.span(inputs = false, output = false, error = false)) catch (error) {
        _ => {
            telemetry_event(telemetry, "wrapper_unhandled_error", fields = { "event_kind": "unhandled_error" });
            baml.io.eprintln(`error: ${error_message(error)}`);
            baml.sys.exit(1);
        },
    };
    baml.sys.exit(0);
}

function dispatch(telemetry: WrapperTelemetry?) -> null {
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
    let paths = WrapperInvocation.parse(
        forwarded,
        $trace = trace.span(inputs = false, output = false, error = false),
    );
    let args = paths.args;
    paths.telemetry = telemetry;
    if (args.length() == 0 || args == ["--help"] || args == ["-h"] || args == ["help"]) {
        root_help();
        return null;
    }
    if (args[0] == "help" && args.at(1) == "toolchain") {
        paths.management(args.slice(2, args.length()).concat(["--help"]));
        return null;
    }
    if (args[0] == "help" && args.at(1) == "self-update") {
        ensure(args.length() == 2, "usage: baml help self-update");
        baml.io.println(SELF_UPDATE_HELP);
        return null;
    }
    match (args[0]) {
        "--version" | "-V" => {
            paths.print_version(args[0]);
            return null;
        },
        "toolchain" => {
            paths.management(args.slice(1, args.length()));
            return null;
        },
        "self-update" => {
            if (args == ["self-update", "--help"] || args == ["self-update", "-h"]) {
                baml.io.println(SELF_UPDATE_HELP);
                return null;
            }
            ensure(args.length() == 1, "unexpected self-update arguments");
            WrapperUpdater.update(
                telemetry = paths.telemetry,
                $trace = trace.span(inputs = false, output = false, error = false),
            );
            return null;
        },
        _ => {},
    }
    let active = paths.selection();
    let binary = active.ensure_installed(telemetry = paths.telemetry);
    let warned = active.warn_if_outdated();
    let refresh = spawn { active.refresh_channel() };
    WrapperTelemetry.finish(paths.telemetry);
    let result = baml.sys.run(binary, args = forwarded, env = { "BAML_CLI_ALLOW_DIRECT": "1" });
    await refresh;
    if (!warned) {
        active.warn_if_outdated();
    }
    baml.sys.exit(result.exit_code)
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

const SELF_UPDATE_HELP = "baml self-update: update the wrapper executable independently of language toolchains; use baml toolchain status to check language updates";
