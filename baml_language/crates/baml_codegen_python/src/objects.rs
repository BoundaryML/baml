use crate::{
    docstring::{DocString, PyString},
    ty::{Name, Namespace, Ty},
};

macro_rules! impl_eq_ord_by_name {
    ($($ty:ty),* $(,)?) => {
        $(
            impl PartialEq for $ty {
                fn eq(&self, other: &Self) -> bool {
                    self.name == other.name
                }
            }
            impl Eq for $ty {}
            impl PartialOrd for $ty {
                fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
                    Some(self.cmp(other))
                }
            }
            impl Ord for $ty {
                fn cmp(&self, other: &Self) -> std::cmp::Ordering {
                    self.name.cmp(&other.name)
                }
            }
        )*
    };
}

impl_eq_ord_by_name!(Class, Enum, TypeAlias, Function);

/// Ordering: Enum < Class < `TypeAlias` (enums first, type aliases last)
#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Object {
    // Enums first, then classes, then type aliases
    // This is important for the code-gen to be able to generate the correct imports
    Enum(Enum),
    Class(Class),
    TypeAlias(TypeAlias),
}

pub(crate) struct Class {
    name: Name,
    docstring: Option<DocString>,
    properties: Vec<ClassProperty>,
}

pub(crate) struct ClassProperty {
    name: baml_base::Name,
    docstring: Option<DocString>,
    ty: Ty,
}

impl Class {
    pub(crate) fn from_codegen_types(class: &baml_codegen_types::Class) -> Self {
        Self {
            name: Name::from_codegen_types(&class.name),
            docstring: class.docstring.as_ref().map(DocString::new),
            properties: class
                .properties
                .iter()
                .map(ClassProperty::from_codegen_types)
                .collect(),
        }
    }
}

impl ClassProperty {
    pub(crate) fn from_codegen_types(property: &baml_codegen_types::ClassProperty) -> Self {
        Self {
            name: property.name.clone(),
            docstring: property.docstring.as_ref().map(DocString::new),
            ty: Ty::from_codegen_types(&property.ty),
        }
    }
}

pub(crate) struct Enum {
    name: Name,
    docstring: Option<DocString>,
    variants: Vec<EnumVariant>,
}

impl Enum {
    pub(crate) fn from_codegen_types(enum_: &baml_codegen_types::Enum) -> Self {
        Self {
            name: Name::from_codegen_types(&enum_.name),
            docstring: enum_.docstring.as_ref().map(DocString::new),
            variants: enum_
                .variants
                .iter()
                .map(EnumVariant::from_codegen_types)
                .collect(),
        }
    }
}

pub(crate) struct EnumVariant {
    name: baml_base::Name,
    docstring: Option<DocString>,
    value: PyString,
}

impl EnumVariant {
    pub(crate) fn from_codegen_types(variant: &baml_codegen_types::EnumVariant) -> Self {
        Self {
            name: variant.name.clone(),
            docstring: variant.docstring.as_ref().map(DocString::new),
            value: PyString::new(&variant.value),
        }
    }
}

pub(crate) struct TypeAlias {
    name: Name,
    resolves_to: Ty,
}

impl TypeAlias {
    pub(crate) fn from_codegen_types(type_alias: &baml_codegen_types::TypeAlias) -> Self {
        Self {
            name: Name::from_codegen_types(&type_alias.name),
            resolves_to: Ty::from_codegen_types(&type_alias.resolves_to),
        }
    }
}

impl Object {
    pub(crate) fn load_types(objects: &baml_codegen_types::ObjectPool) -> Vec<Object> {
        let mut objects = objects
            .iter()
            .filter(|(name, _)| name.namespace == baml_codegen_types::Namespace::Types)
            .filter_map(|(_, object)| match object {
                baml_codegen_types::Object::Function(_) => None,
                baml_codegen_types::Object::Class(class) => {
                    Some(Object::Class(Class::from_codegen_types(class)))
                }
                baml_codegen_types::Object::Enum(enum_) => {
                    Some(Object::Enum(Enum::from_codegen_types(enum_)))
                }
                baml_codegen_types::Object::TypeAlias(type_alias) => {
                    Some(Object::TypeAlias(TypeAlias::from_codegen_types(type_alias)))
                }
            })
            .collect::<Vec<_>>();

        objects.sort();

        objects
    }

