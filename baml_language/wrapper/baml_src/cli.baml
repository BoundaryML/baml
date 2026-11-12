// The wrapper's command line. The wrapper owns `toolchain` and `self`;
// every other command belongs to the selected toolchain's CLI and is
// forwarded with its tokens untouched.

const PROJECT_DIR = "project-dir";

const OUTPUT_FORMAT = "output-format";

// The CLI's other global options that take a value. The wrapper never
// reads them; it declares them to find the command that follows them.
const CLI_VALUE_OPTIONS = ["output-preset", "color", "hyperlinks", "diagnostic-format", "agent-skill-check"];

// The CLI commands telemetry may name. Any other forwarded command is
// recorded as plain `baml`, so a typo or a private name is never sent.
const CLI_COMMANDS = [
    "check",
    "run",
    "test",
    "generate",
    "fmt",
    "lsp",
    "playground",
    "init",
    "new",
    "pack",
    "describe",
    "codequality",
    "local",
    "cloud",
    "query",
    "login",
    "logout",
    "ide",
    "agent",
    "telemetry",
];

const ROOT_HELP = `
    BAML

    Usage: baml [--project-dir <path>] <command> [args...]

    Wrapper commands:
      toolchain     Manage language toolchains
      self update   Update the wrapper executable

    CLI commands (provided by the selected toolchain):
      new, init, check, fmt, test, run, pack, generate, describe, codequality,
      playground, local, cloud, query, login, logout, ide, agent, telemetry, lsp

    Options:
      --project-dir <path>  Select a filesystem project
      -h, --help            Show help
      -V, --version         Report wrapper and selected CLI versions

    Use baml help <command> for command help.
`;

enum OutputFormat {
    Text,
    Json,
}

// The commands the wrapper runs itself.
const WRAPPER_COMMANDS = ["toolchain", "self"];

class WrapperCli {
    function command() -> root.clap.Command {
        let command = root.clap.Command.new("baml")
            .override_help(ROOT_HELP)
            .external_subcommand_passthrough(true)
            .disable_help_subcommand(true)
            .arg(
                root.clap.Arg.new(PROJECT_DIR)
                    .long(PROJECT_DIR)
                    .value_name("PATH")
                    .help("Select a filesystem project")
                    .global(true),
            )
            .arg(WrapperCli.output_format_arg())
            .arg(
                root.clap.Arg.new("version").long("version").short("V").action(
                    root.clap.ArgAction.SetTrue,
                ),
            )
            .subcommand(WrapperCli.toolchain())
            .subcommand(WrapperCli.self_command());
        CLI_VALUE_OPTIONS.reduce(
            (spec: root.clap.Command, name: string) -> root.clap.Command {
                spec.arg(root.clap.Arg.new(name).long(name).global(true).hide(true))
            },
            command,
        )
    }

    function output_format_arg() -> root.clap.Arg {
        root.clap.Arg.new(OUTPUT_FORMAT)
            .long(OUTPUT_FORMAT)
            .value_name("FORMAT")
            .help("What goes to stdout [possible values: text, json]")
            .global(true)
    }

