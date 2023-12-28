use either::Either;
use std::collections::HashMap;

use internal_baml_parser_database::{
    walkers::{ClassWalker, ClientWalker, ConfigurationWalker, EnumWalker, FunctionWalker},
    ParserDatabase,
};
use internal_baml_schema_ast::ast::{self, WithName};
use serde_json::{json, Value};

// TODO:
//
//   [ ] clients
//   [ ] metadata per node (attributes, spans, etc)
//   [ ] FieldArity (optional / required) needs to be handled
//   [ ] other types of identifiers?
//   [ ] `baml update` needs to update lockfile right now
//   [ ]

pub(crate) trait WithRepr<T> {
    fn repr(&self, db: &ParserDatabase) -> T;

    fn node(&self, db: &ParserDatabase) -> Node<T> {
        Node {
            elem: self.repr(db),
            meta: HashMap::new(),
        }
    }
}

#[derive(serde::Serialize)]
pub struct AllElements {
    pub enums: Vec<Node<Enum>>,
    pub classes: Vec<Node<Class>>,
    pub functions: Vec<Node<Function>>,
    pub clients: Vec<Node<Client>>,
    //pub configuration: Configuration,
}

#[derive(serde::Serialize)]
pub struct Node<T> {
    // TODO- do not allow hashmaps, always want order in these
    meta: HashMap<String, String>,
    elem: T,
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
    ENUM(EnumId),
    CLASS(ClassId),
    // TODO- make KD_LIST recursive list
    KD_LIST(u32, Box<Node<FieldType>>),
    MAP(Box<Node<FieldType>>, Box<Node<FieldType>>),
    UNION(Vec<Node<FieldType>>),
    TUPLE(Vec<Node<FieldType>>),
}

impl WithRepr<FieldType> for ast::FieldType {
    fn repr(&self, db: &ParserDatabase) -> FieldType {
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
                // ast has enough info to resolve whether this is a ref to an enum or a class
                ast::Identifier::Local(name, _) => match db.find_type(idn) {
                    Some(Either::Left(_class_walker)) => FieldType::CLASS(name.clone()),
                    Some(Either::Right(_enum_walker)) => FieldType::ENUM(name.clone()),
                    None => panic!("parser DB screwed up, got an invalid identifier"),
                },
                _ => panic!("Not implemented"),
            },
            ast::FieldType::List(ft, dims, _) => FieldType::KD_LIST(*dims, Box::new(ft.node(db))),
            ast::FieldType::Dictionary(kv, _) => {
                // NB: we can't just unpack (*kv) into k, v because that would require a move/copy
                FieldType::MAP(Box::new((*kv).0.node(db)), Box::new((*kv).1.node(db)))
            }
            ast::FieldType::Union(_, t, _) => {
                FieldType::UNION(t.iter().map(|ft| ft.node(db)).collect())
            }
            ast::FieldType::Tuple(_, t, _) => {
                FieldType::TUPLE(t.iter().map(|ft| ft.node(db)).collect())
            }
        }
    }
}

pub enum Identifier {
    /// Starts with env.*
    ENV(String),
    /// The path to a Local Identifer + the local identifer. Separated by '.'
    Ref(String),
    /// A string without spaces or '.' Always starts with a letter. May contain numbers
    Local(String),
    /// Special types (always lowercase).
    Primitive(String),
    /// A string without spaces, but contains '-'
    String(String),
}

#[derive(serde::Serialize)]
pub enum FieldValue {
    PRIMITIVE(PrimitiveType, String),
}

#[derive(serde::Serialize)]
pub enum Expression {
    NUMERIC(String),
    Identifier(String), // TODO
    StringValue(String),
    RawStringValue(String),
    Array(Vec<Expression>),
    Map(Vec<(Expression, Expression)>),
}

