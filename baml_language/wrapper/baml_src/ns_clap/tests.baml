// What these tests pin down is this library's own contract. Agreement
// with clap itself is checked by the conformance test in
// `crates/baml_cli/tests/clap_conformance_e2e.rs`, which runs the same
// command lines through both.

// A tool with nested subcommands, global args and every kind of arg.
function _demo() -> Command {
    Command.new("tool")
        .about("Demo tool")
        .version("1.2.3")
        .arg(Arg.new("dir").long("dir").value_name("PATH").global(true))
        .arg(Arg.new("verbose").long("verbose").short("v").action(ArgAction.SetTrue).global(true))
        .arg(Arg.new("mode").long("mode").value_parser(["fast", "slow"]).default_value("fast"))
        .subcommand(
            Command.new("check")
                .about("Run a check")
                .arg(Arg.new("backend").long("backend").value_name("URL").conflicts_with("base"))
                .arg(Arg.new("base").long("base").value_name("REF"))
                .arg(Arg.new("force").long("force").short("f").action(ArgAction.SetTrue))
                .arg(Arg.new("limit").long("limit").short("l"))
                .arg(Arg.new("file").action(ArgAction.Append)),
        )
        .subcommand(
            Command.new("tools")
                .about("Manage tools")
                .subcommand(Command.new("list").about("List tools"))
                .subcommand(Command.new("add").arg(Arg.new("name").required(true))),
        )
}

// What a command line that is rejected, or asks for help, raises.
function _error(cmd: Command, args: string[]) -> Error {
    let parsed = cmd.try_get_matches_from(args) catch (error) {
        Error => error,
    };
    match (parsed) {
        let error: Error => error,
        _ => {
            throw baml.errors.InvalidArgument { message: `expected an error from ${args.join(" ")}` }
        },
    }
}

function _sub(matches: ArgMatches) -> ArgMatches {
    matches.sub?.matches ?? ArgMatches.new()
}

testset "parsing" {
    test "values, attached values, short clusters and positionals" {
        let m = _demo().try_get_matches_from(
            ["check", "--backend=https://x.example", "-fl5", "a.txt", "b.txt"],
        );
        assert.equal(m.subcommand_name(), "check");
        let check = _sub(m);
        assert.equal(check.get_one("backend"), "https://x.example");
        assert.is_true(check.get_flag("force"));
        assert.equal(check.get_one("limit"), "5");
        assert.equal(check.get_many("file"), ["a.txt", "b.txt"]);
        assert.equal(_sub(_demo().try_get_matches_from(["check", "-l=7"])).get_one("limit"), "7");
        assert.equal(_sub(_demo().try_get_matches_from(["check", "-l", "9"])).get_one("limit"), "9")
    }

    test "defaults fill values without counting as given" {
        let m = _demo().try_get_matches_from([]);
        assert.equal(m.get_one("mode"), "fast");
        assert.equal(m.value_source("mode"), ValueSource.DefaultValue);
        assert.equal(m.get_flag("verbose"), false);
        let given = _demo().try_get_matches_from(["--mode", "slow"]);
        assert.equal(given.value_source("mode"), ValueSource.CommandLine)
    }

    test "a flag's declared default wins over the action's" {
        let cmd = Command.new("app").arg(
            Arg.new("enabled").long("enabled").action(ArgAction.SetTrue).default_value("true"),
        );
        let m = cmd.try_get_matches_from([]);
        assert.is_true(m.get_flag("enabled"));
        assert.equal(m.value_source("enabled"), ValueSource.DefaultValue)
    }

    test "double dash makes everything after it a value" {
        let check = _sub(_demo().try_get_matches_from(["check", "--", "--force", "-l"]));
        assert.equal(check.get_many("file"), ["--force", "-l"]);
        assert.equal(check.get_flag("force"), false)
    }

    test "a lone dash is a value" {
        let check = _sub(_demo().try_get_matches_from(["check", "-"]));
        assert.equal(check.get_many("file"), ["-"])
    }

    test "a positional that takes several values takes subcommand names too" {
        let check = _sub(_demo().try_get_matches_from(["check", "a.txt", "tools"]));
        assert.equal(check.get_many("file"), ["a.txt", "tools"])
    }

    test "a value may start with a hyphen only where allowed" {
        let strict = _error(_demo(), ["check", "--limit", "-5"]);
        assert.equal(strict.kind, ErrorKind.UnknownArgument);
        let lenient = Command.new("tool").arg(Arg.new("limit").long("limit").allow_hyphen_values(true));
        assert.equal(lenient.try_get_matches_from(["--limit", "-5"]).get_one("limit"), "-5")
    }

    test "a long argv is read in place" {
        let files = baml.Array.generate(20000, (i: int) -> string {
            `file${i}.txt`
        });
        let check = _sub(_demo().try_get_matches_from(["check"].concat(files)));
        assert.equal(check.get_many("file").length(), 20000)
    }
}

