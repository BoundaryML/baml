use std::collections::HashSet;

use anyhow::{anyhow, Result};
use baml_types::{
    Constraint, ConstraintLevel, FieldType, JinjaExpression, Resolvable, StreamingBehavior,
    StringOr, UnresolvedValue,
};
use either::Either;
use indexmap::{IndexMap, IndexSet};
use internal_baml_parser_database::{
    walkers::{
        ClassWalker, ClientWalker, ConfigurationWalker, EnumValueWalker, EnumWalker, FieldWalker,
        FunctionWalker, TemplateStringWalker, TypeAliasWalker, Walker as AstWalker,
    },
    Attributes, ParserDatabase, PromptAst, RetryPolicyStrategy, TypeWalker,
};

use internal_baml_schema_ast::ast::{
    self, Attribute, FieldArity, SubType, ValExpId, WithName, WithSpan,
};
use internal_llm_client::{ClientProvider, ClientSpec, UnresolvedClientProperty};
use serde::Serialize;

use crate::Configuration;

/// This class represents the intermediate representation of the BAML AST.
/// It is a representation of the BAML AST that is easier to work with than the
/// raw BAML AST, and should include all information necessary to generate
/// code in any target language.
#[derive(Debug)]
pub struct IntermediateRepr {
    enums: Vec<Node<Enum>>,
    classes: Vec<Node<Class>>,
    type_aliases: Vec<Node<TypeAlias>>,
    functions: Vec<Node<Function>>,
    clients: Vec<Node<Client>>,
    retry_policies: Vec<Node<RetryPolicy>>,
    template_strings: Vec<Node<TemplateString>>,

    /// Strongly connected components of the dependency graph (finite cycles).
    finite_recursive_cycles: Vec<IndexSet<String>>,

    /// Type alias cycles introduced by lists and maps.
    ///
    /// These are the only allowed cycles, because lists and maps introduce a
    /// level of indirection that makes the cycle finite.
    structural_recursive_alias_cycles: Vec<IndexMap<String, FieldType>>,

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
            type_aliases: vec![],
            finite_recursive_cycles: vec![],
            structural_recursive_alias_cycles: vec![],
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

    pub fn required_env_vars(&self) -> HashSet<String> {
        // TODO: We should likely check the full IR.
        let mut env_vars = HashSet::new();

        for client in self.walk_clients() {
            client.required_env_vars().iter().for_each(|v| {
                env_vars.insert(v.to_string());
            });
        }

        // self.walk_functions().filter_map(
        //     |f| f.client_name()
        // ).map(|c| c.required_env_vars())

        // // for any functions, check for shorthand env vars
        // self.functions
        //     .iter()
        //     .filter_map(|f| f.elem.configs())
        //     .into_iter()
        //     .flatten()
        //     .flat_map(|(expr)| expr.client.required_env_vars())
        //     .collect()
        env_vars
    }

    /// Returns a list of all the recursive cycles in the IR.
    ///
    /// Each cycle is represented as a set of strings, where each string is the
    /// name of a class.
    pub fn finite_recursive_cycles(&self) -> &[IndexSet<String>] {
        &self.finite_recursive_cycles
    }

    pub fn structural_recursive_alias_cycles(&self) -> &[IndexMap<String, FieldType>] {
        &self.structural_recursive_alias_cycles
    }

