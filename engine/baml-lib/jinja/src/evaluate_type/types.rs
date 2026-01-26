use core::panic;
use std::{
    collections::{HashMap, HashSet},
    ops::BitOr,
    vec,
};

use baml_types::LiteralValue;
use indexmap::IndexMap;
use minijinja::machinery::{
    ast::{Call, Spanned},
    Span,
};

use super::TypeError;

#[derive(Debug, Clone)]
pub struct EnumDefinition {
    pub name: String,
    pub values: Vec<EnumValueDefinition>,
}

#[derive(Debug, Clone)]
pub struct EnumValueDefinition {
    pub name: String,
    pub alias: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Type {
    Unknown,
    Undefined,
    None,
    Int,
    Float,
    // Too large to handle
    Number,
    String,
    Bool,
    Literal(LiteralValue),
    List(Box<Type>),
    Map(Box<Type>, Box<Type>),
    Tuple(Vec<Type>),
    Union(Vec<Type>),
    // It is simultaneously two types, whichever fits best
    Both(Box<Type>, Box<Type>),
    ClassRef(String),
    EnumTypeRef(String),
    EnumValueRef(String),
    FunctionRef(String),
    /// TODO: This should be `AliasRef(String)` but functions like
    /// [`Self::is_subtype_of`] or [`Self::bitor`] don't have access to the
    /// [`PredefinedTypes`] instance, so we can't grab type resolutions from
    /// there.
    ///
    /// We'll just store all the necessary information in the type itself for
    /// now.
    Alias {
        name: String,
        target: Box<Type>,
        resolved: Box<Type>,
    },
    /// TODO: This one could store the target so that we can report what it
    /// points to instead of just the name.
    RecursiveTypeAlias(String),
    Image,
    Audio,
}

impl Type {
    /// This is very similar to FieldType::is_subtype_of.
    pub fn is_subtype_of(&self, other: &Self) -> bool {
        if self == other {
            return true;
        }
        if let Type::Union(items) = other {
            if items.iter().any(|x| self.is_subtype_of(x)) {
                return true;
            }
        }
        match (self, other) {
            (Type::Unknown, _) => true,
            (_, Type::Unknown) => true,
            (_, Type::Undefined) => false,
            (_, Type::None) => false,
            (Type::Undefined, _) => false,
            (Type::None, _) => false,

            // Handle types that nest other types.
            (Type::List(l0), Type::List(r0)) => l0.is_subtype_of(r0),
            (Type::List(_), _) => false,
            (Type::Map(l0, l1), Type::Map(r0, r1)) => l0.is_subtype_of(r0) && l1.is_subtype_of(r1),
            (Type::Map(_, _), _) => false,

            (Type::Int, Type::Number) => true,
            (Type::Int, _) => false,

            (Type::Float, Type::Number) => true,
            (Type::Float, _) => false,

            // This is cause jinja is dumb and doesn't know the difference between int and float
            (Type::Number, Type::Float | Type::Int) => true,
            (Type::Number, _) => false,

            (Type::Literal(LiteralValue::Int(_)), Type::Int | Type::Number) => true,
            (Type::Literal(LiteralValue::Bool(_)), Type::Bool) => true,
            (Type::Literal(LiteralValue::String(_)), Type::String) => true,
            (Type::Literal(_), _) => false,

            (Type::Union(l0), _) => l0.iter().all(|x| x.is_subtype_of(other)),

            (Type::Both(l0, r0), _) => l0.is_subtype_of(other) || r0.is_subtype_of(other),
            (_, Type::Both(l0, r0)) => self.is_subtype_of(l0) && self.is_subtype_of(r0),

            (Type::Tuple(l0), Type::Tuple(r0)) => {
                if l0.len() != r0.len() {
                    return false;
                }
                l0.iter().zip(r0.iter()).all(|(l, r)| l.is_subtype_of(r))
            }
            (Type::Tuple(_), _) => false,

            (Type::ClassRef(_), _) => false,
            (Type::EnumTypeRef(_), _) => false,
            (Type::EnumValueRef(_), _) => false,
            (Type::FunctionRef(_), _) => false,
            (Type::Alias { resolved, .. }, _) => resolved.is_subtype_of(other),
            (Type::RecursiveTypeAlias(_), _) => false,
            (Type::Image, _) => false,
            (Type::Audio, _) => false,
            (Type::String, _) => false,
            (Type::Bool, _) => false,
        }
    }

