use baml_shell::Shell;

pub fn print_error(msg: impl std::fmt::Display) {
    drop(Shell::new().error(msg));
}

pub fn print_warning(msg: impl std::fmt::Display) {
    drop(Shell::new().warn(msg));
}

pub fn print_note(msg: impl std::fmt::Display) {
    drop(Shell::new().note(msg));
}

pub fn print_anyhow_error(err: &anyhow::Error) {
    let mut shell = Shell::new();
    drop(shell.error(err));
    let causes: Vec<_> = err.chain().skip(1).collect();
    if !causes.is_empty() {
        drop(writeln!(shell.err()));
        drop(writeln!(shell.err(), "caused by:"));
        for (i, cause) in causes.iter().enumerate() {
            drop(writeln!(shell.err(), "    {i}: {cause}"));
        }
    }
}