    pub fn walk_enums(&self) -> impl ExactSizeIterator<Item = Walker<'_, &Node<Enum>>> {
        self.enums.iter().map(|e| Walker { db: self, item: e })
    }

    pub fn walk_classes(&self) -> impl ExactSizeIterator<Item = Walker<'_, &Node<Class>>> {
        self.classes.iter().map(|e| Walker { db: self, item: e })
    }

    pub fn walk_type_aliases(&self) -> impl ExactSizeIterator<Item = Walker<'_, &Node<TypeAlias>>> {
        self.type_aliases
            .iter()
            .map(|e| Walker { db: self, item: e })
    }

    // TODO: Exact size Iterator + Node<>?
    pub fn walk_alias_cycles(&self) -> impl Iterator<Item = Walker<'_, (&String, &FieldType)>> {
        self.structural_recursive_alias_cycles
            .iter()
            .flatten()
            .map(|e| Walker { db: self, item: e })
    }

    pub fn function_names(&self) -> impl ExactSizeIterator<Item = &str> {
        self.functions.iter().map(|f| f.elem.name())
    }

    pub fn walk_functions(&self) -> impl ExactSizeIterator<Item = Walker<'_, &Node<Function>>> {
        self.functions.iter().map(|e| Walker { db: self, item: e })
    }

    pub fn walk_tests(
        &self,
    ) -> impl Iterator<Item = Walker<'_, (&Node<Function>, &Node<TestCase>)>> {
        self.functions.iter().flat_map(move |f| {
            f.elem.tests().iter().map(move |t| Walker {
                db: self,
                item: (f, t),
            })
        })
    }

    pub fn walk_clients(&self) -> impl ExactSizeIterator<Item = Walker<'_, &Node<Client>>> {
        self.clients.iter().map(|e| Walker { db: self, item: e })
    }

    pub fn walk_template_strings(
        &self,
    ) -> impl ExactSizeIterator<Item = Walker<'_, &Node<TemplateString>>> {
        self.template_strings
            .iter()
            .map(|e| Walker { db: self, item: e })
    }

    #[allow(dead_code)]
    pub fn walk_retry_policies(
        &self,
    ) -> impl ExactSizeIterator<Item = Walker<'_, &Node<RetryPolicy>>> {
        self.retry_policies
            .iter()
            .map(|e| Walker { db: self, item: e })
    }

    pub fn from_parser_database(
        db: &ParserDatabase,
        configuration: Configuration,
    ) -> Result<IntermediateRepr> {
        // TODO: We're iterating over the AST tops once for every property in
        // the IR. Easy performance optimization here by iterating only one time
        // and distributing the tops to the appropriate IR properties.
        let mut repr = IntermediateRepr {
            enums: db
                .walk_enums()
                .map(|e| e.node(db))
                .collect::<Result<Vec<_>>>()?,
            classes: db
                .walk_classes()
                .map(|e| e.node(db))
                .collect::<Result<Vec<_>>>()?,
            type_aliases: db
                .walk_type_aliases()
                .map(|e| e.node(db))
                .collect::<Result<Vec<_>>>()?,
            finite_recursive_cycles: db
                .finite_recursive_cycles()
                .iter()
                .map(|ids| {
                    ids.iter()
                        .map(|id| db.ast()[*id].name().to_string())
                        .collect()
                })
                .collect(),
            structural_recursive_alias_cycles: {
                let mut recursive_aliases = vec![];
                for cycle in db.recursive_alias_cycles() {
                    let mut component = IndexMap::new();
                    for id in cycle {
                        let alias = &db.ast()[*id];
                        component.insert(alias.name().to_string(), alias.value.repr(db)?);
                    }
                    recursive_aliases.push(component);
                }
                recursive_aliases
            },
            functions: db
                .walk_functions()
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
            .sort_by(|a, b| a.elem.name().cmp(b.elem.name()));
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

#[derive(Debug)]
pub struct NodeAttributes {
    /// Map of attributes on the corresponding IR node.
    ///
    /// Some follow special conventions:
    ///
    ///   - @skip becomes ("skip", bool)
    ///   - @alias(...) becomes ("alias", ...)
    meta: IndexMap<String, UnresolvedValue<()>>,

    pub constraints: Vec<Constraint>,

    // Spans
    pub span: Option<ast::Span>,
}

impl NodeAttributes {
    pub fn get(&self, key: &str) -> Option<&UnresolvedValue<()>> {
        self.meta.get(key)
    }

    pub fn streaming_behavior(&self) -> StreamingBehavior {
        fn is_some_true(maybe_value: Option<&UnresolvedValue<()>>) -> bool {
            match maybe_value {
                Some(Resolvable::Bool(true, _)) => true,
                _ => false,
            }
        }
        StreamingBehavior {
            done: is_some_true(self.get("stream.done")),
            state: is_some_true(self.get("stream.with_state")),
        }
    }
}

impl Default for NodeAttributes {
    fn default() -> Self {
        NodeAttributes {
            meta: IndexMap::new(),
            constraints: Vec::new(),
            span: None,
        }
    }
}

fn to_ir_attributes(
    db: &ParserDatabase,
    maybe_ast_attributes: Option<&Attributes>,
) -> (IndexMap<String, UnresolvedValue<()>>, Vec<Constraint>) {
    let null_result = (IndexMap::new(), Vec::new());
    maybe_ast_attributes.map_or(null_result, |attributes| {
        let Attributes {
            description,
            alias,
            dynamic_type,
            skip,
            constraints,
            streaming_done,
            streaming_needed,
            streaming_state,
        } = attributes;

        let description = description
            .as_ref()
            .map(|d| ("description".to_string(), d.without_meta()));

        let alias = alias
            .as_ref()
            .map(|v| ("alias".to_string(), v.without_meta()));

        let dynamic_type = dynamic_type.as_ref().and_then(|v| {
            if *v {
                Some(("dynamic_type".to_string(), UnresolvedValue::Bool(true, ())))
            } else {
                None
            }
        });
        let skip = skip.as_ref().and_then(|v| {
            if *v {
                Some(("skip".to_string(), UnresolvedValue::Bool(true, ())))
            } else {
                None
            }
        });
        let streaming_done = streaming_done.as_ref().and_then(|v| {
            if *v {
                Some(("stream.done".to_string(), UnresolvedValue::Bool(true, ())))
            } else {
                None
            }
        });
        let streaming_needed = streaming_needed.as_ref().and_then(|v| {
            if *v {
                Some((
                    "stream.not_null".to_string(),
                    UnresolvedValue::Bool(true, ()),
                ))
            } else {
                None
            }
        });
        let streaming_state = streaming_state.as_ref().and_then(|v| {
            if *v {
                Some((
                    "stream.with_state".to_string(),
                    UnresolvedValue::Bool(true, ()),
                ))
            } else {
                None
            }
        });

        let meta = vec![
            description,
            alias,
            dynamic_type,
            skip,
            streaming_done,
            streaming_needed,
            streaming_state,
        ]
        .into_iter()
        .filter_map(|s| s)
        .collect();
        (meta, constraints.clone())
    })
}

/// Nodes allow attaching metadata to a given IR entity: attributes, source location, etc
#[derive(Debug)]
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
            constraints: Vec::new(),
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
    // TODO: (Greg) This code only extracts constraints, and ignores any
    // other types of attributes attached to the type directly.
    fn attributes(&self, _db: &ParserDatabase) -> NodeAttributes {
        let constraints = self
            .attributes()
            .iter()
            .filter_map(|attr| {
                let level = match attr.name.to_string().as_str() {
                    "assert" => Some(ConstraintLevel::Assert),
                    "check" => Some(ConstraintLevel::Check),
                    _ => None,
                }?;
                let (label, expression) = match attr.arguments.arguments.as_slice() {
                    [arg1, arg2] => match (arg1.clone().value, arg2.clone().value) {
                        (
                            ast::Expression::Identifier(ast::Identifier::Local(s, _)),
                            ast::Expression::JinjaExpressionValue(j, _),
                        ) => Some((Some(s), j)),
                        _ => None,
                    },
                    [arg1] => match arg1.clone().value {
                        ast::Expression::JinjaExpressionValue(JinjaExpression(j), _) => {
                            Some((None, JinjaExpression(j.clone())))
                        }
                        _ => None,
                    },
                    _ => None,
                }?;
                Some(Constraint {
                    level,
                    expression,
                    label,
                })
            })
            .collect::<Vec<Constraint>>();
        let mut meta = IndexMap::new();
        if self
            .attributes()
            .iter()
            .find(|Attribute { name, .. }| name.name() == "stream.done")
            .is_some()
        {
            let val: UnresolvedValue<()> = Resolvable::Bool(true, ());
            meta.insert("stream.done".to_string(), val);
        }
        if self
            .attributes()
            .iter()
            .find(|Attribute { name, .. }| name.name() == "stream.with_state")
            .is_some()
        {
            let val: UnresolvedValue<()> = Resolvable::Bool(true, ());
            meta.insert("stream.with_state".to_string(), val);
        }
        let attributes = NodeAttributes {
            meta,
            constraints,
            span: Some(self.span().clone()),
        };

        attributes
    }

    fn repr(&self, db: &ParserDatabase) -> Result<FieldType> {
        let attributes = WithRepr::attributes(self, db);
        let has_constraints = !attributes.constraints.is_empty();
        let streaming_behavior = attributes.streaming_behavior();
        let has_special_streaming_behavior = streaming_behavior != StreamingBehavior::default();
        let base = match self {
            ast::FieldType::Primitive(arity, typeval, ..) => {
                let repr = FieldType::Primitive(*typeval);
                if arity.is_optional() {
                    FieldType::Optional(Box::new(repr))
                } else {
                    repr
                }
            }
            ast::FieldType::Literal(arity, literal_value, ..) => {
                let repr = FieldType::Literal(literal_value.clone());
                if arity.is_optional() {
                    FieldType::Optional(Box::new(repr))
                } else {
                    repr
                }
            }
            ast::FieldType::Symbol(arity, idn, ..) => type_with_arity(
                match db.find_type(idn) {
                    Some(TypeWalker::Class(class_walker)) => {
                        let base_class = FieldType::Class(class_walker.name().to_string());
                        match class_walker.get_constraints(SubType::Class) {
                            Some(constraints) if !constraints.is_empty() => {
                                FieldType::WithMetadata {
                                    base: Box::new(base_class),
                                    constraints,
                                    streaming_behavior: streaming_behavior.clone(),
                                }
                            }
                            _ => base_class,
                        }
                    }
                    Some(TypeWalker::Enum(enum_walker)) => {
                        let base_type = FieldType::Enum(enum_walker.name().to_string());
                        match enum_walker.get_constraints(SubType::Enum) {
                            Some(constraints) if !constraints.is_empty() => {
                                FieldType::WithMetadata {
                                    base: Box::new(base_type),
                                    constraints,
                                    streaming_behavior: streaming_behavior.clone(),
                                }
                            }
                            _ => base_type,
                        }
                    }
                    Some(TypeWalker::TypeAlias(alias_walker)) => {
                        if db.is_recursive_type_alias(&alias_walker.id) {
                            FieldType::RecursiveTypeAlias(alias_walker.name().to_string())
                        } else {
                            alias_walker.resolved().to_owned().repr(db)?
                        }
                    }

                    None => return Err(anyhow!("Field type uses unresolvable local identifier")),
                },
                arity,
            ),
            ast::FieldType::List(arity, ft, dims, ..) => {
                // NB: potential bug: this hands back a 1D list when dims == 0
                let mut repr = FieldType::List(Box::new(ft.repr(db)?));

                for _ in 1u32..*dims {
                    repr = FieldType::list(repr);
                }

                if arity.is_optional() {
                    repr = FieldType::optional(repr);
                }

                repr
            }
            ast::FieldType::Map(arity, kv, ..) => {
                // NB: we can't just unpack (*kv) into k, v because that would require a move/copy
                let mut repr =
                    FieldType::Map(Box::new((kv).0.repr(db)?), Box::new((kv).1.repr(db)?));

                if arity.is_optional() {
                    repr = FieldType::optional(repr);
                }

                repr
            }
            ast::FieldType::Union(arity, t, ..) => {
                // NB: preempt union flattening by mixing arity into union types
                let mut types = t.iter().map(|ft| ft.repr(db)).collect::<Result<Vec<_>>>()?;

                if arity.is_optional() {
                    types.push(FieldType::Primitive(baml_types::TypeValue::Null));
                }

                FieldType::Union(types)
            }
            ast::FieldType::Tuple(arity, t, ..) => type_with_arity(
                FieldType::Tuple(t.iter().map(|ft| ft.repr(db)).collect::<Result<Vec<_>>>()?),
                arity,
            ),
        };

        let use_metadata = has_constraints || has_special_streaming_behavior;
        let with_constraints = if use_metadata {
            FieldType::WithMetadata {
                base: Box::new(base.clone()),
                constraints: attributes.constraints,
                streaming_behavior,
            }
        } else {
            base
        };
        Ok(with_constraints)
    }
}

type TemplateStringId = String;

#[derive(Debug)]
pub struct TemplateString {
    pub name: TemplateStringId,
    pub params: Vec<Field>,
    pub content: String,
}

impl WithRepr<TemplateString> for TemplateStringWalker<'_> {
    fn attributes(&self, _: &ParserDatabase) -> NodeAttributes {
        NodeAttributes {
            meta: Default::default(),
            constraints: Vec::new(),
            span: Some(self.span().clone()),
        }
    }

    fn repr(&self, _db: &ParserDatabase) -> Result<TemplateString> {
        Ok(TemplateString {
            name: self.name().to_string(),
            params: self.ast_node().input().map_or(vec![], |e| {
                let ast::BlockArgs { args, .. } = e;
                args.iter()
                    .filter_map(|(id, arg)| {
                        arg.field_type
                            .node(_db)
                            .map(|f| Field {
                                name: id.name().to_string(),
                                r#type: f,
                                docstring: None,
                            })
                            .ok()
                    })
                    .collect::<Vec<_>>()
            }),
            content: self.template_string().to_string(),
        })
    }
}
type EnumId = String;

