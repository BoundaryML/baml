// The result of parsing, read the way clap's `ArgMatches` is.

// Where a value came from. Later members take precedence when a global
// arg has a value at more than one level.
enum ValueSource {
    DefaultValue,
    CommandLine,
}

class MatchedArg {
    source: ValueSource,
    vals: string[],
}

class SubCommand {
    name: string,
    matches: ArgMatches,
}

// The id an external subcommand's arguments are kept under.
const EXTERNAL_ID = "";

class ArgMatches {
    args: map<string, MatchedArg>,
    sub: SubCommand?,

    function new() -> ArgMatches {
        ArgMatches { args: {}, sub: null }
    }

    // The value of an arg that takes one; the first when it takes several.
    function get_one(self, id: string) -> string? {
        self.get_many(id).at(0)
    }

    function get_many(self, id: string) -> string[] {
        self.args.get(id)?.vals ?? []
    }

    function get_flag(self, id: string) -> bool {
        self.get_one(id) == "true"
    }

    // Whether the arg has a value, from the command line or a default.
    function contains_id(self, id: string) -> bool {
        self.args.get(id) != null
    }

    function value_source(self, id: string) -> ValueSource? {
        self.args.get(id)?.source
    }

    function subcommand(self) -> SubCommand? {
        self.sub
    }

    function subcommand_name(self) -> string? {
        self.sub?.name
    }

    function subcommand_matches(self, name: string) -> ArgMatches? {
        match (self.sub) {
            let sub: SubCommand if sub.name == name => sub.matches,
            _ => null,
        }
    }

    // Whether these are the matches of an external subcommand.
    function is_external(self) -> bool {
        self.contains_id(EXTERNAL_ID)
    }

    // An external subcommand's arguments, verbatim.
    function external_args(self) -> string[] {
        self.get_many(EXTERNAL_ID)
    }

    // The subcommand names chosen, outermost first.
    function subcommand_path(self) -> string[] {
        match (self.sub) {
            null => [],
            let sub: SubCommand => [sub.name].concat(sub.matches.subcommand_path()),
        }
    }
}
