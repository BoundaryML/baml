use internal_baml_parser_database::ParserDatabase;
use internal_baml_schema_ast::ast::WithName;
use serde_json::json;

use crate::generate::generate_python_client::{FileCollector, WithToCode};

use super::{
    template::{render_template, HSTemplate},
    TestRequest,
};

impl TestRequest {
    pub fn generate_python(self, db: &ParserDatabase) -> Result<String, Vec<String>> {
        let mut diagnostics = vec![];
        let mut fc = FileCollector::default();

        fc.start_py_file(".", "test_generated.py");

        let content = fc.last_file();
        self.functions.iter().for_each(|f| {
            if let Some(func) = db.find_function_by_name(&f.name) {
                content.add_import("baml_client.baml_types", &format!("I{}", func.name()));
                func
                    .walk_input_args()
                    .for_each(|a| {
                        a.required_classes().for_each(|c| {
                            content.add_import("baml_client.baml_types", &c.name());
                        });
                        a.required_enums()
                            .for_each(|e| content.add_import("baml_client.baml_types", &e.name()));
                    });


                f.tests.iter().for_each(|t| {
                     match &t.params {
                        super::TestParam::Positional(p) => {
                            let input_args = func.find_input_arg_by_position(0).map(|a| a.ast_arg().1.to_py_string(content)).unwrap_or("str".into());
                            render_template(
                                HSTemplate::SingleArgTestSnippet,
                                content,
                                json!({
                                    "function_name": func.name(),
                                    "test_case_name": t.name,
                                    "test_case_input": p,
                                    "test_case_type": input_args,
                                }),
                            );
                        }
                        super::TestParam::Keyword(args) => {
                            let data = json!({
                                "function_name": func.name(),
                                "test_case_name": t.name,
                                "test_case_input": args.iter().map(|(k, v)| json!({ 
                                    "name": k, "value": v,
                                    "type": func.find_input_arg_by_name(k).map(|a| a.ast_arg().1.to_py_string(content)).unwrap_or("str".into()),
                                 })).collect::<Vec<_>>(),
                            });
                            render_template(
                                HSTemplate::MultiArgTestSnippet,
                                content,
                                data,
                            );
                        }
                     }
                });
            } else {
                diagnostics.push(format!("Function {} not found", f.name));
            }
        });

        let content = content.content();
        fc.complete_file();

        if diagnostics.len() > 0 {
            return Err(diagnostics);
        }

        Ok(content)
    }
}
