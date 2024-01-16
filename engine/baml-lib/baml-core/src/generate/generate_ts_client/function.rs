use chrono::format;
use serde_json::json;

use crate::generate::{
    dir_writer::WithFileContent,
    generate_ts_client::ts_language_features::ToTypeScript,
    ir::{Function, FunctionArgs},
};

use super::{
    field_type::walk_custom_types,
    template::render_with_hbs,
    ts_language_features::{TSFileCollector, TSLanguageFeatures},
};

impl WithFileContent<TSLanguageFeatures> for Function {
    fn file_dir(&self) -> &'static str {
        "."
    }

    fn file_name(&self) -> String {
        "function".into()
    }

    fn write(&self, collector: &mut TSFileCollector) {
        let file = collector.start_file(self.file_dir(), self.file_name(), false);

        match &self.elem.inputs {
            FunctionArgs::UnnamedArg(arg) => {
                walk_custom_types(arg).for_each(|t| {
                    file.add_import("./types", t, None, false);
                });
            }
            FunctionArgs::NamedArgList(args) => {
                args.iter().for_each(|(_, r#type)| {
                    walk_custom_types(r#type).for_each(|t| {
                        file.add_import("./types", t, None, false);
                    });
                });
            }
        }
        walk_custom_types(&self.elem.output.elem).for_each(|t| {
            file.add_import("./types", t, None, false);
        });

        let function_content = json!({
          "name": self.elem.name.clone(),
          "params": match &self.elem.inputs {
            FunctionArgs::UnnamedArg(arg) => {
              json!({
                "positional": true,
                "name": "arg",
                "type": arg.to_ts(),
              })
            }
            FunctionArgs::NamedArgList(args) => json!({
                "positional": false,
                "name": "args",
                "values": args.iter().map(|(name, r#type)| json!({
                  "name": name.clone(),
                  "type": r#type.to_ts(),
                })).collect::<Vec<_>>(),
            }),
          },
          "return_type": self.elem.output.elem.to_ts(),
          "impls": self.elem.impls.iter().map(|i| i.elem.name.clone()).collect::<Vec<_>>(),
          "default_impl": self.elem.default_impl,
        });

        file.append(render_with_hbs(
            super::template::Template::Function,
            &function_content,
        ));
        file.add_export(self.elem.name.clone());
        file.add_export(format!("I{}", self.elem.name));
        file.add_export(format!("{}Function", self.elem.name));
        collector.finish_file();
    }
}
