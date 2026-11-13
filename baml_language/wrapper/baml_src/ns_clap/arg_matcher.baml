// The matches while they are being built.

// How an argument was named on the command line.
enum Identifier {
    Short,
    Long,
    Index,
}

// An arg whose values are still arriving: an option waiting for its
// value, or a positional that takes several.
class PendingArg {
    id: string,
    ident: Identifier?,
    raw_vals: string[],
}

class ArgMatcher {
    matches: ArgMatches,
    pending: PendingArg?,

    function new() -> ArgMatcher {
        ArgMatcher { matches: ArgMatches.new(), pending: null }
    }

    function contains(self, id: string) -> bool {
        self.matches.contains_id(id)
    }

    // Whether the arg was given on the command line.
    function check_explicit(self, id: string) -> bool {
        self.matches.value_source(id) == ValueSource.CommandLine
    }

    // The ids given on the command line, in the order first seen.
    function explicit_ids(self) -> string[] {
        self.matches.args.keys().filter(self.check_explicit)
    }

    // Removes the arg. True when it had been given on the command line.
    function remove(self, id: string) -> bool {
        let explicit = self.check_explicit(id);
        self.matches.args.delete(id);
        explicit
    }

    // Begins an occurrence of the arg from `source`.
    function start_arg(self, id: string, source: ValueSource) -> null {
        let matched = self.matches.args.get_or_insert(id, MatchedArg { source: source, vals: [] });
        matched.source = source;
        null
    }

    function add_val_to(self, id: string, value: string) -> null {
        self.matches.args[id].vals.push(value);
        null
    }

    // The values arriving for `id`, starting it if another arg, or none, is pending.
    function pending_values(self, id: string, ident: Identifier?) -> string[] {
        match (self.pending) {
            let pending: PendingArg if pending.id == id => pending.raw_vals,
            _ => {
                let pending = PendingArg { id: id, ident: ident, raw_vals: [] };
                self.pending = pending;
                pending.raw_vals
            },
        }
    }

    function pending_id(self) -> string? {
        self.pending?.id
    }

    function take_pending(self) -> PendingArg? {
        let pending = self.pending;
        self.pending = null;
        pending
    }

    function subcommand(self, sub: SubCommand) -> null {
        self.matches.sub = sub;
        null
    }

    // Gives every level the same value for each global arg: the one
    // from the deepest level that has it, unless a shallower level's
    // came from a stronger source.
    function propagate_globals(self, global_ids: string[]) -> null {
        ArgMatcher._fill_in_global_values(self.matches, global_ids, {})
    }

    function _fill_in_global_values(
        matches: ArgMatches,
        global_ids: string[],
        vals: map<string, MatchedArg>,
    ) -> null {
        for (let id in global_ids) {
            if let matched: MatchedArg = matches.args.get(id) {
                let keep = match (vals.get(id)) {
                    let parent: MatchedArg if ArgMatcher._outranks(parent.source, matched.source) => {
                        parent
                    },
                    _ => matched,
                };
                vals.set(id, keep);
            }
        }
        if let sub: SubCommand = matches.sub {
            ArgMatcher._fill_in_global_values(sub.matches, global_ids, vals);
        }
        for (let id in vals.keys()) {
            matches.args.set(id, vals[id]);
        }
        null
    }

    function _outranks(left: ValueSource, right: ValueSource) -> bool {
        left == ValueSource.CommandLine && right == ValueSource.DefaultValue
    }
}
