// A clap-style command-line parser in pure BAML. Depends only on the
// standard library (never on wrapper code), so it could be lifted into
// a shared library unchanged.
//
// Declare commands and arguments with the builder functions, then
// parse argv (without the program name) into typed Matches:
//
//   let spec = Command.new("tool").about("Does things").subcommand(
//       Command.new("check").arg(Arg.opt("backend").with_value_name("URL")),
//   );
//   let matches = spec.parse(args);    // throws ParseError
//   matches.subcommand()               // "check"
//   matches.sub().get("backend")       // string?
//
// Every command gets built-in -h/--help and (when it has a version)
// --version args, as in clap. Matching one throws DisplayHelp or
// DisplayVersion carrying rendered text; the application prints and
// chooses the exit code. Disable them per command (`auto_help(false)`)
// or replace them by declaring an arg with the same name or with the
// Help/Version action. A `help` subcommand is synthesized when
// subcommands exist.
//
// A `global()` arg is also accepted by every subcommand below the
// command that declares it, and its value reads the same from any
// level of the Matches.
//
// A command that `allow_external_subcommands()` hands a word it does
// not declare to someone else: the word becomes the subcommand and the
// tokens after it are kept verbatim in `external_args`. Only global
// args are still read from those tokens. Options the command does not
// declare are tolerated in front of an external subcommand, because
// they belong to whoever runs it.

enum ArgAction {
    Flag,
    Value,
    Positional,
    Help,
    Version,
}

class Arg {
    name: string,
    action: ArgAction,
    short: string?,
    value_name: string?,
    help_text: string,
    is_required: bool,
    is_multiple: bool,
    default_value: string?,
    choices: string[],
    conflicts: string[],
    requires_args: string[],
    allow_hyphen: bool,
    is_global: bool,
    is_hidden: bool,

    function _empty(name: string, action: ArgAction) -> Arg {
        Arg {
            name: name,
            action: action,
            short: null,
            value_name: null,
            help_text: "",
            is_required: false,
            is_multiple: false,
            default_value: null,
            choices: [],
            conflicts: [],
            requires_args: [],
            allow_hyphen: false,
            is_global: false,
            is_hidden: false,
        }
    }

    function flag(name: string) -> Arg {
        Arg._empty(name, ArgAction.Flag)
    }

    function opt(name: string) -> Arg {
        Arg._empty(name, ArgAction.Value)
    }

    function pos(name: string) -> Arg {
        Arg._empty(name, ArgAction.Positional)
    }

    function with_action(self, action: ArgAction) -> Arg {
        self.action = action;
        self
    }

    function with_short(self, short: string) -> Arg {
        self.short = short;
        self
    }

    function help(self, text: string) -> Arg {
        self.help_text = text;
        self
    }

    function with_value_name(self, value_name: string) -> Arg {
        self.value_name = value_name;
        self
    }

    function required(self) -> Arg {
        self.is_required = true;
        self
    }

    function multiple(self) -> Arg {
        self.is_multiple = true;
        self
    }

    function default(self, value: string) -> Arg {
        self.default_value = value;
        self
    }

    function value_parser(self, choices: string[]) -> Arg {
        self.choices = choices;
        self
    }

    function conflicts_with(self, other: string) -> Arg {
        self.conflicts.push(other);
        self
    }

    function requires_arg(self, other: string) -> Arg {
        self.requires_args.push(other);
        self
    }

    function allow_hyphen_values(self) -> Arg {
        self.allow_hyphen = true;
        self
    }

    // Also accepted by every subcommand below the declaring command.
    function global(self) -> Arg {
        self.is_global = true;
        self
    }

    // Parsed as usual, left out of generated help.
    function hidden(self) -> Arg {
        self.is_hidden = true;
        self
    }

    function is_option(self) -> bool {
        self.action != ArgAction.Positional
    }

    function _check_value(self, value: string, command: string) -> null throws InvalidValueError {
        if (self.choices.length() > 0 && !self.choices.includes(value)) {
            throw InvalidValueError.not_possible(self, value, command);
        }
        null
    }

