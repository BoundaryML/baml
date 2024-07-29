use std::collections::HashSet;

use anyhow::{anyhow, bail, Context, Result};
use baml_types::FieldType;
use either::Either;

use indexmap::IndexMap;
use internal_baml_parser_database::{
    walkers::{
        ClassWalker, ClientWalker, ConfigurationWalker, EnumValueWalker, EnumWalker, FieldWalker,
        FunctionWalker, TemplateStringWalker,
    },
    ParserDatabase, PromptAst, RetryPolicyStrategy, ToStringAttributes, WithStaticRenames,
};

use internal_baml_schema_ast::ast::{self, FieldArity, WithName, WithSpan};
use serde::Serialize;

use crate::Configuration;

/// This class represents the intermediate representation of the BAML AST.
/// It is a representation of the BAML AST that is easier to work with than the
/// raw BAML AST, and should include all information necessary to generate
/// code in any target language.
#[derive(serde::Serialize, Debug)]
pub struct IntermediateRepr {
    enums: Vec<Node<Enum>>,
    classes: Vec<Node<Class>>,
    functions: Vec<Node<Function>>,
    clients: Vec<Node<Client>>,
    retry_policies: Vec<Node<RetryPolicy>>,
    template_strings: Vec<Node<TemplateString>>,

    #[serde(skip)]
    configuration: Configuration,
}

/// A generic walker. Only walkers instantiated with a concrete ID type (`I`) are useful.
#[derive(Clone, Copy)]
pub struct Walker<'db, I> {
    /// The parser database being traversed.
    pub db: &'db IntermediateRepr,
    /// The identifier of the focused element.
    pub item: I,
}

impl IntermediateRepr {
    pub fn create_empty() -> IntermediateRepr {
        IntermediateRepr {
            enums: vec![],
            classes: vec![],
            functions: vec![],
            clients: vec![],
            retry_policies: vec![],
            template_strings: vec![],
            configuration: Configuration::new(),
        }
    }

    pub fn configuration(&self) -> &Configuration {
        &self.configuration
    }

    pub fn required_env_vars(&self) -> HashSet<&str> {
        // TODO: We should likely check the full IR.

        self.clients
            .iter()
            .flat_map(|c| c.elem.options.iter())
            .flat_map(|(_, expr)| expr.required_env_vars())
            .collect::<HashSet<&str>>()
    }

