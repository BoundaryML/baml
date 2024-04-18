use core::panic;
use std::{collections::HashMap, ops::BitOr};

use minijinja::machinery::{
    ast::{Call, Expr, Spanned},
    Span,
};

use super::TypeError;

#[derive(Debug, Clone, PartialOrd, Ord)]
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
    List(Box<Type>),
    Map(Box<Type>, Box<Type>),
    Tuple(Vec<Type>),
    Union(Vec<Type>),
    ClassRef(String),
    FunctionRef(String),
}

impl PartialEq for Type {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Unknown, Self::Unknown) => true,
            (Self::Unknown, _) => true,
            (_, Self::Unknown) => true,
            (Self::Number, Self::Int | Self::Float) => true,
            (Self::Int | Self::Float, Self::Number) => true,
            (Self::List(l0), Self::List(r0)) => l0 == r0,
            (Self::Map(l0, l1), Self::Map(r0, r1)) => l0 == r0 && l1 == r1,
            (Self::Union(l0), Self::Union(r0)) => l0 == r0,
            (Self::ClassRef(l0), Self::ClassRef(r0)) => l0 == r0,
            (Self::FunctionRef(l0), Self::FunctionRef(r0)) => l0 == r0,
            _ => core::mem::discriminant(self) == core::mem::discriminant(other),
        }
    }
}

impl Eq for Type {}

impl Type {
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
            Type::ClassRef(name) => format!("class {}", name),
            Type::FunctionRef(name) => format!("function {}", name),
        }
    }

    pub fn merge<'a, I>(v: I) -> Type
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
                if t1 == t2 {
                    return t1;
                }
                Type::Union(vec![t1, t2])
            }
        }
    }
}

#[derive(Debug)]
enum Scope {
    CodeBlock(HashMap<String, Type>),
    Branch(HashMap<String, Type>, HashMap<String, Type>, bool),
}

#[derive(Debug)]
pub struct PredefinedTypes {
    functions: HashMap<String, (Type, Vec<(String, Type)>)>,
    classes: HashMap<String, HashMap<String, Type>>,
    // Variable name <--> Definition
    variables: HashMap<String, Type>,
    scopes: Vec<Scope>,

    errors: Vec<TypeError>,
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
            }))
            .map(|k| k.to_string())
            .collect()
    }

    pub fn default() -> Self {
        Self {
            functions: HashMap::from([(
                "baml::Chat".into(),
                (Type::String, vec![("role".into(), Type::String)]),
            )]),
            classes: HashMap::from([
                (
                    "baml::Client".into(),
                    HashMap::from([
                        ("name".into(), Type::String),
                        ("provider".into(), Type::String),
                    ]),
                ),
                (
                    "baml::Context".into(),
                    HashMap::from([
                        ("output_schema".into(), Type::String),
                        ("client".into(), Type::ClassRef("baml::Client".into())),
                        (
                            "env".into(),
                            Type::Map(Box::new(Type::String), Box::new(Type::String)),
                        ),
                    ]),
                ),
                (
                    "baml::BuiltIn".into(),
                    HashMap::from([("chat".into(), Type::FunctionRef("baml::Chat".into()))]),
                ),
                (
                    "jinja::loop".into(),
                    HashMap::from([
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
            variables: HashMap::from([
                ("ctx".into(), Type::ClassRef("baml::Context".into())),
                ("_".into(), Type::ClassRef("baml::BuiltIn".into())),
            ]),
            scopes: Vec::new(),
            errors: Vec::new(),
        }
    }

    pub fn start_scope(&mut self) {
        self.scopes.push(Scope::CodeBlock(HashMap::new()));
    }

    pub fn end_scope(&mut self) {
        self.scopes.pop();
    }

    pub fn start_branch(&mut self) {
        self.scopes
            .push(Scope::Branch(HashMap::new(), HashMap::new(), true));
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
            })
            .or_else(|| self.variables.get(name))
    }

    pub fn as_class(&self, name: &str) -> Option<&HashMap<String, Type>> {
        self.classes.get(name)
    }

    pub fn as_function(&self, name: &str) -> Option<&(Type, Vec<(String, Type)>)> {
        self.functions.get(name)
    }

    pub fn add_function(&mut self, name: &str, ret: Type, args: Vec<(String, Type)>) {
        self.functions.insert(name.to_string(), (ret, args));
    }

    pub fn add_class(&mut self, name: &str, fields: HashMap<String, Type>) {
        self.classes.insert(name.to_string(), fields);
    }

    pub fn add_variable(&mut self, name: &str, t: Type) {
        match self.scopes.last_mut() {
            Some(Scope::Branch(true_vars, false_vars, branch_cond)) => {
                if *branch_cond {
                    true_vars.insert(name.to_string(), t);
                } else {
                    false_vars.insert(name.to_string(), t);
                }
            }
            Some(Scope::CodeBlock(vars)) => {
                vars.insert(name.to_string(), t);
            }
            None => {
                self.variables.insert(name.to_string(), t);
            }
        }
    }

    pub fn check_property(
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

    pub fn check_function_args(
        &self,
        (func, expr): (&str, &Spanned<Call>),
        positional_args: &[Type],
        kwargs: &HashMap<&str, Type>,
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

        // Check count
        if positional_args.len() + kwargs.len() != args.len() {
            errors.push(TypeError::new_wrong_arg_count(
                func,
                span,
                args.len(),
                positional_args.len() + kwargs.len(),
            ));
        } else {
            // Check types
            for (i, (name, t)) in args.iter().enumerate() {
                if i < positional_args.len() {
                    let arg_t = &positional_args[i];
                    if arg_t != t {
                        errors.push(TypeError::new_wrong_arg_type(
                            func,
                            span,
                            name,
                            span.clone(),
                            t.clone(),
                            arg_t.clone(),
                        ));
                    }
                } else {
                    if let Some(arg_t) = kwargs.get(name.as_str()) {
                        if arg_t != t {
                            errors.push(TypeError::new_wrong_arg_type(
                                func,
                                span,
                                name,
                                span.clone(),
                                t.clone(),
                                arg_t.clone(),
                            ));
                        }
                    } else {
                        errors.push(TypeError::new_missing_arg(func, span, name));
                    }
                }
            }

            kwargs.iter().for_each(|(name, _)| {
                if !args.iter().any(|(arg_name, _)| arg_name == name) {
                    errors.push(TypeError::new_unknown_arg(func, span, name));
                }
            });
        }
        (ret.clone(), errors)
    }
}
