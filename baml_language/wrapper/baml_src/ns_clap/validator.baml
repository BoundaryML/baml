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

    // What must be present: the args the command requires, and those
    // required by an arg that was given.
    function required_ids(self, matcher: ArgMatcher) -> string[] {
        Usage.required_ids(self.cmd).concat(
            self._present(matcher).flat_map((arg: Arg) -> string[] {
                arg.require_ids
            }),
        )
    }

    // The args given that an error's usage should show.
    function _shown(self, matcher: ArgMatcher) -> Arg[] {
        self._present(matcher).filter((arg: Arg) -> bool {
            !arg.is_hidden
        })
    }

    function validate_conflicts(self, matcher: ArgMatcher) -> null throws Error {
        for (let arg in self._present(matcher)) {
            let clashing = self.conflicts_of(arg).filter((other: Arg) -> bool {
                matcher.check_explicit(other.id)
            });
            if (clashing) {
                throw Error.argument_conflict(self.cmd, arg, clashing, self._conflict_usage(matcher, clashing));
            }
        }
        null
    }

    // The usage without the args that clash: what was given besides
    // them, led by anything those args require.
    function _conflict_usage(self, matcher: ArgMatcher, clashing: Arg[]) -> string {
        let excluded = clashing.map((arg: Arg) -> string {
            arg.id
        });
        let kept = self._shown(matcher).filter((arg: Arg) -> bool {
            !excluded.includes(arg.id)
        });
        let kept_ids = kept.map((arg: Arg) -> string {
            arg.id
        });
        let needed = kept
            .flat_map((arg: Arg) -> string[] {
                arg.require_ids
            })
            .filter((id: string) -> bool {
                !kept_ids.includes(id) && !excluded.includes(id)
            });
        Usage.with(self.cmd, needed.concat(kept_ids), self.required_ids(matcher))
    }

    // An arg is missing when it is required, or required by an arg that
    // is present, and nothing present excludes it.
    function validate_required(self, matcher: ArgMatcher) -> null throws Error {
        let needed = self.required_ids(matcher);
        let missing = self.cmd.arg_list.filter((arg: Arg) -> bool {
            needed.includes(arg.id)
                && !matcher.check_explicit(arg.id)
                && !self.conflicts_of(arg).some((other: Arg) -> bool {
                    matcher.check_explicit(other.id)
                })
        });
        if (missing) {
            let used = self._shown(matcher).concat(missing).map((arg: Arg) -> string {
                arg.id
            });
            throw Error.missing_required_argument(self.cmd, missing, Usage.with(self.cmd, used, needed));
        }
        null
    }
}
