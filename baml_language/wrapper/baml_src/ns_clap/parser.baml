// The parse loop, following clap's `Parser`: read one argument at a
// time, decide whether it is a subcommand, an option, a value for the
// arg still collecting values, or a positional, and hand finished args
// to the matcher.

// What the parser expects next.
class ValuesDone {
}

// The option `id` is waiting for its value.
class Opt {
    id: string,
}

// The positional `id` takes any further values.
class Pos {
    id: string,
}

type ParseState = ValuesDone | Opt | Pos;

// The argument was not a flag at all.
class NoArg {
}

// No arg of the command has this name.
class NoMatchingArg {
    arg: string,
}

// `--flag=value` for a flag that takes none.
class UnneededAttachedValue {
    rest: string,
    arg: Arg,
}

// It looks like a flag, but the arg collecting values accepts values
// that start with `-`.
class MaybeHyphenValue {
}

type ParseResult = ValuesDone
    | Opt
    | NoArg
    | NoMatchingArg
    | UnneededAttachedValue
    | MaybeHyphenValue;

// Where the parser is in one command's arguments.
class _Progress {
    state: ParseState,
    // The positional the next plain value belongs to.
    pos_counter: int,
    // After `--`, everything is a value.
    trailing_values: bool,
    // Options a passthrough command does not declare, seen before its subcommand.
    deferred: string[],

    function start() -> _Progress {
        _Progress { state: ValuesDone {}, pos_counter: 1, trailing_values: false, deferred: [] }
    }
}

class Parser {
    cmd: Command,

    function new(cmd: Command) -> Parser {
        Parser { cmd: cmd }
    }

    function get_matches_with(
        self,
        matcher: ArgMatcher,
        raw: RawArgs,
        cursor: ArgCursor,
    ) -> null throws Error {
        self.parse(matcher, raw, cursor);
        self.resolve_pending(matcher);
        self.add_defaults(matcher);
        Validator.new(self.cmd).validate(matcher)
    }

    function parse(
        self,
        matcher: ArgMatcher,
        raw: RawArgs,
        cursor: ArgCursor,
    ) -> null throws Error {
        let progress = _Progress.start();
        let subcommand: Command? = null;
        while let arg_os: ParsedArg = raw.next(cursor) {
            if (!progress.trailing_values) {
                // A subcommand name is only a name where a value is not expected.
                if (progress.state is ValuesDone) {
                    if let sub: Command = self.cmd.find_subcommand(arg_os.inner) {
                        if (sub._is_builtin_help(self.cmd)) {
                            self.parse_help_subcommand(raw.remaining(cursor));
                        }
                        subcommand = sub;
                        break;
                    }
                }
                if (self._read_flag(matcher, arg_os, raw, cursor, progress)) {
                    continue;
                }
            }
            if (self._read_positional(matcher, arg_os, raw, cursor, progress)) {
                return null;
            }
        }

        // Only an external subcommand can own options this command lacks.
        if let unknown: string = progress.deferred.at(0) {
            self._ignore_errors_of_pending(matcher);
            throw self.unknown_argument(unknown, [], matcher);
        }
        if let sub: Command = subcommand {
            self.parse_subcommand(sub, matcher, raw, cursor);
        }
        null
    }

    // Reads an argument that is not a positional: the `--` escape, a
    // flag, or the value an option is waiting for. False when it is
    // none of these.
    function _read_flag(
        self,
        matcher: ArgMatcher,
        arg_os: ParsedArg,
        raw: RawArgs,
        cursor: ArgCursor,
        progress: _Progress,
    ) -> bool throws Error {
        if (arg_os.is_escape()) {
            if (!self._pending_allows_hyphen(progress.state)) {
                progress.trailing_values = true;
                return true;
            }
        } else {
            match (self._parse_flag(matcher, arg_os, progress)) {
                let done: ValuesDone => {
                    progress.state = done;
                    return true;
                },
                let opt: Opt => {
                    progress.state = opt;
                    return true;
                },
                let unknown: NoMatchingArg => {
                    self._ignore_errors_of_pending(matcher);
                    if (!self.cmd.passes_externals_through) {
                        throw self.unknown_argument(unknown.arg, raw.remaining(cursor), matcher);
                    }
                    progress.deferred.push(unknown.arg);
                    return true;
                },
                let unneeded: UnneededAttachedValue => {
                    // The usage names the optional args given before this one.
                    let used = self._given(matcher)
                        .filter((arg: Arg) -> bool {
                            !arg.is_required
                        })
                        .concat([unneeded.arg])
                        .map((arg: Arg) -> string {
                            arg.id
                        });
                    self._ignore_errors_of_pending(matcher);
                    throw Error.too_many_values(self.cmd, unneeded.arg, unneeded.rest, used);
                },
                // A plain value, or one allowed to look like a flag.
                NoArg | MaybeHyphenValue => {},
            }
        }
        match (progress.state) {
            // The value the option was waiting for.
            let opt: Opt => {
                matcher.pending_values(opt.id, null).push(arg_os.inner);
                progress.state = ValuesDone {};
                true
            },
            _ => false,
        }
    }