    function _annotation(self) -> string {
        Text.join_present(
            [
                Text.non_empty(self.help_text),
                match (Text.non_empty(self.choices.join(", "))) {
                    null => null,
                    let choices: string => `[possible values: ${choices}]`,
                },
                match (self.default_value) {
                    null => null,
                    let d: string => `[default: ${d}]`,
                },
            ],
            " ",
        )
    }

    function _usage(self) -> string {
        let rendered = if (self.is_multiple) {
            `${self.display_name()}...`
        } else {
            self.display_name()
        };
        if (self.is_required) {
            rendered
        } else {
            `[${rendered}]`
        }
    }

    function display_name(self) -> string {
        let value = `<${self.value_name ?? self.name.to_upper_case()}>`;
        let short = match (self.short) {
            null => "",
            let s: string => `-${s}, `,
        };
        match (self.action) {
            ArgAction.Positional => value,
            ArgAction.Value => `${short}--${self.name} ${value}`,
            _ => `${short}--${self.name}`,
        }
    }
}

// One argv token, classified the way clap_lex does. Tokens after `--`
// are `_Escaped`: positional values even if they look like flags or
// subcommands.
class _DoubleDash {
}

class _Long {
    name: string,
    value: string?,
}

class _Shorts {
    chars: string[],

    // What follows the first flag in the cluster: its inline value, if
    // that flag takes one.
    function tail(self) -> string? {
        Text.non_empty(self.chars.slice(1, self.chars.length()).join(""))
    }
}

class _Word {
    text: string,
}

class _Escaped {
    text: string,
}

type _Token = _DoubleDash | _Long | _Shorts | _Word | _Escaped;

// Cursor state threaded through `Command._consume`; steps return a
// new state rather than mutating this one.
class _ParseState {
    path: string,
    positional_index: int,
    escaped: bool,
    // Global args declared by the commands above this one.
    inherited: Arg[],

    function escape(self) -> _ParseState {
        _ParseState { ...self, escaped: true }
    }

    function next_positional(self) -> _ParseState {
        _ParseState { ...self, positional_index: self.positional_index + 1 }
    }

    function lex(self, token: string) -> _Token {
        if (self.escaped) {
            _Escaped { text: token }
        } else {
            _lex(token)
        }
    }
}

function _looks_like_flag(token: string) -> bool {
    token.length() > 1 && token.starts_with("-")
}

// Classifies a token that no `--` has escaped.
function _lex(token: string) -> _DoubleDash | _Long | _Shorts | _Word {
    if (token == "--") {
        _DoubleDash {}
    } else if (token.starts_with("--")) {
        let body = token.slice(2, token.length());
        match (body.index_of("=")) {
            null => _Long { name: body, value: null },
            let i: int => {
                _Long { name: body.slice(0, i), value: body.slice(i + 1, body.length()) }
            },
        }
    } else if (_looks_like_flag(token)) {
        _Shorts { chars: token.slice(1, token.length()).chars() }
    } else {
        _Word { text: token }
    }
}

class Command {
    name: string,
    about_text: string,
    version_text: string?,
    args: Arg[],
    subcommands: Command[],
    help_enabled: bool,
    version_enabled: bool,
    subcommand_is_required: bool,
    help_override: string?,
    after_help_text: string?,
    externals_allowed: bool,

    function new(name: string) -> Command {
        Command {
            name: name,
            about_text: "",
            version_text: null,
            args: [],
            subcommands: [],
            help_enabled: true,
            version_enabled: true,
            subcommand_is_required: false,
            help_override: null,
            after_help_text: null,
            externals_allowed: false,
        }
    }

    function parse(
        self,
        args: string[],
    ) -> Matches throws ParseError | DisplayHelp | DisplayVersion {
        self._parse(args, self.name, [], Matches.new(self.name))
    }

