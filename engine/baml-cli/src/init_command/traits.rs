use std::{io::Write, path::PathBuf};

use crate::errors::CliError;

pub(super) struct Writer {
    term: console::Term,
    no_prompt: bool,
}

impl Writer {
    pub(super) fn new(no_prompt: bool) -> Self {
        Self {
            term: console::Term::stdout(),
            no_prompt,
        }
    }

    pub(super) fn write_fmt(&mut self, args: std::fmt::Arguments) -> std::io::Result<()> {
        if self.no_prompt {
            return Ok(());
        }

        self.term.write_fmt(args)?;

        // Sleep for 200ms to make the user read the message
        std::thread::sleep(std::time::Duration::from_millis(200));
        Ok(())
    }
}

pub(super) trait WithLoader<T> {
    fn from_dialoguer(
        no_prompt: bool,
        project_root: &PathBuf,
        writer: &mut Writer,
    ) -> Result<T, CliError>;
}

pub(super) trait ToBamlSrc {
    fn to_baml(&self) -> String;
}

pub(super) trait WithLanguage {
    fn install_command(&self) -> String;
    fn test_command<T: AsRef<str>>(&self, prefix: Option<T>) -> String;
    fn package_version_command(&self) -> String;
}
