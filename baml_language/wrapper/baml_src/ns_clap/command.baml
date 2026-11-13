// A clap-style command-line parser in pure BAML, laid out like clap:
//
//   lexer.baml        argv into flags, clusters and values (clap_lex)
//   arg.baml          Arg and ArgAction (builder)
//   command.baml      Command (builder)
//   parser.baml       the parse loop (parser)
//   arg_matcher.baml  matches while they are built (parser)
//   validator.baml    conflicts and requirements (parser)
//   matches.baml      ArgMatches (parser)
//   error.baml        Error and ErrorKind (error)
//   help.baml         usage and help (output)
//   suggestions.baml  "did you mean" (parser/features)
//
// It depends only on the standard library. Declare a command, then
// parse the arguments that follow the program name:
//
//   let cmd = Command.new("tool").about("Does things").subcommand(
//       Command.new("check").arg(Arg.new("backend").long("backend").value_name("URL")),
//   );
//   let matches = cmd.try_get_matches_from(args);   // throws Error
//   matches.subcommand_name()                       // "check"
//   matches.subcommand_matches("check")?.get_one("backend")
//
// It follows clap's behavior for the features it has; `tests.baml` and
// the conformance test against clap itself pin that down. One setting
// goes beyond clap: `external_subcommand_passthrough`, for a launcher
// that hands commands on to another program.

class Command {
    name: string,
    about_text: string?,
    version_text: string?,
    arg_list: Arg[],
    subcommand_list: Command[],
    is_subcommand_required: bool,
    externals_allowed: bool,
    passes_externals_through: bool,
    help_flag_disabled: bool,
    version_flag_disabled: bool,
    help_subcommand_disabled: bool,
    is_hidden: bool,
    help_override: string?,
    after_help_text: string?,
    // The subcommands leading here from the root, outermost first. Set when built.
    path: string[],
    // The root's name. Set when built.
    root_name: string,
    // The command as usage lines name it. Set when built.
    usage_name: string,
    is_built: bool,

    function new(name: string) -> Command {
        Command {
            name: name,
            about_text: null,
            version_text: null,
            arg_list: [],
            subcommand_list: [],
            is_subcommand_required: false,
            externals_allowed: false,
            passes_externals_through: false,
            help_flag_disabled: false,
            version_flag_disabled: false,
            help_subcommand_disabled: false,
            is_hidden: false,
            help_override: null,
            after_help_text: null,
            path: [],
            root_name: name,
            usage_name: name,
            is_built: false,
        }
    }

    function about(self, text: string) -> Command {
        self.about_text = text;
        self
    }

    function version(self, text: string) -> Command {
        self.version_text = text;
        self
    }

    function arg(self, arg: Arg) -> Command {
        self.arg_list.push(arg);
        self
    }

    function subcommand(self, sub: Command) -> Command {
        self.subcommand_list.push(sub);
        self
    }

    function subcommand_required(self, yes: bool) -> Command {
        self.is_subcommand_required = yes;
        self
    }

    // A word that is not a declared subcommand becomes an external one:
    // its name is the word and every argument after it is kept
    // verbatim, unread.
    function allow_external_subcommands(self, yes: bool) -> Command {
        self.externals_allowed = yes;
        self
    }

    // Beyond clap, for a launcher that hands external subcommands to
    // another program with its own options. Implies
    // `allow_external_subcommands`, and:
    // - options this command does not declare are accepted in front of
    //   an external subcommand, because they are that program's;
    // - global args are also read out of the external subcommand's
    //   arguments, up to any `--`, as if it were one level deeper; one
    //   given again there is deeper still, so the last wins.
    function external_subcommand_passthrough(self, yes: bool) -> Command {
        self.passes_externals_through = yes;
        self.externals_allowed = self.externals_allowed || yes;
        self
    }

    function disable_help_flag(self, yes: bool) -> Command {
        self.help_flag_disabled = yes;
        self
    }

    function disable_version_flag(self, yes: bool) -> Command {
        self.version_flag_disabled = yes;
        self
    }

    function disable_help_subcommand(self, yes: bool) -> Command {
        self.help_subcommand_disabled = yes;
        self
    }

    function hide(self, yes: bool) -> Command {
        self.is_hidden = yes;
        self
    }

    // Replaces the generated help.
    function override_help(self, text: string) -> Command {
        self.help_override = text;
        self
    }

    // Printed after the generated help.
    function after_help(self, text: string) -> Command {
        self.after_help_text = text;
        self
    }

    // Parses the arguments that follow the program name.
    function try_get_matches_from(self, args: string[]) -> ArgMatches throws Error {
        self.build();
        let raw = RawArgs.new(args);
        let matcher = ArgMatcher.new();
        Parser.new(self).get_matches_with(matcher, raw, raw.cursor());
        matcher.propagate_globals(self._used_global_ids(matcher.matches));
        matcher.matches
    }

    // Parses the process's arguments. BAML runs a program as
    // `<binary> <entry function> args...`, so the first two are skipped.
    function try_get_matches(self) -> ArgMatches throws Error {
        let argv = baml.sys.argv();
        self.try_get_matches_from(argv.slice(2, argv.length()))
    }

    // Finishes the command tree: the built-in help and version args,
    // the `help` subcommand, and each command's inherited global args.
    function build(self) -> null {
        self._build([], self.name, self.name, [])
    }

