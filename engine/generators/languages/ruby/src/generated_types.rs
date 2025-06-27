use crate::{
    package::CurrentRenderPackage,
    r#type::{SerializeType, TypeRb},
};

mod class {
    use super::*;

    /// A class in Rb.
    ///
    /// ```askama
    /// {%- if let Some(docstring) = docstring %}
    /// {{crate::utils::prefix_lines(docstring, "# ") }}
    /// {%- endif %}
    /// class {{name}} < T::Struct
    ///     include Baml::Sorbet::Struct
    ///
    ///     {%- for field in fields %}
    ///     {{- field.render()?|indent(4, true) }}
    ///     {%- endfor %}
    /// end
    /// ```
    #[derive(askama::Template)]
    #[template(in_doc = true, escape = "none", ext = "txt")]
    pub struct ClassRb<'a> {
        pub name: String,
        pub docstring: Option<String>,
        pub fields: Vec<FieldRb<'a>>,
        pub dynamic: bool,
        pub pkg: &'a CurrentRenderPackage,
    }

    /// A field in a class.
    ///
    /// ```askama
    /// {% if let Some(docstring) = docstring %}
    /// {{ crate::utils::prefix_lines(docstring, "# ") }}
    /// {% endif %}
    /// const :{{ name }}, {{ type.serialize_type(pkg) }}
    /// ```
    #[derive(askama::Template, Clone)]
    #[template(in_doc = true, escape = "none", ext = "txt")]
    pub struct FieldRb<'a> {
        pub docstring: Option<String>,
        pub name: String,
        pub r#type: TypeRb,
        pub pkg: &'a CurrentRenderPackage,
    }
    impl std::fmt::Debug for FieldRb<'_> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(
                f,
                "FieldRb {{docstring: {:?}, name: {}, type: {:?}, pkg: <<Mutex>> }}",
                self.docstring, self.name, self.r#type
            )
        }
    }
}

mod enums {
    /// An enum in Rb.
    ///
    /// ```askama
    /// class {{name}} < T::Enum
    ///     {%- if let Some(docstring) = docstring %}
    ///     {{crate::utils::prefix_lines(docstring, "# ") }}
    ///     {% endif %}
    ///     {%- if !values.is_empty() %}
    ///     enums do
    ///     {%- for (value, docstring) in values %}
    ///     {%- if let Some(docstring) = docstring %}
    ///         {{ crate::utils::prefix_lines(docstring, "# ") }}
    ///     {%- endif %}
    ///         {{ value }} = new("{{ value }}")
    ///     {%- endfor %}
    ///     end
    ///     {%- endif %}
    /// end
    /// ```
    #[derive(askama::Template)]
    #[template(in_doc = true, escape = "none", ext = "txt")]
    pub struct EnumRb {
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

    impl super::EnumRb {
        pub fn to_type_builder_property(&self) -> TypeBuilderProperty<'_, Self> {
            TypeBuilderProperty { property: self }
        }
    }

    impl TypeBuilderPropertyTrait for super::EnumRb {
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