    function _parse_flag(
        self,
        matcher: ArgMatcher,
        arg_os: ParsedArg,
        progress: _Progress,
    ) -> ParseResult throws Error {
        match (arg_os.to_long()) {
            let long: LongFlag => {
                self.parse_long_arg(matcher, long, progress.state, progress.pos_counter)
            },
            null => match (arg_os.to_short()) {
                let shorts: ShortFlags => {
                    self.parse_short_arg(matcher, shorts, progress.state, progress.pos_counter)
                },
                null => NoArg {},
            },
        }
    }

    // Gives the argument to the next positional. With none left, it is
    // an external subcommand that takes every remaining argument, and
    // the result is true; or it is an error.
    function _read_positional(
        self,
        matcher: ArgMatcher,
        arg_os: ParsedArg,
        raw: RawArgs,
        cursor: ArgCursor,
        progress: _Progress,
    ) -> bool throws Error {
        match (self.cmd.positional_at(progress.pos_counter)) {
            let arg: Arg => {
                if (matcher.pending_id() != arg.id || !arg.is_multiple()) {
                    self.resolve_pending(matcher);
                }
                matcher.pending_values(arg.id, Identifier.Index).push(arg_os.inner);
                if (arg.is_multiple()) {
                    progress.state = Pos { id: arg.id };
                } else {
                    progress.pos_counter += 1;
                    progress.state = ValuesDone {};
                }
                false
            },
            null => {
                if (!self.cmd.externals_allowed) {
                    self._ignore_errors_of_pending(matcher);
                    throw self.match_arg_error(arg_os, progress.trailing_values);
                }
                self.resolve_pending(matcher);
                matcher.subcommand(
                    self.external_subcommand(
                        arg_os.inner,
                        raw.remaining(cursor),
                        progress.trailing_values,
                    ),
                );
                true
            },
        }
    }

    // The visible args given so far.
    function _given(self, matcher: ArgMatcher) -> Arg[] {
        matcher.explicit_ids().filter_map(self.cmd.find_arg).filter((arg: Arg) -> bool {
            !arg.is_hidden
        })
    }

    function _pending_allows_hyphen(self, state: ParseState) -> bool {
        let id = match (state) {
            let opt: Opt => opt.id,
            let pos: Pos => pos.id,
            ValuesDone => {
                return false;
            },
        };
        self.cmd.find_arg(id)?.hyphen_values_allowed ?? false
    }

    // clap reports the unknown argument even when the arg before it is
    // still missing its value.
    function _ignore_errors_of_pending(self, matcher: ArgMatcher) -> null {
        self.resolve_pending(matcher) catch (ignored) {
            Error => null,
        }
    }

    function parse_long_arg(
        self,
        matcher: ArgMatcher,
        long: LongFlag,
        state: ParseState,
        pos_counter: int,
    ) -> ParseResult throws Error {
        if (self._pending_allows_hyphen(state)) {
            return MaybeHyphenValue {};
        }
        match (self.cmd.find_long(long.name)) {
            let arg: Arg => {
                if (arg.takes_value()) {
                    return self.parse_opt_value(Identifier.Long, long.value, arg, matcher);
                }
                match (long.value) {
                    let rest: string => UnneededAttachedValue { rest: rest, arg: arg },
                    null => self.react(Identifier.Long, ValueSource.CommandLine, arg, [], matcher),
                }
            },
            null => {
                if (self._positional_allows_hyphen(pos_counter)) {
                    MaybeHyphenValue {}
                } else {
                    NoMatchingArg { arg: `--${long.name}` }
                }
            },
        }
    }

