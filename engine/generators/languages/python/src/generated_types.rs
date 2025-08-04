use crate::{
    package::CurrentRenderPackage,
    r#type::{SerializeType, TypePy},
};

mod class {
    use super::*;

    /// A class in Py.
    ///
    /// ```askama
    /// class {{name}}(BaseModel):
    ///     {%- if let Some(docstring) = docstring %}
    ///     {{crate::utils::prefix_lines(docstring, "# ") }}
    ///     {%- endif %}
    ///     {%- if pkg.is_pydantic_2 %}
    ///     {%- if dynamic %}
    ///     model_config = ConfigDict(extra='allow')
    ///     {%- endif %}
    ///     {%- else %}
    ///     class Config:
    ///         {%- if dynamic %}
    ///         extra = Extra.allow
    ///         {%- endif %}
    ///         arbitrary_types_allowed = True
    ///     {%- endif %}
    ///     {%- if fields.is_empty() && !dynamic %}pass{% endif %}
    ///     {%- for field in fields %}
    ///     {{- field.render()?|indent(4, true) }}
    ///     {%- endfor %}
    /// ```
    #[derive(askama::Template)]
    #[template(in_doc = true, escape = "none", ext = "txt")]
    pub struct ClassPy<'a> {
        pub name: String,
        pub docstring: Option<String>,
        pub fields: Vec<FieldPy<'a>>,
        pub dynamic: bool,
        pub pkg: &'a CurrentRenderPackage,
    }

    /// A field in a class.
    ///
    /// ```askama
    /// {% if let Some(docstring) = docstring %}
    /// {{ crate::utils::prefix_lines(docstring, "# ") }}
    /// {% endif %}
    /// {{ name }}: {{ type.serialize_type(&pkg.in_type_definition()) }}{% if let Some(default_value) = type.default_value() %} = {{default_value}}{% endif %}
    /// ```
    #[derive(askama::Template, Clone)]
    #[template(in_doc = true, escape = "none", ext = "txt")]
    pub struct FieldPy<'a> {
        pub docstring: Option<String>,
        pub name: String,
        pub r#type: TypePy,
        pub pkg: &'a CurrentRenderPackage,
    }
    impl std::fmt::Debug for FieldPy<'_> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(
                f,
                "FieldPy {{docstring: {:?}, name: {}, type: {:?}, pkg: <<Mutex>> }}",
                self.docstring, self.name, self.r#type
            )
        }
    }
}

mod enums {
    /// An enum in Py.
    ///
    /// ```askama
    /// class {{name}}(str, Enum):
    ///     {%- if let Some(docstring) = docstring %}
    ///     {{crate::utils::prefix_lines(docstring, "# ") }}
    ///     {% endif %}
    ///     {%- if values.is_empty() %}
    ///     pass
    ///     {%- endif %}
    ///     {%- for (value, docstring) in values %}
    ///     {%- if let Some(docstring) = docstring %}
    ///     {{ crate::utils::prefix_lines(docstring, "# ") }}
    ///     {%- endif %}
    ///     {{ value }} = "{{ value }}"
    ///     {%- endfor %}
    /// ```
    #[derive(askama::Template)]
    #[template(in_doc = true, escape = "none", ext = "txt")]
    pub struct EnumPy {
        pub name: String,
        pub docstring: Option<String>,
        pub values: Vec<(String, Option<String>)>,
        pub dynamic: bool,
    }
}

mod type_builder {
    pub trait TypeBuilderPropertyTrait {
        fn name(&self) -> &str;
        fn type_builder_name(&self) -> String;
    }

