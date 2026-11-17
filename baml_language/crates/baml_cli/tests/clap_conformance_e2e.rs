//! The wrapper's command-line parser (`wrapper/baml_src/ns_clap`) is a port
//! of clap to BAML. This test holds it to clap: the same commands are built
//! with both, thousands of generated command lines are parsed by both, and
//! every outcome must agree.
//!
//! What must agree:
//! - whether the command line is accepted, and the kind of error if not;
//! - when accepted: the subcommand chosen and every value at every level,
//!   defaults and propagated globals included;
//! - help and version text, ignoring trailing spaces;
//! - the whole error message: the problem, its tips and the usage under it.
//!
//! The port's one feature beyond clap, `external_subcommand_passthrough`,
//! has no clap twin and is covered by the BAML tests next to the parser.

mod common;

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Command as Process,
};

use clap::{Arg, ArgAction, ArgMatches, Command};
use serde::Deserialize;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("crates/baml_cli sits three levels under the repo root")
        .to_path_buf()
}

fn flag(id: &'static str) -> Arg {
    Arg::new(id).action(ArgAction::SetTrue)
}

fn demo() -> Command {
    Command::new("run")
        .about("Demo tool")
        .arg(Arg::new("dir").long("dir").global(true))
        .arg(flag("verbose").long("verbose").short('v').global(true))
        .arg(
            Arg::new("mode")
                .long("mode")
                .value_parser(["fast", "slow"])
                .default_value("fast"),
        )
        .arg(flag("force").long("force").short('f'))
        .subcommand(
            Command::new("tools")
                .about("Manage tools")
                .subcommand(
                    Command::new("list")
                        .arg(flag("all").long("all").short('a'))
                        .arg(Arg::new("limit").long("limit").short('l')),
                )
                .subcommand(
                    Command::new("add")
                        .arg(Arg::new("name").required(true).help("The tool"))
                        .arg(Arg::new("extra").action(ArgAction::Append))
                        .arg(flag("dry").long("dry")),
                ),
        )
        .subcommand(
            Command::new("check")
                .arg(
                    Arg::new("backend")
                        .long("backend")
                        .value_name("URL")
                        .conflicts_with("base"),
                )
                .arg(Arg::new("base").long("base"))
                .arg(Arg::new("file").action(ArgAction::Append)),
        )
}

fn deploy() -> Command {
    Command::new("deploy")
        .about("Ship it")
        .version("2.0")
        .subcommand_required(true)
        .arg(
            Arg::new("env")
                .long("env")
                .short('e')
                .value_name("NAME")
                .required(true)
                .help("Where to"),
        )
        .arg(flag("quiet").short('q'))
        .arg(flag("Force").long("force").short('F'))
        .arg(Arg::new("secret").long("secret").hide(true))
        .arg(Arg::new("out").long("out").requires("format"))
        .arg(
            Arg::new("format")
                .long("format")
                .value_parser(["json", "text"]),
        )
        .subcommand(
            Command::new("push")
                .about("Push a build")
                .arg(Arg::new("target").required(true))
                .arg(
                    Arg::new("tag")
                        .long("tag")
                        .short('t')
                        .action(ArgAction::Append),
                ),
        )
        .subcommand(
            Command::new("status")
                .about("Show status")
                .after_help("See the manual."),
        )
}

fn host() -> Command {
    Command::new("host")
        .allow_external_subcommands(true)
        .disable_help_subcommand(true)
        .arg(Arg::new("dir").long("dir").global(true))
        .arg(flag("verbose").short('v'))
        .subcommand(
            Command::new("own")
                .disable_help_flag(true)
                .arg(Arg::new("level").long("level").allow_hyphen_values(true)),
        )
}

/// A command and the tokens its generated command lines are drawn from:
/// its own vocabulary, near misses, and tokens that are special anywhere.
struct Subject {
    name: &'static str,
    command: fn() -> Command,
    tokens: &'static [&'static str],
}

const ANYWHERE: &[&str] = &[
    "--", "-", "-h", "--help", "help", "x", "y", "5", "--nope", "-z", "",
];

