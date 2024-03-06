use serde_json::json;

use crate::generate::{
    dir_writer::WithFileContent,
    generate_ts_client::{field_type::to_parse_expression, ts_language_features::ToTypeScript},
    ir::{Function, FunctionArgs, Impl, Walker},
};

use super::{
    template::render_with_hbs,
    ts_language_features::{TSFileCollector, TSLanguageFeatures},
};

impl WithFileContent<TSLanguageFeatures> for Walker<'_, (&Function, &Impl)> {
    fn file_dir(&self) -> &'static str {
        "./impls"
    }

    fn file_name(&self) -> String {
        format!("{}_{}", self.item.0.elem.name, self.elem().name).to_lowercase()
    }

    fn write(&self, collector: &mut TSFileCollector) {
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
          "params": match &function.elem.inputs {
            FunctionArgs::UnnamedArg(arg) => {
              json!({
                "positional": true,
                "name": "arg",
                "type": arg.to_ts(),
                "expr": to_parse_expression(&"arg".to_string(), arg, file),
              })
            }
            FunctionArgs::NamedArgList(args) => json!({
                "positional": false,
                "name": "args",
                "values": args.iter().map(|(name, r#type)| json!({
                  "name": name.clone(),
                  "type": r#type.to_ts(),
                  "expr": to_parse_expression(&format!("args.{}", name), r#type, file),
                })).collect::<Vec<_>>(),
            }),
          },
          "return_type": function.elem.output.elem.to_ts(),
        });

        let mut prompt = impl_.elem.prompt.clone();
        impl_.elem.output_replacers.iter().for_each(|(k, val)| {
            prompt = prompt.replace(k, &format!("{}", val));
        });
        prompt = prompt.replace("`", "\\`");

        file.append(render_with_hbs(
            super::template::Template::Impl,
            &json!({
                "function": function_content,
                "name": impl_.elem.name.clone(),
                "prompt": prompt,
                "client": impl_.elem.client.clone(),
                "inputs": impl_.elem.input_replacers,
            }),
        ));
        collector.finish_file();

        let file = collector.start_file(self.file_dir(), "index", false);
        file.append(format!("import './{}';", self.file_name()));
        collector.finish_file();
    }
}