#[derive(serde::Serialize, Debug)]
pub struct EnumValue(pub String);

#[derive(Debug)]
pub struct Enum {
    pub name: EnumId,
    pub values: Vec<(Node<EnumValue>, Option<Docstring>)>,
    /// Docstring.
    pub docstring: Option<Docstring>,
}

impl WithRepr<EnumValue> for EnumValueWalker<'_> {
    fn attributes(&self, db: &ParserDatabase) -> NodeAttributes {
        let (meta, constraints) = to_ir_attributes(db, self.get_default_attributes());
        let attributes = NodeAttributes {
            meta,
            constraints,
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
        let (meta, constraints) = to_ir_attributes(db, self.get_default_attributes(SubType::Enum));
        let attributes = NodeAttributes {
            meta,
            constraints,
            span: Some(self.span().clone()),
        };

        attributes
    }

    fn repr(&self, db: &ParserDatabase) -> Result<Enum> {
        Ok(Enum {
            // TODO: #1343 Temporary solution until we implement scoping in the AST.
            name: if self.ast_type_block().is_dynamic_type_def {
                self.name()
                    .strip_prefix(ast::DYNAMIC_TYPE_NAME_PREFIX)
                    .unwrap()
                    .to_string()
            } else {
                self.name().to_string()
            },
            values: self
                .values()
                .map(|w| {
                    w.node(db)
                        .map(|v| (v, w.documentation().map(|s| Docstring(s.to_string()))))
                })
                .collect::<Result<Vec<_>, _>>()?,
            docstring: self.get_documentation().map(Docstring),
        })
    }
}