    function parse_short_arg(
        self,
        matcher: ArgMatcher,
        shorts: ShortFlags,
        state: ParseState,
        pos_counter: int,
    ) -> ParseResult throws Error {
        if (self._pending_allows_hyphen(state)) {
            return MaybeHyphenValue {};
        }
        if (
            self._positional_allows_hyphen(pos_counter)
                && shorts.chars.some((flag: string) -> bool {
                    self.cmd.find_short(flag) == null
                })
        ) {
            return MaybeHyphenValue {};
        }
        let result: ParseResult = NoArg {};
        while let flag: string = shorts.next_flag() {
            match (self.cmd.find_short(flag)) {
                null => {
                    return NoMatchingArg { arg: `-${flag}` };
                },
                let arg: Arg => {
                    if (arg.takes_value()) {
                        // The rest of the cluster is its value: `-ofile` or `-o=file`.
                        let attached = match (shorts.next_value()) {
                            let value: string if value.starts_with("=") => {
                                value.slice(1, value.length())
                            },
                            let value: string? => value,
                        };
                        return self.parse_opt_value(Identifier.Short, attached, arg, matcher);
                    }
                    result = self.react(Identifier.Short, ValueSource.CommandLine, arg, [], matcher);
                },
            }
        }
        result
    }

    function _positional_allows_hyphen(self, pos_counter: int) -> bool {
        self.cmd.positional_at(pos_counter)?.hyphen_values_allowed ?? false
    }

    // An option that takes a value: use the attached one, or wait for the next argument.
    function parse_opt_value(
        self,
        ident: Identifier,
        attached: string?,
        arg: Arg,
        matcher: ArgMatcher,
    ) -> ParseResult throws Error {
        match (attached) {
            let value: string => self.react(ident, ValueSource.CommandLine, arg, [value], matcher),
            null => {
                self.resolve_pending(matcher);
                matcher.pending_values(arg.id, ident);
                Opt { id: arg.id }
            },
        }
    }

    // Finishes the arg still collecting values.
    function resolve_pending(self, matcher: ArgMatcher) -> null throws Error {
        if let pending: PendingArg = matcher.take_pending() {
            if let arg: Arg = self.cmd.find_arg(pending.id) {
                self.react(pending.ident, ValueSource.CommandLine, arg, pending.raw_vals, matcher);
            }
        }
        null
    }

    // Applies the arg's action to one occurrence of it.
    function react(
        self,
        ident: Identifier?,
        source: ValueSource,
        arg: Arg,
        raw_vals: string[],
        matcher: ArgMatcher,
    ) -> ParseResult throws Error {
        self.resolve_pending(matcher);
        if (source == ValueSource.CommandLine && arg.takes_value() && raw_vals.length() == 0) {
            throw Error.empty_value(self.cmd, arg);
        }
        match (arg.get_action()) {
            ArgAction.Set => {
                if (matcher.remove(arg.id)) {
                    throw Error.argument_repeated(self.cmd, arg);
                }
                self.push_arg_values(arg, raw_vals, source, matcher);
            },
            ArgAction.Append => {
                self.push_arg_values(arg, raw_vals, source, matcher);
            },
            ArgAction.SetTrue => {
                if (matcher.remove(arg.id)) {
                    throw Error.argument_repeated(self.cmd, arg);
                }
                let vals = if (raw_vals) {
                    raw_vals
                } else {
                    ["true"]
                };
                self.push_arg_values(arg, vals, source, matcher);
            },
            ArgAction.Help => throw self.help_err(),
            ArgAction.Version => {
                throw Error._display(self.cmd, ErrorKind.DisplayVersion, self.cmd.render_version())
            },
        };
        ValuesDone {}
    }

    function push_arg_values(
        self,
        arg: Arg,
        raw_vals: string[],
        source: ValueSource,
        matcher: ArgMatcher,
    ) -> null throws Error {
        // The arg counts as given even when one of its values is rejected.
        matcher.start_arg(arg.id, source);
        for (let value in raw_vals) {
            if (arg.possible && !arg.possible.includes(value)) {
                throw Error.invalid_value(self.cmd, arg, value);
            }
            matcher.add_val_to(arg.id, value);
        }
        null
    }

