use std::collections::HashSet;

use either::Either;
use internal_baml_parser_database::{walkers::VariantWalker, WithStaticRenames};

use serde_json::json;

use internal_baml_schema_ast::ast::WithName;

use super::{
    file::{clean_file_name, File, FileCollector},
    template::{render_template, HSTemplate},
    traits::{JsonHelper, TargetLanguage, WithFileName, WithToCode},
};

impl<'db> JsonHelper for VariantWalker<'db> {
    fn json(&self, f: &mut File, lang: TargetLanguage) -> serde_json::Value {
        let func = self.walk_function().unwrap();
        let client = self.client().unwrap();
        f.add_import(
            &format!("..functions.{}", func.file_name()),
            &format!("BAML{}", func.name()),
        );
        f.add_import(&format!("..clients.{}", client.file_name()), client.name());

        let mut prompt = self.properties().prompt.value.clone();

        let (input, output) = &self.properties().replacers;

        let inputs: HashSet<_> = input
            .iter()
            .map(|(k, val)| {
                prompt = prompt.replace(&k.key(), &format!("{{{}}}", val));
                val
            })
            .collect();
        let mut inputs: Vec<_> = inputs.into_iter().collect();
        inputs.sort();
        output.iter().for_each(|(k, val)| {
            prompt = prompt.replace(&k.key(), &format!("{}", val));
        });

        let _ = self
            .output_required_classes()
            .map(|cls| f.add_import(&format!("..types.classes.{}", cls.file_name()), cls.name()))
            .collect::<Vec<_>>();
        let _ = self
            .output_required_enums()
            .map(|enm| f.add_import(&format!("..types.enums.{}", enm.file_name()), enm.name()))
            .collect::<Vec<_>>();

        json!({
            "name": self.name(),
            "function": func.json(f, lang),
            "prompt": prompt,
            "client": client.name(),
            "inputs": inputs,
            "output_adapter": self.properties().output_adapter.as_ref().map(|(idx, _)| {
                let adapter = &self.ast_variant()[*idx];

                json!({
                    "type": adapter.from.to_py_string(f),
                    "code": self.properties().output_adapter_for_language("python").unwrap_or("raise NotImplementedError()")
                })
            }),
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

impl WithFileName for VariantWalker<'_> {
    fn file_name(&self) -> String {
        format!(
            "fx_{}_impl_{}",
            clean_file_name(self.function_identifier().name()),
            clean_file_name(self.name())
        )
    }

    fn to_py_file(&self, fc: &mut FileCollector) {
        let lang = TargetLanguage::Python;
        fc.start_py_file("impls", "__init__.py");
        fc.last_file().add_line(format!(
            "from .{0} import {1} as unused_{0}",
            self.file_name(),
            self.name(),
        ));
        fc.complete_file();

        fc.start_py_file("impls", self.file_name());
        let json = self.json(fc.last_file(), lang);
        render_template(lang, HSTemplate::Variant, fc.last_file(), json);
        fc.complete_file();
    }

    fn to_ts_file(&self, fc: &mut FileCollector) {
        let lang = TargetLanguage::TypeScript;
        fc.start_ts_file("impls", self.file_name());
        let json = self.json(fc.last_file(), lang);
        render_template(lang, HSTemplate::Variant, fc.last_file(), json);
        fc.complete_file();
    }
}
