// What a command prints about itself: its usage line and its help,
// laid out the way clap lays them out.

class Usage {
    // The command's full usage: `<name> [OPTIONS] <REQUIRED> [OPTIONAL]... [COMMAND]`.
    function of(cmd: Command) -> string {
        Usage.with(cmd, [], Usage.required_ids(cmd))
    }

    // The usage for a command line that already has the args `used`, as
    // clap narrows it under an error: those args and the `required`
    // ones spelled out, with no `[OPTIONS]` standing in for the rest.
    // With nothing used, it is the full usage.
    function with(cmd: Command, used: string[], required: string[]) -> string {
        // A required arg follows the args it requires in turn.
        let named = required
            .flat_map((id: string) -> string[] {
                (cmd.find_arg(id)?.require_ids ?? []).concat([id])
            })
            .concat(used)
            .reduce(
                (unique: string[], id: string) -> string[] {
                    if (unique.includes(id)) {
                        unique
                    } else {
                        unique.concat([id])
                    }
                },
                [],
            );
        let options = named.filter_map(cmd.find_arg)
            .filter((arg: Arg) -> bool {
                !arg.is_positional()
            })
            .map((arg: Arg) -> string {
                arg.to_string()
            });
        let positionals = cmd.positionals()
            .filter((arg: Arg) -> bool {
                !arg.is_hidden
            })
            .map((arg: Arg) -> string {
                arg.positional_usage(named.includes(arg.id))
            });
        let options_tag = if (used.length() == 0 && Usage.needs_options_tag(cmd)) {
            ["[OPTIONS]"]
        } else {
            []
        };
        [cmd.usage_name].concat(options_tag)
            .concat(options)
            .concat(positionals)
            .concat(Usage._subcommand(cmd, used.length() > 0))
            .join(" ")
    }

    // A narrowed usage names the subcommand only when one is required.
    function _subcommand(cmd: Command, narrowed: bool) -> string[] {
        if (
            cmd.is_subcommand_required
                && (narrowed || cmd.has_visible_subcommands() || cmd.externals_allowed)
        ) {
            ["<COMMAND>"]
        } else if (!narrowed && (cmd.has_visible_subcommands() || cmd.externals_allowed)) {
            ["[COMMAND]"]
        } else {
            []
        }
    }

    // The args the command declares required, in declaration order.
    function required_ids(cmd: Command) -> string[] {
        cmd.arg_list
            .filter((arg: Arg) -> bool {
                arg.is_required
            })
            .map((arg: Arg) -> string {
                arg.id
            })
    }

    // What a subcommand's usage shows of its parent: the args that must come before it.
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