#[derive(serde::Serialize, Debug)]
pub struct Docstring(pub String);

#[derive(Debug)]
pub struct Field {
    pub name: String,
    pub r#type: Node<FieldType>,
    pub docstring: Option<Docstring>,
}

impl WithRepr<Field> for FieldWalker<'_> {
    fn attributes(&self, db: &ParserDatabase) -> NodeAttributes {
        let (meta, constraints) = to_ir_attributes(db, self.get_default_attributes());
        let attributes = NodeAttributes {
            meta,
            constraints,
            span: Some(self.span().clone()),
        };

        attributes
    }

    fn repr(&self, db: &ParserDatabase) -> Result<Field> {
        let ast_field_type = self.ast_field().expr.as_ref().ok_or(anyhow!(
            "Internal error occurred while resolving repr of field {:?}",
            self.name(),
        ))?;
        let field_type_attributes = WithRepr::attributes(ast_field_type, db);
        let field_type = ast_field_type.repr(db)?;
        Ok(Field {
            name: self.name().to_string(),
            r#type: Node {
                elem: field_type,
                attributes: field_type_attributes,
            },
            docstring: self.get_documentation().map(Docstring),
        })
    }
}

type ClassId = String;

/// A BAML Class.
#[derive(Debug)]
pub struct Class {
    /// User defined class name.
    pub name: ClassId,