    /// A property in a type builder.
    ///
    /// ```askama
    /// @property
    /// def {{ property.name() }}(self) -> "{{ property.type_builder_name() }}":
    ///     return {{ property.type_builder_name() }}(self)
    /// ```
    #[derive(askama::Template)]
    #[template(in_doc = true, escape = "none", ext = "txt")]
    pub struct TypeBuilderProperty<'a, T: TypeBuilderPropertyTrait> {
        pub property: &'a T,
    }

    impl super::EnumPy {
        pub fn to_type_builder_property(&self) -> TypeBuilderProperty<'_, Self> {
            TypeBuilderProperty { property: self }
        }
    }

    impl TypeBuilderPropertyTrait for super::EnumPy {
        fn name(&self) -> &str {
            &self.name
        }
        fn type_builder_name(&self) -> String {
            format!(
                "{}{}",
                self.name,
                if self.dynamic { "Builder" } else { "Viewer" }
            )
        }
    }

    impl super::ClassPy<'_> {
        pub fn to_type_builder_property(&self) -> TypeBuilderProperty<'_, Self> {
            TypeBuilderProperty { property: self }
        }
    }

    impl TypeBuilderPropertyTrait for super::ClassPy<'_> {
        fn name(&self) -> &str {
            &self.name
        }
        fn type_builder_name(&self) -> String {
            format!(
                "{}{}",
                self.name,
                if self.dynamic { "Builder" } else { "Viewer" }
            )
        }
    }

    /// An object in a type builder.
    ///
    /// ```askama
    /// class {{ class.name }}Ast:
    ///     def __init__(self, tb: type_builder.TypeBuilder):
    ///         _tb = tb._tb # type: ignore (we know how to use this private attribute)
    ///         self._bldr = _tb.class_("{{ class.name }}")
    ///         self._properties: typing.Set[str] = set([ {% for field in class.fields %} "{{ field.name }}", {% endfor %} ])
    ///         self._props = {{ class.name }}Properties(self._bldr, self._properties)
    ///
    ///     def type(self) -> baml_py.FieldType:
    ///         return self._bldr.field()
    ///
    ///     @property
    ///     def props(self) -> "{{ class.name }}Properties":
    ///         return self._props
    ///
    ///
    /// class {{ class.type_builder_object_name() }}({{ class.name }}Ast):
    ///     def __init__(self, tb: type_builder.TypeBuilder):
    ///         super().__init__(tb)
    ///
    ///     {% if class.dynamic %}
    ///     def add_property(self, name: str, type: baml_py.FieldType) -> {{ class.class_property_type() }}:
    ///         if name in self._properties:
    ///             raise ValueError(f"Property {name} already exists.")
    ///         return self._bldr.property(name).type(type)
    ///
    ///     def list_properties(self) -> typing.List[typing.Tuple[str, {{ class.class_property_type() }}]]:
    ///         return self._bldr.list_properties()
    ///
    ///     def remove_property(self, name: str) -> None:
    ///         self._bldr.remove_property(name)
    ///
    ///     def reset(self) -> None:
    ///         self._bldr.reset()
    ///
    ///     {% else %}
    ///     def list_properties(self) -> typing.List[typing.Tuple[str, {{ class.class_property_type() }}]]:
    ///         return [(name, {{ class.class_property_type() }}(self._bldr.property(name))) for name in self._properties]
    ///     {% endif %}
    ///
    ///
    /// class {{ class.name }}Properties:
    ///     def __init__(self, bldr: baml_py.ClassBuilder, properties: typing.Set[str]):
    ///         self.__bldr = bldr
    ///         self.__properties = properties # type: ignore (we know how to use this private attribute) # noqa: F821
    ///
    ///     {% if class.dynamic %}
    ///     def __getattr__(self, name: str) -> {{ class.class_property_type() }}:
    ///         if name not in self.__properties:
    ///             raise AttributeError(f"Property {name} not found.")
    ///         return self.__bldr.property(name)
    ///
    ///     {% for field in class.fields %}
    ///     @property
    ///     def {{ field.name }}(self) -> {{ class.class_property_type() }}:
    ///         return self.__bldr.property("{{ field.name }}")
    ///     {% endfor %}
    ///     {% else %}
    ///     {% for field in class.fields %}
    ///     @property
    ///     def {{ field.name }}(self) -> {{ class.class_property_type() }}:
    ///         return {{ class.class_property_type() }}(self.__bldr.property("{{ field.name }}"))
    ///     {% endfor %}
    ///     {% endif %}
    ///
    /// ```
    #[derive(askama::Template)]
    #[template(in_doc = true, escape = "none", ext = "txt")]
    pub struct TypeBuilderClassObject<'a> {
        pub class: &'a super::ClassPy<'a>,
    }

    impl<'a> super::ClassPy<'a> {
        fn type_builder_object_type(&self) -> &str {
            if self.dynamic {
                "Builder"
            } else {
                "Viewer"
            }
        }

        fn type_builder_object_name(&self) -> String {
            format!("{}{}", self.name, self.type_builder_object_type())
        }

        fn class_property_type(&self) -> String {
            format!(
                "{}.ClassProperty{}",
                if self.dynamic {
                    "baml_py"
                } else {
                    "type_builder"
                },
                self.type_builder_object_type()
            )
        }

        pub fn to_type_builder_object(&'a self) -> TypeBuilderClassObject<'a> {
            TypeBuilderClassObject { class: self }
        }
    }

    /// An object in a type builder.
    ///
    /// ```askama
    /// class {{ enum_.name }}Ast:
    ///     def __init__(self, tb: type_builder.TypeBuilder):
    ///         _tb = tb._tb # type: ignore (we know how to use this private attribute)
    ///         self._bldr = _tb.enum("{{ enum_.name }}")
    ///         self._values: typing.Set[str] = set([ {% for (value, _) in enum_.values %} "{{ value }}", {% endfor %} ])
    ///         self._vals = {{ enum_.name }}Values(self._bldr, self._values)
    ///
    ///     def type(self) -> baml_py.FieldType:
    ///         return self._bldr.field()
    ///
    ///     @property
    ///     def values(self) -> "{{ enum_.name }}Values":
    ///         return self._vals
    ///
    ///
    /// class {{ enum_.type_builder_object_name() }}({{ enum_.name }}Ast):
    ///     def __init__(self, tb: type_builder.TypeBuilder):
    ///         super().__init__(tb)
    ///
    ///     {% if enum_.dynamic %}
    ///     def list_values(self) -> typing.List[typing.Tuple[str, {{ enum_.enum_value_type() }}]]:
    ///         return [(name, self._bldr.value(name)) for name in self._values]
    ///
    ///     def add_value(self, name: str) -> {{ enum_.enum_value_type() }}:
    ///         if name in self._values:
    ///             raise ValueError(f"Value {name} already exists.")
    ///         return self._bldr.value(name)
    ///     {% else %}
    ///     def list_values(self) -> typing.List[typing.Tuple[str, {{ enum_.enum_value_type() }}]]:
    ///         return [(name, {{ enum_.enum_value_type() }}(self._bldr.value(name))) for name in self._values]
    ///     {% endif %}
    ///
    /// class {{ enum_.name }}Values:
    ///     def __init__(self, enum_bldr: baml_py.EnumBuilder, values: typing.Set[str]):
    ///         self.__bldr = enum_bldr
    ///         self.__values = values # type: ignore (we know how to use this private attribute) # noqa: F821
    ///
    ///     {% if enum_.dynamic %}
    ///     def __getattr__(self, name: str) -> {{ enum_.enum_value_type() }}:
    ///         if name not in self.__values:
    ///             raise AttributeError(f"Value {name} not found.")
    ///         return self.__bldr.value(name)
    ///
    ///     {% for (value, _) in enum_.values %}
    ///     @property
    ///     def {{ value }}(self) -> {{ enum_.enum_value_type() }}:
    ///         return self.__bldr.value("{{ value }}")
    ///     {% endfor %}
    ///     {% else %}
    ///     {% for (value, _) in enum_.values %}
    ///     @property
    ///     def {{ value }}(self) -> {{ enum_.enum_value_type() }}:
    ///         return {{ enum_.enum_value_type() }}(self.__bldr.value("{{ value }}"))
    ///     {% endfor %}
    ///     {% endif %}
    ///
    /// ```
    #[derive(askama::Template)]
    #[template(in_doc = true, escape = "none", ext = "txt")]
    pub struct TypeBuilderEnumObject<'a> {
        pub enum_: &'a super::EnumPy,
    }

    impl<'a> super::EnumPy {
        fn type_builder_object_type(&self) -> &str {
            if self.dynamic {
                "Builder"
            } else {
                "Viewer"
            }
        }

        fn type_builder_object_name(&self) -> String {
            format!("{}{}", self.name, self.type_builder_object_type())
        }

        fn enum_value_type(&self) -> String {
            format!(
                "{}.EnumValue{}",
                if self.dynamic {
                    "baml_py"
                } else {
                    "type_builder"
                },
                self.type_builder_object_type()
            )
        }

        pub fn to_type_builder_object(&'a self) -> TypeBuilderEnumObject<'a> {
            TypeBuilderEnumObject { enum_: self }
        }
    }
}