    // Defaults fill in what the command line left out. A flag defaults to
    // false unless it declares a default of its own.
    function add_defaults(self, matcher: ArgMatcher) -> null throws Error {
        for (let arg in self.cmd.arg_list) {
            if (!matcher.contains(arg.id)) {
                let fallback = match (arg.get_action()) {
                    ArgAction.SetTrue => arg.default_text ?? "false",
                    _ => arg.default_text,
                };
                if let value: string = fallback {
                    self.react(null, ValueSource.DefaultValue, arg, [value], matcher);
                }
            }
        }
        null
    }

    // `help [COMMAND]...`: help for the subcommand the names lead to.
    function parse_help_subcommand(self, names: string[]) -> never throws Error {
        let target = names.reduce(
            (cmd: Command, name: string) -> Command throws Error {
                cmd.find_subcommand(name) ?? throw Error.unrecognized_subcommand(cmd, name)
            },
            self.cmd,
        );
        throw Parser.new(target).help_err()
    }

    function help_err(self) -> Error {
        Error._display(self.cmd, ErrorKind.DisplayHelp, Help.of(self.cmd))
    }

    function parse_subcommand(
        self,
        sub: Command,
        matcher: ArgMatcher,
        raw: RawArgs,
        cursor: ArgCursor,
    ) -> null throws Error {
        let sub_matcher = ArgMatcher.new();
        Parser.new(sub).get_matches_with(sub_matcher, raw, cursor);
        matcher.subcommand(SubCommand { name: sub.name, matches: sub_matcher.matches })
    }

    // The external subcommand `name` and its arguments, kept verbatim.
    // A passthrough command also reads its global args out of them,
    // unless they followed a `--`.
    function external_subcommand(
        self,
        name: string,
        args: string[],
        escaped: bool,
    ) -> SubCommand throws Error {
        let sub_matcher = ArgMatcher.new();
        sub_matcher.start_arg(EXTERNAL_ID, ValueSource.CommandLine);
        for (let arg in args) {
            sub_matcher.add_val_to(EXTERNAL_ID, arg);
        }
        if (self.cmd.passes_externals_through && !escaped) {
            // A bad global among its arguments is reported as the subcommand's.
            self._read_globals(sub_matcher, RawArgs.new(args)) catch (error) {
                Error => throw Error { ...error, command_path: self.cmd.path.concat([name]) },
            };
        }
        SubCommand { name: name, matches: sub_matcher.matches }
    }

    // The arguments may span several of the external program's command
    // levels, so a global given again is a deeper one: the last wins, and
    // a repeat at one level is the external program's to reject.
    function _read_globals(self, matcher: ArgMatcher, raw: RawArgs) -> null throws Error {
        let cursor = raw.cursor();
        let order: string[] = [];
        let hits: map<string, _GlobalHit> = {};
        let given: map<string, string[]> = {};
        while let arg_os: ParsedArg = raw.next(cursor) {
            if (arg_os.is_escape()) {
                break;
            }
            let found = match (arg_os.to_long()) {
                let long: LongFlag => _GlobalHit.of(self.cmd.find_long(long.name), long.value),
                null => match (arg_os.to_short()) {
                    null => null,
                    let shorts: ShortFlags => {
                        let arg = self.cmd.find_short(shorts.next_flag() ?? "");
                        // `-ofile` or `-o=file`.
                        let attached = match (shorts.next_value()) {
                            let value: string if value.starts_with("=") => {
                                value.slice(1, value.length())
                            },
                            let value: string? => value,
                        };
                        _GlobalHit.of(arg, attached)
                    },
                },
            };
            if let hit: _GlobalHit = found {
                let vals = if (!hit.arg.takes_value()) {
                    []
                } else {
                    [hit.attached ?? self._next_value(hit.arg, raw, cursor)]
                };
                if (!hits.has(hit.arg.id)) {
                    order.push(hit.arg.id);
                }
                hits[hit.arg.id] = hit;
                given[hit.arg.id] = vals;
            }
        }
        for (let id in order) {
            self.react(null, ValueSource.CommandLine, hits[id].arg, given[id], matcher);
        }
        null
    }

    function _next_value(self, arg: Arg, raw: RawArgs, cursor: ArgCursor) -> string throws Error {
        match (raw.peek(cursor)) {
            let next: ParsedArg if arg.hyphen_values_allowed || next.is_value() => {
                raw.next(cursor);
                next.inner
            },
            _ => throw Error.empty_value(self.cmd, arg),
        }
    }