    // The subcommands `args` names, outermost first, read without
    // validating anything: unknown options are skipped, and a help flag
    // or an external subcommand ends the path. A synthesized `help` is
    // part of the path. For labelling an invocation that may not parse.
    function command_path(self, args: string[]) -> string[] {
        self._peek(args, [])
    }

    // Help for the subcommand at `names` below this command, as
    // `<path> help <names...>` prints it.
    function help_for(self, names: string[], path: string) -> string throws UnknownCommandError {
        self._help_for(names, path, [])
    }

    // Parses the process argv. BAML always runs a program as
    // `<binary> <entry function> args...`, both under `baml run` and
    // from a packed binary, so the first two entries are skipped.
    function parse_from(
        self,
        argv: string[],
    ) -> Matches throws ParseError | DisplayHelp | DisplayVersion {
        self.parse(argv.slice(2, argv.length()))
    }

    function about(self, text: string) -> Command {
        self.about_text = text;
        self
    }

    function version(self, text: string) -> Command {
        self.version_text = text;
        self
    }

    function arg(self, argument: Arg) -> Command {
        self.args.push(argument);
        self
    }

    function subcommand(self, sub: Command) -> Command {
        self.subcommands.push(sub);
        self
    }

    function help_text(self, text: string) -> Command {
        self.help_override = text;
        self
    }

    // Printed after the generated help.
    function after_help(self, text: string) -> Command {
        self.after_help_text = text;
        self
    }

    function auto_help(self, enabled: bool) -> Command {
        self.help_enabled = enabled;
        self
    }

    function auto_version(self, enabled: bool) -> Command {
        self.version_enabled = enabled;
        self
    }

    function subcommand_required(self) -> Command {
        self.subcommand_is_required = true;
        self
    }

    function allow_external_subcommands(self) -> Command {
        self.externals_allowed = true;
        self
    }

    function version_line(self) -> string {
        `${self.name} ${self.version_text ?? ""}`
    }

    // Declared args followed by the built-in help/version args that
    // apply to this command.
    function all_args(self) -> Arg[] {
        let builtins = [
            Arg.flag("help").with_short("h").help("Print help").with_action(ArgAction.Help),
            Arg.flag("version").help("Print version").with_action(ArgAction.Version),
        ];
        self.args.concat(
            builtins.filter((builtin: Arg) -> bool {
                self._wants_builtin(builtin)
            }),
        )
    }

    function _wants_builtin(self, builtin: Arg) -> bool {
        let enabled = match (builtin.action) {
            ArgAction.Help => self.help_enabled,
            _ => self.version_enabled && self.version_text != null,
        };
        enabled
            && !self.args.some((a: Arg) -> bool {
                a.action == builtin.action || a.name == builtin.name
            })
    }

    function find_arg(self, name: string) -> Arg? {
        self.all_args().find((a: Arg) -> bool {
            a.name == name
        })
    }

    function find_long(self, name: string, inherited: Arg[]) -> Arg? {
        self._options_with(inherited).find((a: Arg) -> bool {
            a.name == name
        })
    }

    function find_short(self, short: string, inherited: Arg[]) -> Arg? {
        self._options_with(inherited).find((a: Arg) -> bool {
            a.short == short
        })
    }

    function options(self) -> Arg[] {
        self.all_args().filter((a: Arg) -> bool {
            a.is_option()
        })
    }

    // This command's options, then the inherited globals it does not
    // redeclare.
    function _options_with(self, inherited: Arg[]) -> Arg[] {
        let own = self.options();
        own.concat(
            inherited.filter((global: Arg) -> bool {
                !own.some((a: Arg) -> bool {
                    a.name == global.name
                })
            }),
        )
    }

    // The globals a subcommand of this command inherits.
    function _globals_below(self, inherited: Arg[]) -> Arg[] {
        self._options_with(inherited).filter((a: Arg) -> bool {
            a.is_global
        })
    }

    function positionals(self) -> Arg[] {
        self.args.filter((a: Arg) -> bool {
            !a.is_option()
        })
    }

