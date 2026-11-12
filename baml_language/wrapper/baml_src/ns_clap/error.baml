// Why a command line could not be parsed, or what it asked to be shown.
// As in clap, a request for help or the version arrives as an error:
// its `kind` says which, and `exit_code` is zero for those.

enum ErrorKind {
    // A value was missing, or not one of the possible values.
    InvalidValue,
    // An option or positional the command does not have.
    UnknownArgument,
    // A word where a subcommand was expected that is not one.
    InvalidSubcommand,
    // A value attached to an option that takes none.
    TooManyValues,
    // Two args that exclude each other, or one given twice.
    ArgumentConflict,
    MissingRequiredArgument,
    MissingSubcommand,
    DisplayHelp,
    DisplayVersion,
}

// The exit status of a command line that could not be parsed.
const USAGE_EXIT_CODE = 2;

class Error {
    kind: ErrorKind,
    // What to print: the help or version text, or the problem with its
    // tips and usage. It does not start with `error:`.
    message: string,
    // The subcommands leading to the command that raised it, outermost first.
    command_path: string[],

    // Whether this is a failure, to be printed on stderr.
    function use_stderr(self) -> bool {
        match (self.kind) {
            ErrorKind.DisplayHelp | ErrorKind.DisplayVersion => false,
            _ => true,
        }
    }

    function exit_code(self) -> int {
        if (self.use_stderr()) {
            USAGE_EXIT_CODE
        } else {
            0
        }
    }

    implements baml.ToString {
        function to_string(self) -> string {
            self.message
        }
    }

    function _display(cmd: Command, kind: ErrorKind, text: string) -> Error {
        Error { kind: kind, message: text, command_path: cmd.path }
    }

    // A failure: the problem, any tips, the usage when there is one to
    // show, and where to look next.
    function _failure(
        cmd: Command,
        kind: ErrorKind,
        problem: string,
        tips: string[],
        usage: string?,
    ) -> Error {
        let tip_lines = tips.map((tip: string) -> string {
            `  tip: ${tip}`
        });
        let sections = [
            problem,
            if (tip_lines) {
                tip_lines.join("\n")
            } else {
                null
            },
            match (usage) {
                let usage: string => `Usage: ${usage}`,
                null => null,
            },
            Error._help_pointer(cmd),
        ];
        Error {
            kind: kind,
            message: sections
                .filter_map((section: string?) -> string? {
                    section
                })
                .join("\n\n"),
            command_path: cmd.path,
        }
    }

    // Where to find help, when the command has a way to ask for it.
    function _help_pointer(cmd: Command) -> string? {
        let asked_with = if (!cmd.help_flag_disabled) {
            "--help"
        } else if (cmd.has_builtin_help_subcommand()) {
            "help"
        } else {
            return null;
        };
        `For more information, try '${asked_with}'.`
    }

    function unknown_argument(cmd: Command, arg: string, tips: string[], usage: string) -> Error {
        Error._failure(
            cmd,
            ErrorKind.UnknownArgument,
            `unexpected argument '${arg}' found`,
            tips,
            usage,
        )
    }

    function unrecognized_subcommand(cmd: Command, name: string) -> Error {
        Error._failure(
            cmd,
            ErrorKind.InvalidSubcommand,
            `unrecognized subcommand '${name}'`,
            [],
            Usage.of(cmd),
        )
    }

    function invalid_subcommand(cmd: Command, name: string, similar: string[]) -> Error {
        let quoted = similar
            .map((candidate: string) -> string {
                `'${candidate}'`
            })
            .join(", ");
        let tip = if (similar.length() == 1) {
            `a similar subcommand exists: ${quoted}`
        } else {
            `some similar subcommands exist: ${quoted}`
        };
        Error._failure(
            cmd,
            ErrorKind.InvalidSubcommand,
            `unrecognized subcommand '${name}'`,
            [tip],
            Usage.of(cmd),
        )
    }

    function empty_value(cmd: Command, arg: Arg) -> Error {
        Error._failure(
            cmd,
            ErrorKind.InvalidValue,
            `a value is required for '${arg}' but none was supplied${Error._possible_values(arg)}`,
            [],
            null,
        )
    }

    // An empty value reads as none supplied.
    function invalid_value(cmd: Command, arg: Arg, value: string) -> Error {
        if (value.is_empty()) {
            return Error.empty_value(cmd, arg);
        }
        let tips = match (Suggestions.did_you_mean(value, arg.possible).at(-1)) {
            let similar: string => [`a similar value exists: '${similar}'`],
            null => [],
        };
        Error._failure(
            cmd,
            ErrorKind.InvalidValue,
            `invalid value '${value}' for '${arg}'${Error._possible_values(arg)}`,
            tips,
            null,
        )
    }

    function _possible_values(arg: Arg) -> string {
        if (arg.possible) {
            `\n  [possible values: ${arg.possible.join(", ")}]`
        } else {
            ""
        }
    }

    // `used` are the args given so far, this one included.
    function too_many_values(cmd: Command, arg: Arg, value: string, used: string[]) -> Error {
        Error._failure(
            cmd,
            ErrorKind.TooManyValues,
            `unexpected value '${value}' for '${arg}' found; no more were expected`,
            [],
            Usage.with(cmd, used, Usage.required_ids(cmd)),
        )
    }

    // `arg` was given more than once.
    function argument_repeated(cmd: Command, arg: Arg) -> Error {
        Error._failure(
            cmd,
            ErrorKind.ArgumentConflict,
            `the argument '${arg}' cannot be used multiple times`,
            [],
            Usage.of(cmd),
        )
    }

    function argument_conflict(cmd: Command, arg: Arg, others: Arg[], usage: string) -> Error {
        let problem = match (others.length()) {
            1 => `the argument '${arg}' cannot be used with '${others[0]}'`,
            _ => {
                let listed = others
                    .map((other: Arg) -> string {
                        `\n  ${other}`
                    })
                    .join("");
                `the argument '${arg}' cannot be used with:${listed}`
            },
        };
        Error._failure(cmd, ErrorKind.ArgumentConflict, problem, [], usage)
    }

    function missing_required_argument(cmd: Command, missing: Arg[], usage: string) -> Error {
        let listed = missing
            .map((arg: Arg) -> string {
                `\n  ${arg}`
            })
            .join("");
        Error._failure(
            cmd,
            ErrorKind.MissingRequiredArgument,
            `the following required arguments were not provided:${listed}`,
            [],
            usage,
        )
    }

    function missing_subcommand(cmd: Command) -> Error {
        let subcommands = cmd.subcommand_list
            .map((sub: Command) -> string {
                sub.name
            })
            .join(", ");
        Error._failure(
            cmd,
            ErrorKind.MissingSubcommand,
            `'${cmd.get_bin_name()}' requires a subcommand but one was not provided\n  [subcommands: ${subcommands}]`,
            [],
            Usage.of(cmd),
        )
    }
}
