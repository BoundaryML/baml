use anyhow::Result;
use std::str::FromStr;

use crate::{errors::CliError, TestArgs};

pub(super) struct FilterArgs {
    includes: Vec<Filter>,
    excludes: Vec<Filter>,
}

impl FilterArgs {
    pub fn from_command(command: &TestArgs) -> Result<FilterArgs> {
        let includes = command
            .include
            .iter()
            .map(|s| Filter::from_string(s))
            .collect::<Result<Vec<_>, CliError>>()?;
        let excludes = command
            .exclude
            .iter()
            .map(|s| Filter::from_string(s))
            .collect::<Result<Vec<_>, CliError>>()?;

        Ok(FilterArgs { includes, excludes })
    }

    pub fn matches_filters(&self, function: &str, test: &str) -> bool {
        let include = self
            .includes
            .iter()
            .any(|filter| filter.matches(function, test));
        let exclude = self
            .excludes
            .iter()
            .any(|filter| filter.matches(function, test));

        match (self.includes.is_empty(), self.excludes.is_empty()) {
            (true, true) => true,
            (true, false) => !exclude,
            (false, true) => include,
            (false, false) => include && !exclude,
        }
    }
}

enum Filter {
    Wildcard(glob::Pattern),
    // Function, Test
    Parts(glob::Pattern, glob::Pattern),
}

impl Filter {
    pub(super) fn from_string(arg: &String) -> Result<Self, CliError> {
        // arg is of the form: "[function]:[test]", if any of the fields are missing, they are
        // replaced with "*"

        if arg.contains(':') {
            let mut parts = arg.split(':');
            let function = parts.next().unwrap_or(Default::default());
            let test = parts.next().unwrap_or(Default::default());

            if parts.next().is_some() {
                panic!("Invalid filter: {}", arg);
            }

            // If any of the fields are missing or empty, replace them with "*"
            let function = if function.is_empty() { "*" } else { function };
            let test = if test.is_empty() { "*" } else { test };

            let function = glob::Pattern::from_str(function)?;
            let test = glob::Pattern::from_str(test)?;

            Ok(Filter::Parts(function, test))
        } else {
            // If the string does not contain any glob characters, add * to the beginning and end
            let glob_chars = ['*', '?', '[', ']'];
            if !arg.chars().any(|c| glob_chars.contains(&c)) {
                return Ok(Filter::Wildcard(glob::Pattern::from_str(&format!(
                    "*{}*",
                    arg
                ))?));
            }
            Ok(Filter::Wildcard(glob::Pattern::from_str(arg)?))
        }
    }

    pub(super) fn matches(&self, function: &str, test: &str) -> bool {
        match self {
            Filter::Wildcard(s) => s.matches(function) || s.matches(test),
            Filter::Parts(f, t) => f.matches(function) && t.matches(test),
        }
    }
}
