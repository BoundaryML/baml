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

class WrapperCli {
    function spec() -> root.clap.Command {
        let command = root.clap.Command.new("baml")
            .help_text(ROOT_HELP)
            .allow_external_subcommands()
            .arg(
                root.clap.Arg.opt(PROJECT_DIR)
                    .with_value_name("PATH")
                    .help("Select a filesystem project")
                    .global(),
            )
            .arg(WrapperCli.output_format_arg())
            .arg(root.clap.Arg.flag("version").with_short("V"))
            .subcommand(WrapperCli.toolchain())
            .subcommand(WrapperCli.self_command());
        CLI_VALUE_OPTIONS.reduce(
            (spec: root.clap.Command, name: string) -> root.clap.Command {
                spec.arg(root.clap.Arg.opt(name).global().hidden())
            },
            command,
        )
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
                        root.clap.Arg.pos("selector").required().help(
                            "canary, nightly, an exact release version, or a local baml-cli path",
                        ),
                    )
                    .arg(
                        root.clap.Arg.flag("global").with_short("g").help(
                            "Set the user default in $BAML_HOME/config.toml instead",
                        ),
                    ),
            )
            .subcommand(
                root.clap.Command.new("install")
                    .about("Download a managed toolchain without changing the selection")
                    .arg(root.clap.Arg.pos("selector").help("Defaults to the active selection"))
                    .arg(root.clap.Arg.flag("force").help("Replace an existing installation")),
            )
            .subcommand(
                root.clap.Command.new("update").about("Advance the resolved machine channel"),
            )
            .subcommand(
                root.clap.Command.new("status")
                    .about("Check the active selection and available releases")
                    .arg(
                        root.clap.Arg.flag("install").help(
                            "Install the selected managed toolchain if it is absent",
                        ),
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
                    .arg(root.clap.Arg.pos("version").required().help("An exact release version")),
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
    function output_format(
        matches: root.clap.Matches,
    ) -> OutputFormat throws root.clap.InvalidValueError {
        match (matches.get(OUTPUT_FORMAT) ?? "text") {
            "text" => OutputFormat.Text,
            "json" => OutputFormat.Json,
            let other: string => {
                throw root.clap.InvalidValueError.not_possible(
                    WrapperCli.output_format_arg().value_parser(["text", "json"]),
                    other,
                    matches.command,
                )
            },
        }
    }

    function output_format_arg() -> root.clap.Arg {
        root.clap.Arg.opt(OUTPUT_FORMAT)
            .with_value_name("FORMAT")
            .help("What goes to stdout [possible values: text, json]")
            .global()
    }

    // The canonical name telemetry records for an invocation. Read
    // leniently, so an invocation that fails to parse is still named.
    function command_name(args: string[]) -> string {
        let path = WrapperCli.spec().command_path(args);
        let asks_for_help = path.at(0) == "help";
        let named = if (asks_for_help) {
            path.slice(1, path.length())
        } else {
            path
        };
        match (named.at(0)) {
            null => "baml",
            "toolchain" => ["baml", "toolchain"].concat(named.slice(1, 2)).join(" "),
            "self" => "baml self update",
            let command: string => {
                if (!asks_for_help && CLI_COMMANDS.includes(command)) {
                    `baml ${command}`
                } else {
                    "baml"
                }
            },
        }
    }
}