    // An option this command does not declare. In front of an external
    // subcommand it belongs to that subcommand's owner, so judgment
    // waits until the subcommand is known (see `_finish`).
    function _unknown_option(
        self,
        matches: Matches,
        flag: string,
    ) -> null throws UnknownArgumentError {
        if (!self.externals_allowed) {
            throw UnknownArgumentError.new(flag, self.name);
        }
        matches.unknown_options.push(flag);
        null
    }

    function find_subcommand(self, name: string) -> Command? {
        self.subcommands.find((c: Command) -> bool {
            c.name == name
        })
    }

    function _path_under(self, parent: string) -> string {
        `${parent} ${self.name}`
    }

    function _synthesizes_help(self) -> bool {
        self.help_enabled && self.subcommands.length() > 0 && self.find_subcommand("help") == null
    }

    function _parse(
        self,
        tokens: string[],
        path: string,
        inherited: Arg[],
        globals: Matches,
    ) -> Matches throws ParseError | DisplayHelp | DisplayVersion {
        let state = _ParseState { path: path, positional_index: 0, escaped: false, inherited: inherited };
        self._finish(self._consume(Matches._under(self.name, globals), tokens, state))
    }

    function _consume(
        self,
        matches: Matches,
        tokens: string[],
        state: _ParseState,
    ) -> Matches throws ParseError | DisplayHelp | DisplayVersion {
        match (tokens.at(0)) {
            null => matches,
            let token: string => {
                let rest = tokens.slice(1, tokens.length());
                match (state.lex(token)) {
                    _DoubleDash => self._consume(matches, rest, state.escape()),
                    let long: _Long => self._consume_long(matches, long, rest, state),
                    let shorts: _Shorts => self._consume_shorts(matches, shorts, rest, state),
                    let word: _Word => self._consume_word(matches, word, rest, state),
                    let value: _Escaped => {
                        self._consume_positional(matches, value.text, rest, state)
                    },
                }
            },
        }
    }

    function _consume_long(
        self,
        matches: Matches,
        long: _Long,
        rest: string[],
        state: _ParseState,
    ) -> Matches throws ParseError | DisplayHelp | DisplayVersion {
        let remaining = match (self.find_long(long.name, state.inherited)) {
            null => {
                self._unknown_option(matches, `--${long.name}`);
                rest
            },
            let arg: Arg => match (arg.action) {
                ArgAction.Value => self._take_value(matches, arg, long.value, rest),
                _ => match (long.value) {
                    null => {
                        self._take_flag(matches, arg, state);
                        rest
                    },
                    let value: string => {
                        throw InvalidValueError.no_value_allowed(arg, value, self.name)
                    },
                },
            },
        };
        self._consume(matches, remaining, state)
    }

    function _consume_shorts(
        self,
        matches: Matches,
        shorts: _Shorts,
        rest: string[],
        state: _ParseState,
    ) -> Matches throws ParseError | DisplayHelp | DisplayVersion {
        let remaining = self._take_shorts(matches, shorts.chars, rest, state);
        self._consume(matches, remaining, state)
    }

    // A word is a subcommand if one has that name; otherwise a positional.
    function _consume_word(
        self,
        matches: Matches,
        word: _Word,
        rest: string[],
        state: _ParseState,
    ) -> Matches throws ParseError | DisplayHelp | DisplayVersion {
        match (self._subcommand_match(matches, word.text, rest, state)) {
            null => self._consume_positional(matches, word.text, rest, state),
            let sub: SubMatch => {
                matches.sub_match = sub;
                matches
            },
        }
    }