    /// Fields of the class.
    pub static_fields: Vec<Node<Field>>,

    /// Parameters to the class definition.
    pub inputs: Vec<(String, FieldType)>,

    /// Docstring.
    pub docstring: Option<Docstring>,
}

impl WithRepr<Class> for ClassWalker<'_> {
    fn attributes(&self, db: &ParserDatabase) -> NodeAttributes {
        let default_attributes = self.get_default_attributes(SubType::Class);
        let (meta, constraints) = to_ir_attributes(db, default_attributes);
        let attributes = NodeAttributes {
            meta,
            constraints,
            span: Some(self.span().clone()),
        };

        attributes
    }

    fn repr(&self, db: &ParserDatabase) -> Result<Class> {
        Ok(Class {
            // TODO: #1343 Temporary solution until we implement scoping in the AST.
            name: if self.ast_type_block().is_dynamic_type_def {
                self.name()
                    .strip_prefix(ast::DYNAMIC_TYPE_NAME_PREFIX)
                    .unwrap()
                    .to_string()
            } else {
                self.name().to_string()
            },
            static_fields: self
                .static_fields()
                .map(|e| e.node(db))
                .collect::<Result<Vec<_>>>()?,
            inputs: match self.ast_type_block().input() {
                Some(input) => input
                    .args
                    .iter()
                    .map(|arg| {
                        let field_type = arg.1.field_type.repr(db)?;
                        Ok((arg.0.to_string(), field_type))
                    })
                    .collect::<Result<Vec<_>>>()?,
                None => Vec::new(),
            },
            docstring: self.get_documentation().map(Docstring),
        })
    }
}

impl Class {
    pub fn inputs(&self) -> &Vec<(String, FieldType)> {
        &self.inputs
    }
}

#[derive(Debug)]
pub struct TypeAlias {
    pub name: String,
    pub r#type: Node<FieldType>,
    pub docstring: Option<Docstring>,
}

impl WithRepr<TypeAlias> for TypeAliasWalker<'_> {
    fn attributes(&self, _: &ParserDatabase) -> NodeAttributes {
        NodeAttributes {
            span: Some(self.span().clone()),
            ..Default::default() // TODO: Rest of attributes.
        }
    }

    fn repr(&self, db: &ParserDatabase) -> Result<TypeAlias> {
        Ok(TypeAlias {
            name: self.name().to_string(),
            r#type: self.target().node(db)?,
            docstring: None, // TODO: Type alias docstring
        })
    }
}

#[derive(serde::Serialize, Debug)]
pub enum OracleType {
    LLM,
}
#[derive(Debug)]
pub struct AliasOverride {
    pub name: String,
    // This is used to generate deserializers with aliased keys (see .overload in python deserializer)
    pub aliased_keys: Vec<AliasedKey>,
}

// TODO, also add skips
#[derive(Debug)]
pub struct AliasedKey {
    pub key: String,
    pub alias: UnresolvedValue<()>,
}

type ImplementationId = String;

#[derive(Debug)]
pub struct Implementation {
    r#type: OracleType,
    pub name: ImplementationId,
    pub function_name: String,

    pub prompt: Prompt,

    pub input_replacers: IndexMap<String, String>,

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

impl Function {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn output(&self) -> &FieldType {
        &self.output
    }

    pub fn inputs(&self) -> &Vec<(String, FieldType)> {
        &self.inputs
    }

    pub fn tests(&self) -> &Vec<Node<TestCase>> {
        &self.tests
    }

