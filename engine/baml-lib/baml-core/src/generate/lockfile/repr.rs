use std::collections::HashMap;

use internal_baml_parser_database::walkers::{
    ClassWalker, EnumWalker, FunctionWalker, VariantWalker,
};
use internal_baml_schema_ast::ast::{self, WithName};
use serde_json::{json, Value};

pub(crate) trait WithRepr<T> {
    fn repr(&self) -> T;
}

#[derive(serde::Serialize)]
pub struct AllElements {
    pub enums: Vec<Enum>,
    pub classes: Vec<Class>,
    pub functions: Vec<Function>,
}
trait WithMetadata {
    fn attributes(&self) -> &HashMap<String, String>;

    fn getAttribute(&self, key: &str) -> Option<&String> {
        self.attributes().get(key)
    }
}

#[derive(serde::Serialize)]
pub enum Primitive {
    STRING,
}

#[derive(serde::Serialize)]
pub enum FieldType {
    PRIMITIVE(Primitive),
    ENUM(String),
    CLASS(String),
}

impl WithRepr<FieldType> for ast::FieldType {
    fn repr(&self) -> FieldType {
        match self {
            ast::FieldType::Identifier(_, idn) => FieldType::CLASS("placeholder-class".to_string()),
            ast::FieldType::List(item, dims, _) => {
                FieldType::CLASS("placeholder-class".to_string())
            }
            ast::FieldType::Dictionary(kv, _) => FieldType::CLASS("placeholder-class".to_string()),
            ast::FieldType::Union(_, t, _) => FieldType::CLASS("placeholder-class".to_string()),
            ast::FieldType::Tuple(_, t, _) => FieldType::CLASS("placeholder-class".to_string()),
        }
    }
}

// impl WithRepr for FieldType {
//     fn repr(&self) -> Value {
//         match self {
//             FieldType::Identifier(_, idn) => match idn {
//                 Identifier::Primitive(t, ..) => json!({
//                     "type": match t {
//                         TypeValue::String => "string",
//                         TypeValue::Int => "integer",
//                         TypeValue::Float => "number",
//                         TypeValue::Bool => "boolean",
//                         TypeValue::Null => "undefined",
//                         TypeValue::Char => "string",
//                     }
//                 }),
//                 Identifier::Local(name, _) => json!({
//                     "$ref": format!("#/definitions/{}", name),
//                 }),
//                 _ => panic!("Not implemented"),
//             },
//             FieldType::List(item, dims, _) => {
//                 let mut inner = json!({
//                     "type": "array",
//                     "items": (*item).json_schema()
//                 });
//                 for _ in 1..*dims {
//                     inner = json!({
//                         "type": "array",
//                         "items": inner
//                     });
//                 }
//
//                 return inner;
//             }
//             FieldType::Dictionary(kv, _) => json!({
//                 "type": "object",
//                 "additionalProperties": {
//                     "type": (*kv).1.json_schema(),
//                 }
//             }),
//             FieldType::Union(_, t, _) => json!({
//                 "anyOf": t.iter().map(|t| {
//                     let res = t.json_schema();
//                     // if res is a map, add a "title" field
//                     if let Value::Object(res) = &res {
//                         let mut res = res.clone();
//                         res.insert("title".to_string(), json!(t.to_string()));
//                         return json!(res);
//                     }
//                     res
//                 }
//             ).collect::<Vec<_>>(),
//             }),
//             FieldType::Tuple(_, t, _) => json!({
//                 "type": "array",
//                 "items": t.iter().map(|t| t.json_schema()).collect::<Vec<_>>(),
//             }),
//         }
//     }
// }

type EnumId = String;
#[derive(serde::Serialize)]
pub struct Enum {
    name: EnumId,
    // DO NOT LAND - need to model attributes
    values: Vec<String>,
}

impl WithRepr<Enum> for EnumWalker<'_> {
    fn repr(&self) -> Enum {
        Enum {
            name: self.name().to_string(),
            values: self.values().map(|v| v.name().to_string()).collect(),
        }
    }
}

#[derive(serde::Serialize)]
pub struct Field {
    name: String,
    r#type: FieldType,
}

type ClassId = String;

#[derive(serde::Serialize)]
pub struct Class {
    name: ClassId,
    fields: Vec<Field>,
}

impl WithRepr<Class> for ClassWalker<'_> {
    fn repr(&self) -> Class {
        Class {
            name: self.name().to_string(),
            fields: self
                .static_fields()
                .map(|field| Field {
                    name: field.name().to_string(),
                    // DO NOT LAND- needs to recurse
                    r#type: FieldType::PRIMITIVE(Primitive::STRING),
                })
                .collect(),
        }
    }
}

// DO NOT LAND - these are also client types
#[derive(serde::Serialize)]
pub enum ImplementationType {
    LLM,
}

type ImplementationId = String;

#[derive(serde::Serialize)]
pub struct Implementation {
    // DO NOT LAND - need to capture overrides (currently represented as metadata)
    r#type: ImplementationType,
    name: ImplementationId,

    prompt: String,
    // input and output replacers are for the AST of the prompt itself
    // lockfile is doable w/o the prompt AST, but we /could/ do it- Q is if there's any benefit
    input_replacers: HashMap<String, String>,
    output_replacers: HashMap<String, String>,
    client: ClientId,
}

type ClientId = String;

#[derive(serde::Serialize)]
pub struct NamedArgList {
    arg_list: Vec<String>,
}

/// BAML does not allow UnnamedArgList nor a lone NamedArg
#[derive(serde::Serialize)]
pub enum FunctionArgs {
    UNNAMED_ARG,
    NAMED_ARG_LIST(NamedArgList),
}

type FunctionId = String;

#[derive(serde::Serialize)]
pub struct Function {
    name: FunctionId,
    inputs: FunctionArgs,
    output: FieldType,
    impls: Vec<Implementation>,
}

impl WithRepr<Function> for FunctionWalker<'_> {
    fn repr(&self) -> Function {
        Function {
            name: self.name().to_string(),
            inputs: match self.ast_function().input() {
                ast::FunctionArgs::Named(arg_list) => {
                    FunctionArgs::NAMED_ARG_LIST(NamedArgList { arg_list: vec![] })
                }
                ast::FunctionArgs::Unnamed(arg) => FunctionArgs::UNNAMED_ARG,
            },
            output: match self.ast_function().output() {
                ast::FunctionArgs::Named(arg_list) => FieldType::PRIMITIVE(Primitive::STRING),
                ast::FunctionArgs::Unnamed(arg) => FieldType::PRIMITIVE(Primitive::STRING),
            },
            impls: self
                .walk_variants()
                .map(|e| Implementation {
                    r#type: ImplementationType::LLM,
                    name: e.name().to_string(),
                    prompt: e.properties().prompt.value.clone(),
                    input_replacers: e
                        .properties()
                        .replacers
                        // NB: .0 should really be .input
                        .0
                        .iter()
                        .map(|r| (r.0.key(), r.1.clone()))
                        .collect(),
                    output_replacers: e
                        .properties()
                        .replacers
                        // NB: .1 should really be .output
                        .1
                        .iter()
                        .map(|r| (r.0.key(), r.1.clone()))
                        .collect(),
                    client: e.properties().client.value.clone(),
                })
                .collect(),
        }
    }
}
