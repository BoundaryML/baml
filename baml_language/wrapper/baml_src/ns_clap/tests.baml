testset "parser" {
    let _demo_spec = () -> Command {
        Command.new("tool")
            .about("Demo tool")
            .version("1.2.3")
            .subcommand(
                Command.new("check")
                    .about("Run a check")
                    .arg(
                        Arg.opt("backend")
                            .with_value_name("URL")
                            .help("Backend origin")
                            .conflicts_with("base"),
                    )
                    .arg(Arg.opt("base").with_value_name("REF").help("Comparison base"))
                    .arg(Arg.flag("force").with_short("f").help("Force it"))
                    .arg(Arg.pos("file").multiple()),
            )
            .subcommand(Command.new("lint").about("Lint only"))
    };

    let _expect_help = (args: string[]) -> string {
        let result = _demo_spec().parse(args) catch (e) {
            DisplayHelp => e.text,
        };
        match (result) {
            let text: string => text,
            _ => "NO_HELP",
        }
    };

    let _expect_version = (args: string[]) -> string {
        let result = _demo_spec().parse(args) catch (e) {
            DisplayVersion => e.text,
        };
        match (result) {
            let text: string => text,
            _ => "NO_VERSION",
        }
    };

    test "parses values equals shorts and positionals" {
        let m = _demo_spec().parse(["check", "--backend=https://x.example", "-f", "a.txt", "b.txt"]);
        assert.equal(m.subcommand(), "check");
        let sub = m.sub() ?? Matches.empty();
        assert.equal(sub.get("backend"), "https://x.example");
        assert.is_true(sub.is_present("force"));
        assert.equal(sub.get_many("file").join(","), "a.txt,b.txt")
    }

    test "conflicts duplicates and unknown args are typed errors" {
        let conflict = _demo_spec().parse(["check", "--backend", "u", "--base", "main"]) catch (e) {
            ConflictError => `${e.argument}/${e.other}`,
        };
        assert.equal(conflict, "backend/base");
        let dup = _demo_spec().parse(["check", "--base", "a", "--base", "b"]) catch (e) {
            DuplicateArgumentError => e.argument,
        };
        assert.equal(dup, "base");
        let unknown = _demo_spec().parse(["check", "--nope"]) catch (e) {
            UnknownArgumentError => e.argument,
        };
        assert.equal(unknown, "--nope");
        let missing = _demo_spec().parse(["check", "--backend"]) catch (e) {
            MissingValueError => e.message(),
        };
        assert.equal(missing, "a value is required for '--backend <URL>' but none was supplied")
    }

    test "parse_from skips the binary and entry function" {
        let m = _demo_spec().parse_from(["/bin/tool", "main", "check", "-f"]);
        assert.equal(m.subcommand(), "check");
        assert.is_true((m.sub() ?? Matches.empty()).is_present("force"))
    }

    test "double dash stops flag parsing" {
        let m = _demo_spec().parse(["check", "--", "--force"]);
        let sub = m.sub() ?? Matches.empty();
        assert.equal(sub.get_many("file").join(","), "--force");
        assert.equal(sub.is_present("force"), false)
    }

    test "defaults fill values without counting as present" {
        let spec = Command.new("tool").arg(Arg.opt("mode").default("fast").conflicts_with("slow")).arg(
            Arg.flag("slow"),
        );
        let m = spec.parse(["--slow"]);
        assert.equal(m.get("mode"), "fast");
        assert.equal(m.is_present("mode"), false);
        assert.equal(m.value_source("mode"), ValueSource.Default);
        assert.equal(spec.parse(["--mode", "eco"]).value_source("mode"), ValueSource.CommandLine)
    }

    test "help and version are builtin args that can be replaced" {
        let spec = Command.new("tool").version("9.0").arg(
            Arg.flag("version").with_short("V").help("Show version").with_action(ArgAction.Version),
        );
        let version = spec.parse(["-V"]) catch (e) {
            DisplayVersion => e.text,
        };
        assert.equal(version, "tool 9.0");
        let help = spec.parse(["-h"]) catch (e) {
            DisplayHelp => e.text,
        };
        assert.equal(
            help,
            "Usage: tool [OPTIONS]\n\nOptions:\n  -V, --version  Show version\n  -h, --help     Print help",
        );
        let disabled = Command.new("tool").auto_help(false).parse(["--help"]) catch (e) {
            UnknownArgumentError => e.argument,
        };
        assert.equal(disabled, "--help")
    }

    test "help override replaces generated help" {
        let spec = Command.new("tool").help_text("CUSTOM HELP");
        let result = spec.parse(["--help"]) catch (e) {
            DisplayHelp => e.text,
        };
        let text = match (result) {
            let s: string => s,
            _ => "NO_HELP",
        };
        assert.equal(text, "CUSTOM HELP")
    }

    test "help and version are display requests" {
        let help = _expect_help(["check", "--help"]);
        assert.is_true(help.includes("Usage: tool check"));
        assert.is_true(help.includes("--backend <URL>"));
        assert.equal(_expect_version(["--version"]), "tool 1.2.3");
        assert.is_true(_expect_help(["help", "check"]).includes("Run a check"))
    }
}

