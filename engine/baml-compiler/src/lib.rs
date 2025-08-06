pub mod hir;
pub mod thir;

/// Testing utilities.
pub mod test {
    use internal_baml_parser_database::ParserDatabase;

    /// For tests.
    ///
    /// We reuse this in the VM.
    pub fn ast(source: &str) -> anyhow::Result<ParserDatabase> {
        let path = std::path::PathBuf::from("test.baml");
        let source_file = internal_baml_diagnostics::SourceFile::from((path.clone(), source));

        let validated_schema = internal_baml_core::validate(&path, vec![source_file]);

        if validated_schema.diagnostics.has_errors() {
            let errors = validated_schema.diagnostics.to_pretty_string();
            anyhow::bail!("{}", errors);
        }

        Ok(validated_schema.db)
    }
}