    // `inherited` are the parent's global args.
    function _build(
        self,
        path: string[],
        root_name: string,
        usage_name: string,
        inherited: Arg[],
    ) -> null {
        if (self.is_built) {
            return null;
        }
        self.is_built = true;
        self.path = path;
        self.root_name = root_name;
        self.usage_name = usage_name;
        self._number_args();
        // Inherited args keep the place in help they have in the parent.
        self._inherit(inherited);
        self._add_builtin_args();
        self._add_help_subcommand();
        let globals = self.arg_list.filter((arg: Arg) -> bool {
            arg.is_global
        });
        for (let sub in self.subcommand_list) {
            // The help subcommand only names other subcommands.
            let passed = if (sub._is_builtin_help(self)) {
                []
            } else {
                globals
            };
            sub._build(path.concat([sub.name]), root_name, self._usage_name_of(sub), passed);
        }
        null
    }

    // A subcommand's usage names what must come before it: this
    // command and its required args.
    function _usage_name_of(self, sub: Command) -> string {
        [self.usage_name].concat(Usage.required(self)).concat([sub.name]).join(" ")
    }

    // Positionals are numbered by declaration, and so are options, for
    // the order help lists them in.
    function _number_args(self) -> null {
        let positional_index = 1;
        let display_order = 0;
        for (let arg in self.arg_list) {
            if (arg.is_positional()) {
                arg.index = positional_index;
                positional_index += 1;
            } else {
                arg.display_order = display_order;
                display_order += 1;
            }
        }
        null
    }

    function _add_builtin_args(self) -> null {
        if (!self.help_flag_disabled && self.find_arg("help") == null) {
            let help = Arg.new("help").short("h").long("help").action(ArgAction.Help).help("Print help");
            help.display_order = HELP_DISPLAY_ORDER;
            self.arg_list.push(help);
        }
        if (
            !self.version_flag_disabled
                && self.version_text != null
                && self.find_arg("version") == null
        ) {
            let version = Arg.new("version").short("V").long("version").action(ArgAction.Version).help(
                "Print version",
            );
            version.display_order = VERSION_DISPLAY_ORDER;
            self.arg_list.push(version);
        }
        null
    }

    function _add_help_subcommand(self) -> null {
        if (self.has_builtin_help_subcommand() && self.find_subcommand("help") == null) {
            self.subcommand_list.push(
                Command.new("help")
                    .about("Print this message or the help of the given subcommand(s)")
                    .disable_help_flag(true)
                    .arg(
                        Arg.new("command").value_name("COMMAND").action(ArgAction.Append).help(
                            "Print help for the subcommand(s)",
                        ),
                    ),
            );
        }
        null
    }

    // Takes the parent's global args this command does not declare itself.
    function _inherit(self, globals: Arg[]) -> null {
        for (let global in globals) {
            if (self.find_arg(global.id) == null) {
                self.arg_list.push(Arg { ...global });
            }
        }
        null
    }

    function _is_builtin_help(self, parent: Command) -> bool {
        self.name == "help" && parent.has_builtin_help_subcommand()
    }

    // Whether `help` is the built-in subcommand, which prints help for
    // the subcommand path that follows it.
    function has_builtin_help_subcommand(self) -> bool {
        !self.help_subcommand_disabled && self.has_subcommands()
    }

    // The global args that have a value at any level.
    function _used_global_ids(self, matches: ArgMatches) -> string[] {
        let own = self.arg_list
            .filter((arg: Arg) -> bool {
                arg.is_global && matches.contains_id(arg.id)
            })
            .map((arg: Arg) -> string {
                arg.id
            });
        let below: string[] = match (matches.sub) {
            null => [],
            let sub: SubCommand => match (self.find_subcommand(sub.name)) {
                // An external subcommand holds only this command's globals.
                null => self._used_global_ids(sub.matches),
                let cmd: Command => cmd._used_global_ids(sub.matches),
            },
        };
        own.concat(
            below.filter((id: string) -> bool {
                !own.includes(id)
            }),
        )
    }

    function find_arg(self, id: string) -> Arg? {
        self.arg_list.find((arg: Arg) -> bool {
            arg.id == id
        })
    }

    function find_long(self, name: string) -> Arg? {
        self.arg_list.find((arg: Arg) -> bool {
            arg.long_name == name
        })
    }

    function find_short(self, name: string) -> Arg? {
        self.arg_list.find((arg: Arg) -> bool {
            arg.short_name == name
        })
    }

    function find_subcommand(self, name: string) -> Command? {
        self.subcommand_list.find((sub: Command) -> bool {
            sub.name == name
        })
    }

    function positionals(self) -> Arg[] {
        self.arg_list.filter((arg: Arg) -> bool {
            arg.is_positional()
        })
    }

    function positional_at(self, index: int) -> Arg? {
        self.arg_list.find((arg: Arg) -> bool {
            arg.index == index
        })
    }

    function options(self) -> Arg[] {
        self.arg_list.filter((arg: Arg) -> bool {
            !arg.is_positional()
        })
    }

    function has_subcommands(self) -> bool {
        self.subcommand_list.length() > 0
    }

    function has_visible_subcommands(self) -> bool {
        self.subcommand_list.some((sub: Command) -> bool {
            !sub.is_hidden
        })
    }

    // The command as usage names it: `tool check`.
    function get_bin_name(self) -> string {
        [self.root_name].concat(self.path).join(" ")
    }

    function render_usage(self) -> string {
        self.build();
        `Usage: ${Usage.of(self)}`
    }

    function render_help(self) -> string {
        self.build();
        Help.of(self)
    }

    function render_version(self) -> string {
        `${self.name} ${self.version_text ?? ""}`.trim_end()
    }
}