const SUBJECTS: &[Subject] = &[
    Subject {
        name: "demo",
        command: demo,
        tokens: &[
            "tools",
            "list",
            "add",
            "check",
            "tols",
            "ad",
            "--dir",
            "--dir=x",
            "--dir=",
            "-v",
            "--verbose",
            "--mode",
            "fast",
            "slow",
            "bad",
            "--mode=slow",
            "-f",
            "--force",
            "--force=1",
            "-a",
            "--all",
            "--limit",
            "-l",
            "-l5",
            "-l=5",
            "--dry",
            "--backend",
            "--bakend",
            "u",
            "--base",
            "b",
            "-vf",
            "-fv",
            "-va",
            "-fz",
            "a.txt",
        ],
    },
    Subject {
        name: "deploy",
        command: deploy,
        tokens: &[
            "push",
            "status",
            "stat",
            "--env",
            "-e",
            "-eprod",
            "--env=prod",
            "prod",
            "-q",
            "-F",
            "--force",
            "-qF",
            "-Fq",
            "--secret",
            "s",
            "--out",
            "o",
            "--format",
            "json",
            "text",
            "yaml",
            "--format=json",
            "-V",
            "--version",
            "--tag",
            "-t",
            "-tv1",
            "v1",
            "v2",
            "target",
            "-qe",
            "-qeprod",
        ],
    },
    Subject {
        name: "host",
        command: host,
        tokens: &[
            "own",
            "build",
            "test",
            "--dir",
            "--dir=x",
            "-v",
            "--level",
            "-3",
            "--level=-3",
            "--release",
            "-j4",
            "target",
        ],
    },
];

/// Command lines per subject.
const CASES: usize = 4000;

/// A small deterministic generator, so a failure reproduces.
struct Xorshift(u64);

impl Xorshift {
    fn below(&mut self, bound: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        usize::try_from(self.0 % bound as u64).expect("a remainder below a usize fits")
    }
}

fn cases(subject: &Subject) -> Vec<Vec<String>> {
    let tokens: Vec<&str> = subject.tokens.iter().chain(ANYWHERE).copied().collect();
    let mut random = Xorshift(0x9E37_79B9_7F4A_7C15 ^ subject.name.len() as u64);
    (0..CASES)
        .map(|_| {
            let length = random.below(8);
            (0..length)
                .map(|_| tokens[random.below(tokens.len())].to_string())
                .collect()
        })
        .collect()
}

#[derive(Debug, Deserialize, PartialEq)]
struct Outcome {
    kind: String,
    path: Vec<String>,
    values: BTreeMap<String, Vec<String>>,
    text: String,
}

