use either::Either;
use std::collections::HashMap;

use indexmap::IndexMap;
use internal_baml_parser_database::{
    walkers::{
        ClassWalker, ClientWalker, ConfigurationWalker, EnumValueWalker, EnumWalker, FieldWalker,
        FunctionWalker, VariantWalker,
    },
    DynamicStringAttributes, ParserDatabase, RetryPolicyStrategy, StaticStringAttributes,
    ToStringAttributes, WithStaticRenames,
};
use internal_baml_schema_ast::ast::{self, FieldArity, WithName};
use serde_json::{json, Value};

// TODO:
//
//   [x] clients - need to finish expressions
//   [ ] metadata per node (attributes, spans, etc)
//           block-level attributes on enums, classes
//           field-level attributes on enum values, class fields
//           overrides can only exist in impls
//   [x] FieldArity (optional / required) needs to be handled
//   [x] other types of identifiers?
//   [ ] `baml update` needs to update lockfile right now
//          but baml CLI is installed globally
//   [ ] baml configuration - retry policies, generator, etc
//          [x] retry policies
//   [ ] rename lockfile/mod.rs to ir/mod.rs
//   [ ] wire Result<> type through, need this to be more sane

#[derive(Default, serde::Serialize)]
pub struct NodeAttributes {
    #[serde(with = "indexmap::map::serde_seq")]
    meta: IndexMap<String, Expression>,
    #[serde(with = "indexmap::map::serde_seq")]
    overrides: IndexMap<(FunctionId, ImplementationId), IndexMap<String, Expression>>,
}

fn to_ir_attributes(
    db: &ParserDatabase,
    maybe_ast_attributes: Option<&ToStringAttributes>,
) -> IndexMap<String, Expression> {
    let mut attributes = IndexMap::new();

    if let Some(ast_attributes) = maybe_ast_attributes {
        match ast_attributes {
            ToStringAttributes::Static(s) => {
                if s.skip().is_some() {
                    attributes.insert("skip".to_string(), Expression::String("".to_string()));
                }
                if let Some(v) = s.alias() {
                    attributes.insert("alias".to_string(), Expression::String(db[*v].to_string()));
                }
                for (&k, &v) in s.meta().into_iter() {
                    attributes.insert(db[k].to_string(), Expression::String(db[v].to_string()));
                }
            }
            ToStringAttributes::Dynamic(d) => {
                for (&lang, &lang_code) in d.code.iter() {
                    attributes.insert(
                        format!("get/{}", db[lang].to_string()),
                        Expression::String(db[lang_code].to_string()),
                    );
                }
            }
        }
    }

    attributes
}

/// Nodes allow attaching metadata to a given IR entity: attributes, source location, etc
#[derive(serde::Serialize)]
pub struct Node<T> {
    // TODO- do not allow hashmaps, always want order in these
    attributes: NodeAttributes,
    //overrides: HashMap<String, HashMap<String, Expression>>,
    elem: T,
}

pub trait WithRepr<T> {
    fn attributes(&self, db: &ParserDatabase) -> NodeAttributes;

    fn repr(&self, db: &ParserDatabase) -> T;

    fn node(&self, db: &ParserDatabase) -> Node<T> {
        Node {
            elem: self.repr(db),
            attributes: self.attributes(db),
        }
    }
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
    Primitive(ast::TypeValue),
    Enum(EnumId),
    Class(ClassId),
    List(Box<FieldType>),
    Map(Box<FieldType>, Box<FieldType>),
    Union(Vec<FieldType>),
    Tuple(Vec<FieldType>),
}

impl FieldType {
    fn with_arity(self, arity: &FieldArity) -> FieldType {
        match arity {
            FieldArity::Required => self,
            FieldArity::Optional => {
                FieldType::Union(vec![self, FieldType::Primitive(ast::TypeValue::Null)])
            }
        }
    }
}

impl WithRepr<FieldType> for ast::FieldType {
    fn attributes(&self, db: &ParserDatabase) -> NodeAttributes {
        NodeAttributes::default()
    }

