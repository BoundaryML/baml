use std::collections::HashMap;

use internal_baml_parser_database::walkers::{
    ClassWalker, EnumWalker, FunctionWalker, VariantWalker,
};
use internal_baml_schema_ast::ast::{self, WithName};
use serde_json::{json, Value};

// TODO:
//
// [ ] clients
// [ ] metadata per node (attributes, spans, etc)

pub(crate) trait WithRepr<T> {
    fn repr(&self) -> T;
}

#[derive(serde::Serialize)]
pub struct AllElements {
    pub enums: Vec<Enum>,
    pub classes: Vec<Class>,
    pub functions: Vec<Function>,
}

#[derive(serde::Serialize)]
pub enum PrimitiveType {
    CHAR,
    STRING,
    FLOAT,
    INTEGER,
    BOOL,
    NULL,
}

#[derive(serde::Serialize)]
pub enum FieldType {
    PRIMITIVE(PrimitiveType),
    REF(TypeId),
    KD_LIST(u32, Box<FieldType>),
    MAP(Box<FieldType>, Box<FieldType>),
    UNION(Vec<Box<FieldType>>),
    TUPLE(Vec<Box<FieldType>>),
}

impl WithRepr<FieldType> for ast::FieldType {
    fn repr(&self) -> FieldType {
        match self {
            ast::FieldType::Identifier(_, idn) => match idn {
                ast::Identifier::Primitive(t, ..) => FieldType::PRIMITIVE(match t {
                    ast::TypeValue::String => PrimitiveType::STRING,
                    ast::TypeValue::Int => PrimitiveType::INTEGER,
                    ast::TypeValue::Float => PrimitiveType::FLOAT,
                    ast::TypeValue::Bool => PrimitiveType::BOOL,
                    ast::TypeValue::Null => PrimitiveType::NULL,
                    ast::TypeValue::Char => PrimitiveType::CHAR,
                }),
                ast::Identifier::Local(name, _) => FieldType::REF(name.to_string()),
                _ => panic!("Not implemented"),
            },
            ast::FieldType::List(item, dims, _) => FieldType::KD_LIST(*dims, Box::new(item.repr())),
            ast::FieldType::Dictionary(kv, _) => {
                // NB: we can't just unpack (*kv) into k, v because that would require a move/copy
                FieldType::MAP(Box::new((*kv).0.repr()), Box::new((*kv).1.repr()))
            }
            ast::FieldType::Union(_, t, _) => {
                FieldType::UNION(t.iter().map(|ft| Box::new(ft.repr())).collect())
            }
            ast::FieldType::Tuple(_, t, _) => {
                FieldType::TUPLE(t.iter().map(|ft| Box::new(ft.repr())).collect())
            }
        }
    }
}

#[derive(serde::Serialize)]
pub struct Enum {
    name: TypeId,
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

type TypeId = String;

#[derive(serde::Serialize)]
pub struct Class {
    name: TypeId,
    // DO NOT LAND- these should not be diff
    static_fields: Vec<Field>,
    dynamic_fields: Vec<Field>,
}

impl WithRepr<Class> for ClassWalker<'_> {
    fn repr(&self) -> Class {
        Class {
            name: self.name().to_string(),
            static_fields: self
                .static_fields()
                .map(|field| Field {
                    name: field.name().to_string(),
                    r#type: field.ast_field().field_type.repr(),
                })
                .collect(),
            dynamic_fields: self
                .dynamic_fields()
                .map(|field| Field {
                    name: field.name().to_string(),
                    r#type: field.ast_field().field_type.repr(),
                })
                .collect(),
        }
    }
}

// DO NOT LAND - these are also client types
#[derive(serde::Serialize)]
pub enum BackendType {
    LLM,
}

type ImplementationId = String;

#[derive(serde::Serialize)]
pub struct Implementation {
    // DO NOT LAND - need to capture overrides (currently represented as metadata)
    r#type: BackendType,
    name: ImplementationId,

    prompt: String,
    // input and output replacers are for the AST of the prompt itself
    // lockfile is doable w/o the prompt AST, but we /could/ do it- Q is if there's any benefit
    // NB: we should avoid maps, b/c we want to preserve insertion order - maybe IndexMap?
    input_replacers: HashMap<String, String>,
    output_replacers: HashMap<String, String>,
    client: ClientId,
}

type ClientId = String;

#[derive(serde::Serialize)]
pub struct NamedArgList {
    arg_list: Vec<(String, FieldType)>,
}

/// BAML does not allow UnnamedArgList nor a lone NamedArg
#[derive(serde::Serialize)]
pub enum FunctionArgs {
    UNNAMED_ARG(FieldType),
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
                ast::FunctionArgs::Named(arg_list) => FunctionArgs::NAMED_ARG_LIST(NamedArgList {
                    arg_list: arg_list
                        .args
                        .iter()
                        .map(|(id, arg)| (id.name().to_string(), arg.field_type.repr()))
                        .collect(),
                }),
                ast::FunctionArgs::Unnamed(arg) => FunctionArgs::UNNAMED_ARG(arg.field_type.repr()),
            },
            output: match self.ast_function().output() {
                ast::FunctionArgs::Named(arg_list) => {
                    panic!("Functions may not return named args")
                }
                ast::FunctionArgs::Unnamed(arg) => arg.field_type.repr(),
            },
            impls: self
                .walk_variants()
                .map(|e| Implementation {
                    r#type: BackendType::LLM,
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
