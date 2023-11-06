use std::collections::HashSet;

use either::Either;
use internal_baml_parser_database::{
    walkers::{VariantWalker},
    WithStaticRenames,
};
use internal_baml_schema_ast::ast::{WithName};

use log::info;
use serde_json::json;

use crate::generate::generate_python_client::file::clean_file_name;

use super::{
    file::File,
    template::render_template,
    traits::{JsonHelper, WithWritePythonString},
    FileCollector,
};

impl<'db> JsonHelper for VariantWalker<'db> {
    fn json(&self, f: &mut File) -> serde_json::Value {
        let func = self.walk_function().unwrap();
        let client = self.client().unwrap();
        f.add_import(
            &format!("..functions.{}", func.file_name()),
            &format!("BAML{}", func.name()),
        );
        f.add_import(&format!("..clients.{}", client.file_name()), client.name());

        let mut prompt = self.properties().prompt.value.clone();

        let (input, output) = &self.properties().replacers;

        let inputs = input
            .iter()
            .map(|(k, val)| {
                prompt = prompt.replace(&k.key(), &format!("{{{}}}", val));
                val
            })
            .collect::<HashSet<_>>();
        output.iter().for_each(|(k, val)| {
            prompt = prompt.replace(&k.key(), &format!("{}", val));
        });

        json!({
            "name": self.identifier().name(),
            "function": func.json(f),
            "prompt": prompt,
            "client": client.name(),
            "inputs": inputs,
            "overrides": self.ast_variant().iter_serializers().filter_map(|(_k, v)| {
                let matches = match self.db.find_type_by_str(v.name()) {
                    Some(Either::Left(cls)) => {
                        cls.static_fields().filter_map(|f| {
                            let (overrides, _) = f.get_attributes(self);
                            match overrides.and_then(|o| Some(o.alias())) {
                                Some(Some(id)) => {
                                    Some(json!({
                                        "alias": self.db[*id].to_string(),
                                        "value": f.name(),
                                    }))
                                },
                                _ => {
                                    None
                                }
                            }
                        }).collect::<Vec<_>>()
                    },
                    Some(Either::Right(enm)) => {
                        info!("Skipping variant {}", v.name());
                        enm.values().filter_map(|f| {
                            let (overrides, _) = f.get_attributes(self);
                            match overrides.and_then(|o| Some(o.alias())) {
                                Some(Some(id)) => {
                                    Some(json!({
                                        "alias": self.db[*id].to_string(),
                                        "value": f.name(),
                                    }))
                                },
                                _ => {
                                    None
                                }
                            }
                        }).collect::<Vec<_>>()
                    },
                    None => {
                        Vec::new()
                    }
                };

                if matches.is_empty() {
                    None
                } else {
                    Some(json!({
                        "name": v.name(),
                        "aliases": matches,
                    }))
                }
            }).collect::<Vec<_>>(),
        })
    }
}

impl WithWritePythonString for VariantWalker<'_> {
    fn file_name(&self) -> String {
        format!(
            "fx_{}_impl_{}",
            clean_file_name(self.function_identifier().name()),
            clean_file_name(self.identifier().name())
        )
    }

    fn write_py_file(&self, fc: &mut FileCollector) {
        fc.start_py_file("impls", "__init__.py");
        fc.last_file().add_line(format!(
            "from .{0} import {1} as unused_{0}",
            self.file_name(),
            self.identifier().name(),
        ));
        fc.complete_file();

        fc.start_py_file("impls", self.file_name());
        let json = self.json(fc.last_file());
        render_template(super::template::HSTemplate::Variant, fc.last_file(), json);
        fc.complete_file();
    }
}
