#![allow(dead_code)]

use internal_baml_jinja::{
    render_prompt, RenderContext, RenderContext_Client, RenderedChatMessage,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::HashMap, path::PathBuf, sync::Arc};
mod jsonschema;

use jsonschema::WithJsonSchema;

use baml_lib::{
    internal_baml_diagnostics::{DatamodelError, DatamodelWarning},
    internal_baml_parser_database::{
        serialize_with_printer,
        walkers::{FunctionWalker, VariantWalker},
        PromptAst, WithSerialize,
    },
    internal_baml_schema_ast::ast::{self, WithIdentifier, WithName, WithSpan},
    SourceFile, ValidatedSchema,
};

#[derive(Serialize)]
pub struct StringSpan {
    value: String,
    start: usize,
    end: usize,
    source_file: String,
}

impl StringSpan {
    pub fn new(value: &str, span: &baml_lib::internal_baml_diagnostics::Span) -> Self {
        Self {
            value: value.to_string(),
            start: span.start,
            end: span.end,
            source_file: span.file.path(),
        }
    }
}

#[derive(Serialize)]
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
            "name": StringSpan::new(e.name(), e.identifier().span()),
            "jsonSchema": e.json_schema(),
        })).collect::<Vec<_>>(),
        "classes": schema.db.walk_classes().map(|c| json!({
            "name": StringSpan::new(c.name(), c.identifier().span()),
            "jsonSchema": c.json_schema(),
        })).collect::<Vec<_>>(),
        "clients": schema.db.walk_clients().map(|c| json!({
            "name": StringSpan::new(c.name(), c.identifier().span()),
        })).collect::<Vec<_>>(),
        "functions": std::iter::empty()
            .chain(schema.db.walk_old_functions().map(|f| serialize_function(&schema, f, SFunctionSyntax::Version1)))
            .chain(schema.db.walk_new_functions().map(|f| serialize_function(&schema, f, SFunctionSyntax::Version2)))
            .collect::<Vec<_>>(),
    });

    print_diagnostics(mini_errors, Some(response))
}

// keep in sync with typescript/common/src/parser_db.ts
#[derive(Serialize)]
struct Span {
    start: usize,
    end: usize,
    source_file: String,
}

impl From<&baml_lib::internal_baml_diagnostics::Span> for Span {
    fn from(span: &baml_lib::internal_baml_diagnostics::Span) -> Self {
        Self {
            start: span.start,
            end: span.end,
            source_file: span.file.path(),
        }
    }
}

#[derive(Serialize)]
enum SFunctionSyntax {
    Version1, // "impl<llm, ClassifyResume>"
    Version2, // functions and impls are collapsed into a single function Name(args) -> Output {...}
}

#[derive(Serialize)]
struct SFunction {
    name: StringSpan,
    input: serde_json::Value,
    output: serde_json::Value,
    test_cases: Vec<serde_json::Value>,
    impls: Vec<Impl>,
    syntax: SFunctionSyntax,
}

