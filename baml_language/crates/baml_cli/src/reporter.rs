//! Cargo-style status reporting for BAML commands.

use std::{cell::RefCell, time::Instant};

pub use baml_shell::CLAP_STYLING;
use baml_shell::Shell;
use indicatif::HumanDuration;

pub struct Reporter {
    started: Instant,
    shell: RefCell<Shell>,
}

impl Reporter {
    pub fn new() -> Self {
        Self {
            started: Instant::now(),
            shell: RefCell::new(Shell::new()),
        }
    }

    pub fn status(&self, verb: &str, msg: impl AsRef<str>) {
        drop(self.shell.borrow_mut().status(verb, msg.as_ref()));
    }

    pub fn spin(&self, verb: &str, msg: impl AsRef<str>) {
        self.status(verb, msg);
    }

    pub fn finish(&self, verb: &str, msg: impl AsRef<str>) {
        let elapsed = HumanDuration(self.started.elapsed());
        drop(
            self.shell
                .borrow_mut()
                .status(verb, format_args!("{} in {elapsed:#}", msg.as_ref())),
        );
    }

    pub fn abandon(&self) {}

    pub fn suspend<R>(&self, f: impl FnOnce() -> R) -> R {
        f()
    }

    pub fn error(&self, msg: impl std::fmt::Display) {
        self.abandon();
        drop(self.shell.borrow_mut().error(msg));
    }

    pub fn warning(&self, msg: impl std::fmt::Display) {
        drop(self.shell.borrow_mut().warn(msg));
    }
}

impl Default for Reporter {
    fn default() -> Self {
        Self::new()
    }
}

pub use baml_exec::{print_anyhow_error, print_error, print_note, print_warning};
