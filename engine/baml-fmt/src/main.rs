#![allow(dead_code)]

use std::{
    io::{self, Read},
    path::PathBuf,
};
use structopt::StructOpt;

#[derive(Debug, StructOpt, Clone)]
#[allow(dead_code)]
pub struct FormatOpts {
    /// Specifies the input file to use. If none is given, the input is read
    /// from STDIN
    #[structopt(short = "i", long)]
    input: Option<PathBuf>,
    /// Specifies the output file to use. If none is given, the output is
    /// written to STDOUT
    #[structopt(short = "o", long)]
    output: Option<PathBuf>,
    /// Specifies which tab width to use when formatting
    #[structopt(short = "s", long, default_value = "2")]
    tabwidth: usize,
}

#[derive(Debug, StructOpt, Clone)]
#[structopt(version = env!("GIT_HASH"))]
/// Prisma Datamodel v2 formatter
pub enum FmtOpts {
    /// Specifies linter mode
    Lint,
    /// Specifiers the generate test code mode (this is temporary until baml cli is ready)
    /// Specifies format mode
    // Format(FormatOpts),
    /// Specifies Native Types mode
    // NativeTypes,
    /// List of available referential actions
    // ReferentialActions,
    // /// Specifies preview features mode
    // PreviewFeatures,
    /// Artificially panic (for testing the CLI)
    DebugPanic,
}

fn main() {
    match FmtOpts::from_args() {
        FmtOpts::DebugPanic => panic!("This is the debugPanic artificial panic"),
        FmtOpts::Lint => panic!("This is the lint panic"),
        // FmtOpts::Format(opts) => format::run(opts),
        // FmtOpts::NativeTypes => plug(native::run),
        // FmtOpts::ReferentialActions => plug(actions::run),
        // FmtOpts::PreviewFeatures => plug(|_s| preview::run()),
    }
}

fn plug<F: Fn(&str) -> String>(f: F) {
    let mut datamodel_string = String::new();

    io::stdin()
        .read_to_string(&mut datamodel_string)
        .expect("Unable to read from stdin.");

    print!("{}", f(&datamodel_string))
}