    // A `multiple` positional keeps its slot; the last positional also
    // absorbs any overflow so it reports duplicates instead of
    // "unexpected argument".
    function _consume_positional(
        self,
        matches: Matches,
        token: string,
        rest: string[],
        state: _ParseState,
    ) -> Matches throws ParseError | DisplayHelp | DisplayVersion {
        let positionals = self.positionals();
        match (positionals.at(state.positional_index) ?? positionals.at(-1)) {
            null => {
                // Reached only after `--`: an escaped word no positional
                // takes is an external subcommand, tokens and all.
                if (!self.externals_allowed) {
                    throw UnexpectedPositionalError.new(token, self.name);
                }
                let external = Matches._under(token, matches._global_store());
                external.external_args = rest;
                matches.sub_match = SubMatch { name: token, matches: external, is_external: true };
                matches
            },
            let arg: Arg => {
                matches._take(arg, token);
                let next = if (arg.is_multiple) {
                    state
                } else {
                    state.next_positional()
                };

                self._consume(matches, rest, next)
            },
        }
    }

    // `-abc` is three flags; `-ofile` is option `o` with value `file`.
    // Returns the tokens left after the cluster.
    function _take_shorts(
        self,
        matches: Matches,
        chars: string[],
        rest: string[],
        state: _ParseState,
    ) -> string[] throws UnknownArgumentError
        | MissingValueError
        | InvalidValueError
        | DuplicateArgumentError
        | DisplayHelp
        | DisplayVersion {
        match (chars.at(0)) {
            null => rest,
            let head: string => {
                let tail = chars.slice(1, chars.length());
                match (self.find_short(head, state.inherited)) {
                    null => {
                        self._unknown_option(matches, `-${chars.join("")}`);
                        rest
                    },
                    let arg: Arg => match (arg.action) {
                        ArgAction.Value => {
                            self._take_value(matches, arg, Text.non_empty(tail.join("")), rest)
                        },
                        _ => {
                            self._take_flag(matches, arg, state);
                            self._take_shorts(matches, tail, rest, state)
                        },
                    },
                }
            },
        }
    }

    function _take_flag(
        self,
        matches: Matches,
        arg: Arg,
        state: _ParseState,
    ) -> null throws InvalidValueError | DuplicateArgumentError | DisplayHelp | DisplayVersion {
        match (arg.action) {
            ArgAction.Help => {
                throw DisplayHelp { text: self._format_help(state.path, state.inherited) }
            },
            ArgAction.Version => {
                throw DisplayVersion { text: self.version_line() }
            },
            _ => matches._slot(arg)._take(arg, "true"),
        }
    }

    // Takes one value for `arg`: `inline` (from `--name=v` or `-nv`) if
    // given, otherwise the next token unless it looks like a flag.
    // Returns the tokens left after the value.
    function _take_value(
        self,
        matches: Matches,
        arg: Arg,
        inline: string?,
        rest: string[],
    ) -> string[] throws MissingValueError | InvalidValueError | DuplicateArgumentError {
        let value = match (inline) {
            let value: string => value,
            null => {
                let next = rest.slice(0, 1).find((token: string) -> bool {
                    arg.allow_hyphen || !_looks_like_flag(token)
                });
                match (next) {
                    null => throw MissingValueError.new(arg, self.name),
                    let value: string => value,
                }
            },
        };
        matches._slot(arg)._take(arg, value);
        match (inline) {
            null => rest.slice(1, rest.length()),
            _ => rest,
        }
    }

    // The subcommand `word` names, parsed with the remaining tokens, or
    // null when `word` should be taken as a positional instead.
    function _subcommand_match(
        self,
        matches: Matches,
        word: string,
        rest: string[],
        state: _ParseState,
    ) -> SubMatch? throws ParseError | DisplayHelp | DisplayVersion {
        if (self._synthesizes_help() && word == "help") {
            let names = rest.filter((token: string) -> bool {
                !_looks_like_flag(token)
            });
            // Help for an external subcommand is its owner's to print.
            if (names.length() > 0 && self.externals_allowed && self.find_subcommand(names[0]) == null) {
                return self._external_match(matches, word, rest, state);
            }
            throw DisplayHelp { text: self._help_for(names, state.path, state.inherited) };
        }
        match (self.find_subcommand(word)) {
            let sub: Command => {
                SubMatch {
                    name: sub.name,
                    matches: sub._parse(
                        rest,
                        sub._path_under(state.path),
                        self._globals_below(state.inherited),
                        matches._global_store(),
                    ),
                    is_external: false,
                }
            },
            null => {
                if (self.externals_allowed) {
                    return self._external_match(matches, word, rest, state);
                }
                if (self.subcommands.length() > 0 && self.positionals().length() == 0) {
                    throw UnknownCommandError.new(word, self.name);
                }
                null
            },
        }
    }