fn serialize_function(
    schema: &ValidatedSchema,
    func: FunctionWalker,
    syntax: SFunctionSyntax,
) -> SFunction {
    SFunction {
        name: StringSpan::new(func.name(), func.identifier().span()),
        input: match func.ast_function().input() {
            ast::FunctionArgs::Named(arg_list) => json!({
                "arg_type": "named",
                "values": arg_list.args.iter().map(
                    |(id, arg)| json!({
                        "name": StringSpan::new(id.name(), id.span()),
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
        output: match func.ast_function().output() {
            ast::FunctionArgs::Named(arg_list) => json!({
                "arg_type": "named",
                "values": arg_list.args.iter().map(
                    |(id, arg)| json!({
                        "name": StringSpan::new(id.name(), id.span()),
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
        test_cases: func
            .walk_tests()
            .map(|t| {
                let props = t.test_case();
                json!({
                    "name": StringSpan::new(t.name(), t.identifier().span()),
                    "content": Into::<serde_json::Value>::into(&props.content).to_string(),
                })
            })
            .collect::<Vec<_>>(),
        impls: serialize_impls(&schema, func),
        syntax: syntax,
    }
}

// keep in sync with typescript/common/src/parser_db.ts
#[derive(Serialize)]
#[serde(tag = "type")] // JSON is { "type": "completion", "completion": "..." }
enum PromptPreview {
    Completion { completion: String },
    Chat { chat: Vec<RenderedChatMessage> },
    Error { error: String },
}

// keep in sync with typescript/common/src/parser_db.ts
#[derive(Serialize)]
struct Impl {
    name: StringSpan,
    prompt_key: Span,
    prompt: PromptPreview,
    client: StringSpan,
    input_replacers: Vec<(String, String)>,
    output_replacers: Vec<(String, String)>,
}

fn apply_replacers(variant: VariantWalker, mut content: String) -> String {
    let (input_replacers, output_replacers, _) = &variant.properties().replacers;
    for (input_var, input_replacement) in input_replacers {
        content = content.replace(&input_var.key(), &format!("{{{input_replacement}}}"));
    }
    for (output_var, output_replacement) in output_replacers {
        content = content.replace(&output_var.key(), &format!("{output_replacement}"));
    }
    content
}

fn serialize_impls(schema: &ValidatedSchema, func: FunctionWalker) -> Vec<Impl> {
    if func.is_old_function() {
        func.walk_variants()
            .map(|i| {
                let props = i.properties();
                Impl {
                    name: StringSpan::new(i.ast_variant().name(), i.identifier().span()),
                    prompt_key: (&props.prompt.key_span).into(),
                    prompt: match props.to_prompt() {
                        PromptAst::String(content, _) => PromptPreview::Completion {
                            completion: apply_replacers(i, content.clone()),
                        },
                        PromptAst::Chat(parts, _) => PromptPreview::Chat {
                            chat: parts
                                .iter()
                                .map(|(ctx, text)| RenderedChatMessage {
                                    role: ctx
                                        .map(|c| c.role.0.as_str())
                                        .unwrap_or("system")
                                        .to_string(),
                                    message: apply_replacers(i, text.clone()),
                                })
                                .collect::<Vec<_>>(),
                        },
                    },
                    client: schema
                        .db
                        .find_client(&props.client.value)
                        .map(|c| StringSpan::new(c.name(), c.identifier().span()))
                        .unwrap_or_else(|| {
                            StringSpan::new(&props.client.value, &props.client.span)
                        }),
                    input_replacers: vec![],
                    output_replacers: vec![],
                }
            })
            .collect::<Vec<_>>()
    } else {
        let prompt = func.metadata().prompt.as_ref().unwrap();
        let (client_name, client_span) = func.metadata().client.as_ref().unwrap();
        let client_walker = schema.db.find_client(client_name);

        let client = client_walker
            .map(|c| StringSpan::new(c.name(), c.identifier().span()))
            .unwrap_or_else(|| StringSpan::new(client_name, client_span));
        let args = func
            .walk_tests()
            .nth(0)
            .map(
                |t| match Into::<serde_json::Value>::into(&t.test_case().content) {
                    serde_json::Value::Object(map) => map,
                    _ => serde_json::Map::new(),
                },
            )
            .unwrap_or(serde_json::Map::new());
        let output_schema = match func.ast_function().output() {
            ast::FunctionArgs::Named(arg_list) => {
                // TODO(sam): handle multiple named args in... a return type?
                format!("{{{{ Failed to render output schema: multiple named return types }}}}")
            }
            ast::FunctionArgs::Unnamed(arg) => {
                let identifiers = arg
                    .field_type
                    .flat_idns()
                    .iter()
                    .map(|i| i.name())
                    .collect::<Vec<_>>();
                let class = schema
                    .db
                    .walk_classes()
                    // TODO(sam): this does the wrong thing on functions that return a union
                    .find(|c| identifiers.contains(&c.name()));

                match class {
                    None => format!("{:#}", arg.field_type),
                    Some(c) => {
                        let class_schema = c
                            .serialize(&schema.db, None, None, c.identifier().span())
                            .unwrap_or(format!("{:#}", arg.field_type));
                        let enum_schemas = c
                            .required_enums()
                            // TODO(sam) - if enum serialization fails, then we do not surface the error to the user.
                            // That is bad!!!!!!!
                            .filter_map(|e| {
                                e.serialize(&schema.db, None, None, e.identifier().span())
                                    .ok()
                            })
                            .collect::<Vec<_>>();

                        let enum_schemas = match enum_schemas.len() {
                            0 => "".to_string(),
                            1 => format!(
                                "\n\nUse this enum for the output:\n{}",
                                enum_schemas.join("\n\n")
                            ),
                            _ => format!(
                                "\n\nUse these enums for the output:\n\n{}",
                                enum_schemas.join("\n\n")
                            ),
                        };

                        format!("Use this output format:\n{}{}", class_schema, enum_schemas)
                    }
                }
            }
        };

        let rendered = render_prompt(
            prompt.value(),
            args,
            RenderContext {
                client: RenderContext_Client {
                    name: client_walker
                        .map(|c| c.name().to_string())
                        .unwrap_or(client_name.to_string()),
                    provider: client_walker
                        .map(|c| c.properties().provider.0.clone())
                        // TODO(sam): how are fallback/round-robin clients represented here?
                        .unwrap_or("???".to_string()),
                },
                output_schema: output_schema,
                env: HashMap::new(),
            },
            vec![],
        );
        vec![Impl {
            name: StringSpan::new("default_impl", func.identifier().span()),
            prompt_key: prompt.span().into(),
            prompt: match rendered {
                Ok(internal_baml_jinja::RenderedPrompt::Completion(completion)) => {
                    PromptPreview::Completion {
                        completion: completion,
                    }
                }
                Ok(internal_baml_jinja::RenderedPrompt::Chat(chat)) => {
                    PromptPreview::Chat { chat: chat }
                }
                Err(err) => PromptPreview::Error {
                    error: format!("{err:#}"),
                },
            },
            client: client,
            input_replacers: vec![],
            output_replacers: vec![],
        }]
    }
}

fn print_diagnostics(diagnostics: Vec<MiniError>, response: Option<Value>) -> String {
    json!({
        "ok": response.is_some(),
        "diagnostics": diagnostics,
        "response": response,
    })
    .to_string()
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