    function toolchain() -> root.clap.Command {
        root.clap.Command.new("toolchain")
            .about("Manage BAML toolchains")
            .after_help(
                `
                    Selectors: canary, nightly, exact release version, or a local baml-cli path
                    Wrapper updates: baml self update
                `,
            )
            .subcommand(
                root.clap.Command.new("use")
                    .about("Install if needed and select in the closest baml.toml")
                    .arg(
                        root.clap.Arg.new("selector").value_name("SELECTOR").required(true).help(
                            "canary, nightly, an exact release version, or a local baml-cli path",
                        ),
                    )
                    .arg(
                        root.clap.Arg.new("global")
                            .long("global")
                            .short("g")
                            .action(root.clap.ArgAction.SetTrue)
                            .help("Set the user default in $BAML_HOME/config.toml instead"),
                    ),
            )
            .subcommand(
                root.clap.Command.new("install")
                    .about("Download a managed toolchain without changing the selection")
                    .arg(
                        root.clap.Arg.new("selector").value_name("SELECTOR").help(
                            "Defaults to the active selection",
                        ),
                    )
                    .arg(
                        root.clap.Arg.new("force")
                            .long("force")
                            .action(root.clap.ArgAction.SetTrue)
                            .help("Replace an existing installation"),
                    ),
            )
            .subcommand(
                root.clap.Command.new("update").about("Advance the resolved machine channel"),
            )
            .subcommand(
                root.clap.Command.new("status")
                    .about("Check the active selection and available releases")
                    .arg(
                        root.clap.Arg.new("install")
                            .long("install")
                            .action(root.clap.ArgAction.SetTrue)
                            .help("Install the selected managed toolchain if it is absent"),
                    ),
            )
            .subcommand(
                root.clap.Command.new("list").about(
                    "Show the local inventory after attempting active-toolchain installation",
                ),
            )
            .subcommand(
                root.clap.Command.new("uninstall")
                    .about("Remove a concrete version, preserving selectors and channel mappings")
                    .arg(
                        root.clap.Arg.new("version").value_name("VERSION").required(true).help(
                            "An exact release version",
                        ),
                    ),
            )
    }

    function self_command() -> root.clap.Command {
        root.clap.Command.new("self").about("Manage the wrapper executable").subcommand(
            root.clap.Command.new("update")
                .about("Update the wrapper executable independently of language toolchains")
                .after_help("Use baml toolchain status to check language updates."),
        )
    }

    // The CLI's `--output-format` also takes values only it understands,
    // so the wrapper checks the value where one of its own commands reads it.
    function output_format(matches: root.clap.ArgMatches) -> OutputFormat throws root.clap.Error {
        match (matches.get_one(OUTPUT_FORMAT) ?? "text") {
            "text" => OutputFormat.Text,
            "json" => OutputFormat.Json,
            let other: string => {
                let command = WrapperCli.command();
                command.build();
                throw root.clap.Error.invalid_value(
                    command,
                    WrapperCli.output_format_arg().value_parser(["text", "json"]),
                    other,
                )
            },
        }
    }

    // Help for one of the wrapper's own commands, as `baml help
    // <command>...` asks for it: the same as `<command>... --help`.
    function help_for(names: string[]) -> string throws root.clap.Error {
        let shown = WrapperCli.command().try_get_matches_from(names.concat(["--help"])) catch (display) {
            root.clap.Error => {
                if (display.use_stderr()) {
                    throw display;
                }
                display.message
            },
        };
        match (shown) {
            let text: string => text,
            _ => ROOT_HELP,
        }
    }

    // The canonical name telemetry records for an invocation. A command
    // line that fails to parse is named after the command that rejected it.
    function command_name(args: string[]) -> string {
        let parsed = WrapperCli.command().try_get_matches_from(args) catch (error) {
            root.clap.Error => error,
        };
        match (parsed) {
            let error: root.clap.Error => WrapperCli._name_of(error.command_path, false),
            let matches: root.clap.ArgMatches => match (matches.sub) {
                null => "baml",
                let sub: root.clap.SubCommand => {
                    if (!sub.matches.is_external()) {
                        WrapperCli._name_of(matches.subcommand_path(), false)
                    } else if (sub.name == "help") {
                        WrapperCli._help_name(sub.matches.external_args())
                    } else {
                        WrapperCli._name_of([sub.name], true)
                    }
                },
            },
        }
    }

    // `baml help <wrapper command>...` is named after that command.
    function _help_name(names: string[]) -> string {
        if (WRAPPER_COMMANDS.includes(names.at(0) ?? "")) {
            WrapperCli.command_name(names)
        } else {
            "baml"
        }
    }

    function _name_of(path: string[], external: bool) -> string {
        match (path.at(0)) {
            null => "baml",
            let command: string => {
                if (external || !WRAPPER_COMMANDS.includes(command)) {
                    if (CLI_COMMANDS.includes(command)) {
                        `baml ${command}`
                    } else {
                        "baml"
                    }
                } else if (command == "self") {
                    "baml self update"
                } else {
                    ["baml"].concat(path.slice(0, 2)).join(" ")
                }
            },
        }
    }
}