    /// Used for type narrowing.
    ///
    /// Basic example:
    ///
    /// ```ignore
    /// class UserMessage {
    ///     kind "user_message"
    ///     user_message String
    /// }
    ///
    /// class AssistantMessage {
    ///     kind "assistant_message"
    ///     assistant_message String
    /// }
    /// ```
    ///
    /// Now on narrowing:
    ///
    /// ```ignore
    /// {% if message.kind == "user_message" %}
    /// ```
    pub fn equals_ignoring_literal_values(&self, other: &Self) -> bool {
        match (self, other) {
            (Type::Literal(left), Type::Literal(right)) => matches!(
                (left, right),
                (LiteralValue::Int(_), LiteralValue::Int(_))
                    | (LiteralValue::Bool(_), LiteralValue::Bool(_))
                    | (LiteralValue::String(_), LiteralValue::String(_))
            ),

            // Fallback to PartialEq
            _ => self == other,
        }
    }

    // pub fn matches(&self, r: &Self) -> bool {
    //     match (self, r) {
    //         (Self::Unknown, Self::Unknown) => true,
    //         (Self::Unknown, _) => true,
    //         (_, Self::Unknown) => true,
    //         (Self::Number, Self::Int | Self::Float) => true,
    //         (Self::Int | Self::Float, Self::Number) => true,
    //         (Self::List(l0), Self::List(r0)) => l0.matches(r0),
    //         (Self::Map(l0, l1), Self::Map(r0, r1)) => l0.matches(r0) && l1.matches(r1),
    //         (Self::Union(l0), Self::Union(r0)) => {
    //             // Sort l0 and r0 to make sure the order doesn't matter
    //             let mut l0 = l0.clone();
    //             let mut r0 = r0.clone();
    //             l0.sort();
    //             r0.sort();
    //             l0 == r0
    //         }
    //         (l0, Self::Union(r0)) => r0.iter().any(|x| l0.matches(x)),
    //         (Self::ClassRef(l0), Self::ClassRef(r0)) => l0 == r0,
    //         (Self::FunctionRef(l0), Self::FunctionRef(r0)) => l0 == r0,
    //         _ => core::mem::discriminant(self) == core::mem::discriminant(r),
    //     }
    // }

    pub fn name(&self) -> String {
        match self {
            Type::Unknown => "<unknown>".into(),
            Type::Undefined => "undefined".into(),
            Type::None => "none".into(),
            Type::Int => "int".into(),
            Type::Float => "float".into(),
            Type::Number => "number".into(),
            Type::String => "string".into(),
            Type::Bool => "bool".into(),
            Type::Literal(value) => format!("literal[{value}]"),
            Type::List(l) => format!("list[{}]", l.name()),
            Type::Map(k, v) => format!("map[{}, {}]", k.name(), v.name()),
            Type::Tuple(v) => format!(
                "({})",
                v.iter().map(|x| x.name()).collect::<Vec<_>>().join(", ")
            ),
            Type::Union(v) => format!(
                "({})",
                v.iter().map(|x| x.name()).collect::<Vec<_>>().join(" | ")
            ),
            Type::Both(l, r) => format!("{} & {}", l.name(), r.name()),
            Type::ClassRef(name) => format!("class {name}"),
            Type::EnumTypeRef(name) => format!("enum definition {name}"),
            Type::EnumValueRef(name) => format!("enum {name}"),
            Type::FunctionRef(name) => format!("function {name}"),
            Type::Alias { name, resolved, .. } => {
                format!("type alias {name} (resolves to {})", resolved.name())
            }
            Type::RecursiveTypeAlias(name) => format!("recursive type alias {name}"),
            Type::Image => "image".into(),
            Type::Audio => "audio".into(),
        }
    }

