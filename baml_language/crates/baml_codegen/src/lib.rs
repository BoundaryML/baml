//! Code generation for BAML.
//!
//! Generates bytecode and other target outputs from THIR.

use baml_base::SourceFile;
use baml_workspace::ProjectRoot;

mod bytecode;
pub use bytecode::*;

/// Tracked: generate bytecode for a file
#[salsa::tracked]
pub fn generate_file_bytecode(_db: &dyn salsa::Database, _file: SourceFile) -> BytecodeModule {
    // TODO: Implement bytecode generation
    // Would use HIR and THIR information to generate bytecode
    BytecodeModule {
        instructions: vec![],
        constants: vec![],
    }
}

/// Tracked: generate bytecode for entire project
#[salsa::tracked]
pub fn generate_project_bytecode(db: &dyn salsa::Database, root: ProjectRoot) -> BytecodeModule {
    // TODO: Combine bytecode from all files
    let files = baml_workspace::project_files(db, root);

    let mut combined = BytecodeModule {
        instructions: vec![],
        constants: vec![],
    };

    for file in files {
        let module = generate_file_bytecode(db, file);
        // TODO: Merge modules properly
        combined.instructions.extend(module.instructions);
        combined.constants.extend(module.constants);
    }

    combined
}