mod type_aliases {
    use super::*;

    /// A type alias in Py.
    ///
    /// ```askama
    /// {% if let Some(docstring) = docstring -%}
    /// {{ crate::utils::prefix_lines(docstring, "# ") }}
    /// {%- endif %}
    /// {{ name }}: typing_extensions.TypeAlias = {{ type_.serialize_type(&pkg.in_type_definition()) }}
    /// ```
    #[derive(askama::Template)]
    #[template(in_doc = true, escape = "none", ext = "txt")]
    pub struct TypeAliasPy<'a> {
        pub name: String,
        pub type_: TypePy,
        pub docstring: Option<String>,
        pub pkg: &'a CurrentRenderPackage,
    }
}

/// A list of types in Py.
///
/// ```askama
/// import typing
/// import typing_extensions
/// from enum import Enum
///
/// {% if pkg.is_pydantic_2 %}
/// from pydantic import BaseModel, ConfigDict
/// {% else %}
/// from pydantic import BaseModel, Extra
/// from pydantic.generics import GenericModel
/// {% endif %}
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
/// {%- if pkg.is_pydantic_2 %}
/// class Checked(BaseModel, typing.Generic[CheckT, CheckName]):
/// {%- else %}
/// class Checked(GenericModel, typing.Generic[CheckT, CheckName]):
/// {%- endif %}
///     value: CheckT
///     checks: typing.Dict[CheckName, Check]
///
/// def get_checks(checks: typing.Dict[CheckName, Check]) -> typing.List[Check]:
///     return list(checks.values())
///
/// def all_succeeded(checks: typing.Dict[CheckName, Check]) -> bool:
///     return all(check.status == "succeeded" for check in get_checks(checks))
/// ```
///
#[derive(askama::Template)]
#[template(in_doc = true, escape = "none", ext = "txt")]
struct PyTypesUtils<'a> {
    pkg: &'a CurrentRenderPackage,
}