testset "errors" {
    test "each kind of mistake has its own kind and clap's wording" {
        let cases: map<string, string[]> = {
            "unexpected argument '--nope' found": ["check", "--nope"],
            "unexpected argument '-z' found": ["check", "-fz"],
            "unrecognized subcommand 'nope'": ["tools", "nope"],
            "a value is required for '--base <REF>' but none was supplied": ["check", "--base"],
            "invalid value 'bad' for '--mode <mode>'\n  [possible values: fast, slow]": ["--mode", "bad"],
            "unexpected value '1' for '--force' found; no more were expected": ["check", "--force=1"],
            "the argument '--base <REF>' cannot be used multiple times": ["check", "--base", "a", "--base", "b"],
            "the argument '--backend <URL>' cannot be used with '--base <REF>'": ["check", "--backend", "u", "--base", "b"],
            "the following required arguments were not provided:\n  <name>": ["tools", "add"],
        };
        for (let problem in cases.keys()) {
            let error = _error(_demo(), cases[problem]);
            assert.equal(error.message.split("\n\n")[0], problem);
            assert.equal(error.exit_code(), 2);
            assert.is_true(error.use_stderr());
        }
        assert.equal(_error(_demo(), ["check", "--nope"]).kind, ErrorKind.UnknownArgument);
        assert.equal(_error(_demo(), ["tools", "nope"]).kind, ErrorKind.InvalidSubcommand);
        assert.equal(_error(_demo(), ["check", "--base"]).kind, ErrorKind.InvalidValue);
        assert.equal(_error(_demo(), ["check", "--force=1"]).kind, ErrorKind.TooManyValues);
        assert.equal(
            _error(_demo(), ["check", "--base", "a", "--base", "b"]).kind,
            ErrorKind.ArgumentConflict,
        );
        assert.equal(_error(_demo(), ["tools", "add"]).kind, ErrorKind.MissingRequiredArgument)
    }

    test "an error carries tips, the usage of the command that raised it, and its path" {
        let error = _error(_demo(), ["tools", "ad"]);
        assert.equal(
            error.message,
            "unrecognized subcommand 'ad'\n\n  tip: a similar subcommand exists: 'add'\n\nUsage: tool tools [OPTIONS] [COMMAND]\n\nFor more information, try '--help'.",
        );
        assert.equal(error.command_path, ["tools"]);
        assert.equal(_error(_demo(), ["tools", "add"]).command_path, ["tools", "add"]);
        let flag = _error(_demo(), ["check", "--bakend", "u"]);
        assert.is_true(flag.message.includes("tip: a similar argument exists: '--backend'"));
        let elsewhere = _error(_demo(), ["--backend", "u", "check"]);
        assert.is_true(elsewhere.message.includes("tip: 'check --backend' exists"))
    }

    test "the unknown argument is reported even when a value is still missing" {
        let error = _error(_demo(), ["check", "--base", "--nope"]);
        assert.equal(error.kind, ErrorKind.UnknownArgument)
    }

    test "a required subcommand that is missing is its own kind" {
        let cmd = Command.new("tool").subcommand_required(true).subcommand(Command.new("run"));
        let error = _error(cmd, []);
        assert.equal(error.kind, ErrorKind.MissingSubcommand);
        assert.equal(
            error.message.split("\n\n")[0],
            "'tool' requires a subcommand but one was not provided\n  [subcommands: run, help]",
        );
        assert.is_true(error.message.includes("Usage: tool <COMMAND>"))
    }

    test "requires makes another arg required" {
        let cmd = Command.new("tool").arg(Arg.new("out").long("out").requires("format")).arg(
            Arg.new("format").long("format"),
        );
        assert.equal(_error(cmd, ["--out", "x"]).kind, ErrorKind.MissingRequiredArgument);
        assert.equal(
            cmd.try_get_matches_from(["--out", "x", "--format", "json"]).get_one("format"),
            "json",
        )
    }
}