    // `word` is someone else's subcommand: keep its tokens verbatim and
    // read only the global args out of them.
    function _external_match(
        self,
        matches: Matches,
        word: string,
        rest: string[],
        state: _ParseState,
    ) -> SubMatch throws MissingValueError | InvalidValueError | DuplicateArgumentError {
        let external = Matches._under(word, matches._global_store());
        external.external_args = rest;
        self._scan_external(external, rest, self._globals_below(state.inherited));
        SubMatch { name: word, matches: external, is_external: true }
    }

    function _scan_external(
        self,
        external: Matches,
        tokens: string[],
        globals: Arg[],
    ) -> null throws MissingValueError | InvalidValueError | DuplicateArgumentError {
        match (tokens.at(0)) {
            null => null,
            let token: string => {
                let rest = tokens.slice(1, tokens.length());
                match (_lex(token)) {
                    _DoubleDash => null,
                    let long: _Long => {
                        let found = globals.find((a: Arg) -> bool {
                            a.name == long.name
                        });
                        self._scan_global(external, found, long.value, rest, globals)
                    },
                    let shorts: _Shorts => {
                        let found = globals.find((a: Arg) -> bool {
                            a.short == shorts.chars.at(0)
                        });
                        self._scan_global(external, found, shorts.tail(), rest, globals)
                    },
                    _Word => self._scan_external(external, rest, globals),
                }
            },
        }
    }

    function _scan_global(
        self,
        external: Matches,
        found: Arg?,
        inline: string?,
        rest: string[],
        globals: Arg[],
    ) -> null throws MissingValueError | InvalidValueError | DuplicateArgumentError {
        let remaining = match (found) {
            null => rest,
            let arg: Arg => match (arg.action) {
                ArgAction.Value => self._take_value(external, arg, inline, rest),
                _ => {
                    external._slot(arg)._take(arg, "true");
                    rest
                },
            },
        };
        self._scan_external(external, remaining, globals)
    }

    function _peek(self, tokens: string[], inherited: Arg[]) -> string[] {
        match (tokens.at(0)) {
            null => [],
            let token: string => {
                let rest = tokens.slice(1, tokens.length());
                match (_lex(token)) {
                    _DoubleDash => [],
                    let long: _Long => {
                        self._peek_option(
                            self.find_long(long.name, inherited),
                            long.value,
                            rest,
                            inherited,
                        )
                    },
                    let shorts: _Shorts => {
                        let found = self.find_short(shorts.chars.at(0) ?? "", inherited);
                        self._peek_option(found, shorts.tail(), rest, inherited)
                    },
                    let word: _Word => match (self.find_subcommand(word.text)) {
                        let sub: Command => {
                            [sub.name].concat(sub._peek(rest, self._globals_below(inherited)))
                        },
                        null => {
                            if (self._synthesizes_help() && word.text == "help") {
                                ["help"].concat(self._peek(rest, inherited))
                            } else if (self.externals_allowed) {
                                [word.text]
                            } else {
                                self._peek(rest, inherited)
                            }
                        },
                    },
                }
            },
        }
    }

    // Continues past an option: a help flag ends the path, and an
    // option that takes its value from the next token skips it.
    function _peek_option(
        self,
        found: Arg?,
        inline: string?,
        rest: string[],
        inherited: Arg[],
    ) -> string[] {
        match (found) {
            let arg: Arg if arg.action == ArgAction.Help => [],
            let arg: Arg if arg.action == ArgAction.Value && inline == null => {
                self._peek(rest.slice(1, rest.length()), inherited)
            },
            _ => self._peek(rest, inherited),
        }
    }

