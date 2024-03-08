#![allow(dead_code)]

use serde::Deserialize;
use serde_json::{json, Value};
use std::{path::PathBuf, sync::Arc};
mod jsonschema;

use jsonschema::WithJsonSchema;

use baml_lib::{
    internal_baml_diagnostics::{DatamodelError, DatamodelWarning, Span},
    internal_baml_parser_database::PromptAst,
    internal_baml_schema_ast::ast::{self, WithIdentifier, WithName, WithSpan},
    SourceFile,
};

#[derive(serde::Serialize)]
pub struct StringSpan {
    value: String,
    start: usize,
    end: usize,
    source_file: String,
}

impl StringSpan {
    pub fn new(value: &str, span: &Span) -> Self {
        Self {
            value: value.to_string(),
            start: span.start,
            end: span.end,
            source_file: span.file.path(),
        }
    }
}

#[derive(serde::Serialize)]
pub struct MiniError {
    start: usize,
    end: usize,
    text: String,
    is_warning: bool,
    source_file: String,
}

#[derive(Deserialize)]
struct File {
    path: String,
    content: String,
}

#[derive(Deserialize)]
struct Input {
    root_path: String,
    files: Vec<File>,
}

pub(crate) fn run(input: &str) -> String {
    let input: Input = serde_json::from_str(input).expect("Failed to parse input");

    let files: Vec<SourceFile> = input
        .files
        .into_iter()
        .map(|file| SourceFile::new_allocated(file.path.into(), Arc::from(file.content)))
        .collect();

    let path = PathBuf::from(input.root_path);
    let schema = baml_lib::validate(&path, files);
    let diagnostics = &schema.diagnostics;

    let mut mini_errors: Vec<MiniError> = diagnostics
        .warnings()
        .iter()
        .map(|warn: &DatamodelWarning| MiniError {
            start: warn.span().start,
            end: warn.span().end,
            text: warn.message().to_owned(),
            is_warning: true,
            source_file: warn.span().file.path(),
        })
        .collect();

    if diagnostics.has_errors() {
        mini_errors.extend(
            diagnostics
                .errors()
                .iter()
                .map(|err: &DatamodelError| MiniError {
                    start: err.span().start,
                    end: err.span().end,
                    text: err.message().to_string(),
                    is_warning: false,
                    source_file: err.span().file.path(),
                }),
        );

        return print_diagnostics(mini_errors, None);
    }

    let response = json!({
        "enums": schema.db.walk_enums().map(|e| json!({
            "name": StringSpan::new(e.name(), &e.identifier().span()),
            "jsonSchema": e.json_schema(),
        })).collect::<Vec<_>>(),
        "classes": schema.db.walk_classes().map(|c| json!({
            "name": StringSpan::new(c.name(), &c.identifier().span()),
            "jsonSchema": c.json_schema(),
        })).collect::<Vec<_>>(),
        "clients": schema.db.walk_clients().map(|c| json!({
            "name": StringSpan::new(c.name(), &c.identifier().span()),
        })).collect::<Vec<_>>(),
        "functions": schema
        .db
        .walk_functions()
        .map(|func| {
            json!({
                "name": StringSpan::new(func.name(), &func.identifier().span()),
                "input": match func.ast_function().input() {
                    ast::FunctionArgs::Named(arg_list) => json!({
                        "arg_type": "named",
                        "values": arg_list.args.iter().map(
                            |(id, arg)| json!({
                                "name": StringSpan::new(id.name(), &id.span()),
                                "type": format!("{}", arg.field_type),
                                "jsonSchema": arg.field_type.json_schema()

                            })
                        ).collect::<Vec<_>>(),
                    }),
                    ast::FunctionArgs::Unnamed(arg) => json!({
                        "arg_type": "positional",
                        "type": format!("{}", arg.field_type),
                        "jsonSchema": arg.field_type.json_schema()
                    }),
                },
                "output": match func.ast_function().output() {
                    ast::FunctionArgs::Named(arg_list) => json!({
                        "arg_type": "named",
                        "values": arg_list.args.iter().map(
                            |(id, arg)| json!({
                                "name": StringSpan::new(id.name(), &id.span()),
                                "type": format!("{}", arg.field_type),
                                "jsonSchema": arg.field_type.json_schema()
                            })
                        ).collect::<Vec<_>>(),
                    }),
                    ast::FunctionArgs::Unnamed(arg) => json!({
                        "arg_type": "positional",
                        "type": format!("{}", arg.field_type),
                        "jsonSchema": arg.field_type.json_schema()
                    }),
                },
                "test_cases": func.walk_tests().map(
                    |t| {
                        let props = t.test_case();
                        json!({
                            "name": StringSpan::new(t.name(), &t.identifier().span()),
                            "content": Into::<serde_json::Value>::into(&props.content),
                        })
                    }
                ).collect::<Vec<_>>(),
                "impls": func.walk_variants().map(
                    |i| {
                        let props = i.properties();
                        let prompt = props.to_prompt();
                        let is_chat = match &prompt {
                            PromptAst::Chat(..) => true,
                            _ => false,
                        };
                        json!({
                            "type": "llm",
                            "name": StringSpan::new(i.ast_variant().name(), &i.identifier().span()),
                            "prompt_key": {
                                "start": props.prompt.key_span.start,
                                "end": props.prompt.key_span.end,
                                "source_file": props.prompt.key_span.file.path(),
                            },
                            "has_v2": true,
                            // Passed for legacy reasons
                            "prompt": props.prompt.value,
                            // This is the new value to use
                            "prompt_v2": {
                                "is_chat": is_chat,
                                "prompt": match &prompt {
                                    PromptAst::Chat(parts, _) => {
                                        json!(parts.iter().map(|(ctx, text)| {
                                            json!({
                                                "role": ctx.map(|c| c.role.0.as_str()).unwrap_or("system"),
                                                "content": text,
                                            })
                                        }).collect::<Vec<_>>())
                                    },
                                    PromptAst::String(content, _) => {
                                        json!(content)
                                    },
                                },
                            },
                            "input_replacers": props.replacers.0.iter().map(
                                |r| json!({
                                    "key": r.0.key(),
                                    "value": r.1,
                                })
                            ).collect::<Vec<_>>(),
                            "output_replacers": props.replacers.1.iter().map(
                                |r| json!({
                                    "key": r.0.key(),
                                    "value": r.1,
                                })
                            ).collect::<Vec<_>>(),
                            "client": schema.db.find_client(&props.client.value).map(|c| StringSpan::new(c.name(), &c.identifier().span())).unwrap_or_else(|| StringSpan::new(&props.client.value, &props.client.span)),

                        })
                    }
                ).collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>()
    });

    print_diagnostics(mini_errors, Some(response))
}

fn print_diagnostics(diagnostics: Vec<MiniError>, response: Option<Value>) -> String {
    return json!({
        "ok": response.is_some(),
        "diagnostics": diagnostics,
        "response": response,
    })
    .to_string();
}

#[cfg(test)]
mod tests {
    // use expect_test::expect;
    // use indoc::indoc;

    fn lint(s: &str) -> String {
        let result = super::run(s);
        let value: serde_json::Value = serde_json::from_str(&result).unwrap();

        serde_json::to_string_pretty(&value).unwrap()
    }

    // #[test]
    // fn deprecated_preview_features_should_give_a_warning() {
    //     let dml = indoc! {r#"
    //         datasource db {
    //           provider = "postgresql"
    //           url      = env("DATABASE_URL")
    //         }

    //         generator client {
    //           provider = "prisma-client-js"
    //           previewFeatures = ["createMany"]
    //         }

    //         model A {
    //           id  String   @id
    //         }
    //     "#};

    //     let expected = expect![[r#"
    //         [
    //           {
    //             "start": 149,
    //             "end": 163,
    //             "text": "Preview feature \"createMany\" is deprecated. The functionality can be used without specifying it as a preview feature.",
    //             "is_warning": true
    //           }
    //         ]"#]];

    //     expected.assert_eq(&lint(dml));
    // }
}
