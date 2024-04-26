use serde_json::json;

use crate::generate::{
    dir_writer::WithFileContent,
    generate_ts_client::ts_language_features::ToTypeScript,
    ir::{repr, Function, FunctionArgs, Walker},
};

use super::{
    field_type::walk_custom_types,
    template::render_with_hbs,
    ts_language_features::{TSFileCollector, TSLanguageFeatures},
};

impl WithFileContent<TSLanguageFeatures> for Walker<'_, &Function> {
    fn file_dir(&self) -> &'static str {
        "."
    }

    fn file_name(&self) -> String {
        "function".into()
    }

    fn write(&self, collector: &mut TSFileCollector) {
        let file = collector.start_file(self.file_dir(), self.file_name(), false);

        match self.inputs() {
            either::Either::Left(FunctionArgs::UnnamedArg(arg)) => {
                walk_custom_types(arg).for_each(|t| {
                    file.add_import("./types", t, None, false);
                });
            }
            either::Either::Left(FunctionArgs::NamedArgList(args))
            | either::Either::Right(args) => {
                args.iter().for_each(|(_, r#type)| {
                    walk_custom_types(r#type).for_each(|t| {
                        file.add_import("./types", t, None, false);
                    });
                });
            }
        }

        walk_custom_types(self.output()).for_each(|t| {
            file.add_import("./types", t, None, false);
        });

        let params = match self.inputs() {
            either::Either::Left(FunctionArgs::UnnamedArg(arg)) => json!({
                "positional": true,
                "name": "arg",
                "type": arg.to_ts(),
            }),
            either::Either::Left(FunctionArgs::NamedArgList(args))
            | either::Either::Right(args) => json!({
                "positional": false,
                "name": "args",
                "values": args.iter().map(|(name, r#type)| json!({
                    "name": name.clone(),
                    "type": r#type.to_ts(),
                })).collect::<Vec<_>>(),
            }),
        };

        let function_content = json!({
          "name": self.name(),
          "params": params,
          "return_type": self.output().to_ts(),
          "impls": match self.elem() {
            repr::Function::V1(f) => f.impls.iter().map(|i| i.elem.name.clone()).collect::<Vec<_>>(),
            repr::Function::V2(f) => f.configs.iter().map(|c| c.name.clone()).collect::<Vec<_>>(),
          },
          "default_impl": match self.elem() {
            repr::Function::V1(f) => f.default_impl.clone(),
            repr::Function::V2(f) => Some(f.default_config.clone()),
          },
        });

        file.append(render_with_hbs(
            super::template::Template::Function,
            &function_content,
        ));
        file.add_import(
            "@boundaryml/baml-core/ffi_layer",
            "FireBamlEvent",
            None,
            false,
        );
        file.add_import("@boundaryml/baml-core/ffi_layer", "traceAsync", None, false);
        file.add_export(self.name());
        file.add_export(format!("I{}", self.name()));
        file.add_export(format!("{}Function", self.name()));
        collector.finish_file();
    }
}
