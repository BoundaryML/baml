use serde_json::json;

use crate::generate::{
    dir_writer::WithFileContent,
    generate_ts_client::{field_type::to_parse_expression, ts_language_features::ToTypeScript},
    ir::{repr::FunctionConfig, Function, FunctionArgs, Walker},
};

use super::{
    template::render_with_hbs,
    ts_language_features::{TSFileCollector, TSLanguageFeatures},
};

impl WithFileContent<TSLanguageFeatures> for Walker<'_, (&Function, &FunctionConfig)> {
    fn file_dir(&self) -> &'static str {
        "./impls"
    }

    fn file_name(&self) -> String {
        format!("{}_{}", self.item.0.elem.name(), self.item.1.name).to_lowercase()
    }

    fn write(&self, collector: &mut TSFileCollector) {
        let (function, impl_) = self.item;

        let file = collector.start_file(self.file_dir(), self.file_name(), false);
        file.add_import("../client", impl_.client.clone(), None, false);
        file.add_import("../function", function.elem.name().clone(), None, false);
        file.add_import(
            "@boundaryml/baml-core/deserializer/deserializer",
            "Deserializer",
            None,
            false,
        );
        file.add_import("../json_schema", "schema", None, false);

        let function_content = json!({
          "name": function.elem.name(),
          "params": match function.elem.inputs() {
            either::Either::Left(FunctionArgs::UnnamedArg(arg)) => json!({
                "positional": true,
                "name": "arg",
                "type": arg.to_ts(),
                "expr": to_parse_expression(&"arg".to_string(), arg, file),
              }),
            either::Left(FunctionArgs::NamedArgList(args)) |
            either::Either::Right(args) => json!({
                "positional": false,
                "name": "args",
                "values": args.iter().map(|(name, r#type)| json!({
                  "name": name.clone(),
                  "type": r#type.to_ts(),
                  "expr": to_parse_expression(&format!("args.{}", name), r#type, file),
                })).collect::<Vec<_>>(),
            }),
          },
          "return_type": function.elem.output().to_ts()
        });

        file.append(render_with_hbs(
            super::template::Template::DefaultImpl,
            &json!({
                "function": function_content,
                "name": impl_.name,
                "prompt": impl_.prompt_template.replace("`", "\\`"),
                "client": impl_.client,
                "output_schema": impl_.output_schema,
                "template_macros": [],
            }),
        ));
        collector.finish_file();

        let file = collector.start_file(self.file_dir(), "index", false);
        file.append(format!("import './{}';", self.file_name()));
        collector.finish_file();
    }
}
