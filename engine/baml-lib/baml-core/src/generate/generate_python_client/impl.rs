use serde_json::json;

use crate::generate::{
    dir_writer::WithFileContentPy as WithFileContent,
    generate_python_client::python_language_features::ToPython,
    ir::{Expression, Function, FunctionArgs, Impl, Prompt, Walker},
};

use super::{
    python_language_features::{PythonFileCollector, PythonLanguageFeatures},
    template::render_with_hbs,
};

impl WithFileContent<PythonLanguageFeatures> for Walker<'_, (&Function, &Impl)> {
    fn file_dir(&self) -> &'static str {
        "./impls"
    }

    fn file_name(&self) -> String {
        format!("{}_{}", self.item.0.elem.name, self.elem().name).to_lowercase()
    }

    fn write(&self, collector: &mut PythonFileCollector) {
        let (function, impl_) = self.item;

        let file = collector.start_file(self.file_dir(), self.file_name(), false);
        file.add_import("../client", impl_.elem.client.clone(), None, false);
        file.add_import("../function", function.elem.name.clone(), None, false);
        file.add_import(
            "@boundaryml/baml-core/deserializer/deserializer",
            "Deserializer",
            None,
            false,
        );
        file.add_import("../json_schema", "schema", None, false);

        let function_content = json!({
          "name": function.elem.name.clone(),
          // add the json for unnamed args
          "unnamed_args": match &function.elem.inputs {
            FunctionArgs::UnnamedArg(_) => true,
            _ => false,
          },
          "args": match &function.elem.inputs {
            FunctionArgs::NamedArgList(args) => {
                Some(args.iter().map(|(name, r#type)| json!({
                    "name": name.clone(),
                    "type": r#type.to_py(),
                    // "default": default_value(r#type)
                })).collect::<Vec<_>>())
            }
            FunctionArgs::UnnamedArg(arg) => {
                Some(vec![json!({
                    "positional": true,
                    "name": "arg",
                    "type": arg.to_py(),
                    // "default": default_value(arg)
                })])
            },
          },

          "return_type": function.elem.output.elem.to_py(),
          "return_type_partial": function.elem.output.elem.to_py(),
          "impls": function.elem.impls.iter().map(|i| i.elem.name.clone()).collect::<Vec<_>>(),
          "has_impls": !function.elem.impls.is_empty(),
          "default_impl": function.elem.default_impl,
        });
        println!("func content: {:#?}", function_content);

        let prompt = &impl_.elem.prompt;

        let is_chat = match &prompt {
            Prompt::Chat(_, _) => true,
            _ => false,
        };

        let impl_content = json!({
            "function": function_content,
            "name": impl_.elem.name.clone(),
            "is_chat": is_chat,
            "prompt": match &prompt {
                Prompt::Chat(messages, _) => {
                    json!(messages.iter().map(|message| {
                        json!({
                            "role": message.role,
                            "content": escape_python_triple_quote(&message.content),
                        })
                    }).collect::<Vec<_>>())
                },
                Prompt::String(content, _) => {
                    json!(escape_python_triple_quote(content))
                },
            },
            "client": impl_.elem.client.clone(),
            "inputs": impl_.elem.input_replacers,
            // add empty override
            "overrides": impl_.elem.overrides.iter().map(|f| {
                json!({
                    "name": f.name,
                    "aliases": f.aliased_keys.iter().map(|aliased_key| {
                        json!({
                            "alias": aliased_key.alias.to_py(),
                            "value": aliased_key.key,
                        })
                    }).collect::<Vec<_>>(),
                })
            }).collect::<Vec<_>>(),
            // "overrides": impl_.attributes.get("overrides").map(|overrides| {
            //     match overrides {
            //         Expression::String(s) => "s",
            //         _ => "s",
            //     }
            //     // overrides.iter().map(|(alias, value)| {
            //     //     json!({
            //     //         "alias": alias,
            //     //         "value": value,
            //     //     })
            //     // }).collect::<Vec<_>>()
            // }),

        });
        println!("impl content: {:#?}", impl_content);

        file.append(render_with_hbs(
            super::template::Template::Impl,
            &impl_content,
        ));
        collector.finish_file();

        let file = collector.start_file(self.file_dir(), "index", false);
        file.append(format!("import './{}';", self.file_name()));
        collector.finish_file();
    }
}

fn escape_python_triple_quote(input: &str) -> String {
    input.replace("\"\"\"", "\\\"\\\"\\\"") // Escape triple double quotes
}