    function _help_for(
        self,
        names: string[],
        path: string,
        inherited: Arg[],
    ) -> string throws UnknownCommandError {
        match (names.at(0)) {
            null => self._format_help(path, inherited),
            let name: string => match (self.find_subcommand(name)) {
                null => throw UnknownCommandError.new(name, self.name),
                let sub: Command => {
                    sub._help_for(
                        names.slice(1, names.length()),
                        sub._path_under(path),
                        self._globals_below(inherited),
                    )
                },
            },
        }
    }

    function _finish(
        self,
        matches: Matches,
    ) -> Matches throws MissingRequiredError | ConflictError | UnknownCommandError | UnknownArgumentError {
        for (let arg in self.args) {
            matches._slot(arg)._apply_default(arg);
            self._validate_arg(matches, arg);
        }
        if (!matches.is_external()) {
            if let flag: string = matches.unknown_options.at(0) {
                throw UnknownArgumentError.new(flag, self.name);
            }
        }
        if (self.subcommand_is_required && matches.sub_match == null) {
            throw UnknownCommandError.missing(self.name);
        }
        matches
    }

    function _validate_arg(
        self,
        matches: Matches,
        arg: Arg,
    ) -> null throws MissingRequiredError | ConflictError {
        if (matches.is_present(arg.name)) {
            self._check_relations(matches, arg)
        } else {
            self._check_required(arg)
        }
    }

    function _check_required(self, arg: Arg) -> null throws MissingRequiredError {
        if (arg.is_required && arg.default_value == null) {
            throw MissingRequiredError.new(arg, self.name);
        }
        null
    }

    function _check_relations(
        self,
        matches: Matches,
        arg: Arg,
    ) -> null throws MissingRequiredError | ConflictError {
        match (arg.conflicts.find(matches.is_present)) {
            null => null,
            let other: string => {
                throw ConflictError.new(arg, other, self.find_arg(other), self.name)
            },
        };
        match (
            arg.requires_args.find((needed: string) -> bool {
                !matches.is_present(needed)
            })
        ) {
            null => null,
            let needed: string => throw MissingRequiredError.required_by(needed, arg, self.name),
        }
    }

    function _only_if<T>(condition: bool, items: T[]) -> T[] {
        if (condition) {
            items
        } else {
            []
        }
    }

    function _help_rows(rows: string[][]) -> string {
        let width = Text.max_length(
            rows.map((row: string[]) -> string {
                row[0]
            }),
        );

        rows
            .map((row: string[]) -> string {
                match (Text.non_empty(row[1])) {
                    null => `  ${row[0]}`,
                    let about: string => `  ${Text.pad_to(row[0], width)}  ${about}`,
                }
            })
            .join("\n")
    }

    function _help_section(title: string, rows: string[][]) -> string? {
        match (rows.length()) {
            0 => null,
            _ => `${title}:\n${Command._help_rows(rows)}`,
        }
    }

    function format_help(self, path: string) -> string {
        self._format_help(path, [])
    }

    function _format_help(self, path: string, inherited: Arg[]) -> string {
        self.help_override ?? self._generated_help(path, inherited)
    }

    function _generated_help(self, path: string, inherited: Arg[]) -> string {
        let options = self._options_with(inherited).filter((a: Arg) -> bool {
            !a.is_hidden
        });
        let positionals = self.positionals();
        let arg_row = (a: Arg) -> string[] {
            [a.display_name(), a._annotation()]
        };
        let usage = [
            [path],
            Command._only_if(options.length() > 0, ["[OPTIONS]"]),
            positionals.map((a: Arg) -> string {
                a._usage()
            }),
            Command._only_if(self.subcommands.length() > 0, ["[COMMAND]"]),
        ]
            .flat_map((part: string[]) -> string[] {
                part
            })
            .join(" ");
        let command_rows = self.subcommands
            .map((c: Command) -> string[] {
                [c.name, c.about_text]
            })
            .concat(
                Command._only_if(self._synthesizes_help(), [["help", "Print help for a command"]]),
            );
        Text.join_present(
            [
                Text.non_empty(self.about_text),
                `Usage: ${usage}`,
                Command._help_section("Commands", command_rows),
                Command._help_section("Arguments", positionals.map(arg_row)),
                Command._help_section("Options", options.map(arg_row)),
                self.after_help_text,
            ],
            "\n\n",
        )
    }
}

