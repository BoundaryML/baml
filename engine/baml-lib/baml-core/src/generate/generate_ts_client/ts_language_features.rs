use std::collections::{HashMap, HashSet};

use crate::generate::dir_writer::{FileCollector, Import, LanguageFeatures, LibImport};

pub(super) struct TSLanguageFeatures {}

impl LanguageFeatures for TSLanguageFeatures {
    fn content_prefix(&self) -> &'static str {
        r#"
// This file is auto-generated. Do not edit this file manually.
//
// Disable formatting for this file to avoid linting errors.
// tslint:disable
// @ts-nocheck
/* eslint-disable */
        "#
        .trim()
    }

    fn format_exports(&self, exports: &Vec<String>) -> String {
        format!("export {{ {} }}", exports.join(", "))
    }

    fn format_imports(&self, libs: &HashSet<LibImport>, imports: &Vec<Import>) -> String {
        let lib_imports = libs.iter().fold(String::new(), |mut buffer, lib| {
            buffer.push_str(&format!(
                "import {} from '{}';\n",
                lib.as_name.as_ref().unwrap_or(&lib.lib),
                lib.lib
            ));
            buffer
        });

        // group imports by lib
        let mut imports_by_lib = imports
            .iter()
            .fold(HashMap::new(), |mut map, import| {
                let imports = map.entry(&import.lib).or_insert(HashSet::new());
                imports.insert(import);
                map
            })
            .into_iter()
            .collect::<Vec<_>>();
        imports_by_lib.sort_by(|a, b| a.0.cmp(b.0));

        let imports = imports_by_lib
            .iter()
            .fold(String::new(), |mut buffer, (lib, imports)| {
                let mut sorted_imports: Vec<_> = imports.iter().collect();
                sorted_imports.sort_by(|a, b| a.name.cmp(&b.name));

                buffer.push_str(&format!(
                    "import {{ {} }} from '{}';\n",
                    sorted_imports
                        .iter()
                        .fold(String::new(), |mut buffer, import| {
                            match import.as_name {
                                Some(ref as_name) => {
                                    buffer.push_str(&format!("{} as {}", import.name, as_name));
                                }
                                None => {
                                    buffer.push_str(&import.name);
                                }
                            }
                            buffer.push_str(", ");
                            buffer
                        })
                        .trim_end_matches(", "),
                    lib
                ));
                buffer
            });

        format!("{}\n{}", lib_imports, imports)
    }

    fn to_file_path(&self, path: &str, name: &str) -> std::path::PathBuf {
        std::path::PathBuf::from(format!("{}/{}.ts", path, name).to_lowercase())
    }
}

pub(super) trait ToTypeScript {
    fn to_ts(&self) -> String;
}

pub(super) type TSFileCollector = FileCollector<TSLanguageFeatures>;

pub(super) fn get_file_collector() -> TSFileCollector {
    TSFileCollector::new(TSLanguageFeatures {})
}
