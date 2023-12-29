use either::Either;
use std::collections::HashMap;

use internal_baml_parser_database::{
    walkers::{ClassWalker, ClientWalker, ConfigurationWalker, EnumWalker, FunctionWalker},
    ParserDatabase, RetryPolicyStrategy,
};
use internal_baml_schema_ast::ast::{self, WithName};
use serde_json::{json, Value};

// TODO:
//
//   [x] clients - need to finish expressions
//   [ ] metadata per node (attributes, spans, etc)
//           block-level attributes on enums, classes
//           field-level attributes on enum values, class fields
//           overrides can only exist in impls
//   [ ] FieldArity (optional / required) needs to be handled
//   [x] other types of identifiers?
//   [ ] `baml update` needs to update lockfile right now
//          but baml CLI is installed globally
//   [ ] baml configuration - retry policies, generator, etc
//          [x] retry policies

pub trait WithRepr<T> {
    fn repr(&self, db: &ParserDatabase) -> T;

    fn node(&self, db: &ParserDatabase) -> Node<T> {
        Node {
            elem: self.repr(db),
            meta: HashMap::new(),
        }
    }
}

/// Nodes allow attaching metadata to a given IR entity: attributes, source location, etc
#[derive(serde::Serialize)]
pub struct Node<T> {
    // TODO- do not allow hashmaps, always want order in these
    meta: HashMap<String, String>,
    elem: T,
}

#[derive(serde::Serialize)]
pub struct AllElements {
    pub enums: Vec<Node<Enum>>,
    pub classes: Vec<Node<Class>>,
    pub functions: Vec<Node<Function>>,
    pub clients: Vec<Node<Client>>,
    pub retry_policies: Vec<Node<RetryPolicy>>,
}

#[derive(serde::Serialize)]
pub enum FieldType {
    PRIMITIVE(ast::TypeValue),
    ENUM(EnumId),
    CLASS(ClassId),
    LIST(Box<FieldType>),
    MAP(Box<FieldType>, Box<FieldType>),
    UNION(Vec<FieldType>),
    TUPLE(Vec<FieldType>),
}

impl WithRepr<FieldType> for ast::FieldType {
    fn repr(&self, db: &ParserDatabase) -> FieldType {
        match self {
            ast::FieldType::Identifier(_, idn) => match idn {
                ast::Identifier::Primitive(t, ..) => FieldType::PRIMITIVE(*t),
                // ast has enough info to resolve whether this is a ref to an enum or a class
                ast::Identifier::Local(name, _) => match db.find_type(idn) {
                    Some(Either::Left(_class_walker)) => FieldType::CLASS(name.clone()),
                    Some(Either::Right(_enum_walker)) => FieldType::ENUM(name.clone()),
                    None => panic!("parser DB screwed up, got an invalid identifier"),
                },
                _ => panic!("Not implemented"),
            },
            ast::FieldType::List(ft, dims, _) => {
                // NB: potential bug: this hands back a 1D list when dims == 0
                let mut repr = FieldType::LIST(Box::new(ft.repr(db)));

                for _ in 1u32..*dims {
                    repr = FieldType::LIST(Box::new(repr));
                }

                repr
            }
            ast::FieldType::Dictionary(kv, _) => {
                // NB: we can't just unpack (*kv) into k, v because that would require a move/copy
                FieldType::MAP(Box::new((*kv).0.repr(db)), Box::new((*kv).1.repr(db)))
            }
            ast::FieldType::Union(_, t, _) => {
                FieldType::UNION(t.iter().map(|ft| ft.repr(db)).collect())
            }
            ast::FieldType::Tuple(_, t, _) => {
                FieldType::TUPLE(t.iter().map(|ft| ft.repr(db)).collect())
            }
        }
    }
}

#[derive(serde::Serialize)]
pub enum Identifier {
    /// Starts with env.*
    ENV(String),
    /// The path to a Local Identifer + the local identifer. Separated by '.'
    Ref(Vec<String>),
    /// A string without spaces or '.' Always starts with a letter. May contain numbers
    Local(String),
    /// Special types (always lowercase).
    Primitive(ast::TypeValue),
    /// A string without spaces, but contains '-'
    String(String),
}

