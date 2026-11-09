// The command line of a test binary (`baml pack --tests`): the selection
// flags `baml test` accepts, parsed through clap for the same `--help` /
// unknown-flag handling as every other packed binary.

use clap::{Arg, ArgAction, Command};

use crate::clap_target::{CLAP_STYLING, leak};

/// A test binary's parsed command line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TestArgv {
    /// `-i`/`--include` and the positional selectors, in command-line order.
    pub include: Vec<String>,
    /// `-x`/`--exclude`.
    pub exclude: Vec<String>,
    /// `--max-concurrency`; `None` leaves the runner's default.
    pub max_concurrency: Option<u32>,
}

/// Build the test binary's clap command.
pub fn build_test_command(bin_name: &str) -> Command {
    Command::new(leak(bin_name.to_string()))
        .styles(CLAP_STYLING)
        .about("Run the packed tests and print a report")
        .after_long_help(
            "SELECTORS:\n  \
             Test IDs are case-sensitive and canonical: `root[.namespace]::testset::test`.\n  \
             Plain selectors match anywhere in the full ID. A selector containing `*` is\n  \
             an anchored full-ID glob, and `*` also matches `::`. Repeated includes are OR.\n  \
             Excludes always win. With no includes, every non-excluded test is selected.\n  \
             The project's default test profile, applied at pack time, selects first;\n  \
             these selectors narrow what it selected.",
        )
        .arg(
            Arg::new("filter")
                .value_name("FILTER")
                .action(ArgAction::Append)
                .help("Include tests matching a canonical-ID selector. Repeatable."),
        )
        .arg(
            Arg::new("include")
                .short('i')
                .long("include")
                .value_name("SELECTOR")
                .action(ArgAction::Append)
                .help("Include tests matching a canonical-ID selector. Repeatable."),
        )
        .arg(
            Arg::new("exclude")
                .short('x')
                .long("exclude")
                .value_name("SELECTOR")
                .action(ArgAction::Append)
                .help("Exclude tests matching a selector. Exclusions take precedence."),
        )
        .arg(
            Arg::new("max_concurrency")
                .long("max-concurrency")
                .value_name("N")
                .value_parser(clap::value_parser!(u32).range(1..))
                .help("Run at most this many test bodies at once [default: twice the CPU parallelism]"),
        )
}

/// Parse the tokens after the binary name.
pub fn parse_test_argv(bin_name: &str, tokens: &[String]) -> Result<TestArgv, clap::Error> {
    let matches = build_test_command(bin_name).try_get_matches_from(
        std::iter::once(bin_name.to_string()).chain(tokens.iter().cloned()),
    )?;
    let strings = |id: &str| -> Vec<String> {
        matches
            .get_many::<String>(id)
            .map(|values| values.cloned().collect())
            .unwrap_or_default()
    };
    let mut include = strings("filter");
    include.extend(strings("include"));
    Ok(TestArgv {
        include,
        exclude: strings("exclude"),
        max_concurrency: matches.get_one::<u32>("max_concurrency").copied(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(tokens: &[&str]) -> Result<TestArgv, clap::Error> {
        let tokens: Vec<String> = tokens.iter().map(|s| (*s).to_string()).collect();
        parse_test_argv("./tests", &tokens)
    }

    #[test]
    fn no_tokens_selects_everything() {
        assert_eq!(parse(&[]).unwrap(), TestArgv::default());
    }

    #[test]
    fn positionals_and_flags_select() {
        let parsed = parse(&[
            "root::unit::*",
            "-x",
            "::live::",
            "--include",
            "slow",
            "--exclude=flaky",
            "--max-concurrency",
            "4",
            "other",
        ])
        .unwrap();
        assert_eq!(parsed.include, ["root::unit::*", "other", "slow"]);
        assert_eq!(parsed.exclude, ["::live::", "flaky"]);
        assert_eq!(parsed.max_concurrency, Some(4));
    }

    #[test]
    fn rejects_unknown_flags_and_zero_concurrency() {
        assert!(parse(&["--list"]).is_err());
        assert!(parse(&["--max-concurrency", "0"]).is_err());
    }

    #[test]
    fn help_is_a_display_help_error() {
        let err = parse(&["--help"]).unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::DisplayHelp);
    }
}