testset "help and version" {
    test "help and version are raised as errors that exit zero" {
        let help = _error(_demo(), ["check", "--help"]);
        assert.equal(help.kind, ErrorKind.DisplayHelp);
        assert.equal(help.exit_code(), 0);
        assert.equal(help.use_stderr(), false);
        assert.equal(help.command_path, ["check"]);
        let version = _error(_demo(), ["--version"]);
        assert.equal(version.kind, ErrorKind.DisplayVersion);
        assert.equal(version.message, "tool 1.2.3");
        assert.equal(_error(_demo(), ["-V"]).message, "tool 1.2.3")
    }

    test "generated help is laid out like clap's" {
        assert.equal(
            _error(_demo(), ["-h"]).message,
            "Demo tool\n\nUsage: tool [OPTIONS] [COMMAND]\n\nCommands:\n  check  Run a check\n  tools  Manage tools\n  help   Print this message or the help of the given subcommand(s)\n\nOptions:\n      --dir <PATH>   \n  -v, --verbose      \n      --mode <mode>  [default: fast] [possible values: fast, slow]\n  -h, --help         Print help\n  -V, --version      Print version"
                .split("\n")
                .map((line: string) -> string {
                    line.trim_end()
                })
                .join("\n"),
        );
        assert.equal(
            _error(_demo(), ["check", "-h"]).message,
            "Run a check\n\nUsage: tool check [OPTIONS] [file]...\n\nArguments:\n  [file]...\n\nOptions:\n      --backend <URL>\n      --dir <PATH>\n      --base <REF>\n  -v, --verbose\n  -f, --force\n  -l, --limit <limit>\n  -h, --help           Print help",
        )
    }

    test "the help subcommand prints help for the path that follows it" {
        let nested = _error(_demo(), ["help", "tools", "add"]);
        assert.equal(nested.kind, ErrorKind.DisplayHelp);
        assert.is_true(nested.message.includes("Usage: tool tools add [OPTIONS] <name>"));
        assert.equal(nested.command_path, ["tools", "add"]);
        assert.equal(_error(_demo(), ["tools", "help", "add"]).message, nested.message);
        assert.equal(_error(_demo(), ["help", "nope"]).kind, ErrorKind.InvalidSubcommand)
    }

    test "help can be overridden, extended and switched off" {
        let custom = Command.new("tool").override_help("CUSTOM HELP");
        assert.equal(_error(custom, ["--help"]).message, "CUSTOM HELP");
        let extended = Command.new("tool").after_help("See the manual.");
        assert.is_true(_error(extended, ["-h"]).message.ends_with("\n\nSee the manual."));
        let disabled = Command.new("tool").disable_help_flag(true);
        assert.equal(_error(disabled, ["--help"]).kind, ErrorKind.UnknownArgument);
        let hidden = Command.new("tool").arg(Arg.new("secret").long("secret").hide(true));
        assert.equal(_error(hidden, ["-h"]).message.includes("--secret"), false);
        assert.equal(hidden.try_get_matches_from(["--secret", "x"]).get_one("secret"), "x")
    }
}

testset "global args" {
    test "a global is accepted at every level and reads the same from each" {
        for (
            let args in [
                ["--dir", "x", "tools", "list"],
                ["tools", "--dir", "x", "list"],
                ["tools", "list", "--dir=x"],
            ]
        ) {
            let m = _demo().try_get_matches_from(args);
            assert.equal(m.get_one("dir"), "x");
            assert.equal(_sub(m).get_one("dir"), "x");
            assert.equal(_sub(_sub(m)).get_one("dir"), "x");
        }
        assert.equal(_demo().try_get_matches_from(["tools", "list"]).get_one("dir"), null)
    }

    test "given at two levels, the deeper value wins" {
        let m = _demo().try_get_matches_from(["--dir", "outer", "tools", "--dir", "inner", "list"]);
        assert.equal(m.get_one("dir"), "inner");
        assert.equal(_sub(_sub(m)).get_one("dir"), "inner")
    }

    test "given twice at one level is a conflict" {
        let error = _error(_demo(), ["tools", "--dir", "a", "--dir", "b"]);
        assert.equal(error.kind, ErrorKind.ArgumentConflict)
    }

    test "an arg that is not global stays with its command" {
        assert.equal(_error(_demo(), ["check", "--mode", "slow"]).kind, ErrorKind.UnknownArgument)
    }
}

testset "external subcommands" {
    let _host = () -> Command {
        Command.new("host")
            .allow_external_subcommands(true)
            .arg(Arg.new("dir").long("dir").global(true))
            .subcommand(Command.new("own"))
    };

    test "an undeclared word is a subcommand whose arguments are kept verbatim" {
        let m = _host().try_get_matches_from(["build", "--release", "-j4", "--dir", "x", "--", "y"]);
        assert.equal(m.subcommand_name(), "build");
        assert.is_true(_sub(m).is_external());
        assert.equal(_sub(m).external_args(), ["--release", "-j4", "--dir", "x", "--", "y"]);
        // Plain clap: the arguments are not read, so no global is found in them.
        assert.equal(m.get_one("dir"), null);
        assert.equal(_sub(_host().try_get_matches_from(["own"])).is_external(), false)
    }

    test "an option the command lacks is an error even in front of an external subcommand" {
        assert.equal(_error(_host(), ["-q", "build"]).kind, ErrorKind.UnknownArgument)
    }

    test "after a double dash, a word is still an external subcommand" {
        let m = _host().try_get_matches_from(["--", "own", "x"]);
        assert.equal(m.subcommand_name(), "own");
        assert.is_true(_sub(m).is_external());
        assert.equal(_sub(m).external_args(), ["x"])
    }
}