    impl super::ClassRb<'_> {
        pub fn to_type_builder_property(&self) -> TypeBuilderProperty<'_, Self> {
            TypeBuilderProperty { property: self }
        }
    }

    impl TypeBuilderPropertyTrait for super::ClassRb<'_> {
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
    ///     def type(self) -> baml_rb.FieldType:
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
    ///     def add_property(self, name: str, type: baml_rb.FieldType) -> {{ class.class_property_type() }}:
    ///         if name in self._properties:
    ///             raise ValueError(f"Property {name} already exists.")
    ///         return self._bldr.property(name).type(type)
    ///
    ///     def list_properties(self) -> typing.List[typing.Tuple[str, {{ class.class_property_type() }}]]:
    ///         return [(name, self._bldr.property(name)) for name in self._properties]
    ///
    ///     {% else %}
    ///     def list_properties(self) -> typing.List[typing.Tuple[str, {{ class.class_property_type() }}]]:
    ///         return [(name, {{ class.class_property_type() }}(self._bldr.property(name))) for name in self._properties]
    ///     {% endif %}
    ///
    ///
    /// class {{ class.name }}Properties:
    ///     def __init__(self, bldr: baml_rb.ClassBuilder, properties: typing.Set[str]):
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
        pub class: &'a super::ClassRb<'a>,
    }

    impl<'a> super::ClassRb<'a> {
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
                    "baml_rb"
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
    ///     def type(self) -> baml_rb.FieldType:
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
    ///     def __init__(self, enum_bldr: baml_rb.EnumBuilder, values: typing.Set[str]):
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
        pub enum_: &'a super::EnumRb,
    }

    impl<'a> super::EnumRb {
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
                    "baml_rb"
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

    /// A type alias in Rb.
    ///
    /// ```askama
    /// {% if let Some(docstring) = docstring -%}
    /// {{ crate::utils::prefix_lines(docstring, "# ") }}
    /// {%- endif %}
    /// {{ name }} = T.type_alias{ {{ type_.serialize_type(&pkg.define_alias(&name)) }} }
    /// ```
    #[derive(askama::Template)]
    #[template(in_doc = true, escape = "none", ext = "txt")]
    pub struct TypeAliasRb<'a> {
        pub name: String,
        pub type_: TypeRb,
        pub docstring: Option<String>,
        pub pkg: &'a CurrentRenderPackage,
    }
}

/// A list of types in Rb.
///
/// ```askama
/// class Check < T::Struct
///     extend T::Sig
///
///     const :name, String
///     const :expr, String
///     const :status, String
/// end
///
/// class Checked < T::Struct
///     extend T::Sig
///     extend T::Generic
///     Value = type_member
///     const :value, Value
///     const :checks, T::Hash[Symbol, Check]
/// end
/// ```
///
#[derive(askama::Template)]
#[template(in_doc = true, escape = "none", ext = "txt")]
struct RbTypesUtils<'a> {
    pkg: &'a CurrentRenderPackage,
}

pub(crate) fn render_rb_types_utils(pkg: &CurrentRenderPackage) -> Result<String, askama::Error> {
    use askama::Template;

    RbTypesUtils { pkg }.render()
}

/// A list of types in Rb.
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
struct RbTypes<'ir, T: askama::Template> {
    items: &'ir [T],
    name: &'ir str,
}

pub(crate) fn render_rb_types<T: askama::Template>(
    items: &[T],
    _pkg: &CurrentRenderPackage,
) -> Result<String, askama::Error> {
    use askama::Template;

    RbTypes {
        items,
        name: match std::any::type_name::<T>() {
            "generators_ruby::generated_types::class::ClassRb" => "classes",
            "generators_ruby::generated_types::enums::EnumRb" => "enums",
            "generators_ruby::generated_types::type_aliases::TypeAliasRb" => "type aliases",
            other => panic!("Unknown type: {other}"),
        },
    }
    .render()
}

/// A list of types in Rb.
///
/// ```askama
/// class StreamState < T::Struct
///     extend T::Sig
///     extend T::Generic
///     Value = type_member
///     const :value, Value
///     const :state, Symbol
/// end
/// ```
#[derive(askama::Template)]
#[template(in_doc = true, escape = "none", ext = "txt")]
pub struct RbStreamTypesUtils<'a> {
    pkg: &'a CurrentRenderPackage,
}

pub(crate) fn render_rb_stream_types_utils(
    pkg: &CurrentRenderPackage,
) -> Result<String, askama::Error> {
    use askama::Template;

    RbStreamTypesUtils { pkg }.render()
}

#[derive(askama::Template)]
#[template(path = "type_builder.rb.j2", escape = "none", ext = "txt")]
struct RbTypeBuilder<'a> {
    classes: &'a [ClassRb<'a>],
    enums: &'a [EnumRb],
}

pub(crate) fn render_rb_type_builder(
    classes: &[ClassRb],
    enums: &[EnumRb],
) -> Result<String, askama::Error> {
    use askama::Template;

    RbTypeBuilder { classes, enums }.render()
}

pub use class::{ClassRb, FieldRb};
pub use enums::EnumRb;
pub use type_aliases::TypeAliasRb;