    pub fn walk_enums<'a>(&'a self) -> impl ExactSizeIterator<Item = Walker<'a, &'a Node<Enum>>> {
        self.enums.iter().map(|e| Walker { db: self, item: e })
    }

    pub fn walk_classes<'a>(
        &'a self,
    ) -> impl ExactSizeIterator<Item = Walker<'a, &'a Node<Class>>> {
        self.classes.iter().map(|e| Walker { db: self, item: e })
    }

    pub fn function_names(&self) -> impl ExactSizeIterator<Item = &str> {
        self.functions.iter().map(|f| f.elem.name())
    }

    pub fn walk_functions<'a>(
        &'a self,
    ) -> impl ExactSizeIterator<Item = Walker<'a, &'a Node<Function>>> {
        self.functions.iter().map(|e| Walker { db: self, item: e })
    }

    pub fn walk_tests<'a>(
        &'a self,
    ) -> impl Iterator<Item = Walker<'a, (&'a Node<Function>, &'a Node<TestCase>)>> {
        self.functions.iter().flat_map(move |f| {
            f.elem.tests().iter().map(move |t| Walker {
                db: self,
                item: (f, t),
            })
        })
    }

    pub fn walk_clients<'a>(
        &'a self,
    ) -> impl ExactSizeIterator<Item = Walker<'a, &'a Node<Client>>> {
        self.clients.iter().map(|e| Walker { db: self, item: e })
    }

    pub fn walk_template_strings<'a>(
        &'a self,
    ) -> impl ExactSizeIterator<Item = Walker<'a, &'a Node<TemplateString>>> {
        self.template_strings
            .iter()
            .map(|e| Walker { db: self, item: e })
    }

    #[allow(dead_code)]
    pub fn walk_retry_policies<'a>(
        &'a self,
    ) -> impl ExactSizeIterator<Item = Walker<'a, &'a Node<RetryPolicy>>> {
        self.retry_policies
            .iter()
            .map(|e| Walker { db: self, item: e })
    }

    pub fn from_parser_database(
        db: &ParserDatabase,
        configuration: Configuration,
    ) -> Result<IntermediateRepr> {
        let mut repr = IntermediateRepr {
            enums: db
                .walk_enums()
                .map(|e| e.node(db))
                .collect::<Result<Vec<_>>>()?,
            classes: db
                .walk_classes()
                .map(|e| e.node(db))
                .collect::<Result<Vec<_>>>()?,
            functions: db
                .walk_new_functions()
                .map(|e| e.node(db))
                .collect::<Result<Vec<_>>>()?,
            clients: db
                .walk_clients()
                .map(|e| e.node(db))
                .collect::<Result<Vec<_>>>()?,
            retry_policies: db
                .walk_retry_policies()
                .map(|e| WithRepr::<RetryPolicy>::node(&e, db))
                .collect::<Result<Vec<_>>>()?,
            template_strings: db
                .walk_templates()
                .map(|e| e.node(db))
                .collect::<Result<Vec<_>>>()?,
            configuration,
        };

        // Sort each item by name.
        repr.enums.sort_by(|a, b| a.elem.name.cmp(&b.elem.name));
        repr.classes.sort_by(|a, b| a.elem.name.cmp(&b.elem.name));
        repr.functions
            .sort_by(|a, b| a.elem.name().cmp(&b.elem.name()));
        repr.clients.sort_by(|a, b| a.elem.name.cmp(&b.elem.name));
        repr.retry_policies
            .sort_by(|a, b| a.elem.name.0.cmp(&b.elem.name.0));

        Ok(repr)
    }
}

// TODO:
//
//   [x] clients - need to finish expressions
//   [x] metadata per node (attributes, spans, etc)
//           block-level attributes on enums, classes
//           field-level attributes on enum values, class fields
//           overrides can only exist in impls
//   [x] FieldArity (optional / required) needs to be handled
//   [x] other types of identifiers?
//   [ ] `baml update` needs to update lockfile right now
//          but baml CLI is installed globally
//   [ ] baml configuration - retry policies, generator, etc
//          [x] retry policies
//   [x] rename lockfile/mod.rs to ir/mod.rs
//   [x] wire Result<> type through, need this to be more sane

#[derive(Debug, serde::Serialize)]
pub struct NodeAttributes {
    /// Map of attributes on the corresponding IR node.
    ///
    /// Some follow special conventions:
    ///
    ///   - @skip becomes ("skip", bool)
    ///   - @alias(...) becomes ("alias", ...)
    ///   - @get(python code) becomes ("get/python", python code)
    #[serde(with = "indexmap::map::serde_seq")]
    meta: IndexMap<String, Expression>,

    /// Overrides for the specified AST node in a given implementation (which is keyed by FunctionId
    /// and ImplementationId). In .baml files these are represented in the implementation, but in the
    /// IR AST we attach them to the AST node so that all metadata associated with an IRnode can be
    /// accessed from that node, rather than through a different IR node.
    #[serde(with = "indexmap::map::serde_seq")]
    overrides: IndexMap<(FunctionId, ImplementationId), IndexMap<String, Expression>>,

    // Spans
    #[serde(skip)]
    pub span: Option<ast::Span>,
}

impl NodeAttributes {
    pub fn get(&self, key: &str) -> Option<&Expression> {
        self.meta.get(key)
    }
}