    pub fn is_optional(&self) -> bool {
        match self {
            Type::None => true,
            Type::Union(v) => v.iter().any(Type::is_optional),
            _ => false,
        }
    }

    pub fn merge<I>(v: I) -> Type
    where
        I: IntoIterator<Item = Type>,
    {
        v.into_iter().fold(Type::Unknown, |acc, x| acc | x)
    }
}

impl BitOr for Type {
    type Output = Type;

    fn bitor(self, rhs: Type) -> Type {
        match (self, rhs) {
            (Type::Unknown, t) => t,
            (t, Type::Unknown) => t,
            (Type::Union(mut v1), Type::Union(v2)) => {
                v1.extend(v2);
                // Remove duplicates
                v1.sort();
                v1.dedup();
                Type::Union(v1)
            }
            (Type::Union(mut v), t) => {
                v.push(t);
                v.sort();
                v.dedup();
                Type::Union(v)
            }
            (t, Type::Union(mut v)) => {
                v.push(t);
                v.sort();
                v.dedup();
                Type::Union(v)
            }
            (t1, t2) => {
                if t1.is_subtype_of(&t2) {
                    return t1;
                }
                if t2.is_subtype_of(&t1) {
                    return t2;
                }
                let mut types = vec![t1, t2];
                types.sort();
                Type::Union(types)
            }
        }
    }
}

#[derive(Debug)]
enum Scope {
    CodeBlock(IndexMap<String, Type>),
    Branch(IndexMap<String, Type>, IndexMap<String, Type>, bool),
    /// A scope for type narrowing (e.g., from type guards in if conditions).
    /// Variables in this scope shadow outer variables but don't participate
    /// in branch merging. When the scope ends, the narrowed types disappear
    /// and the original types are restored.
    Narrowing(IndexMap<String, Type>),
}

#[derive(Debug)]
pub struct PredefinedTypes {
    functions: IndexMap<String, (Type, Vec<(String, Type)>)>,
    classes: HashMap<String, IndexMap<String, Type>>,
    enum_definitions: IndexMap<String, EnumDefinition>,
    /// TODO: See the comment for [`Type::AliasRef`].
    ///
    /// We should use this but we can't without a significant refactor.
    aliases: IndexMap<String, Type>,
    // Variable name <--> Definition
    variables: IndexMap<String, Type>,
    scopes: Vec<Scope>,

    errors: Vec<TypeError>,
}

pub enum JinjaContext {
    Prompt,
    Parsing,
}

impl PredefinedTypes {
    pub fn variable_names(&self) -> Vec<String> {
        self.variables
            .keys()
            .chain(self.scopes.iter().flat_map(|s| match s {
                Scope::CodeBlock(vars) => vars.keys(),
                Scope::Branch(on_true, on_false, cond) => {
                    if *cond {
                        on_true.keys()
                    } else {
                        on_false.keys()
                    }
                }
                Scope::Narrowing(vars) => vars.keys(),
            }))
            .map(String::to_owned)
            .collect()
    }

