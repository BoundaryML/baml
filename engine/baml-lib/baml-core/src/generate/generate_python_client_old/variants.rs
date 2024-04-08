use either::Either;
use internal_baml_parser_database::{walkers::VariantWalker, PromptAst, WithStaticRenames};
use internal_baml_schema_ast::ast::WithName;

use serde_json::json;

use crate::generate::generate_python_client_old::file::clean_file_name;

use super::{
    file::File,
    template::render_template,
    traits::{JsonHelper, WithToCode, WithWritePythonString},
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

        let prompt = self.to_prompt();

        let _ = self
            .output_required_classes()
            .map(|cls| f.add_import(&format!("..types.classes.{}", cls.file_name()), cls.name()))
            .collect::<Vec<_>>();
        let _ = self
            .output_required_enums()
            .map(|enm| f.add_import(&format!("..types.enums.{}", enm.file_name()), enm.name()))
            .collect::<Vec<_>>();

        let inputs = match &prompt {
            PromptAst::Chat(_, used_inputs) => used_inputs,
            PromptAst::String(_, used_inputs) => used_inputs,
        };

        let is_chat = match &prompt {
            PromptAst::Chat(..) => true,
            _ => false,
        };

        if is_chat {
            f.add_import("typing", "List");
            f.add_import(
                "baml_core.provider_manager.llm_provider_base",
                "LLMChatMessage",
            );
        }

        json!({
            "name": self.name(),
            "function": func.json(f),
            "is_chat": is_chat,
            "prompt": match &prompt {
                PromptAst::Chat(parts, _) => {


                        json!(parts.iter().map(|(ctx, text)| {
                            let mut new_text = text.clone();
                            for input in inputs.iter() {
                                new_text = new_text.replace(&input.0, &format!("{{{}}}", &input.1));
                            }

                            json!({
                                "role": ctx.map(|c| c.role.0.as_str()).unwrap_or("system"),
                                "content": escape_python_triple_quote(&new_text),
                            })
                        }).collect::<Vec<_>>())
                },
                PromptAst::String(content, _) => {

                    let mut new_content = content.clone();
                    for input in inputs.iter() {
                        new_content = new_content.replace(&input.0, &format!("{{{}}}", &input.1));
                    }

                    json!(escape_python_triple_quote(&new_content))
                },
            },
            "client": client.name(),
            "inputs": inputs.iter().map(|(_, second)| second.clone()).collect::<Vec<_>>(),
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
                            let (overrides, _) = f.get_attributes(Some(self));
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
                            let (overrides, _) = f.get_attributes(Some(self));
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

// Until we move to the IR patch a bug where characters are not escaped prpoerly in the generated prompt:
fn escape_python_triple_quote(input: &str) -> String {
    input.replace("\"\"\"", "\\\"\\\"\\\"") // Escape triple double quotes
}

impl WithWritePythonString for VariantWalker<'_> {
    fn file_name(&self) -> String {
        format!(
            "fx_{}_impl_{}",
            clean_file_name(self.function_identifier().name()),
            clean_file_name(self.name())
        )
    }

    fn write_py_file(&self, fc: &mut FileCollector) {
        fc.start_py_file("impls", "__init__.py");
        fc.last_file().add_line(format!(
            "from .{0} import {1} as unused_{0}",
            self.file_name(),
            self.name(),
        ));
        fc.complete_file();

        fc.start_py_file("impls", self.file_name());
        let json = self.json(fc.last_file());
        render_template(super::template::HSTemplate::Variant, fc.last_file(), json);
        fc.complete_file();
    }
}
