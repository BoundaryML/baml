// What a command prints about itself: its usage line and its help,
// laid out the way clap lays them out.

class Usage {
    // `<name> [OPTIONS] <REQUIRED> [OPTIONAL]... [COMMAND]`.
    function of(cmd: Command) -> string {
        let optional_positionals = cmd.positionals()
            .filter((arg: Arg) -> bool {
                !arg.is_required
            })
            .map((arg: Arg) -> string {
                arg.to_string()
            });
        let subcommand = if (!cmd.has_visible_subcommands() && !cmd.externals_allowed) {
            []
        } else if (cmd.is_subcommand_required) {
            ["<COMMAND>"]
        } else {
            ["[COMMAND]"]
        };
        [cmd.usage_name]
            .concat(
                if (Usage.needs_options_tag(cmd)) {
                    ["[OPTIONS]"]
                } else {
                    []
                },
            )
            .concat(Usage.required(cmd))
            .concat(optional_positionals)
            .concat(subcommand)
            .join(" ")
    }

    // The args a command line must have: required options, then required positionals.
    function required(cmd: Command) -> string[] {
        cmd.options()
            .concat(cmd.positionals())
            .filter((arg: Arg) -> bool {
                arg.is_required && !arg.is_hidden
            })
            .map((arg: Arg) -> string {
                arg.to_string()
            })
    }

    // `[OPTIONS]` stands for the options a user may leave out, so not
    // for required or hidden ones, and not for help and version alone.
    function needs_options_tag(cmd: Command) -> bool {
        cmd.options().some((arg: Arg) -> bool {
            !arg.is_builtin() && !arg.is_hidden && !arg.is_required
        })
    }
}

class Help {
    function of(cmd: Command) -> string {
        match (cmd.help_override) {
            let text: string => text,
            null => Help._generated(cmd),
        }
    }

    function _generated(cmd: Command) -> string {
        let commands = cmd.subcommand_list
            .filter((sub: Command) -> bool {
                !sub.is_hidden
            })
            .map((sub: Command) -> string[] {
                [sub.name, sub.about_text ?? ""]
            });
        let arguments = cmd.positionals()
            .filter((arg: Arg) -> bool {
                !arg.is_hidden
            })
            .map((arg: Arg) -> string[] {
                [arg.to_string(), arg.help_annotated()]
            });
        let options = cmd.options()
            .filter((arg: Arg) -> bool {
                !arg.is_hidden
            })
            .sort_by_key((arg: Arg) -> string {
                arg.sort_key()
            });
        let option_rows = options.map((arg: Arg) -> string[] {
            [arg.help_spec(), arg.help_annotated()]
        });
        let sections = [
            cmd.about_text,
            `Usage: ${Usage.of(cmd)}`,
            Help._section("Commands", commands),
            Help._section("Arguments", arguments),
            Help._section("Options", option_rows),
            cmd.after_help_text,
        ];
        sections
            .filter_map((section: string?) -> string? {
                section
            })
            .join("\n\n")
    }

    function _section(title: string, rows: string[][]) -> string? {
        if (!rows) {
            return null;
        }
        let width = rows.reduce(
            (widest: int, row: string[]) -> int {
                if (row[0].length() > widest) {
                    row[0].length()
                } else {
                    widest
                }
            },
            0,
        );
        let lines = rows.map((row: string[]) -> string {
            `  ${row[0]}${" ".repeat(width - row[0].length())}  ${row[1]}`.trim_end()
        });
        `${title}:\n${lines.join("\n")}`
    }
}