pub(crate) fn render_py_types_utils(pkg: &CurrentRenderPackage) -> Result<String, askama::Error> {
    use askama::Template;

    PyTypesUtils { pkg }.render()
}

/// A list of types in Py.
///
/// ```askama
/// # #########################################################################
/// # Generated {{ name }} ({{ items.len() }})
/// # #########################################################################
/// {% for item in items %}
/// {{ item.render()? }}
/// {% endfor %}
/// ```
#[derive(askama::Template)]
#[template(in_doc = true, escape = "none", ext = "txt")]
struct PyTypes<'ir, T: askama::Template> {
    items: &'ir [T],
    name: &'ir str,
}

pub(crate) fn render_py_types<T: askama::Template>(
    items: &[T],
    _pkg: &CurrentRenderPackage,
) -> Result<String, askama::Error> {
    use askama::Template;

    PyTypes {
        items,
        name: match std::any::type_name::<T>() {
            "generators_python::generated_types::class::ClassPy" => "classes",
            "generators_python::generated_types::enums::EnumPy" => "enums",
            "generators_python::generated_types::type_aliases::TypeAliasPy" => "type aliases",
            other => panic!("Unknown type: {other}"),
        },
    }
    .render()
}

/// A list of types in Py.
///
/// ```askama
/// import typing
/// import typing_extensions
///
/// {%- if pkg.is_pydantic_2 %}
/// from pydantic import BaseModel, ConfigDict
/// {%- else %}
/// from pydantic import BaseModel, Extra
/// from pydantic.generics import GenericModel
/// {%- endif %}
///
/// import baml_py
///
/// from . import types
///
/// StreamStateValueT = typing.TypeVar('StreamStateValueT')
/// {%- if pkg.is_pydantic_2 %}
/// class StreamState(BaseModel, typing.Generic[StreamStateValueT]):
/// {%- else %}
/// class StreamState(GenericModel, typing.Generic[StreamStateValueT]):
/// {%- endif %}
///     value: StreamStateValueT
///     state: typing_extensions.Literal["Pending", "Incomplete", "Complete"]
/// ```
#[derive(askama::Template)]
#[template(in_doc = true, escape = "none", ext = "txt")]
pub struct PyStreamTypesUtils<'a> {
    pkg: &'a CurrentRenderPackage,
}

pub(crate) fn render_py_stream_types_utils(
    pkg: &CurrentRenderPackage,
) -> Result<String, askama::Error> {
    use askama::Template;

    PyStreamTypesUtils { pkg }.render()
}

#[derive(askama::Template)]
#[template(path = "type_builder.py.j2", escape = "none", ext = "txt")]
struct PyTypeBuilder<'a> {
    classes: &'a [ClassPy<'a>],
    enums: &'a [EnumPy],
}

pub(crate) fn render_py_type_builder(
    classes: &[ClassPy],
    enums: &[EnumPy],
) -> Result<String, askama::Error> {
    use askama::Template;

    PyTypeBuilder { classes, enums }.render()
}

pub use class::{ClassPy, FieldPy};
pub use enums::EnumPy;
pub use type_aliases::TypeAliasPy;
