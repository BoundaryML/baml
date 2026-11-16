// One argument of a command, built the way clap's `Arg` is. An arg
// with a long or short name is an option; one with neither is a
// positional, matched by its place among the positionals.

// What happens when the argument is seen.
enum ArgAction {
    // Takes one value; a second occurrence is an error.
    Set,
    // Takes one value per occurrence and keeps them all.
    Append,
    // Takes no value; present means true.
    SetTrue,
    // Prints help for the command.
    Help,
    // Prints the command's version.
    Version,
}

// Display orders for the built-in help and version args, which follow
// every declared arg.
const HELP_DISPLAY_ORDER = 1000000;

const VERSION_DISPLAY_ORDER = 1000001;

class Arg {
    id: string,
    long_name: string?,
    short_name: string?,
    arg_action: ArgAction?,
    value_label: string?,
    help_text: string?,
    is_required: bool,
    is_global: bool,
    is_hidden: bool,
    default_text: string?,
    possible: string[],
    conflict_ids: string[],
    require_ids: string[],
    hyphen_values_allowed: bool,
    // Place among the command's positionals, from 1. Set when the command is built.
    index: int?,
    // Place among the command's options in help. Set when the command is built.
    display_order: int,

    function new(id: string) -> Arg {
        Arg {
            id: id,
            long_name: null,
            short_name: null,
            arg_action: null,
            value_label: null,
            help_text: null,
            is_required: false,
            is_global: false,
            is_hidden: false,
            default_text: null,
            possible: [],
            conflict_ids: [],
            require_ids: [],
            hyphen_values_allowed: false,
            index: null,
            display_order: 0,
        }
    }

    function long(self, name: string) -> Arg {
        self.long_name = name;
        self
    }

    function short(self, name: string) -> Arg {
        self.short_name = name;
        self
    }

    function action(self, action: ArgAction) -> Arg {
        self.arg_action = action;
        self
    }

    function value_name(self, name: string) -> Arg {
        self.value_label = name;
        self
    }

    function help(self, text: string) -> Arg {
        self.help_text = text;
        self
    }

    function required(self, yes: bool) -> Arg {
        self.is_required = yes;
        self
    }

    // Also accepted by every subcommand below the declaring command.
    // The value given deepest wins, and reads the same from every level.
    function global(self, yes: bool) -> Arg {
        self.is_global = yes;
        self
    }

    // Parsed as usual, left out of help.
    function hide(self, yes: bool) -> Arg {
        self.is_hidden = yes;
        self
    }

    function default_value(self, value: string) -> Arg {
        self.default_text = value;
        self
    }

    // The only values accepted.
    function value_parser(self, values: string[]) -> Arg {
        self.possible = values;
        self
    }

    function conflicts_with(self, id: string) -> Arg {
        self.conflict_ids.push(id);
        self
    }

    function requires(self, id: string) -> Arg {
        self.require_ids.push(id);
        self
    }

    // Lets a value start with `-`, which otherwise reads as a new option.
    function allow_hyphen_values(self, yes: bool) -> Arg {
        self.hyphen_values_allowed = yes;
        self
    }

    function is_positional(self) -> bool {
        self.long_name == null && self.short_name == null
    }

    function get_action(self) -> ArgAction {
        self.arg_action ?? ArgAction.Set
    }

    function takes_value(self) -> bool {
        match (self.get_action()) {
            ArgAction.Set | ArgAction.Append => true,
            ArgAction.SetTrue | ArgAction.Help | ArgAction.Version => false,
        }
    }

    // Whether this is a help or version arg.
    function is_builtin(self) -> bool {
        match (self.get_action()) {
            ArgAction.Help | ArgAction.Version => true,
            ArgAction.Set | ArgAction.Append | ArgAction.SetTrue => false,
        }
    }

    // Whether a positional keeps taking values.
    function is_multiple(self) -> bool {
        self.get_action() == ArgAction.Append
    }

    function get_value_name(self) -> string {
        self.value_label ?? self.id
    }

    // Orders options in help: by declaration, then short before long.
    function sort_key(self) -> string {
        let key = match (self.short_name) {
            let short: string => {
                let rank = if (short == short.to_lower_case()) {
                    "0"
                } else {
                    "1"
                };
                `${short.to_lower_case()}${rank}`
            },
            null => self.long_name ?? `{${self.id}`,
        };
        `${Arg._padded(self.display_order)}${key}`
    }

    function _padded(order: int) -> string {
        let digits = order.to_string();
        `${"0".repeat(8 - digits.length())}${digits}`
    }

    // The option as help lists it: `-s, --long <VALUE>`, with the short
    // column left blank for a long-only option.
    function help_spec(self) -> string {
        if (self.is_positional()) {
            return self.to_string();
        }
        let short = match (self.short_name) {
            let short: string => `-${short}`,
            null => "  ",
        };
        let separator = if (self.short_name != null && self.long_name != null) {
            ", "
        } else {
            "  "
        };
        let long = match (self.long_name) {
            let long: string => `--${long}`,
            null => "",
        };
        let value = if (self.takes_value()) {
            ` <${self.get_value_name()}>`
        } else {
            ""
        };
        `${short}${separator}${long}${value}`.trim_end()
    }

    // The help text with what clap appends to it: the default and the
    // possible values.
    function help_annotated(self) -> string {
        let notes = [
            self.help_text,
            match (self.default_text) {
                null => null,
                let value: string => `[default: ${value}]`,
            },
            if (self.possible) {
                `[possible values: ${self.possible.join(", ")}]`
            } else {
                null
            },
        ];
        notes
            .filter_map((note: string?) -> string? {
                note
            })
            .join(" ")
    }

    // A positional as usage shows it: `<NAME>` where it must be given,
    // `[NAME]` where it may be, with `...` when it takes several.
    function positional_usage(self, required: bool) -> string {
        let many = if (self.is_multiple()) {
            "..."
        } else {
            ""
        };
        if (required) {
            `<${self.get_value_name()}>${many}`
        } else {
            `[${self.get_value_name()}]${many}`
        }
    }

    // The arg as errors and usage name it: `--long <VALUE>`, `-s`,
    // `<NAME>`, or `[NAME]...`.
    implements baml.ToString {
        function to_string(self) -> string {
            if (self.is_positional()) {
                return self.positional_usage(self.is_required);
            }
            let name = match (self.long_name) {
                let long: string => `--${long}`,
                null => `-${self.short_name ?? ""}`,
            };
            if (self.takes_value()) {
                `${name} <${self.get_value_name()}>`
            } else {
                name
            }
        }
    }
}
