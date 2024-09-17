//! An end-to-end demonstration of compiling .BAML code and generating
//! a client SDK.
//!
//! This code is not meant to be used in production. It is intended to
//! serve as documentation for contributors to the compiler who want to
//! understand its structure.
//!

use std::path::PathBuf;

use baml_lib::{
    parse_and_validate_schema, SourceFile, ValidatedSchema
};
use baml_lib::internal_baml_parser_database::ParserDatabase;
use internal_baml_core::ir::repr::IntermediateRepr;
use baml_runtime::BamlRuntime;
use internal_baml_codegen::GenerateOutput;

pub fn run_end_to_end() {

    // ************************ BAML-CLI ******************************* //
    // The first three operations are unually orchestrated by the
    // baml-cli executable.

    // Import our example .baml file, which contains
    // an LLM client, a schema, a function and a test.
    let baml_dir = PathBuf::from("./baml_src");
    let src_files = vec![PathBuf::from("./baml_src/example00_receipt.baml")];
    let sources = src_files
        .iter()
        .map(|path| SourceFile::from((
            path.clone(),
            std::fs::read_to_string(path).unwrap()
        )))
        .collect::<Vec<_>>();

    // Generate an abstract syntax tree (AST) by parsing the sources.
    let parsed: ValidatedSchema = parse_and_validate_schema(
        &baml_dir,
        sources
    ).expect("These test data should parse and validate.");

    // Inspect the result of parsing and validation: the ParserDatabase.
    let db: &ParserDatabase = &parsed.db;
    dbg!(db.ast());

    // Generate typed intermediate representation (IR) from the parsed AST.
    let ir = IntermediateRepr::from_parser_database(
        &parsed.db,
        parsed.configuration
    );
    dbg!(&ir);

    // Compile the IR into a client SDK.
    let runtime = BamlRuntime::from_directory(&baml_dir, std::env::vars().collect()).expect("Should work");
    let named_sources = src_files.iter().map(|path| (path.clone(), std::fs::read_to_string(path).expect("Path exists"))).collect();
    let generated: Vec<GenerateOutput> = runtime.run_generators(&named_sources, false).expect("Generation should work");
    // dbg!(generated);

    // let generate_args = GenerateArgs {
    //     from: "./baml_src".to_string(),
    //     no_version_check: true,
    // };
    // generate_args.run(CallerType::Python).expect("Generation should succeed");

}