testset "globals and external subcommands" {
    // A launcher: it owns `tools`, and hands every other command on.
    let _launcher = () -> Command {
        Command.new("run")
            .allow_external_subcommands()
            .arg(Arg.opt("dir").with_value_name("PATH").global())
            .arg(Arg.opt("format").with_short("F").global().hidden())
            .arg(Arg.flag("version").with_short("V"))
            .subcommand(
                Command.new("tools")
                    .about("Manage tools")
                    .subcommand(Command.new("list").about("List tools").arg(Arg.flag("all")))
                    .subcommand(Command.new("add").arg(Arg.pos("name").required())),
            )
    };

    let _sub = (matches: Matches) -> Matches {
        matches.sub() ?? Matches.empty()
    };

    test "a global is accepted at every level and reads the same from each" {
        for (
            let args in [
                ["--dir", "x", "tools", "list"],
                ["tools", "--dir", "x", "list"],
                ["tools", "list", "--dir=x"],
            ]
        ) {
            let m = _launcher().parse(args);
            assert.equal(m.get("dir"), "x");
            assert.equal(_sub(m).get("dir"), "x");
            assert.equal(_sub(_sub(m)).get("dir"), "x");
            assert.is_true(_sub(_sub(m)).is_present("dir"));
        }
        assert.equal(_launcher().parse(["tools", "list"]).get("dir"), null)
    }

    test "a global given at two levels is a duplicate" {
        let duplicate = _launcher().parse(["--dir", "a", "tools", "list", "--dir", "b"]) catch (e) {
            DuplicateArgumentError => e.argument,
        };
        assert.equal(duplicate, "dir")
    }

    test "a non-global option stays with its own command" {
        let unknown = _launcher().parse(["tools", "--all", "list"]) catch (e) {
            UnknownArgumentError => e.argument,
        };
        assert.equal(unknown, "--all")
    }

    test "an undeclared word is an external subcommand with verbatim tokens" {
        let m = _launcher().parse(["build", "--release", "-j4", "target", "--", "--dir", "y"]);
        assert.equal(m.subcommand(), "build");
        assert.is_true(m.is_external());
        assert.equal(_sub(m).external_args, ["--release", "-j4", "target", "--", "--dir", "y"]);
        assert.equal(m.get("dir"), null)
    }

    test "globals are read out of an external subcommand's tokens" {
        let after = _launcher().parse(["build", "--release", "--dir", "x", "-Fjson"]);
        assert.equal(after.get("dir"), "x");
        assert.equal(after.get("format"), "json");
        assert.equal(_sub(after).external_args, ["--release", "--dir", "x", "-Fjson"]);

        let missing = _launcher().parse(["build", "--dir"]) catch (e) {
            MissingValueError => e.argument,
        };
        assert.equal(missing, "dir");
        let duplicate = _launcher().parse(["--dir", "a", "build", "--dir=b"]) catch (e) {
            DuplicateArgumentError => e.argument,
        };
        assert.equal(duplicate, "dir")
    }

    test "undeclared options are tolerated only in front of an external subcommand" {
        let m = _launcher().parse(["-q", "--color", "build"]);
        assert.equal(m.subcommand(), "build");
        assert.equal(m.unknown_options, ["-q", "--color"]);

        let declared = _launcher().parse(["-q", "tools", "list"]) catch (e) {
            UnknownArgumentError => e.argument,
        };
        assert.equal(declared, "-q");
        let none = _launcher().parse(["--color"]) catch (e) {
            UnknownArgumentError => e.argument,
        };
        assert.equal(none, "--color")
    }

    test "help and its own flags are not read out of an external subcommand" {
        let m = _launcher().parse(["build", "--help", "-V"]);
        assert.equal(_sub(m).external_args, ["--help", "-V"]);
        assert.equal(m.is_present("version"), false)
    }

    test "help for an external subcommand is handed on with it" {
        let m = _launcher().parse(["help", "build", "now"]);
        assert.equal(m.subcommand(), "help");
        assert.is_true(m.is_external());
        assert.equal(_sub(m).external_args, ["build", "now"]);
        let own = _launcher().parse(["help", "tools", "add"]) catch (e) {
            DisplayHelp => e.text,
        };
        assert.is_true(own.to_string().includes("Usage: run tools add [OPTIONS] <NAME>"))
    }

    test "an escaped word no positional takes is an external subcommand" {
        let m = _launcher().parse(["--", "tools", "--dir", "x"]);
        assert.equal(m.subcommand(), "tools");
        assert.is_true(m.is_external());
        assert.equal(_sub(m).external_args, ["--dir", "x"]);
        assert.equal(m.get("dir"), null)
    }

    test "after_help follows the generated help and errors render their detail" {
        let spec = Command.new("tool").arg(Arg.flag("x")).after_help("See the manual.");
        assert.equal(
            spec.format_help("tool"),
            "Usage: tool [OPTIONS]\n\nOptions:\n  --x\n  -h, --help  Print help\n\nSee the manual.",
        );
        let unknown = spec.parse(["--nope"]) catch (e) {
            ParseError => e.to_string(),
        };
        assert.equal(unknown, "unexpected argument '--nope' found")
    }

    test "command_path names the subcommands without validating" {
        let spec = _launcher();
        assert.equal(
            spec.command_path(["--dir", "x", "tools", "list", "--bogus"]),
            ["tools", "list"],
        );
        assert.equal(spec.command_path(["tools", "--dir"]), ["tools"]);
        assert.equal(spec.command_path(["tools", "help", "add"]), ["tools", "help", "add"]);
        assert.equal(spec.command_path(["help", "tools", "list"]), ["help", "tools", "list"]);
        assert.equal(spec.command_path(["help", "build", "x"]), ["help", "build"]);
        assert.equal(spec.command_path(["--help", "tools", "list"]), []);
        assert.equal(spec.command_path(["tools", "-h", "list"]), ["tools"]);
        assert.equal(spec.command_path(["-q", "--dir=x", "build", "tools", "list"]), ["build"]);
        assert.equal(spec.command_path(["--", "tools"]), []);
        assert.equal(spec.command_path([]), [])
    }

    test "generated help lists visible globals and nested help walks the path" {
        let help = _launcher().parse(["tools", "list", "--help"]) catch (e) {
            DisplayHelp => e.text,
        };
        assert.equal(
            help,
            "List tools\n\nUsage: run tools list [OPTIONS]\n\nOptions:\n  --all\n  -h, --help    Print help\n  --dir <PATH>",
        );
        assert.equal(_launcher().help_for(["tools", "list"], "run"), help);
        let nested = _launcher().parse(["tools", "help", "list"]) catch (e) {
            DisplayHelp => e.text,
        };
        assert.equal(nested, help);
        let unknown = _launcher().help_for(["tools", "nope"], "run") catch (e) {
            UnknownCommandError => e.argument,
        };
        assert.equal(unknown, "nope")
    }
}
