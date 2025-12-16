pub mod builtin;
pub mod codegen;
pub mod hir;
pub mod thir;
pub mod viz;
mod viz_builder;
pub mod watch;

pub use codegen::compile;

pub mod test {
    use internal_baml_diagnostics::Diagnostics;
    use internal_baml_parser_database::{parse_and_diagnostics, ParserDatabase};

    use crate::{hir, thir};

    /// Shim helper function for testing.
    pub fn ast(source: &'static str) -> anyhow::Result<ParserDatabase> {
        let (parser_db, mut diagnostics) = parse_and_diagnostics(source)?;

        if diagnostics.has_errors() {
            let errors = diagnostics.to_pretty_string();
            anyhow::bail!("{errors}");
        }

        // Here because of cycle dependencies between crates and shit.
        // TODO: We're building this like 3 different times, needs refactoring.
        thir::typecheck::typecheck(&hir::Hir::from_ast(&parser_db.ast), &mut diagnostics);
        if diagnostics.has_errors() {
            let errors = diagnostics.to_pretty_string();
            anyhow::bail!("{errors}");
        }

        Ok(parser_db)
    }

    /// Shim helper function for testing.
    pub fn ast_and_diagnostics(
        source: &'static str,
    ) -> anyhow::Result<(ParserDatabase, Diagnostics)> {
        let (parser_db, diagnostics) =
            parse_and_diagnostics(source).expect("Failed to parse source");
        Ok((parser_db, diagnostics))
    }
}