    // `remaining` are the arguments after the unknown one.
    function unknown_argument(
        self,
        arg: string,
        remaining: string[],
        matcher: ArgMatcher,
    ) -> Error {
        if (!arg.starts_with("--")) {
            return Error.unknown_argument(self.cmd, arg, self._as_value_tip(arg), Usage.of(self.cmd));
        }
        let name = arg.slice(2, arg.length());
        // The usage shows what was given, and the arg that was probably meant.
        let similar = self.cmd.find_long(Suggestions.did_you_mean(name, Parser._longs(self.cmd)).at(-1) ?? "");
        let used = self._given(matcher)
            .map((arg: Arg) -> string {
                arg.id
            })
            .concat(
                match (similar) {
                    let arg: Arg => [arg.id],
                    null => [],
                },
            );
        let tips: string[] = match (self._similar_flag_tip(similar, name, remaining)) {
            let tip: string => [tip],
            null => self._as_value_tip(arg),
        };
        Error.unknown_argument(
            self.cmd,
            arg,
            tips,
            Usage.with(self.cmd, used, Usage.required_ids(self.cmd)),
        )
    }

    // Where the command takes positionals, the argument may have been meant as one.
    function _as_value_tip(self, arg: string) -> string[] {
        if (self.cmd.positionals().length() > 0) {
            [`to pass '${arg}' as a value, use '-- ${arg}'`]
        } else {
            []
        }
    }

    // A long flag of this command similar to `name`, or failing that
    // one of a subcommand named later on the command line.
    function _similar_flag_tip(self, similar: Arg?, name: string, remaining: string[]) -> string? {
        if let arg: Arg = similar {
            return `a similar argument exists: '--${arg.long_name ?? ""}'`;
        }
        let nearest: _SubcommandFlag? = null;
        for (let sub in self.cmd.subcommand_list) {
            if let position: int = remaining.index_of(sub.name) {
                if let flag: string = Suggestions.did_you_mean(name, Parser._longs(sub)).at(-1) {
                    let closer = match (nearest) {
                        null => true,
                        let current: _SubcommandFlag => position < current.position,
                    };
                    if (closer) {
                        nearest
                            = _SubcommandFlag {
                                subcommand: sub.name,
                                flag: flag,
                                position: position,
                            };
                    }
                }
            }
        }
        match (nearest) {
            null => null,
            let found: _SubcommandFlag => `'${found.subcommand} --${found.flag}' exists`,
        }
    }

    function _longs(cmd: Command) -> string[] {
        cmd.arg_list.filter_map((arg: Arg) -> string? {
            arg.long_name
        })
    }

    // A value with no positional left to take it.
    function match_arg_error(self, arg_os: ParsedArg, trailing_values: bool) -> Error {
        if (trailing_values && self.cmd.find_subcommand(arg_os.inner) != null) {
            return Error.unknown_argument(
                self.cmd,
                arg_os.inner,
                [`subcommand '${arg_os.inner}' exists; to use it, remove the '--' before it`],
                Usage.of(self.cmd),
            );
        }
        if (self.cmd.has_subcommands()) {
            let similar = Suggestions.did_you_mean(
                arg_os.inner,
                self.cmd.subcommand_list.map((sub: Command) -> string {
                    sub.name
                }),
            );
            if (similar) {
                return Error.invalid_subcommand(self.cmd, arg_os.inner, similar);
            }
            if (self.cmd.positionals().length() == 0) {
                return Error.unrecognized_subcommand(self.cmd, arg_os.inner);
            }
        }
        Error.unknown_argument(self.cmd, arg_os.inner, [], Usage.of(self.cmd))
    }
}

// A global arg found among an external subcommand's arguments.
class _GlobalHit {
    arg: Arg,
    attached: string?,

    function of(found: Arg?, attached: string?) -> _GlobalHit? {
        match (found) {
            let arg: Arg if arg.is_global => _GlobalHit { arg: arg, attached: attached },
            _ => null,
        }
    }
}

// A similar flag found on a subcommand named at `position` among the
// remaining arguments.
class _SubcommandFlag {
    subcommand: string,
    flag: string,
    position: int,
}