impl WithRepr<FieldValue> for ast::Expression {
    fn repr(&self, db: &ParserDatabase) -> FieldValue {
        match self {
            // DO NOT LAND- this needs to distinguish between "integer" and "float"
            ast::Expression::NumericValue(val, _) => {
                FieldValue::PRIMITIVE(PrimitiveType::FLOAT, val.clone())
            }
            ast::Expression::StringValue(val, _) => {
                FieldValue::PRIMITIVE(PrimitiveType::STRING, "placeholder".to_string())
            }
            ast::Expression::RawStringValue(val) => {
                FieldValue::PRIMITIVE(PrimitiveType::STRING, "placeholder".to_string())
            }
            ast::Expression::Identifier(idn) => {
                FieldValue::PRIMITIVE(PrimitiveType::STRING, "placeholder".to_string())
            }
            ast::Expression::Array(arr, _) => {
                FieldValue::PRIMITIVE(PrimitiveType::STRING, "placeholder".to_string())
            }
            ast::Expression::Map(arr, _) => {
                FieldValue::PRIMITIVE(PrimitiveType::STRING, "placeholder".to_string())
            }
        }
    }
}

type EnumId = String;
#[derive(serde::Serialize)]
pub struct Enum {
    name: EnumId,
    values: Vec<String>,
}

impl WithRepr<Enum> for EnumWalker<'_> {
    fn repr(&self, db: &ParserDatabase) -> Enum {
        Enum {
            name: self.name().to_string(),
            values: self.values().map(|v| v.name().to_string()).collect(),
        }
    }
}

#[derive(serde::Serialize)]
pub struct Field {
    name: String,
    r#type: Node<FieldType>,
}

type ClassId = String;

#[derive(serde::Serialize)]
pub struct Class {
    name: ClassId,
    // DO NOT LAND- these should not be diff
    static_fields: Vec<Field>,
    dynamic_fields: Vec<Field>,
}

// block-level attributes on enums, classes
// field-level attributes on enum values, class fields
// overrides can only exist in impls
impl WithRepr<Class> for ClassWalker<'_> {
    fn repr(&self, db: &ParserDatabase) -> Class {
        Class {
            name: self.name().to_string(),
            static_fields: self
                .static_fields()
                .map(|field| Field {
                    name: field.name().to_string(),
                    r#type: field.ast_field().field_type.node(db),
                })
                .collect(),
            dynamic_fields: self
                .dynamic_fields()
                .map(|field| Field {
                    name: field.name().to_string(),
                    r#type: field.ast_field().field_type.node(db),
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
    UNNAMED_ARG(Node<FieldType>),
    NAMED_ARG_LIST(NamedArgList),
}

type FunctionId = String;

#[derive(serde::Serialize)]
pub struct Function {
    name: FunctionId,
    inputs: FunctionArgs,
    output: Node<FieldType>,
    impls: Vec<Implementation>,
}

impl WithRepr<Function> for FunctionWalker<'_> {
    fn repr(&self, db: &ParserDatabase) -> Function {
        Function {
            name: self.name().to_string(),
            inputs: match self.ast_function().input() {
                ast::FunctionArgs::Named(arg_list) => FunctionArgs::NAMED_ARG_LIST(NamedArgList {
                    arg_list: arg_list
                        .args
                        .iter()
                        .map(|(id, arg)| (id.name().to_string(), arg.field_type.repr(db)))
                        .collect(),
                }),
                ast::FunctionArgs::Unnamed(arg) => {
                    FunctionArgs::UNNAMED_ARG(arg.field_type.node(db))
                }
            },
            output: match self.ast_function().output() {
                ast::FunctionArgs::Named(arg_list) => {
                    panic!("Functions may not return named args")
                }
                ast::FunctionArgs::Unnamed(arg) => arg.field_type.node(db),
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

#[derive(serde::Serialize)]
pub struct Client {
    name: ClientId,
    // TODO
}

impl WithRepr<Client> for ClientWalker<'_> {
    fn repr(&self, db: &ParserDatabase) -> Client {
        Client {
            name: self.name().to_string(),
        }
    }
}
