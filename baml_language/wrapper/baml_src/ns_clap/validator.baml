// Checks that run once a command's arguments are all read, in clap's
// order: a required subcommand, then conflicts, then requirements.

class Validator {
    cmd: Command,

    function new(cmd: Command) -> Validator {
        Validator { cmd: cmd }
    }

    function validate(self, matcher: ArgMatcher) -> null throws Error {
        if (matcher.matches.sub == null && self.cmd.is_subcommand_required) {
            throw Error.missing_subcommand(self.cmd);
        }
        self.validate_conflicts(matcher);
        self.validate_required(matcher)
    }

    // The args that cannot be used with `arg`, whichever side declared it.
    function conflicts_of(self, arg: Arg) -> Arg[] {
        self.cmd.arg_list.filter((other: Arg) -> bool {
            other.id != arg.id
                && (arg.conflict_ids.includes(other.id) || other.conflict_ids.includes(arg.id))
        })
    }

    function _present(self, matcher: ArgMatcher) -> Arg[] {
        matcher.explicit_ids().filter_map(self.cmd.find_arg)
    }

    function validate_conflicts(self, matcher: ArgMatcher) -> null throws Error {
        for (let arg in self._present(matcher)) {
            let clashing = self.conflicts_of(arg).filter((other: Arg) -> bool {
                matcher.check_explicit(other.id)
            });
            if (clashing) {
                throw Error.argument_conflict(self.cmd, arg, clashing);
            }
        }
        null
    }

    // An arg is missing when it is required, or required by an arg that
    // is present, and nothing present excludes it.
    function validate_required(self, matcher: ArgMatcher) -> null throws Error {
        let present = self._present(matcher);
        let needed = present.flat_map((arg: Arg) -> string[] {
            arg.require_ids
        });
        let missing = self.cmd.arg_list.filter((arg: Arg) -> bool {
            (arg.is_required || needed.includes(arg.id))
                && !matcher.check_explicit(arg.id)
                && !self.conflicts_of(arg).some((other: Arg) -> bool {
                    matcher.check_explicit(other.id)
                })
        });
        if (missing) {
            throw Error.missing_required_argument(self.cmd, missing);
        }
        null
    }
}