// The one setting that goes beyond clap.
testset "external subcommand passthrough" {
    let _launcher = () -> Command {
        Command.new("run")
            .external_subcommand_passthrough(true)
            .arg(Arg.new("dir").long("dir").global(true))
            .arg(Arg.new("format").long("format").short("F").global(true))
            .arg(Arg.new("local").long("local"))
            .subcommand(Command.new("tools").subcommand(Command.new("list")))
    };

    test "global args are read out of the external subcommand's arguments" {
        let m = _launcher().try_get_matches_from(
            ["build", "--release", "--dir", "x", "-Fjson", "--local", "y"],
        );
        assert.equal(m.get_one("dir"), "x");
        assert.equal(m.get_one("format"), "json");
        let attached = _launcher().try_get_matches_from(["build", "-F=json"]);
        assert.equal(attached.get_one("format"), "json");
        // Only globals: `--local` is the launcher's own, so here it is the external program's.
        assert.equal(m.get_one("local"), null);
        assert.equal(_sub(m).external_args(), ["--release", "--dir", "x", "-Fjson", "--local", "y"])
    }

    test "the value among the external arguments wins over one given before the subcommand" {
        let m = _launcher().try_get_matches_from(["--dir", "outer", "build", "--dir=inner"]);
        assert.equal(m.get_one("dir"), "inner")
    }

    test "a double dash ends the reading, and nothing is read after a leading one" {
        let stopped = _launcher().try_get_matches_from(["build", "--", "--dir", "x"]);
        assert.equal(stopped.get_one("dir"), null);
        let escaped = _launcher().try_get_matches_from(["--", "build", "--dir", "x"]);
        assert.equal(escaped.subcommand_name(), "build");
        assert.equal(escaped.get_one("dir"), null)
    }

    test "a bad global among the external arguments is an error named after the subcommand" {
        let missing = _error(_launcher(), ["build", "--dir"]);
        assert.equal(missing.kind, ErrorKind.InvalidValue);
        assert.equal(missing.command_path, ["build"])
    }

    test "a global given again among the external arguments is a deeper level's, so the last wins" {
        let m = _launcher().try_get_matches_from(["build", "--dir", "a", "deeper", "--dir", "b"]);
        assert.equal(m.get_one("dir"), "b");
        assert.equal(_sub(m).external_args(), ["--dir", "a", "deeper", "--dir", "b"])
    }

    test "options the launcher lacks are accepted only in front of an external subcommand" {
        let m = _launcher().try_get_matches_from(["-q", "--color", "build"]);
        assert.equal(m.subcommand_name(), "build");
        assert.equal(_error(_launcher(), ["-q", "tools", "list"]).kind, ErrorKind.UnknownArgument);
        assert.equal(_error(_launcher(), ["--color"]).kind, ErrorKind.UnknownArgument)
    }

    test "the external program's own help flag is left for it" {
        let m = _launcher().try_get_matches_from(["build", "--help"]);
        assert.equal(_sub(m).external_args(), ["--help"])
    }

    test "a long external argv is read in place" {
        let files = baml.Array.generate(20000, (i: int) -> string {
            `file${i}.txt`
        });
        let m = _launcher().try_get_matches_from(["build"].concat(files).concat(["--dir", "x"]));
        assert.equal(_sub(m).external_args().length(), 20002);
        assert.equal(m.get_one("dir"), "x")
    }
}

testset "suggestions" {
    test "jaro similarity matches the reference values" {
        assert.equal(Suggestions.jaro("", ""), 1.0);
        assert.equal(Suggestions.jaro("a", ""), 0.0);
        assert.equal(Suggestions.jaro("same", "same"), 1.0);
        assert.approx_equal(Suggestions.jaro("martha", "marhta"), 0.9444, 0.0001);
        assert.approx_equal(Suggestions.jaro("dixon", "dicksonx"), 0.7667, 0.0001);
        assert.approx_equal(Suggestions.jaro("dri", "dir"), 0.5556, 0.0001)
    }

    test "only similar candidates are suggested, most similar last" {
        assert.equal(Suggestions.did_you_mean("ad", ["add", "list", "added"]), ["added", "add"]);
        assert.equal(Suggestions.did_you_mean("zzz", ["add", "list"]), [])
    }
}