    pub(crate) fn load_stream_types(objects: &baml_codegen_types::ObjectPool) -> Vec<Object> {
        let mut objects = objects
            .iter()
            .filter(|(name, _)| name.namespace == baml_codegen_types::Namespace::StreamTypes)
            .filter_map(|(_, object)| match object {
                baml_codegen_types::Object::Function(_) => None,
                baml_codegen_types::Object::Class(class) => {
                    Some(Object::Class(Class::from_codegen_types(class)))
                }
                baml_codegen_types::Object::Enum(enum_) => {
                    Some(Object::Enum(Enum::from_codegen_types(enum_)))
                }
                baml_codegen_types::Object::TypeAlias(type_alias) => {
                    Some(Object::TypeAlias(TypeAlias::from_codegen_types(type_alias)))
                }
            })
            .collect::<Vec<_>>();

        objects.sort();

        objects
    }
}

pub(crate) struct Function {
    /// functions in python have no namespace
    name: baml_base::Name,
    assembed_docstring: DocString,
    arguments: Vec<FunctionArgument>,
    return_type: Ty,
}

pub(crate) struct FunctionArgument {
    name: baml_base::Name,
    ty: Ty,
    default_value: Option<String>,
}

impl Function {
    pub(crate) fn load_functions(objects: &baml_codegen_types::ObjectPool) -> Vec<Function> {
        let mut objects = objects
            .iter()
            .filter(|(name, _)| name.namespace == baml_codegen_types::Namespace::Types)
            .filter_map(|(_, object)| match object {
                baml_codegen_types::Object::Function(function) => {
                    Some(Function::from_codegen_types(
                        function,
                        Ty::from_codegen_types(&function.return_type),
                    ))
                }
                _ => None,
            })
            .collect::<Vec<_>>();

        objects.sort();

        objects
    }

    pub(crate) fn load_stream_functions(objects: &baml_codegen_types::ObjectPool) -> Vec<Function> {
        let mut objects = objects
            .iter()
            .filter(|(name, _)| name.namespace == baml_codegen_types::Namespace::StreamTypes)
            .filter_map(|(_, object)| match object {
                baml_codegen_types::Object::Function(function) => {
                    function.stream_return_type.as_ref().map(|return_type| {
                        Function::from_codegen_types(
                            function,
                            Ty::Stream {
                                stream_type: Box::new(Ty::from_codegen_types(
                                    &function.return_type,
                                )),
                                return_type: Box::new(Ty::from_codegen_types(return_type)),
                            },
                        )
                    })
                }
                _ => None,
            })
            .collect::<Vec<_>>();

        objects.sort();

        objects
    }

    fn from_codegen_types(function: &baml_codegen_types::Function, return_type: Ty) -> Self {
        /// ```askama
        /// {% if let Some(function) = function %}
        /// {{ function }}
        /// {% endif %}
        /// Args:
        /// {%- for (name, docstring) in arguments %}
        ///   {{ name }}: {{ docstring|indent(4) }}
        /// {%- endfor %}
        /// ```
        #[derive(Debug, askama::Template)]
        #[template(in_doc = true, ext = "txt")]
        struct FunctionDocString<'a> {
            function: Option<&'a str>,
            arguments: Vec<(baml_base::Name, &'a str)>,
        }

        let baml_options_arg = baml_codegen_types::FunctionArgument {
            name: "baml_options".into(),
            docstring: Some("See `baml.Options` for more information".into()),
            ty: baml_codegen_types::Ty::Optional(Box::new(baml_codegen_types::Ty::BamlOptions)),
        };

        let docstring = FunctionDocString {
            function: function.docstring.as_deref(),
            arguments: function
                .arguments
                .iter()
                .chain(Some(&baml_options_arg))
                .map(|arg| (arg.name.clone(), arg.docstring.as_deref().unwrap_or("none")))
                .collect(),
        };

        let arguments: Vec<FunctionArgument> = function
            .arguments
            .iter()
            .chain(Some(&baml_options_arg))
            .rev()
            .fold(Vec::new(), |mut acc, arg| {
                acc.insert(
                    0,
                    FunctionArgument::from_codegen_types(
                        arg,
                        acc.first()
                            .map(|first| first.default_value.is_some())
                            .unwrap_or(true),
                    ),
                );
                acc
            });

        Self {
            name: function.name.clone(),
            assembed_docstring: DocString::new(docstring.to_string()),
            arguments,
            return_type,
        }
    }
}

