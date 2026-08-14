//! Web TypeScript SDK emitter for browser and workerd runtimes.
//!
//! The generated surface intentionally stays identical to the Node.js SDK.
//! Only the runtime bridge package differs.

use std::{collections::HashMap, path::PathBuf};

use baml_codegen_types::SymbolPool;
pub use baml_codegen_types::{NamingConvention, OutputType};

const RUNTIME_PACKAGE: &str = "@boundaryml/baml-bridge-web";

pub fn to_source_code(
    pool: &SymbolPool,
    baml_bytecode: &[u8],
    naming_convention: NamingConvention,
) -> HashMap<PathBuf, String> {
    crate::to_source_code(
        pool,
        baml_bytecode,
        naming_convention,
        crate::GeneratorConfig::new(RUNTIME_PACKAGE),
    )
}

pub fn to_source_code_with_bytecode(
    pool: &SymbolPool,
    baml_bytecode: &[u8],
    naming_convention: NamingConvention,
) -> HashMap<PathBuf, String> {
    to_source_code(pool, baml_bytecode, naming_convention)
}

pub fn to_source_code_with_bytecode_and_metadata(
    pool: &SymbolPool,
    baml_bytecode: &[u8],
    embedded_baml_toml: &str,
    naming_convention: NamingConvention,
) -> HashMap<PathBuf, String> {
    crate::to_source_code_with_metadata(
        pool,
        baml_bytecode,
        Some(embedded_baml_toml),
        naming_convention,
        crate::GeneratorConfig::new(RUNTIME_PACKAGE),
    )
}

#[cfg(test)]
mod tests {
    use baml_base::Name as BaseName;
    use baml_codegen_types::{Class, Name, Origin, Symbol};

    use super::*;

    fn runtime_owned_pool() -> SymbolPool {
        let mut pool = SymbolPool::new();
        for (root, namespace, class_name) in [
            ("baml", "media", "Image"),
            ("baml", "media", "Audio"),
            ("baml", "media", "Video"),
            ("baml", "media", "Pdf"),
            ("ai", "stream", "Stream"),
        ] {
            let name = Name::new(
                BaseName::new(root),
                vec![BaseName::new(namespace)],
                BaseName::new(class_name),
            );
            pool.insert(
                name.clone(),
                Symbol::Class(Class {
                    name,
                    generic_params: Vec::new(),
                    docstring: None,
                    properties: Vec::new(),
                    static_methods: Vec::new(),
                    instance_methods: Vec::new(),
                    origin: Origin {
                        source_file_path: "builtins.baml".to_string(),
                        span_start: 0,
                    },
                }),
            );
        }
        pool
    }

    #[test]
    fn generated_sdk_uses_web_runtime_package() {
        let output = to_source_code(&SymbolPool::new(), &[], NamingConvention::PreserveCase);
        let root = &output[&PathBuf::from("index.ts")];
        let typemap = &output[&PathBuf::from("_typemap.ts")];

        assert!(root.contains(
            "//  $ pnpm add @boundaryml/baml-bridge-web\n\
             //  $ npm install @boundaryml/baml-bridge-web\n\
             //  $ yarn add @boundaryml/baml-bridge-web"
        ));
        assert!(!root.contains("baml package"));
        assert!(root.contains("from \"@boundaryml/baml-bridge-web\";"));
        assert!(typemap.contains("from \"@boundaryml/baml-bridge-web\";"));
        assert!(!root.contains("from \"@boundaryml/baml-bridge\";"));
        assert!(!typemap.contains("from \"@boundaryml/baml-bridge\";"));
    }

    #[test]
    fn generated_builtin_leaves_use_web_constructor_identity() {
        let output = to_source_code(&runtime_owned_pool(), &[], NamingConvention::PreserveCase);
        let media = &output[&PathBuf::from("baml/media/index.ts")];
        let stream = &output[&PathBuf::from("ai/stream/index.ts")];
        let typemap = &output[&PathBuf::from("_typemap.ts")];
        for (runtime_name, local_name) in [
            ("BamlImage", "Image"),
            ("BamlAudio", "Audio"),
            ("BamlVideo", "Video"),
            ("BamlPdf", "Pdf"),
        ] {
            assert!(media.contains(&format!(
                "import {{ {runtime_name} as {local_name} }} from \"{RUNTIME_PACKAGE}\";"
            )));
        }
        assert!(stream.contains(&format!(
            "import {{ BamlStream as Stream }} from \"{RUNTIME_PACKAGE}\";"
        )));
        for fqn in [
            "baml.media.Image",
            "baml.media.Audio",
            "baml.media.Video",
            "baml.media.Pdf",
            baml_base::qualified_name::AI_STREAM_STREAM,
        ] {
            assert!(typemap.contains(&format!("\"{fqn}\": () => (__leaf_")));
        }
        assert!(
            output
                .values()
                .all(|source| !source.contains("from \"@boundaryml/baml-bridge\";"))
        );
    }
}