    fn repr(&self, db: &ParserDatabase) -> FieldType {
        match self {
            ast::FieldType::Identifier(arity, idn) => (match idn {
                ast::Identifier::Primitive(t, ..) => FieldType::Primitive(*t),
                ast::Identifier::Local(name, _) => match db.find_type(idn) {
                    Some(Either::Left(_class_walker)) => FieldType::Class(ClassId(name.clone())),
                    Some(Either::Right(_enum_walker)) => FieldType::Enum(name.clone()),
                    None => panic!("parser DB screwed up, got an invalid identifier"),
                },
                // DO NOT LAND - do we need to handle other identifiers here?
                _ => panic!("Not implemented"),
            })
            .with_arity(arity),
            ast::FieldType::List(ft, dims, _) => {
                // NB: potential bug: this hands back a 1D list when dims == 0
                let mut repr = FieldType::List(Box::new(ft.repr(db)));

                for _ in 1u32..*dims {
                    repr = FieldType::List(Box::new(repr));
                }

                repr
            }
            ast::FieldType::Dictionary(kv, _) => {
                // NB: we can't just unpack (*kv) into k, v because that would require a move/copy
                FieldType::Map(Box::new((*kv).0.repr(db)), Box::new((*kv).1.repr(db)))
            }
            ast::FieldType::Union(arity, t, _) => {
                // NB: preempt union flattening by mixing arity into union types
                let mut types = t.iter().map(|ft| ft.repr(db)).collect::<Vec<_>>();

                if arity.is_optional() {
                    types.push(FieldType::Primitive(ast::TypeValue::Null));
                }

                FieldType::Union(types)
            }
            ast::FieldType::Tuple(arity, t, _) => {
                FieldType::Tuple(t.iter().map(|ft| ft.repr(db)).collect()).with_arity(arity)
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
    fn attributes(&self, db: &ParserDatabase) -> NodeAttributes {
        NodeAttributes::default()
    }

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

#[derive(serde::Serialize)]
pub struct EnumValue(String);

#[derive(serde::Serialize)]
pub struct Enum {
    name: EnumId,
    values: Vec<Node<EnumValue>>,
}

impl WithRepr<EnumValue> for EnumValueWalker<'_> {
    fn attributes(&self, db: &ParserDatabase) -> NodeAttributes {
        NodeAttributes {
            meta: to_ir_attributes(db, self.get_default_attributes()),
            // TODO - enum values can have overrides
            overrides: IndexMap::new(),
        }
    }

    fn repr(&self, db: &ParserDatabase) -> EnumValue {
        EnumValue(self.name().to_string())
    }
}

impl WithRepr<Enum> for EnumWalker<'_> {
    fn attributes(&self, db: &ParserDatabase) -> NodeAttributes {
        let mut attributes = NodeAttributes::default();

        attributes.meta = to_ir_attributes(db, self.get_default_attributes());

        for r#fn in db.walk_functions() {
            for r#impl in r#fn.walk_variants() {
                let node_attributes =
                    to_ir_attributes(db, r#impl.find_serializer_attributes(self.name()));
                if !node_attributes.is_empty() {
                    attributes.overrides.insert(
                        (
                            FunctionId(r#fn.name().to_string()),
                            ImplementationId(r#impl.name().to_string()),
                        ),
                        node_attributes,
                    );
                }
            }
        }

        attributes
    }

    fn repr(&self, db: &ParserDatabase) -> Enum {
        Enum {
            name: self.name().to_string(),
            values: self.values().map(|v| v.node(db)).collect(),
        }
    }
}

#[derive(serde::Serialize)]
pub struct Field {
    name: String,
    r#type: Node<FieldType>,
}

impl WithRepr<Field> for FieldWalker<'_> {
    fn attributes(&self, db: &ParserDatabase) -> NodeAttributes {
        let mut attributes = NodeAttributes::default();

        attributes.meta = to_ir_attributes(db, self.get_default_attributes());

        for r#fn in db.walk_functions() {
            for r#impl in r#fn.walk_variants() {
                let node_attributes = to_ir_attributes(
                    db,
                    r#impl.find_serializer_field_attributes(self.model().name(), self.name()),
                );
                // TODO
                if !node_attributes.is_empty() {
                    attributes.overrides.insert(
                        (
                            FunctionId(r#fn.name().to_string()),
                            ImplementationId(r#impl.name().to_string()),
                        ),
                        node_attributes,
                    );
                }
            }
        }

        attributes
    }

    fn repr(&self, db: &ParserDatabase) -> Field {
        Field {
            name: self.name().to_string(),
            r#type: self.ast_field().field_type.node(db),
        }
    }
}

#[derive(serde::Serialize)]
pub struct ClassId(String);

#[derive(serde::Serialize)]
pub struct Class {
    name: ClassId,
    static_fields: Vec<Node<Field>>,
    dynamic_fields: Vec<Node<Field>>,
}

impl WithRepr<Class> for ClassWalker<'_> {
    fn attributes(&self, db: &ParserDatabase) -> NodeAttributes {
        NodeAttributes::default()
    }

    fn repr(&self, db: &ParserDatabase) -> Class {
        Class {
            name: ClassId(self.name().to_string()),
            static_fields: self.static_fields().map(|e| e.node(db)).collect(),
            dynamic_fields: self.dynamic_fields().map(|e| e.node(db)).collect(),
        }
    }
}

// DO NOT LAND - these are also client types
#[derive(serde::Serialize)]
pub enum BackendType {
    LLM,
}

#[derive(Eq, Hash, PartialEq, serde::Serialize)]
pub struct ImplementationId(String);

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

#[derive(serde::Serialize)]
pub struct NamedArgList {
    arg_list: Vec<(String, FieldType)>,
}

/// BAML does not allow UnnamedArgList nor a lone NamedArg
#[derive(serde::Serialize)]
pub enum FunctionArgs {
    UnnamedArg(Node<FieldType>),
    NamedArgList(NamedArgList),
}

#[derive(Eq, Hash, PartialEq, serde::Serialize)]
pub struct FunctionId(String);

#[derive(serde::Serialize)]
pub struct Function {
    name: FunctionId,
    inputs: FunctionArgs,
    output: Node<FieldType>,
    impls: Vec<Node<Implementation>>,
}

impl WithRepr<Implementation> for VariantWalker<'_> {
    fn attributes(&self, db: &ParserDatabase) -> NodeAttributes {
        NodeAttributes::default()
    }