class SubMatch {
    name: string,
    matches: Matches,
    // Not declared by the command: `matches.external_args` holds its
    // tokens verbatim.
    is_external: bool,
}

// Where a matched value came from. Defaults fill in values but do not
// count as present, so they never trigger conflicts or duplicates.
enum ValueSource {
    CommandLine,
    Default,
}

class MatchedArg {
    values: string[],
    source: ValueSource,
}

class Matches {
    command: string,
    args: map<string, MatchedArg>,
    sub_match: SubMatch?,
    // Where global args are kept: one store shared by every level, so
    // a global reads the same wherever it was written. Null on the
    // store itself.
    globals: Matches?,
    // The tokens after an external subcommand, verbatim.
    external_args: string[],
    // Options the command does not declare, seen before its subcommand.
    unknown_options: string[],

    function new(command: string) -> Matches {
        Matches {
            command: command,
            args: {},
            sub_match: null,
            globals: null,
            external_args: [],
            unknown_options: [],
        }
    }

    function _under(command: string, globals: Matches) -> Matches {
        Matches { ...Matches.new(command), globals: globals }
    }

    function _global_store(self) -> Matches {
        self.globals ?? self
    }

    // Where `arg`'s value is kept.
    function _slot(self, arg: Arg) -> Matches {
        if (arg.is_global) {
            self._global_store()
        } else {
            self
        }
    }

    function _matched(self, name: string) -> MatchedArg? {
        self.args.get(name) ?? self.globals?._matched(name)
    }

    // Whether the subcommand is one the command does not declare.
    function is_external(self) -> bool {
        self.sub_match?.is_external ?? false
    }

    function empty() -> Matches {
        Matches.new("")
    }

    function get(self, name: string) -> string? {
        self.get_many(name).at(0)
    }

    function get_or(self, name: string, fallback: string) -> string {
        self.get(name) ?? fallback
    }

    function get_many(self, name: string) -> string[] {
        self._matched(name)?.values ?? []
    }

    function get_int(self, name: string) -> int? throws InvalidValueError {
        match (self.get(name)) {
            null => null,
            let raw: string => match (Text.parse_int_or_null(raw)) {
                null => throw InvalidValueError.not_an_int(name, raw, self.command),
                let n: int => n,
            },
        }
    }

    function value_source(self, name: string) -> ValueSource? {
        self._matched(name)?.source
    }

    function is_present(self, name: string) -> bool {
        self.value_source(name) == ValueSource.CommandLine
    }

    function subcommand(self) -> string? {
        match (self.sub_match) {
            null => null,
            let sub: SubMatch => sub.name,
        }
    }

    function sub(self) -> Matches? {
        match (self.sub_match) {
            null => null,
            let matched: SubMatch => matched.matches,
        }
    }

    function _take(
        self,
        arg: Arg,
        value: string,
    ) -> null throws InvalidValueError | DuplicateArgumentError {
        arg._check_value(value, self.command);
        if (!arg.is_multiple && self.is_present(arg.name)) {
            throw DuplicateArgumentError.new(arg, self.command);
        }
        self.args
            .get_or_insert(arg.name, MatchedArg { values: [], source: ValueSource.CommandLine })
            .values.push(value);
        null
    }

    function _apply_default(self, arg: Arg) -> null {
        match (arg.default_value) {
            null => null,
            let value: string => {
                self.args.get_or_insert(
                    arg.name,
                    MatchedArg { values: [value], source: ValueSource.Default },
                );
                null
            },
        }
    }
}