fn collect(
    matches: &ArgMatches,
    path: &mut Vec<String>,
    values: &mut BTreeMap<String, Vec<String>>,
) {
    for id in matches.ids() {
        let raw = matches
            .get_raw(id.as_str())
            .map(|values| {
                values
                    .map(|value| value.to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        values.insert(format!("{}/{id}", path.join(" ")), raw);
    }
    if let Some((name, sub)) = matches.subcommand() {
        path.push(name.to_string());
        collect(sub, path, values);
    }
}

fn clap_outcome(subject: &Subject, args: &[String]) -> Outcome {
    let command = (subject.command)();
    // clap takes the program name from the first argument.
    let argv = std::iter::once(command.get_name().to_string()).chain(args.iter().cloned());
    match command.try_get_matches_from(argv) {
        Ok(matches) => {
            let mut path = Vec::new();
            let mut values = BTreeMap::new();
            collect(&matches, &mut path, &mut values);
            Outcome {
                kind: "ok".to_string(),
                path,
                values,
                text: String::new(),
            }
        }
        Err(error) => {
            let rendered = error.render().to_string();
            Outcome {
                kind: format!("{:?}", error.kind()),
                path: Vec::new(),
                values: BTreeMap::new(),
                text: rendered
                    .strip_prefix("error: ")
                    .unwrap_or(&rendered)
                    .to_string(),
            }
        }
    }
}

/// Text with trailing spaces dropped: clap pads help rows that have no help.
fn tidy(text: &str) -> String {
    text.lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// How the two outcomes differ, or `None` when they agree.
fn difference(baml: &Outcome, clap: &Outcome) -> Option<String> {
    // `ErrorKind.InvalidValue` on one side, `InvalidValue` on the other.
    let kind = baml.kind.rsplit('.').next().unwrap_or(&baml.kind);
    if kind != clap.kind {
        return Some(format!("kind: baml {kind}, clap {}", clap.kind));
    }
    match kind {
        "ok" if (&baml.path, &baml.values) != (&clap.path, &clap.values) => Some(format!(
            "values:\n  baml {:?} {:?}\n  clap {:?} {:?}",
            baml.path, baml.values, clap.path, clap.values
        )),
        "DisplayHelp" | "DisplayVersion" if tidy(&baml.text) != tidy(&clap.text) => Some(format!(
            "text:\n--- baml\n{}\n--- clap\n{}",
            tidy(&baml.text),
            tidy(&clap.text)
        )),
        "ok" | "DisplayHelp" | "DisplayVersion" => None,
        _ if tidy(&baml.text) != tidy(&clap.text) => Some(format!(
            "message:\n--- baml\n{}\n--- clap\n{}",
            tidy(&baml.text),
            tidy(&clap.text)
        )),
        _ => None,
    }
}

/// A project holding the parser and the commands under test.
fn materialize() -> tempfile::TempDir {
    let project = tempfile::tempdir().expect("create a temporary project");
    let parser = project.path().join("baml_src/ns_clap");
    std::fs::create_dir_all(&parser).unwrap();
    std::fs::write(
        project.path().join("baml.toml"),
        "[package]\nname = \"conformance\"\n",
    )
    .unwrap();
    let source = repo_root().join("baml_language/wrapper/baml_src/ns_clap");
    for entry in std::fs::read_dir(&source).expect("the wrapper's parser sources") {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap();
        // The parser's own tests are not part of the parser.
        if name != "tests.baml" {
            std::fs::copy(&path, parser.join(name)).unwrap();
        }
    }
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/clap_conformance/main.baml"),
        project.path().join("baml_src/main.baml"),
    )
    .unwrap();
    project
}

fn baml_outcomes(project: &Path, subject: &Subject, cases: &[Vec<String>]) -> Vec<Outcome> {
    let cases_path = project.join(format!("{}.cases.json", subject.name));
    let out_path = project.join(format!("{}.out.json", subject.name));
    std::fs::write(&cases_path, serde_json::to_string(cases).unwrap()).unwrap();
    let output = Process::new(common::baml_cli())
        .args(["--agent-skill-check", "off", "--color", "never", "run"])
        .arg("--project-dir")
        .arg(project)
        .args(["conformance", "--", "--name", subject.name])
        .arg("--cases_path")
        .arg(&cases_path)
        .arg("--out_path")
        .arg(&out_path)
        .env("BAML_CLI_ALLOW_DIRECT", "1")
        .env("BAML_HOME", common::shared_baml_home())
        .env("BAML_TELEMETRY", "off")
        .env("DO_NOT_TRACK", "1")
        .output()
        .expect("spawn baml-cli run");
    assert!(
        output.status.success(),
        "the BAML parser failed to run:\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    serde_json::from_str(&std::fs::read_to_string(&out_path).unwrap())
        .expect("the BAML outcomes are JSON")
}

#[test]
fn the_baml_parser_agrees_with_clap() {
    let project = materialize();
    // Each distinct difference, with the first command line that shows it
    // and how many do.
    let mut differences: BTreeMap<String, (Vec<String>, usize)> = BTreeMap::new();
    let mut differing = 0;
    for subject in SUBJECTS {
        let cases = cases(subject);
        let baml = baml_outcomes(project.path(), subject, &cases);
        assert_eq!(baml.len(), cases.len(), "{}", subject.name);
        for (args, baml) in cases.iter().zip(&baml) {
            if let Some(difference) = difference(baml, &clap_outcome(subject, args)) {
                differing += 1;
                differences
                    .entry(format!("{}: {difference}", subject.name))
                    .or_insert_with(|| (args.clone(), 0))
                    .1 += 1;
            }
        }
    }
    let report: Vec<String> = differences
        .iter()
        .take(12)
        .map(|(difference, (args, count))| format!("{count} like {args:?}\n{difference}"))
        .collect();
    assert!(
        differences.is_empty(),
        "{differing} of {} command lines parse differently, in {} ways; the first few:\n\n{}",
        SUBJECTS.len() * CASES,
        differences.len(),
        report.join("\n\n"),
    );
}