    pub fn default(context: JinjaContext) -> Self {
        Self {
            functions: IndexMap::from([
                (
                    "baml::Chat".into(),
                    (
                        Type::String,
                        vec![
                            ("role".into(), Type::String),
                            (
                                "cache_control".into(),
                                Type::merge(vec![
                                    Type::Map(Box::new(Type::String), Box::new(Type::Unknown)),
                                    Type::None,
                                ]),
                            ),
                        ],
                    ),
                ),
                (
                    "baml::OutputFormat".into(),
                    (
                        Type::String,
                        vec![
                            ("prefix".into(), Type::merge(vec![Type::String, Type::None])),
                            (
                                "or_splitter".into(),
                                Type::merge(vec![Type::String, Type::None]),
                            ),
                            (
                                "enum_value_prefix".into(),
                                Type::merge(vec![Type::String, Type::None]),
                            ),
                            (
                                "always_hoist_enums".into(),
                                Type::merge(vec![Type::Bool, Type::None]),
                            ),
                            (
                                "hoisted_class_prefix".into(),
                                Type::merge(vec![Type::String, Type::None]),
                            ),
                            (
                                "hoist_classes".into(),
                                Type::merge(vec![
                                    Type::None,
                                    Type::Bool,
                                    Type::Literal(LiteralValue::String(String::from("auto"))),
                                    Type::List(Box::new(Type::String)),
                                ]),
                            ),
                            (
                                "quote_class_fields".into(),
                                Type::merge(vec![Type::Bool, Type::None]),
                            ),
                        ],
                    ),
                ),
            ]),
            classes: HashMap::from([
                (
                    "baml::Client".into(),
                    IndexMap::from([
                        ("name".into(), Type::String),
                        ("provider".into(), Type::String),
                    ]),
                ),
                (
                    "baml::Context".into(),
                    IndexMap::from([
                        (
                            "output_format".into(),
                            Type::Both(
                                Type::String.into(),
                                Type::FunctionRef("baml::OutputFormat".into()).into(),
                            ),
                        ),
                        ("client".into(), Type::ClassRef("baml::Client".into())),
                        (
                            "tags".into(),
                            Type::Map(Box::new(Type::String), Box::new(Type::String)),
                        ),
                    ]),
                ),
                (
                    "baml::BuiltIn".into(),
                    IndexMap::from([
                        ("chat".into(), Type::FunctionRef("baml::Chat".into())),
                        ("role".into(), Type::FunctionRef("baml::Chat".into())),
                    ]),
                ),
                (
                    "jinja::loop".into(),
                    IndexMap::from([
                        ("index".into(), Type::Int),
                        ("index0".into(), Type::Int),
                        ("revindex".into(), Type::Int),
                        ("revindex0".into(), Type::Int),
                        ("first".into(), Type::Bool),
                        ("last".into(), Type::Bool),
                        ("length".into(), Type::Int),
                        ("depth".into(), Type::Int),
                        ("depth0".into(), Type::Int),
                    ]),
                ),
            ]),
            enum_definitions: Default::default(),
            variables: match context {
                JinjaContext::Prompt => IndexMap::from([
                    ("ctx".into(), Type::ClassRef("baml::Context".into())),
                    ("_".into(), Type::ClassRef("baml::BuiltIn".into())),
                ]),
                JinjaContext::Parsing => Default::default(),
            },
            aliases: IndexMap::new(),
            scopes: Vec::new(),
            errors: Vec::new(),
        }
    }

    pub fn start_scope(&mut self) {
        self.scopes.push(Scope::CodeBlock(IndexMap::new()));
    }

    pub fn end_scope(&mut self) {
        self.scopes.pop();
    }

    /// Start a narrowing scope for type guards in if conditions.
    /// Variables added via `add_narrowing` will shadow outer variables
    /// but won't participate in branch merging.
    pub fn start_narrowing_scope(&mut self) {
        self.scopes.push(Scope::Narrowing(IndexMap::new()));
    }

    /// End the current narrowing scope, restoring original variable types.
    pub fn end_narrowing_scope(&mut self) {
        match self.scopes.pop() {
            Some(Scope::Narrowing(_)) => {}
            _ => panic!("Cannot end narrowing scope without starting one"),
        }
    }

    /// Add a narrowed variable type to the current narrowing scope.
    /// This should only be called after `start_narrowing_scope`.
    pub fn add_narrowing(&mut self, name: &str, t: Type) {
        match self.scopes.last_mut() {
            Some(Scope::Narrowing(vars)) => {
                vars.insert(name.to_string(), t);
            }
            _ => panic!("Cannot add narrowing without a Narrowing scope"),
        }
    }

    pub fn start_branch(&mut self) {
        self.scopes
            .push(Scope::Branch(IndexMap::new(), IndexMap::new(), true));
    }

    pub fn start_else_branch(&mut self) {
        match self.scopes.last_mut() {
            Some(Scope::Branch(_, _, x)) => {
                *x = false;
            }
            _ => {
                panic!("Cannot start else branch without starting a branch");
            }
        }
    }

