mod docstring;
mod objects;
mod ty;
use std::path::PathBuf;

use crate::objects::{Function, Object};

pub fn to_source_code(
    generators: &baml_codegen_types::SymbolPool,
    _baml_client_path: &std::path::Path,
) -> std::collections::HashMap<PathBuf, String> {
    let mut out = std::collections::HashMap::new();

    // Emit baml_types/ directory tree.
    let type_groups = Object::load_type_groups(generators, false);
    objects::emit_type_tree(&mut out, "baml_types", type_groups);

    // Emit baml_stream_types/ directory tree.
    let stream_type_groups = Object::load_type_groups(generators, true);
    objects::emit_type_tree(&mut out, "baml_stream_types", stream_type_groups);

    // Emit baml_sync/ and baml_async/ directory trees (Phase 4: companion functions).
    let function_groups = Function::load_function_groups(generators);
    objects::emit_function_tree(&mut out, function_groups);

    // Emit the root baml_client/__init__.py (Phase 5).
    out.insert(PathBuf::from("__init__.py"), objects::get_client_init_py());

    out.insert(
        PathBuf::from("runtime.py"),
        include_str!("./_askama/runtime.py").to_string(),
    );
    out.insert(
        PathBuf::from("config.py"),
        include_str!("./_askama/config.py").to_string(),
    );
    out.insert(
        PathBuf::from("globals.py"),
        include_str!("./_askama/globals.py").to_string(),
    );
    out.insert(
        PathBuf::from("tracing.py"),
        include_str!("./_askama/tracing.py").to_string(),
    );

    out
}