impl FunctionArgument {
    pub(crate) fn from_codegen_types(
        arg: &baml_codegen_types::FunctionArgument,
        allow_default_value: bool,
    ) -> Self {
        Self {
            name: arg.name.clone(),
            ty: Ty::from_codegen_types(&arg.ty),
            default_value: if allow_default_value {
                arg.ty
                    .default_value()
                    .and_then(|value| value.to_py_string())
            } else {
                None
            },
        }
    }
}

trait ToPyString {
    fn to_py_string(&self) -> Option<String>;
}

impl ToPyString for baml_codegen_types::DefaultValue {
    fn to_py_string(&self) -> Option<String> {
        use baml_codegen_types::DefaultValue;

        match self {
            DefaultValue::Null => Some("None".to_string()),
            DefaultValue::Literal(lit) => match lit {
                baml_base::Literal::Int(v) => Some(v.to_string()),
                baml_base::Literal::Float(s) => Some(s.clone()),
                baml_base::Literal::String(v) => Some(PyString::new(v).to_string()),
                baml_base::Literal::Bool(true) => Some("True".to_string()),
                baml_base::Literal::Bool(false) => Some("False".to_string()),
            },
        }
    }
}

mod render {
    use super::{Function, Namespace, Object};
    mod class;
    mod r#enum;
    mod function;
    mod type_alias;

    impl Object {
        fn print(&self, namespace: Namespace) -> String {
            match self {
                Object::Class(class) => class::print(class, namespace),
                Object::Enum(r#enum) => r#enum::print(r#enum),
                Object::TypeAlias(type_alias) => type_alias::print(type_alias, namespace),
            }
        }
    }

    impl Function {
        fn print_signature(&self) -> String {
            function::print_signature(self, Namespace::Other)
        }
    }

    baml_codegen_types::render_fn! {
        /// ```askama
        /// import typing
        /// import typing_extensions
        /// from enum import Enum
        /// from pydantic import BaseModel, ConfigDict, Field
        ///
        /// import baml_py
        ///
        /// CheckT = typing_extensions.TypeVar('CheckT')
        /// CheckName = typing_extensions.TypeVar('CheckName', bound=str)
        ///
        /// class Check(BaseModel):
        ///     name: str
        ///     expression: str
        ///     status: str
        ///
        /// class Checked(BaseModel, typing.Generic[CheckT, CheckName]):
        ///     value: CheckT
        ///     checks: typing.Dict[CheckName, Check]
        ///
        /// def get_checks(checks: typing.Dict[CheckName, Check]) -> typing.List[Check]:
        ///     return list(checks.values())
        ///
        /// def all_succeeded(checks: typing.Dict[CheckName, Check]) -> bool:
        ///     return all(check.status == "succeeded" for check in get_checks(checks))
        ///
        /// {% for object in objects %}
        /// {{ object.print(Namespace::Types) }}
        /// {% endfor %}
        /// ```
        pub fn get_types_py(objects: &Vec<Object>) -> String;
    }

    baml_codegen_types::render_fn! {
        /// ```askama
        /// import typing
        /// import typing_extensions
        /// from pydantic import BaseModel, ConfigDict, Field
        ///
        /// import baml_py
        ///
        /// from . import types
        ///
        /// StreamStateValueT = typing.TypeVar('StreamStateValueT')
        /// class StreamState(BaseModel, typing.Generic[StreamStateValueT]):
        ///     value: StreamStateValueT
        ///     state: typing_extensions.Literal["Pending", "Incomplete", "Complete"]
        ///
        /// {% for object in objects %}
        /// {{ object.print(Namespace::StreamTypes) }}
        ///
        /// {% endfor %}
        /// ```
        pub fn get_stream_types_py(objects: &Vec<Object>) -> String;
    }

    baml_codegen_types::render_fn! {
        /// ```askama
        /// {% for fn_ in fns %}
        /// {{ fn_.print_signature() }}
        /// {% endfor %}
        /// ```
        pub fn get_functions_pyi(fns: &Vec<Function>) -> String;
    }
}

pub(crate) use render::*;