    fn repr(&self, db: &ParserDatabase) -> Implementation {
        Implementation {
            r#type: BackendType::LLM,
            name: ImplementationId(self.name().to_string()),
            prompt: self.properties().prompt.value.clone(),
            input_replacers: self
                .properties()
                .replacers
                // NB: .0 should really be .input
                .0
                .iter()
                .map(|r| (r.0.key(), r.1.clone()))
                .collect(),
            output_replacers: self
                .properties()
                .replacers
                // NB: .1 should really be .output
                .1
                .iter()
                .map(|r| (r.0.key(), r.1.clone()))
                .collect(),
            client: ClientId(self.properties().client.value.clone()),
        }
    }
}

impl WithRepr<Function> for FunctionWalker<'_> {
    fn attributes(&self, db: &ParserDatabase) -> NodeAttributes {
        NodeAttributes::default()
    }

    fn repr(&self, db: &ParserDatabase) -> Function {
        Function {
            name: FunctionId(self.name().to_string()),
            inputs: match self.ast_function().input() {
                ast::FunctionArgs::Named(arg_list) => FunctionArgs::NamedArgList(NamedArgList {
                    arg_list: arg_list
                        .args
                        .iter()
                        .map(|(id, arg)| (id.name().to_string(), arg.field_type.repr(db)))
                        .collect(),
                }),
                ast::FunctionArgs::Unnamed(arg) => {
                    FunctionArgs::UnnamedArg(arg.field_type.node(db))
                }
            },
            output: match self.ast_function().output() {
                ast::FunctionArgs::Named(_) => {
                    panic!("Functions may not return named args")
                }
                ast::FunctionArgs::Unnamed(arg) => arg.field_type.node(db),
            },
            impls: self.walk_variants().map(|e| e.node(db)).collect(),
        }
    }
}

#[derive(serde::Serialize)]
pub struct ClientId(String);

#[derive(serde::Serialize)]
pub struct Client {
    name: ClientId,
    provider: String,
    options: Vec<(String, Expression)>,
}

impl WithRepr<Client> for ClientWalker<'_> {
    fn attributes(&self, db: &ParserDatabase) -> NodeAttributes {
        NodeAttributes::default()
    }

    fn repr(&self, db: &ParserDatabase) -> Client {
        Client {
            name: ClientId(self.name().to_string()),
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

#[derive(serde::Serialize)]
pub struct RetryPolicyId(String);

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
    fn attributes(&self, db: &ParserDatabase) -> NodeAttributes {
        NodeAttributes::default()
    }

    fn repr(&self, db: &ParserDatabase) -> RetryPolicy {
        RetryPolicy {
            name: RetryPolicyId(self.name().to_string()),
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