#[derive(serde::Serialize)]
pub enum Expression {
    Identifier(Identifier),
    Numeric(String),
    String(String),
    RawString(String),
    List(Vec<Expression>),
    Map(Vec<(Expression, Expression)>),
}

impl WithRepr<Expression> for ast::Expression {
    fn repr(&self, db: &ParserDatabase) -> Expression {
        match self {
            ast::Expression::NumericValue(val, _) => Expression::Numeric(val.clone()),
            ast::Expression::StringValue(val, _) => Expression::String(val.clone()),
            ast::Expression::RawStringValue(val) => Expression::RawString(val.value().to_string()),
            ast::Expression::Identifier(idn) => Expression::Identifier(match idn {
                ast::Identifier::ENV(k, _) => Identifier::ENV(k.clone()),
                ast::Identifier::String(s, _) => Identifier::String(s.clone()),
                ast::Identifier::Local(l, _) => Identifier::Local(l.clone()),
                ast::Identifier::Ref(r, _) => Identifier::Ref(r.path.clone()),
                ast::Identifier::Primitive(p, _) => Identifier::Primitive(*p),
                ast::Identifier::Invalid(_, _) => panic!("parser db should never hand these out"),
            }),
            ast::Expression::Array(arr, _) => {
                Expression::List(arr.iter().map(|e| e.repr(db)).collect())
            }
            ast::Expression::Map(arr, _) => {
                Expression::Map(arr.iter().map(|(k, v)| (k.repr(db), v.repr(db))).collect())
            }
        }
    }
}

type EnumId = String;
type EnumValue = String;
#[derive(serde::Serialize)]
pub struct Enum {
    name: EnumId,
    values: Vec<Node<EnumValue>>,
}

impl WithRepr<Enum> for EnumWalker<'_> {
    fn repr(&self, db: &ParserDatabase) -> Enum {
        Enum {
            name: self.name().to_string(),
            values: self
                .values()
                .map(|v| Node {
                    meta: HashMap::new(),
                    elem: v.name().to_string(),
                })
                .collect(),
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
    static_fields: Vec<Field>,
    dynamic_fields: Vec<Field>,
}

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
    impls: Vec<Node<Implementation>>,
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
                ast::FunctionArgs::Named(_) => {
                    panic!("Functions may not return named args")
                }
                ast::FunctionArgs::Unnamed(arg) => arg.field_type.node(db),
            },
            impls: self
                .walk_variants()
                .map(|e| Node {
                    meta: HashMap::new(),
                    elem: Implementation {
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
                    },
                })
                .collect(),
        }
    }
}

#[derive(serde::Serialize)]
pub struct Client {
    name: ClientId,
    provider: String,
    options: Vec<(String, Expression)>,
}

impl WithRepr<Client> for ClientWalker<'_> {
    fn repr(&self, db: &ParserDatabase) -> Client {
        Client {
            name: self.name().to_string(),
            provider: self.properties().provider.0.clone(),
            options: self
                .properties()
                .options
                .iter()
                .map(|(k, v)| (k.clone(), v.repr(db)))
                .collect(),
        }
    }
}

type RetryPolicyId = String;

#[derive(serde::Serialize)]
pub struct RetryPolicy {
    name: RetryPolicyId,
    max_retries: u32,
    strategy: RetryPolicyStrategy,
    // NB: the parser DB has a notion of "empty options" vs "no options"; we collapse
    // those here into an empty vec
    options: Vec<(String, Expression)>,
}

impl WithRepr<RetryPolicy> for ConfigurationWalker<'_> {
    fn repr(&self, db: &ParserDatabase) -> RetryPolicy {
        RetryPolicy {
            name: self.name().to_string(),
            max_retries: self.retry_policy().max_retries,
            strategy: self.retry_policy().strategy,
            options: match &self.retry_policy().options {
                Some(o) => o
                    .iter()
                    .map(|((k, _), v)| (k.clone(), v.repr(db)))
                    .collect(),
                None => vec![],
            },
        }
    }
}
