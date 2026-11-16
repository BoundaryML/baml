// The BAML half of `clap_conformance_e2e.rs`. Each command here has a
// twin built with clap in that test; keep the two in step.

class Outcome {
    // "ok", or the error kind.
    kind: string,
    // The subcommands chosen, or the command that raised the error.
    path: string[],
    // Every value at every level, keyed `<path>/<id>`.
    values: map<string, string[]>,
    // The help or version text, or the error message.
    text: string,
}

function demo() -> root.clap.Command {
    root.clap.Command.new("run")
        .about("Demo tool")
        .arg(root.clap.Arg.new("dir").long("dir").global(true))
        .arg(
            root.clap.Arg.new("verbose")
                .long("verbose")
                .short("v")
                .action(root.clap.ArgAction.SetTrue)
                .global(true),
        )
        .arg(
            root.clap.Arg.new("mode").long("mode").value_parser(["fast", "slow"]).default_value(
                "fast",
            ),
        )
        .arg(
            root.clap.Arg.new("force").long("force").short("f").action(root.clap.ArgAction.SetTrue),
        )
        .subcommand(
            root.clap.Command.new("tools")
                .about("Manage tools")
                .subcommand(
                    root.clap.Command.new("list")
                        .arg(
                            root.clap.Arg.new("all").long("all").short("a").action(
                                root.clap.ArgAction.SetTrue,
                            ),
                        )
                        .arg(root.clap.Arg.new("limit").long("limit").short("l")),
                )
                .subcommand(
                    root.clap.Command.new("add")
                        .arg(root.clap.Arg.new("name").required(true).help("The tool"))
                        .arg(root.clap.Arg.new("extra").action(root.clap.ArgAction.Append))
                        .arg(
                            root.clap.Arg.new("dry").long("dry").action(root.clap.ArgAction.SetTrue),
                        ),
                ),
        )
        .subcommand(
            root.clap.Command.new("check")
                .arg(
                    root.clap.Arg.new("backend").long("backend").value_name("URL").conflicts_with(
                        "base",
                    ),
                )
                .arg(root.clap.Arg.new("base").long("base"))
                .arg(root.clap.Arg.new("file").action(root.clap.ArgAction.Append)),
        )
}

function deploy() -> root.clap.Command {
    root.clap.Command.new("deploy")
        .about("Ship it")
        .version("2.0")
        .subcommand_required(true)
        .arg(
            root.clap.Arg.new("env").long("env").short("e").value_name("NAME").required(true).help(
                "Where to",
            ),
        )
        .arg(root.clap.Arg.new("quiet").short("q").action(root.clap.ArgAction.SetTrue))
        .arg(
            root.clap.Arg.new("Force").long("force").short("F").action(root.clap.ArgAction.SetTrue),
        )
        .arg(root.clap.Arg.new("secret").long("secret").hide(true))
        .arg(root.clap.Arg.new("out").long("out").requires("format"))
        .arg(root.clap.Arg.new("format").long("format").value_parser(["json", "text"]))
        .subcommand(
            root.clap.Command.new("push")
                .about("Push a build")
                .arg(root.clap.Arg.new("target").required(true))
                .arg(
                    root.clap.Arg.new("tag").long("tag").short("t").action(
                        root.clap.ArgAction.Append,
                    ),
                ),
        )
        .subcommand(
            root.clap.Command.new("status").about("Show status").after_help("See the manual."),
        )
}

function host() -> root.clap.Command {
    root.clap.Command.new("host")
        .allow_external_subcommands(true)
        .disable_help_subcommand(true)
        .arg(root.clap.Arg.new("dir").long("dir").global(true))
        .arg(root.clap.Arg.new("verbose").short("v").action(root.clap.ArgAction.SetTrue))
        .subcommand(
            root.clap.Command.new("own").disable_help_flag(true).arg(
                root.clap.Arg.new("level").long("level").allow_hyphen_values(true),
            ),
        )
}

function command(name: string) -> root.clap.Command {
    match (name) {
        "demo" => demo(),
        "deploy" => deploy(),
        "host" => host(),
        _ => throw baml.errors.InvalidArgument { message: `no command named ${name}` },
    }
}

function collect(
    matches: root.clap.ArgMatches,
    path: string[],
    values: map<string, string[]>,
) -> null {
    for (let id in matches.args.keys()) {
        values.set(`${path.join(" ")}/${id}`, matches.get_many(id));
    }
    if let sub: root.clap.SubCommand = matches.sub {
        collect(sub.matches, path.concat([sub.name]), values);
    }
    null
}

function outcome(name: string, args: string[]) -> Outcome {
    let parsed = command(name).try_get_matches_from(args) catch (error) {
        root.clap.Error => error,
    };
    match (parsed) {
        let error: root.clap.Error => {
            Outcome {
                kind: error.kind.to_string(),
                path: error.command_path,
                values: {},
                text: error.message,
            }
        },
        let matches: root.clap.ArgMatches => {
            let values: map<string, string[]> = {};
            collect(matches, [], values);
            Outcome { kind: "ok", path: matches.subcommand_path(), values: values, text: "" }
        },
    }
}

// Parses every command line in `cases_path` with the command `name`
// and writes the outcomes, in order, to `out_path`.
function conformance(name: string, cases_path: string, out_path: string) -> int {
    let cases = baml.json.decode<string[][]>(baml.fs.read(cases_path));
    baml.fs.write(
        out_path,
        baml.json.stringify(
            cases.map((args: string[]) -> Outcome {
                outcome(name, args)
            }),
        ),
    );
    cases.length()
}