fn to_ir_attributes(
    db: &ParserDatabase,
    maybe_ast_attributes: Option<&ToStringAttributes>,
) -> IndexMap<String, Expression> {
    let mut attributes = IndexMap::new();

    if let Some(ast_attributes) = maybe_ast_attributes {
        match ast_attributes {
            ToStringAttributes::Static(s) => {
                if let Some(true) = s.dynamic_type() {
                    attributes.insert("dynamic_type".to_string(), Expression::Bool(true));
                }

                if let Some(skip) = s.skip() {
                    attributes.insert("skip".to_string(), Expression::Bool(*skip));
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
#[derive(serde::Serialize, Debug)]
pub struct Node<T> {
    pub attributes: NodeAttributes,
    pub elem: T,
}

/// Implement this for every node in the IR AST, where T is the type of IR node
pub trait WithRepr<T> {
    /// Represents block or field attributes - @@ for enums and classes, @ for enum values and class fields
    fn attributes(&self, _: &ParserDatabase) -> NodeAttributes {
        NodeAttributes {
            meta: IndexMap::new(),
            overrides: IndexMap::new(),
            span: None,
        }
    }

    fn repr(&self, db: &ParserDatabase) -> Result<T>;

    fn node(&self, db: &ParserDatabase) -> Result<Node<T>> {
        Ok(Node {
            elem: self.repr(db)?,
            attributes: self.attributes(db),
        })
    }
}

fn type_with_arity(t: FieldType, arity: &FieldArity) -> FieldType {
    match arity {
        FieldArity::Required => t,
        FieldArity::Optional => FieldType::Optional(Box::new(t)),
    }
}

impl WithRepr<FieldType> for ast::FieldType {
    fn repr(&self, db: &ParserDatabase) -> Result<FieldType> {
        Ok(match self {
            ast::FieldType::Symbol(arity, idn) => type_with_arity(
                match idn {
                    ast::Identifier::Primitive(t, ..) => FieldType::Primitive(*t),
                    ast::Identifier::Local(name, _) => match db.find_type(idn) {
                        Some(Either::Left(_class_walker)) => Ok(FieldType::Class(name.clone())),
                        Some(Either::Right(_enum_walker)) => Ok(FieldType::Enum(name.clone())),
                        None => Err(anyhow!("Field type uses unresolvable local identifier")),
                    }?,
                    _ => bail!("Field type uses unsupported identifier type"),
                },
                arity,
            ),
            ast::FieldType::List(ft, dims, _) => {
                // NB: potential bug: this hands back a 1D list when dims == 0
                let mut repr = FieldType::List(Box::new(ft.repr(db)?));

                for _ in 1u32..*dims {
                    repr = FieldType::List(Box::new(repr));
                }

                repr
            }
            ast::FieldType::Map(kv, _) => {
                // NB: we can't just unpack (*kv) into k, v because that would require a move/copy
                FieldType::Map(Box::new((*kv).0.repr(db)?), Box::new((*kv).1.repr(db)?))
            }
            ast::FieldType::Union(arity, t, _) => {
                // NB: preempt union flattening by mixing arity into union types
                let mut types = t.iter().map(|ft| ft.repr(db)).collect::<Result<Vec<_>>>()?;

                if arity.is_optional() {
                    types.push(FieldType::Primitive(baml_types::TypeValue::Null));
                }

                FieldType::Union(types)
            }
            ast::FieldType::Tuple(arity, t, _) => type_with_arity(
                FieldType::Tuple(t.iter().map(|ft| ft.repr(db)).collect::<Result<Vec<_>>>()?),
                arity,
            ),
        })
    }
}

#[derive(serde::Serialize, Debug)]
pub enum Identifier {
    /// Starts with env.*
    ENV(String),
    /// The path to a Local Identifer + the local identifer. Separated by '.'
    #[allow(dead_code)]
    Ref(Vec<String>),
    /// A string without spaces or '.' Always starts with a letter. May contain numbers
    Local(String),
    /// Special types (always lowercase).
    Primitive(baml_types::TypeValue),
}

impl Identifier {
    pub fn name(&self) -> String {
        match self {
            Identifier::ENV(k) => k.clone(),
            Identifier::Ref(r) => r.join("."),
            Identifier::Local(l) => l.clone(),
            Identifier::Primitive(p) => p.to_string(),
        }
    }
}

#[derive(serde::Serialize, Debug)]
pub enum Expression {
    Identifier(Identifier),
    Bool(bool),
    Numeric(String),
    String(String),
    RawString(String),
    List(Vec<Expression>),
    Map(Vec<(Expression, Expression)>),
}

impl Expression {
    pub fn required_env_vars(&self) -> Vec<&str> {
        match self {
            Expression::Identifier(Identifier::ENV(k)) => vec![k.as_str()],
            Expression::List(l) => l.iter().flat_map(Expression::required_env_vars).collect(),
            Expression::Map(m) => m
                .iter()
                .flat_map(|(k, v)| {
                    let mut keys = k.required_env_vars();
                    keys.extend(v.required_env_vars());
                    keys
                })
                .collect(),
            _ => vec![],
        }
    }
}

impl WithRepr<Expression> for ast::Expression {
    fn repr(&self, db: &ParserDatabase) -> Result<Expression> {
        Ok(match self {
            ast::Expression::BoolValue(val, _) => Expression::Bool(val.clone()),
            ast::Expression::NumericValue(val, _) => Expression::Numeric(val.clone()),
            ast::Expression::StringValue(val, _) => Expression::String(val.clone()),
            ast::Expression::RawStringValue(val) => Expression::RawString(val.value().to_string()),
            ast::Expression::Identifier(idn) => match idn {
                ast::Identifier::ENV(k, _) => {
                    Ok(Expression::Identifier(Identifier::ENV(k.clone())))
                }
                ast::Identifier::String(s, _) => Ok(Expression::String(s.clone())),
                ast::Identifier::Local(l, _) => {
                    Ok(Expression::Identifier(Identifier::Local(l.clone())))
                }
                ast::Identifier::Ref(r, _) => {
                    // NOTE(sam): this feels very very wrong, but per vbv, we don't really use refs
                    // right now, so this should be safe. this is done to ensure that
                    // "options { model gpt-3.5-turbo }" is represented correctly in the resulting IR,
                    // specifically that "gpt-3.5-turbo" is actually modelled as Expression::String
                    //
                    // this does not impact the handling of "options { api_key env.OPENAI_API_KEY }"
                    // because that's modelled as Identifier::ENV, not Identifier::Ref
                    Ok(Expression::String(r.full_name.clone()))
                }
                ast::Identifier::Primitive(p, _) => {
                    Ok(Expression::Identifier(Identifier::Primitive(*p)))
                }
                ast::Identifier::Invalid(_, _) => {
                    Err(anyhow!("Cannot represent an invalid parser-AST identifier"))
                }
            }?,
            ast::Expression::Array(arr, _) => {
                Expression::List(arr.iter().map(|e| e.repr(db)).collect::<Result<Vec<_>>>()?)
            }
            ast::Expression::Map(arr, _) => Expression::Map(
                arr.iter()
                    .map(|(k, v)| Ok((k.repr(db)?, v.repr(db)?)))
                    .collect::<Result<Vec<_>>>()?,
            ),
        })
    }
}

type TemplateStringId = String;

#[derive(serde::Serialize, Debug)]

pub struct TemplateString {
    pub name: TemplateStringId,
    pub params: Vec<Field>,
    pub content: String,
}

impl WithRepr<TemplateString> for TemplateStringWalker<'_> {
    fn attributes(&self, _: &ParserDatabase) -> NodeAttributes {
        NodeAttributes {
            meta: Default::default(),
            overrides: Default::default(),
            span: Some(self.span().clone()),
        }
    }

    fn repr(&self, _db: &ParserDatabase) -> Result<TemplateString> {
        Ok(TemplateString {
            name: self.name().to_string(),
            params: self.ast_node().input().map_or(vec![], |e| match e {
                ast::FunctionArgs::Named(arg_list) => arg_list
                    .args
                    .iter()
                    .filter_map(|(id, arg)| {
                        arg.field_type
                            .node(_db)
                            .map(|f| Field {
                                name: id.name().to_string(),
                                r#type: f,
                            })
                            .ok()
                    })
                    .collect::<Vec<_>>(),
                ast::FunctionArgs::Unnamed(_) => {
                    vec![]
                }
            }),
            content: self.template_string().to_string(),
        })
    }
}

type EnumId = String;

#[derive(serde::Serialize, Debug)]
pub struct EnumValue(pub String);

#[derive(serde::Serialize, Debug)]
pub struct Enum {
    pub name: EnumId,
    pub values: Vec<Node<EnumValue>>,
}

impl WithRepr<EnumValue> for EnumValueWalker<'_> {
    fn attributes(&self, db: &ParserDatabase) -> NodeAttributes {
        let mut attributes = NodeAttributes {
            meta: to_ir_attributes(db, self.get_default_attributes()),
            overrides: IndexMap::new(),
            span: Some(self.span().clone()),
        };

        attributes
    }

    fn repr(&self, _db: &ParserDatabase) -> Result<EnumValue> {
        Ok(EnumValue(self.name().to_string()))
    }
}

impl WithRepr<Enum> for EnumWalker<'_> {
    fn attributes(&self, db: &ParserDatabase) -> NodeAttributes {
        let mut attributes = NodeAttributes {
            meta: to_ir_attributes(db, self.get_default_attributes()),
            overrides: IndexMap::new(),
            span: Some(self.span().clone()),
        };

        attributes.meta = to_ir_attributes(db, self.get_default_attributes());

        attributes
    }

    fn repr(&self, db: &ParserDatabase) -> Result<Enum> {
        Ok(Enum {
            name: self.name().to_string(),
            values: self
                .values()
                .map(|v| v.node(db))
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

#[derive(serde::Serialize, Debug)]
pub struct Field {
    pub name: String,
    pub r#type: Node<FieldType>,
}

impl WithRepr<Field> for FieldWalker<'_> {
    fn attributes(&self, db: &ParserDatabase) -> NodeAttributes {
        let mut attributes = NodeAttributes {
            meta: to_ir_attributes(db, self.get_default_attributes()),
            overrides: IndexMap::new(),
            span: Some(self.span().clone()),
        };

        attributes
    }

    fn repr(&self, db: &ParserDatabase) -> Result<Field> {
        Ok(Field {
            name: self.name().to_string(),
            r#type: self.ast_field().field_type.node(db)?,
        })
    }
}

type ClassId = String;

#[derive(serde::Serialize, Debug)]
pub struct Class {
    pub name: ClassId,
    pub static_fields: Vec<Node<Field>>,
    pub dynamic_fields: Vec<Node<Field>>,
}

impl WithRepr<Class> for ClassWalker<'_> {
    fn attributes(&self, db: &ParserDatabase) -> NodeAttributes {
        let mut attributes = NodeAttributes {
            meta: to_ir_attributes(db, self.get_default_attributes()),
            overrides: IndexMap::new(),
            span: Some(self.span().clone()),
        };

        attributes.meta = to_ir_attributes(db, self.get_default_attributes());

        attributes
    }

    fn repr(&self, db: &ParserDatabase) -> Result<Class> {
        Ok(Class {
            name: self.name().to_string(),
            static_fields: self
                .static_fields()
                .map(|e| e.node(db))
                .collect::<Result<Vec<_>>>()?,
            dynamic_fields: self
                .dynamic_fields()
                .map(|e| e.node(db))
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

#[derive(serde::Serialize, Debug)]
pub enum OracleType {
    LLM,
}
#[derive(serde::Serialize, Debug)]
pub struct AliasOverride {
    pub name: String,
    // This is used to generate deserializers with aliased keys (see .overload in python deserializer)
    pub aliased_keys: Vec<AliasedKey>,
}

// TODO, also add skips
#[derive(serde::Serialize, Debug)]
pub struct AliasedKey {
    pub key: String,
    pub alias: Expression,
}

type ImplementationId = String;

#[derive(serde::Serialize, Debug)]
pub struct Implementation {
    r#type: OracleType,
    pub name: ImplementationId,
    pub function_name: String,

    pub prompt: Prompt,

    #[serde(with = "indexmap::map::serde_seq")]
    pub input_replacers: IndexMap<String, String>,

    #[serde(with = "indexmap::map::serde_seq")]
    pub output_replacers: IndexMap<String, String>,

    pub client: ClientId,

    /// Inputs for deserializer.overload in the generated code.
    ///
    /// This is NOT 1:1 with "override" clauses in the .baml file.
    ///
    /// For enums, we generate one for "alias", one for "description", and one for "alias: description"
    /// (this means that we currently don't support deserializing "alias[^a-zA-Z0-9]{1,5}description" but
    /// for now it suffices)
    pub overrides: Vec<AliasOverride>,
}

/// BAML does not allow UnnamedArgList nor a lone NamedArg
#[derive(serde::Serialize, Debug)]
pub enum FunctionArgs {
    UnnamedArg(FieldType),
    NamedArgList(Vec<(String, FieldType)>),
}

type FunctionId = String;

#[derive(serde::Serialize, Debug)]
#[serde(tag = "version")]
pub enum Function {
    V1(FunctionV1),
    V2(FunctionV2),
}

impl Function {
    pub fn name(&self) -> &str {
        match self {
            Function::V1(f) => &f.name,
            Function::V2(f) => &f.name,
        }
    }

    pub fn output(&self) -> &FieldType {
        match &self {
            Function::V1(f) => &f.output.elem,
            Function::V2(f) => &f.output.elem,
        }
    }

    pub fn inputs(&self) -> either::Either<&FunctionArgs, &Vec<(String, FieldType)>> {
        match &self {
            Function::V1(f) => either::Either::Left(&f.inputs),
            Function::V2(f) => either::Either::Right(&f.inputs),
        }
    }

    pub fn tests(&self) -> &Vec<Node<TestCase>> {
        match &self {
            Function::V1(f) => &f.tests,
            Function::V2(f) => &f.tests,
        }
    }

    pub fn configs(&self) -> Option<&Vec<FunctionConfig>> {
        match &self {
            Function::V1(_) => None,
            Function::V2(f) => Some(&f.configs),
        }
    }
}

#[derive(serde::Serialize, Debug)]
pub struct FunctionV1 {
    pub name: FunctionId,
    pub inputs: FunctionArgs,
    pub output: Node<FieldType>,
    pub impls: Vec<Node<Implementation>>,
    pub tests: Vec<Node<TestCase>>,
    pub default_impl: Option<ImplementationId>,
}

#[derive(serde::Serialize, Debug)]
pub struct FunctionV2 {
    pub name: FunctionId,
    pub inputs: Vec<(String, FieldType)>,
    pub output: Node<FieldType>,
    pub tests: Vec<Node<TestCase>>,
    pub configs: Vec<FunctionConfig>,
    pub default_config: String,
}

#[derive(serde::Serialize, Debug)]
pub struct FunctionConfig {
    pub name: String,
    pub prompt_template: String,
    #[serde(skip)]
    pub prompt_span: ast::Span,
    pub client: ClientId,
}

fn process_field(
    overrides: &IndexMap<(String, String), IndexMap<String, Expression>>, // Adjust the type according to your actual field type
    original_name: &str,
    function_name: &str,
    impl_name: &str,
) -> Vec<AliasedKey> {
    // This feeds into deserializer.overload; the registerEnumDeserializer counterpart is in generate_ts_client.rs
    match overrides.get(&((*function_name).to_string(), (*impl_name).to_string())) {
        Some(overrides) => {
            if let Some(Expression::String(alias)) = overrides.get("alias") {
                if let Some(Expression::String(description)) = overrides.get("description") {
                    // "alias" and "alias: description"
                    vec![
                        AliasedKey {
                            key: original_name.to_string(),
                            alias: Expression::String(alias.clone()),
                        },
                        AliasedKey {
                            key: original_name.to_string(),
                            alias: Expression::String(format!("{}: {}", alias, description)),
                        },
                    ]
                } else {
                    // "alias"
                    vec![AliasedKey {
                        key: original_name.to_string(),
                        alias: Expression::String(alias.clone()),
                    }]
                }
            } else if let Some(Expression::String(description)) = overrides.get("description") {
                // "description"
                vec![AliasedKey {
                    key: original_name.to_string(),
                    alias: Expression::String(description.clone()),
                }]
            } else {
                // no overrides
                vec![]
            }
        }
        None => Vec::new(),
    }
}

impl WithRepr<Function> for FunctionWalker<'_> {
    fn attributes(&self, _: &ParserDatabase) -> NodeAttributes {
        NodeAttributes {
            meta: Default::default(),
            overrides: Default::default(),
            span: Some(self.span().clone()),
        }
    }

    fn repr(&self, db: &ParserDatabase) -> Result<Function> {
        if self.is_old_function() {
            Ok(Function::V1(self.repr(db)?))
        } else {
            Ok(Function::V2(self.repr(db)?))
        }
    }
}

impl WithRepr<FunctionV1> for FunctionWalker<'_> {
    fn repr(&self, db: &ParserDatabase) -> Result<FunctionV1> {
        if !self.is_old_function() {
            bail!("Cannot represent a new function as a FunctionV1")
        }
        Ok(FunctionV1 {
            name: self.name().to_string(),
            inputs: match self.ast_function().input() {
                ast::FunctionArgs::Named(arg_list) => FunctionArgs::NamedArgList(
                    arg_list
                        .args
                        .iter()
                        .map(|(id, arg)| Ok((id.name().to_string(), arg.field_type.repr(db)?)))
                        .collect::<Result<Vec<_>>>()?,
                ),
                ast::FunctionArgs::Unnamed(arg) => {
                    FunctionArgs::UnnamedArg(arg.field_type.node(db)?.elem)
                }
            },
            output: match self.ast_function().output() {
                ast::FunctionArgs::Named(_) => bail!("Functions may not return named args"),
                ast::FunctionArgs::Unnamed(arg) => arg.field_type.node(db),
            }?,
            default_impl: self.metadata().default_impl.as_ref().map(|f| f.0.clone()),
            impls: {
                let mut impls = self
                    .walk_variants()
                    .map(|e| e.node(db))
                    .collect::<Result<Vec<_>>>()?;
                impls.sort_by(|a, b| a.elem.name.cmp(&&b.elem.name));
                impls
            },
            tests: self
                .walk_tests()
                .map(|e| e.node(db))
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

impl WithRepr<FunctionV2> for FunctionWalker<'_> {
    fn repr(&self, db: &ParserDatabase) -> Result<FunctionV2> {
        if self.is_old_function() {
            bail!("Cannot represent a new function as a FunctionV1")
        }
        Ok(FunctionV2 {
            name: self.name().to_string(),
            inputs: match self.ast_function().input() {
                ast::FunctionArgs::Named(arg_list) => arg_list
                    .args
                    .iter()
                    .map(|(id, arg)| Ok((id.name().to_string(), arg.field_type.repr(db)?)))
                    .collect::<Result<Vec<_>>>()?,
                ast::FunctionArgs::Unnamed(_) => bail!("Unnamed args not supported"),
            },
            output: match self.ast_function().output() {
                ast::FunctionArgs::Named(_) => bail!("Functions may not return named args"),
                ast::FunctionArgs::Unnamed(arg) => arg.field_type.node(db),
            }?,
            configs: vec![FunctionConfig {
                name: "default_config".to_string(),
                prompt_template: self.jinja_prompt().to_string(),
                prompt_span: self.ast_function().span().clone(),
                client: self
                    .client()
                    .context("Unable to generate ctx.client")?
                    .name()
                    .to_string(),
            }],
            default_config: "default_config".to_string(),
            tests: self
                .walk_tests()
                .map(|e| e.node(db))
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

type ClientId = String;

#[derive(serde::Serialize, Debug)]
pub struct Client {
    pub name: ClientId,
    pub provider: String,
    pub retry_policy_id: Option<String>,
    pub options: Vec<(String, Expression)>,
}

impl WithRepr<Client> for ClientWalker<'_> {
    fn attributes(&self, _: &ParserDatabase) -> NodeAttributes {
        NodeAttributes {
            meta: IndexMap::new(),
            overrides: IndexMap::new(),
            span: Some(self.span().clone()),
        }
    }

    fn repr(&self, db: &ParserDatabase) -> Result<Client> {
        Ok(Client {
            name: self.name().to_string(),
            provider: self.properties().provider.0.clone(),
            options: self
                .properties()
                .options
                .iter()
                .map(|(k, v)| Ok((k.clone(), v.repr(db)?)))
                .collect::<Result<Vec<_>>>()?,
            retry_policy_id: self
                .properties()
                .retry_policy
                .as_ref()
                .map(|(id, _)| id.clone()),
        })
    }
}

#[derive(serde::Serialize, Debug)]
pub struct RetryPolicyId(pub String);

#[derive(serde::Serialize, Debug)]
pub struct RetryPolicy {
    pub name: RetryPolicyId,
    pub max_retries: u32,
    pub strategy: RetryPolicyStrategy,
    // NB: the parser DB has a notion of "empty options" vs "no options"; we collapse
    // those here into an empty vec
    options: Vec<(String, Expression)>,
}

impl WithRepr<RetryPolicy> for ConfigurationWalker<'_> {
    fn attributes(&self, _db: &ParserDatabase) -> NodeAttributes {
        NodeAttributes {
            meta: IndexMap::new(),
            overrides: IndexMap::new(),
            span: Some(self.span().clone()),
        }
    }

    fn repr(&self, db: &ParserDatabase) -> Result<RetryPolicy> {
        Ok(RetryPolicy {
            name: RetryPolicyId(self.name().to_string()),
            max_retries: self.retry_policy().max_retries,
            strategy: self.retry_policy().strategy,
            options: match &self.retry_policy().options {
                Some(o) => o
                    .iter()
                    .map(|((k, _), v)| Ok((k.clone(), v.repr(db)?)))
                    .collect::<Result<Vec<_>>>()?,
                None => vec![],
            },
        })
    }
}

#[derive(serde::Serialize, Debug)]
pub struct TestCase {
    pub name: String,
    pub args: IndexMap<String, Expression>,
}

impl WithRepr<TestCase> for ConfigurationWalker<'_> {
    fn attributes(&self, _db: &ParserDatabase) -> NodeAttributes {
        NodeAttributes {
            meta: IndexMap::new(),
            overrides: IndexMap::new(),
            span: Some(self.span().clone()),
        }
    }

    fn repr(&self, db: &ParserDatabase) -> Result<TestCase> {
        Ok(TestCase {
            name: self.name().to_string(),
            args: self
                .test_case()
                .args
                .iter()
                .map(|(k, (_, v))| Ok((k.clone(), v.repr(db)?)))
                .collect::<Result<IndexMap<_, _>>>()?,
        })
    }
}
#[derive(Debug, Clone, Serialize)]
pub enum Prompt {
    // The prompt stirng, and a list of input replacer keys (raw key w/ magic string, and key to replace with)
    String(String, Vec<(String, String)>),

    // same thing, the chat message, and the replacer input keys (raw key w/ magic string, and key to replace with)
    Chat(Vec<ChatMessage>, Vec<(String, String)>),
}

#[derive(serde::Serialize, Debug, Clone)]
pub struct ChatMessage {
    pub idx: u32,
    pub role: String,
    pub content: String,
}

impl WithRepr<Prompt> for PromptAst<'_> {
    fn repr(&self, _db: &ParserDatabase) -> Result<Prompt> {
        Ok(match self {
            PromptAst::String(content, _) => Prompt::String(content.clone(), vec![]),
            PromptAst::Chat(messages, input_replacers) => Prompt::Chat(
                messages
                    .iter()
                    .filter_map(|(message, content)| {
                        message.as_ref().map(|m| ChatMessage {
                            idx: m.idx,
                            role: m.role.0.clone(),
                            content: content.clone(),
                        })
                    })
                    .collect::<Vec<_>>(),
                input_replacers.to_vec(),
            ),
        })
    }
}

// impl ChatBlock {
//     /// Unique Key
//     pub fn key(&self) -> String {
//         format!("{{//BAML_CLIENT_REPLACE_ME_CHAT_MAGIC_{}//}}", self.idx)
//     }
// }
