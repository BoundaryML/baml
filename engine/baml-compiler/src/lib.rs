pub mod builtin;
pub mod hir;
pub mod thir;
pub mod codegen;

pub use codegen::compile;

use anyhow::Context;

#[cfg(test)]
mod test {
    use internal_baml_diagnostics::Diagnostics;
    use internal_baml_parser_database::ParserDatabase;
    use internal_baml_parser_database::{parse, parse_and_diagnostics};

    /// Shim helper function for testing.
    pub fn ast(source: &'static str) -> anyhow::Result<ParserDatabase> {
        let parser_db = parse(source).expect("Failed to parse source");
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