    pub fn resolve_branch(&mut self) {
        let (true_vars, false_vars) = match self.scopes.pop() {
            Some(Scope::Branch(true_vars, false_vars, _)) => (true_vars, false_vars),
            _ => {
                panic!("Cannot resolve branch without starting a branch");
            }
        };

        // Any vars that are in both branches are merged
        // Any vars that are only in one branch, unioned with undefined

        let mut new_vars = HashMap::new();
        for (name, t) in true_vars.iter() {
            if let Some(false_t) = false_vars.get(name) {
                new_vars.insert(name.clone(), t.clone() | false_t.clone());
            } else {
                new_vars.insert(name.clone(), t.clone() | Type::Undefined);
            }
        }
        for (name, t) in false_vars.iter() {
            if !new_vars.contains_key(name) {
                new_vars.insert(name.clone(), t.clone() | Type::Undefined);
            }
        }

        new_vars.iter().for_each(|(name, t)| {
            self.add_variable(name, t.clone());
        });
    }

    pub fn errors_mut(&mut self) -> &mut Vec<TypeError> {
        &mut self.errors
    }

    pub fn errors(&self) -> &[TypeError] {
        &self.errors
    }

    pub fn resolve(&self, name: &str) -> Option<Type> {
        if let Some(t) = self.as_variable(name) {
            return Some(t.clone());
        }
        if self.as_function(name).is_some() {
            return Some(Type::FunctionRef(name.to_string()));
        }
        if self.as_class(name).is_some() {
            return Some(Type::ClassRef(name.to_string()));
        }
        if self.as_enum(name).is_some() {
            return Some(Type::EnumTypeRef(name.to_string()));
        }
        None
    }

