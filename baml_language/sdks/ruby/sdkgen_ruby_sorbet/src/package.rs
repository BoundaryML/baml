use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt::Write as _,
    path::PathBuf,
};

use crate::{
    emit::Emitter,
    names::{quote, snake_case},
};

pub(crate) fn emit(
    namespaces: BTreeMap<String, String>,
    emitter: &Emitter<'_>,
    bytecode: &[u8],
    metadata: Option<&str>,
) -> HashMap<PathBuf, String> {
    let mut root = String::from(
        "# typed: strict\n# frozen_string_literal: true\nrequire \"baml/bridge\"\nrequire \"sorbet-runtime\"\nrequire_relative \"baml_sdk/__bytecode\"\nmodule BamlSdk\n  extend ::T::Sig\n  sig { returns(::NilClass) }\n  def self.initialize!\n    ::Baml::Bridge.initialize!\n    nil\n  end\nend\n",
    );
    // Runtime shells preserve forward/recursive references. Create them dynamically
    // so Sorbet sees each T::Struct/T::Enum declaration once, with all its fields;
    // a normal empty T::Struct declaration synthesizes a zero-argument constructor.
    let mut modules = BTreeSet::new();
    for module in namespaces.keys() {
        let parts: Vec<_> = module.split("::").collect();
        for end in 2..=parts.len() {
            modules.insert(parts[..end].join("::"));
        }
    }
    for module in modules {
        writeln!(root, "module {module}; end").unwrap();
    }
    root.push_str(&emitter.shells);
    root.push_str(&emitter.aliases);
    let mut files = HashMap::new();
    for (module, body) in namespaces {
        // Ruby constants never start with underscores, so namespaces cannot
        // produce the reserved declarations, bytecode or vendor filenames.
        // Vendored packages avoid `vendor/`: Tapioca's default Sorbet config
        // ignores every directory with that name.
        let relative = module.strip_prefix("BamlSdk::").map_or_else(
            || "__root".to_owned(),
            |namespace| {
                namespace
                    .split("::")
                    .map(snake_case)
                    .enumerate()
                    .map(|(depth, part)| {
                        if depth == 0 && part == "vendor" {
                            "__vendor".to_owned()
                        } else {
                            part
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("/")
            },
        );
        let path = format!("baml_sdk/{relative}.rb");
        let body = body.lines().fold(String::new(), |mut out, line| {
            writeln!(out, "  {line}").unwrap();
            out
        });
        insert(
            &mut files,
            &path,
            format!(
                "# typed: strict\n# frozen_string_literal: true\nmodule {module}\n  extend ::T::Sig\n{body}end\n"
            ),
        );
        writeln!(
            root,
            "require_relative {}",
            quote(path.strip_suffix(".rb").unwrap())
        )
        .unwrap();
    }
    root.push_str("::Baml::Bridge.register_program(::BamlSdk::BYTECODE, ::BamlSdk::METADATA)\n");
    root.push_str(&emitter.registry);
    insert(&mut files, "baml_sdk.rb", root);
    let bytes = baml_sdkgen_types::embedded_bytecode_base64(bytecode);
    let metadata = metadata.map_or_else(|| "nil".into(), |s| format!("{}.freeze", quote(s)));
    insert(
        &mut files,
        "baml_sdk/__bytecode.rb",
        format!(
            "# typed: strict\n# frozen_string_literal: true\nmodule BamlSdk\n  BYTECODE = ::T.let(\"{bytes}\".b.freeze, ::String)\n  METADATA = ::T.let({metadata}, ::T.nilable(::String))\nend\n"
        ),
    );
    files
}

fn insert(files: &mut HashMap<PathBuf, String>, path: &str, source: String) {
    match files.entry(PathBuf::from(path)) {
        std::collections::hash_map::Entry::Vacant(entry) => {
            entry.insert(source);
        }
        std::collections::hash_map::Entry::Occupied(_) => {
            panic!("duplicate Ruby output path: {path}");
        }
    }
}
