use serde_json::json;

use crate::generate::{
    dir_writer::WithFileContentPy as WithFileContent,
    generate_python_client::{field_type::default_value, python_language_features::ToPython},
    ir::{FieldType, Function, FunctionArgs, Walker},
};

use super::{
    field_type::{self, walk_custom_types},
    python_language_features::{PythonFileCollector, PythonLanguageFeatures},
    template::render_with_hbs,
};

impl WithFileContent<PythonLanguageFeatures> for Walker<'_, &Function> {
    fn file_dir(&self) -> &'static str {
        "."
    }

    fn file_name(&self) -> String {
        "function".into()
    }

    fn write(&self, collector: &mut PythonFileCollector) {
        let file = collector.start_file(self.file_dir(), self.file_name(), false);

        match &self.elem().inputs {
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
        walk_custom_types(&self.elem().output.elem).for_each(|t| {
            file.add_import("./types", t, None, false);
        });

        let function_content = json!({
          "name": self.elem().name.clone(),
          // add the json for unnamed args
          "unnamed_args": match &self.elem().inputs {
            FunctionArgs::UnnamedArg(_) => true,
            _ => false,
          },
          "args": match &self.elem().inputs {
            FunctionArgs::NamedArgList(args) => {
                Some(args.iter().map(|(name, r#type)| json!({
                    "name": name.clone(),
                    "type": r#type.to_py(),
                    "default": default_value(r#type)
                })).collect::<Vec<_>>())
            }
            FunctionArgs::UnnamedArg(arg) => {
                Some(vec![json!({
                    "positional": true,
                    "name": "arg",
                    "type": arg.to_py(),
                    "default": default_value(arg)
                })])
            },
          },

          "return_type": self.elem().output.elem.to_py(),
          "return_type_partial": self.elem().output.elem.to_py(),
          "impls": self.elem().impls.iter().map(|i| i.elem.name.clone()).collect::<Vec<_>>(),
          "has_impls": !self.elem().impls.is_empty(),
          "default_impl": self.elem().default_impl,
        });

        // println!("function_content: {:#?}", function_content);

        file.append(render_with_hbs(
            super::template::Template::Function,
            &function_content,
        ));
        file.append(render_with_hbs(
            super::template::Template::FunctionPYI,
            &function_content,
        ));
        file.add_import(
            "@boundaryml/baml-core/ffi_layer",
            "FireBamlEvent",
            None,
            false,
        );
        file.add_import("@boundaryml/baml-core/ffi_layer", "traceAsync", None, false);
        file.add_export(self.elem().name.clone());
        file.add_export(format!("I{}", self.elem().name));
        file.add_export(format!("{}Function", self.elem().name));
        collector.finish_file();
    }
}