    pub fn as_variable(&self, name: &str) -> Option<&Type> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| match scope {
                Scope::CodeBlock(vars) => vars.get(name),
                Scope::Branch(true_vars, false_vars, cond) => {
                    if *cond {
                        true_vars.get(name)
                    } else {
                        false_vars.get(name)
                    }
                }
                Scope::Narrowing(vars) => vars.get(name),
            })
            .or_else(|| self.variables.get(name))
    }

    pub fn as_class(&self, name: &str) -> Option<&IndexMap<String, Type>> {
        self.classes.get(name)
    }

    pub fn as_enum(&self, name: &str) -> Option<&EnumDefinition> {
        self.enum_definitions.get(name)
    }

    pub fn as_enum_values(&self, name: &str) -> Option<Vec<String>> {
        self.enum_definitions
            .get(name)
            .map(|def| def.values.iter().map(|v| v.name.clone()).collect())
    }

    pub fn as_function(&self, name: &str) -> Option<&(Type, Vec<(String, Type)>)> {
        self.functions.get(name)
    }

    pub fn add_function(&mut self, name: &str, ret: Type, args: Vec<(String, Type)>) {
        self.functions.insert(name.to_string(), (ret, args));
    }

    pub fn add_class(&mut self, name: &str, fields: IndexMap<String, Type>) {
        self.classes.insert(name.to_string(), fields);
    }

    pub fn add_enum(&mut self, name: &str, values: Vec<String>) {
        self.add_enum_with_metadata(
            name,
            values
                .into_iter()
                .map(|v| EnumValueDefinition {
                    name: v,
                    alias: None,
                })
                .collect(),
        );
    }

    pub fn add_enum_with_metadata(&mut self, name: &str, values: Vec<EnumValueDefinition>) {
        self.enum_definitions.insert(
            name.to_string(),
            EnumDefinition {
                name: name.to_string(),
                values,
            },
        );
    }

    pub fn add_alias(&mut self, name: &str, target: Type) {
        self.aliases.insert(name.to_string(), target);
    }

    pub fn add_variable(&mut self, name: &str, t: Type) {
        // Find the topmost non-Narrowing scope to add the variable to.
        // Narrowing scopes are skipped so that assignments (e.g., {% set x = 1 %})
        // go to the underlying Branch/CodeBlock scope for proper branch merging.
        for scope in self.scopes.iter_mut().rev() {
            match scope {
                Scope::Narrowing(_) => continue, // Skip narrowing scopes
                Scope::Branch(true_vars, false_vars, branch_cond) => {
                    if *branch_cond {
                        true_vars.insert(name.to_string(), t);
                    } else {
                        false_vars.insert(name.to_string(), t);
                    }
                    return;
                }
                Scope::CodeBlock(vars) => {
                    vars.insert(name.to_string(), t);
                    return;
                }
            }
        }
        // No scope found, add to root variables
        self.variables.insert(name.to_string(), t);
    }

    pub fn check_class_property(
        &self,
        variable_name: &str,
        class: &str,
        prop: &str,
        span: Span,
    ) -> (Type, Option<TypeError>) {
        if let Some(fields) = self.as_class(class) {
            if let Some(t) = fields.get(prop) {
                return (t.clone(), None);
            } else {
                return (
                    Type::Unknown,
                    Some(TypeError::new_property_not_defined(
                        variable_name,
                        class,
                        prop,
                        span,
                    )),
                );
            }
        }
        (Type::Unknown, Some(TypeError::new_class_not_defined(class)))
    }

    pub fn check_enum_property(
        &self,
        variable_name: &str,
        enum_name: &str,
        enum_value: &str,
        span: Span,
    ) -> (Type, Option<TypeError>) {
        if let Some(enum_def) = self.as_enum(enum_name) {
            if enum_def.values.iter().any(|v| v.name == enum_value) {
                return (Type::EnumValueRef(enum_value.to_string()), None);
            } else {
                return (
                    Type::Unknown,
                    Some(TypeError::new_property_not_defined(
                        variable_name,
                        enum_name,
                        enum_value,
                        span,
                    )),
                );
            }
        }
        (
            Type::Unknown,
            Some(TypeError::new_enum_not_defined(enum_name)),
        )
    }

    pub fn check_function_args(
        &self,
        (func, expr): (&str, &Spanned<Call>),
        positional_args: &[Type],
        kwargs: &IndexMap<&str, Type>,
    ) -> (Type, Vec<TypeError>) {
        let span = expr.span();
        let val = self.as_function(func);
        if val.is_none() {
            return (
                Type::Unknown,
                vec![TypeError::new_invalid_type(
                    &expr.expr,
                    &Type::Unknown,
                    func,
                    span,
                )],
            );
        }
        let (ret, args) = val.unwrap();
        let mut errors = Vec::new();

        // Check how many args are required.
        let mut optional_args = vec![];
        for (name, t) in args.iter().rev() {
            if !t.is_optional() {
                break;
            }
            optional_args.push(name);
        }
        let required_args = args.len() - optional_args.len();

        // Check count
        if positional_args.len() + kwargs.len() < required_args
            || (positional_args.len() + kwargs.len()) > args.len()
        {
            errors.push(TypeError::new_wrong_arg_count(
                func,
                span,
                args.len(),
                positional_args.len() + kwargs.len(),
            ));
        } else {
            let mut unused_args = args.iter().map(|(name, _)| name).collect::<HashSet<_>>();
            // Check types
            for (i, (name, t)) in args.iter().enumerate() {
                if i < positional_args.len() {
                    unused_args.remove(name);
                    let arg_t = &positional_args[i];
                    if !arg_t.is_subtype_of(t) {
                        errors.push(TypeError::new_wrong_arg_type(
                            func,
                            span,
                            name,
                            span,
                            t.clone(),
                            arg_t.clone(),
                        ));
                    }
                } else if let Some(arg_t) = kwargs.get(name.as_str()) {
                    unused_args.remove(name);
                    if !arg_t.is_subtype_of(t) {
                        errors.push(TypeError::new_wrong_arg_type(
                            func,
                            span,
                            name,
                            span,
                            t.clone(),
                            arg_t.clone(),
                        ));
                    }
                } else if !optional_args.contains(&name) {
                    errors.push(TypeError::new_missing_arg(func, span, name));
                }
            }

            kwargs.iter().for_each(|(name, _)| {
                if !args.iter().any(|(arg_name, _)| arg_name == name) {
                    errors.push(TypeError::new_unknown_arg(
                        func,
                        span,
                        name,
                        unused_args.clone(),
                    ));
                }
            });
        }
        (ret.clone(), errors)
    }
}