    pub fn configs(&self) -> Option<&Vec<FunctionConfig>> {
        Some(&self.configs)
    }
}

#[derive(Debug)]
pub struct Function {
    pub name: FunctionId,
    pub inputs: Vec<(String, FieldType)>,
    pub output: FieldType,
    pub tests: Vec<Node<TestCase>>,
    pub configs: Vec<FunctionConfig>,
    pub default_config: String,
}

#[derive(Debug)]
pub struct FunctionConfig {
    pub name: String,
    pub prompt_template: String,
    pub prompt_span: ast::Span,
    pub client: ClientSpec,
}

// impl std::fmt::Display for ClientSpec {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         write!(f, "{}", self.as_str())
//     }
// }

fn process_field(
    overrides: &IndexMap<(String, String), IndexMap<String, UnresolvedValue<()>>>, // Adjust the type according to your actual field type
    original_name: &str,
    function_name: &str,
    impl_name: &str,
) -> Vec<AliasedKey> {
    // This feeds into deserializer.overload; the registerEnumDeserializer counterpart is in generate_ts_client.rs
    match overrides.get(&((*function_name).to_string(), (*impl_name).to_string())) {
        Some(overrides) => {
            if let Some(UnresolvedValue::String(alias, ..)) = overrides.get("alias") {
                if let Some(UnresolvedValue::String(description, ..)) = overrides.get("description")
                {
                    // "alias" and "alias: description"
                    vec![
                        AliasedKey {
                            key: original_name.to_string(),
                            alias: UnresolvedValue::String(alias.clone(), ()),
                        },
                        // AliasedKey {
                        //     key: original_name.to_string(),
                        //     alias: UnresolvedValue::String(format!("{}: {}", alias, description)),
                        // },
                    ]
                } else {
                    // "alias"
                    vec![AliasedKey {
                        key: original_name.to_string(),
                        alias: UnresolvedValue::String(alias.clone(), ()),
                    }]
                }
            } else if let Some(UnresolvedValue::String(description, ..)) =
                overrides.get("description")
            {
                // "description"
                vec![AliasedKey {
                    key: original_name.to_string(),
                    alias: UnresolvedValue::String(description.clone(), ()),
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
            constraints: Vec::new(),
            span: Some(self.span().clone()),
        }
    }

    fn repr(&self, db: &ParserDatabase) -> Result<Function> {
        Ok(Function {
            name: self.name().to_string(),
            inputs: self
                .ast_function()
                .input()
                .expect("msg")
                .args
                .iter()
                .map(|arg| {
                    let field_type = arg.1.field_type.repr(db)?;
                    Ok((arg.0.to_string(), field_type))
                })
                .collect::<Result<Vec<_>>>()?,
            output: self
                .ast_function()
                .output()
                .expect("need block arg")
                .field_type
                .repr(db)?,
            configs: vec![FunctionConfig {
                name: "default_config".to_string(),
                prompt_template: self.jinja_prompt().to_string(),
                prompt_span: self.ast_function().span().clone(),
                client: match self.client_spec() {
                    Ok(spec) => spec,
                    Err(e) => anyhow::bail!("{}", e.message()),
                },
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

#[derive(Debug)]
pub struct Client {
    pub name: ClientId,
    pub provider: ClientProvider,
    pub retry_policy_id: Option<String>,
    pub options: UnresolvedClientProperty<()>,
}

impl WithRepr<Client> for ClientWalker<'_> {
    fn attributes(&self, _: &ParserDatabase) -> NodeAttributes {
        NodeAttributes {
            meta: IndexMap::new(),
            constraints: Vec::new(),
            span: Some(self.span().clone()),
        }
    }

    fn repr(&self, db: &ParserDatabase) -> Result<Client> {
        Ok(Client {
            name: self.name().to_string(),
            provider: self.properties().provider.0.clone(),
            options: self.properties().options.without_meta(),
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

#[derive(Debug)]
pub struct RetryPolicy {
    pub name: RetryPolicyId,
    pub max_retries: u32,
    pub strategy: RetryPolicyStrategy,
    // NB: the parser DB has a notion of "empty options" vs "no options"; we collapse
    // those here into an empty vec
    options: Vec<(String, UnresolvedValue<()>)>,
}

impl WithRepr<RetryPolicy> for ConfigurationWalker<'_> {
    fn attributes(&self, _db: &ParserDatabase) -> NodeAttributes {
        NodeAttributes {
            meta: IndexMap::new(),
            constraints: Vec::new(),
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
                    .map(|(k, (_, v))| Ok((k.clone(), v.without_meta())))
                    .collect::<Result<Vec<_>>>()?,
                None => vec![],
            },
        })
    }
}

// TODO: #1343 Temporary solution until we implement scoping in the AST.
#[derive(Debug)]
pub enum TypeBuilderEntry {
    Enum(Node<Enum>),
    Class(Node<Class>),
    TypeAlias(Node<TypeAlias>),
}

// TODO: #1343 Temporary solution until we implement scoping in the AST.
#[derive(Debug)]
pub struct TestTypeBuilder {
    pub entries: Vec<TypeBuilderEntry>,
    pub structural_recursive_alias_cycles: Vec<IndexMap<String, FieldType>>,
}

#[derive(serde::Serialize, Debug)]
pub struct TestCaseFunction(String);

impl TestCaseFunction {
    pub fn name(&self) -> &str {
        &self.0
    }
}

#[derive(Debug)]
pub struct TestCase {
    pub name: String,
    pub functions: Vec<Node<TestCaseFunction>>,
    pub args: IndexMap<String, UnresolvedValue<()>>,
    pub constraints: Vec<Constraint>,
    pub type_builder: TestTypeBuilder,
}

impl WithRepr<TestCaseFunction> for (&ConfigurationWalker<'_>, usize) {
    fn attributes(&self, _db: &ParserDatabase) -> NodeAttributes {
        let span = self.0.test_case().functions[self.1].1.clone();
        let constraints = self
            .0
            .test_case()
            .constraints
            .iter()
            .map(|(c, _, _)| c)
            .cloned()
            .collect();
        NodeAttributes {
            meta: IndexMap::new(),
            constraints,
            span: Some(span),
        }
    }

    fn repr(&self, _db: &ParserDatabase) -> Result<TestCaseFunction> {
        Ok(TestCaseFunction(
            self.0.test_case().functions[self.1].0.clone(),
        ))
    }
}

impl WithRepr<TestCase> for ConfigurationWalker<'_> {
    fn attributes(&self, _db: &ParserDatabase) -> NodeAttributes {
        let constraints = self
            .test_case()
            .constraints
            .iter()
            .map(|(c, _, _)| c)
            .cloned()
            .collect();
        NodeAttributes {
            meta: IndexMap::new(),
            span: Some(self.span().clone()),
            constraints,
        }
    }

    fn repr(&self, db: &ParserDatabase) -> Result<TestCase> {
        let functions = (0..self.test_case().functions.len())
            .map(|i| (self, i).node(db))
            .collect::<Result<Vec<_>>>()?;

        // TODO: #1343 Temporary solution until we implement scoping in the AST.
        let enums = self
            .test_case()
            .type_builder_scoped_db
            .walk_enums()
            .filter(|e| {
                self.test_case().type_builder_scoped_db.ast()[e.id].is_dynamic_type_def
                    || db.find_type_by_str(e.name()).is_none()
            })
            .map(|e| e.node(&self.test_case().type_builder_scoped_db))
            .collect::<Result<Vec<Node<Enum>>>>()?;
        let classes = self
            .test_case()
            .type_builder_scoped_db
            .walk_classes()
            .filter(|c| {
                self.test_case().type_builder_scoped_db.ast()[c.id].is_dynamic_type_def
                    || db.find_type_by_str(c.name()).is_none()
            })
            .map(|c| c.node(&self.test_case().type_builder_scoped_db))
            .collect::<Result<Vec<Node<Class>>>>()?;
        let type_aliases = self
            .test_case()
            .type_builder_scoped_db
            .walk_type_aliases()
            .filter(|a| db.find_type_by_str(a.name()).is_none())
            .map(|a| a.node(&self.test_case().type_builder_scoped_db))
            .collect::<Result<Vec<Node<TypeAlias>>>>()?;
        let mut type_builder_entries = Vec::new();

        for e in enums {
            type_builder_entries.push(TypeBuilderEntry::Enum(e));
        }
        for c in classes {
            type_builder_entries.push(TypeBuilderEntry::Class(c));
        }
        for a in type_aliases {
            type_builder_entries.push(TypeBuilderEntry::TypeAlias(a));
        }

        let mut recursive_aliases = vec![];
        for cycle in self
            .test_case()
            .type_builder_scoped_db
            .recursive_alias_cycles()
        {
            let mut component = IndexMap::new();
            for id in cycle {
                let alias = &self.test_case().type_builder_scoped_db.ast()[*id];
                // Those are global cycles, skip.
                if db.find_type_by_str(alias.name()).is_some() {
                    continue;
                }
                // Cycles defined in the scoped test type builder block.
                component.insert(
                    alias.name().to_string(),
                    alias.value.repr(&self.test_case().type_builder_scoped_db)?,
                );
            }
            recursive_aliases.push(component);
        }

        Ok(TestCase {
            name: self.name().to_string(),
            args: self
                .test_case()
                .args
                .iter()
                .map(|(k, (_, v))| Ok((k.clone(), v.without_meta())))
                .collect::<Result<IndexMap<_, _>>>()?,
            functions,
            constraints: <AstWalker<'_, (ValExpId, &str)> as WithRepr<TestCase>>::attributes(
                self, db,
            )
            .constraints
            .into_iter()
            .collect::<Vec<_>>(),
            type_builder: TestTypeBuilder {
                entries: type_builder_entries,
                structural_recursive_alias_cycles: recursive_aliases,
            },
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

/// Generate an IntermediateRepr from a single block of BAML source code.
/// This is useful for generating IR test fixtures.
pub fn make_test_ir(source_code: &str) -> anyhow::Result<IntermediateRepr> {
    use crate::validate;
    use crate::ValidatedSchema;
    use internal_baml_diagnostics::SourceFile;
    use std::path::PathBuf;

    let path: PathBuf = "fake_file.baml".into();
    let source_file: SourceFile = (path.clone(), source_code).into();
    let validated_schema: ValidatedSchema = validate(&path, vec![source_file]);
    let diagnostics = &validated_schema.diagnostics;
    if diagnostics.has_errors() {
        return Err(anyhow::anyhow!(
            "Source code was invalid: \n{:?}",
            diagnostics.errors()
        ));
    }
    let ir = IntermediateRepr::from_parser_database(
        &validated_schema.db,
        validated_schema.configuration,
    )?;
    Ok(ir)
}

/// Pull out `StreamingBehavior` from `NodeAttributes`.
fn streaming_behavior_from_attributes(attributes: &NodeAttributes) -> StreamingBehavior {
    fn is_some_true(maybe_value: Option<&UnresolvedValue<()>>) -> bool {
        match maybe_value {
            Some(Resolvable::Bool(true, _)) => true,
            _ => false,
        }
    }
    StreamingBehavior {
        done: is_some_true(attributes.get("stream.done")),
        state: is_some_true(attributes.get("stream.with_state")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{ir_helpers::IRHelper, TypeValue};

    #[test]
    fn test_docstrings() {
        let ir = make_test_ir(
            r#"
          /// Foo class.
          class Foo {
            /// Bar field.
            bar string

            /// Baz field.
            baz int
          }

          /// Test enum.
          enum TestEnum {
            /// First variant.
            FIRST

            /// Second variant.
            SECOND

            THIRD
          }
        "#,
        )
        .unwrap();

        // Test class docstrings
        let foo = ir.find_class("Foo").as_ref().unwrap().clone().elem();
        assert_eq!(foo.docstring.as_ref().unwrap().0.as_str(), "Foo class.");
        match foo.static_fields.as_slice() {
            [field1, field2] => {
                assert_eq!(field1.elem.docstring.as_ref().unwrap().0, "Bar field.");
                assert_eq!(field2.elem.docstring.as_ref().unwrap().0, "Baz field.");
            }
            _ => {
                panic!("Expected 2 fields");
            }
        }

        // Test enum docstrings
        let test_enum = ir.find_enum("TestEnum").as_ref().unwrap().clone().elem();
        assert_eq!(
            test_enum.docstring.as_ref().unwrap().0.as_str(),
            "Test enum."
        );
        match test_enum.values.as_slice() {
            [val1, val2, val3] => {
                assert_eq!(val1.0.elem.0, "FIRST");
                assert_eq!(val1.1.as_ref().unwrap().0, "First variant.");
                assert_eq!(val2.0.elem.0, "SECOND");
                assert_eq!(val2.1.as_ref().unwrap().0, "Second variant.");
                assert_eq!(val3.0.elem.0, "THIRD");
                assert!(val3.1.is_none());
            }
            _ => {
                panic!("Expected 3 enum values");
            }
        }
    }

    #[test]
    fn test_block_attributes() {
        let ir = make_test_ir(
            r##"
            client<llm> GPT4 {
              provider openai
              options {
                model gpt-4o
                api_key env.OPENAI_API_KEY
              }
            }
            function Foo(a: int) -> int {
              client GPT4
              prompt #"Double the number {{ a }}"#
            }

            test Foo() {
              functions [Foo]
              args {
                a 10
              }
              @@assert( {{ result == 20 }} )
            }
        "##,
        )
        .unwrap();
        let function = ir.find_function("Foo").unwrap();
        let walker = ir.find_test(&function, "Foo").unwrap();
        assert_eq!(walker.item.1.elem.constraints.len(), 1);
    }

    #[test]
    fn test_streaming_attributes() {
        let ir = make_test_ir(
            r##"
            class Foo {
              foo_int int @stream.not_null
              foo_bool bool @stream.with_state
              foo_list int[] @stream.done
            }

            class Bar {
              name string @stream.done
              message string
              @@stream.done
            }
        "##,
        )
        .unwrap();
        let foo = ir.find_class("Foo").unwrap();
        assert!(!foo.streaming_done());
        match foo.walk_fields().collect::<Vec<_>>().as_slice() {
            [field1, field2, field3] => {
                let type1 = &field1.item.elem.r#type;
                assert!(field1.streaming_needed());
                assert!(type1.attributes.get("stream.not_null").is_none());
                let type2 = &field2.item.elem.r#type;
                assert!(!field2.streaming_state());
                assert!(type2.attributes.get("stream.with_state").is_some());
                let type3 = &field3.item.elem.r#type;
                assert!(!field3.streaming_done());
                assert!(type3.attributes.get("stream.done").is_some());
            }
            _ => panic!("Expected exactly 3 fields"),
        }
        let bar = ir.find_class("Bar").unwrap();
        assert!(bar.streaming_done());
        match bar.walk_fields().collect::<Vec<_>>().as_slice() {
            [field1, field2] => {
                assert!(!field1.streaming_done());
                assert!(field1
                    .item
                    .elem
                    .r#type
                    .attributes
                    .get("stream.done")
                    .is_some());
            }
            _ => panic!("Expected exactly 2 fields"),
        }
    }

    fn test_resolve_type_alias() {
        let ir = make_test_ir(
            r##"
            type One = int
            type Two = One
            type Three = Two

            class Test {
                field Three
            }
        "##,
        )
        .unwrap();

        let class = ir.find_class("Test").unwrap();
        let alias = class.find_field("field").unwrap();

        assert_eq!(*alias.r#type(), FieldType::Primitive(TypeValue::Int));
    }

    #[test]
    fn test_merge_type_alias_attributes() {
        let ir = make_test_ir(
            r##"
            type One = int @check(gt_ten, {{ this > 10 }})
            type Two = One @check(lt_twenty, {{ this < 20 }})
            type Three = Two @assert({{ this != 15 }})

            class Test {
                field Three
            }
        "##,
        )
        .unwrap();

        let class = ir.find_class("Test").unwrap();
        let alias = class.find_field("field").unwrap();

        let FieldType::WithMetadata {
            base, constraints, ..
        } = alias.r#type()
        else {
            panic!(
                "expected resolved constrained type, found {:?}",
                alias.r#type()
            );
        };

        assert_eq!(constraints.len(), 3);

        assert_eq!(constraints[0].level, ConstraintLevel::Assert);
        assert_eq!(constraints[0].label, None);

        assert_eq!(constraints[1].level, ConstraintLevel::Check);
        assert_eq!(constraints[1].label, Some("lt_twenty".to_string()));

        assert_eq!(constraints[2].level, ConstraintLevel::Check);
        assert_eq!(constraints[2].label, Some("gt_ten".to_string()));
    }
}
