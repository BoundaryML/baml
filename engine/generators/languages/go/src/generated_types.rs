use crate::{
    package::CurrentRenderPackage,
    r#type::{SerializeType, TypeGo},
};

mod filters {
    // This filter does not have extra arguments
    pub fn exported_name(s: &str, _: &dyn askama::Values) -> askama::Result<String> {
        // make first letter uppercase
        let first_letter = s.chars().next().unwrap().to_uppercase();
        let rest = s[1..].to_string();
        Ok(format!("{first_letter}{rest}"))
    }
}

mod class {
    use super::*;

    #[derive(askama::Template)]
    #[template(path = "class.go.j2", escape = "none", ext = "txt")]
    pub struct ClassGo<'a> {
        pub name: String,
        pub docstring: Option<String>,
        pub fields: Vec<FieldGo<'a>>,
        pub dynamic: bool,
        pub pkg: &'a CurrentRenderPackage,
    }

    /// A field in a class.
    ///
    /// ```askama
    /// {% if let Some(docstring) = docstring -%}
    /// {{ crate::utils::prefix_lines(docstring, "/// ") }}
    /// {%- endif %}
    /// {{ name|exported_name }} {{ type.serialize_type(pkg) }} `json:"{{ name }}"`
    /// ```
    #[derive(askama::Template, Clone)]
    #[template(in_doc = true, escape = "none", ext = "txt")]
    pub struct FieldGo<'a> {
        pub docstring: Option<String>,
        pub name: String,
        pub r#type: TypeGo,
        pub pkg: &'a CurrentRenderPackage,
    }
    impl std::fmt::Debug for FieldGo<'_> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(
                f,
                "FieldGo {{docstring: {:?}, name: {}, type: {:?}, pkg: <<Mutex>> }}",
                self.docstring, self.name, self.r#type
            )
        }
    }
}

mod enums {
    use super::*;

    #[derive(askama::Template)]
    #[template(path = "enums.go.j2", escape = "none")]
    pub struct EnumGo<'a> {
        pub name: String,
        pub docstring: Option<String>,
        pub values: Vec<(String, Option<String>)>,
        pub dynamic: bool,
        pub pkg: &'a CurrentRenderPackage,
    }
}

mod union {
    use super::*;

    #[derive(askama::Template)]
    #[template(path = "unions.go.j2", escape = "none")]
    pub struct UnionGo<'a> {
        pub name: String,
        pub cffi_name: String,
        pub docstring: Option<String>,
        pub variants: Vec<VariantGo>,
        pub pkg: &'a CurrentRenderPackage,
    }

    #[derive(Clone)]
    pub struct VariantGo {
        pub name: String,
        pub cffi_name: String,
        pub literal_repr: Option<String>,
        pub type_: TypeGo,
    }
}

mod type_aliases {
    use super::*;

    /// A type alias in Go.
    ///
    /// ```askama
    /// {% if let Some(docstring) = docstring -%}
    /// {{ crate::utils::prefix_lines(docstring, "/// ") }}
    /// {%- endif %}
    /// type {{ name }} = {{ type_.serialize_type(pkg) }}
    /// ```
    #[derive(askama::Template)]
    #[template(in_doc = true, escape = "none", ext = "txt")]
    pub struct TypeAliasGo<'a> {
        pub name: String,
        pub type_: TypeGo,
        pub docstring: Option<String>,
        pub pkg: &'a CurrentRenderPackage,
    }
}

/// A list of types in Go.
///
/// ```askama
/// package types
///
/// import (
///     baml "github.com/boundaryml/baml/engine/language_client_go/pkg"
/// )
///
/// type Checked[T any] = baml.Checked[T]
///
/// type Image baml.Image
/// type Audio baml.Audio
/// type Video baml.Video
/// type PDF baml.PDF
/// ```
///
#[derive(askama::Template)]
#[template(in_doc = true, escape = "none", ext = "txt")]
struct GoTypesUtils {}

pub(crate) fn render_go_types_utils(_pkg: &CurrentRenderPackage) -> Result<String, askama::Error> {
    use askama::Template;

    GoTypesUtils {}.render()
}

/// A list of types in Go.
///
/// ```askama
/// package types
///
/// import (
///     "encoding/json"
///     "fmt"
///
///     baml "github.com/boundaryml/baml/engine/language_client_go/pkg"
///     "github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
/// )
///
/// {% for item in items -%}
/// {{ item.render()? }}
/// {% endfor %}
///
/// ```
///
#[derive(askama::Template)]
#[template(in_doc = true, escape = "none", ext = "txt")]
struct GoTypes<'ir, T: askama::Template> {
    items: &'ir [T],
}

pub(crate) fn render_go_types<T: askama::Template>(
    items: &[T],
    _pkg: &CurrentRenderPackage,
) -> Result<String, askama::Error> {
    use askama::Template;

    GoTypes { items }.render()
}

/// A list of types in Go.
///
/// ```askama
/// package stream_types
///
/// import (
///     "encoding/json"
///     "fmt"
///
///     baml "github.com/boundaryml/baml/engine/language_client_go/pkg"
///     "github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
/// )
///
/// ```
///
#[derive(askama::Template)]
#[template(in_doc = true, escape = "none", ext = "txt")]
pub struct GoStreamTypesUtils {}

pub(crate) fn render_go_stream_types_utils(
    _pkg: &CurrentRenderPackage,
) -> Result<String, askama::Error> {
    use askama::Template;

    GoStreamTypesUtils {}.render()
}

mod type_builder {
    use super::*;

    #[derive(askama::Template)]
    #[template(path = "type_builder_enums.go.j2", escape = "none")]
    pub struct TypeBuilderEnumsGo<'a> {
        pub enums: &'a [EnumGo<'a>],
    }

    #[derive(askama::Template)]
    #[template(path = "type_builder_classes.go.j2", escape = "none")]
    pub struct TypeBuilderClassesGo<'a> {
        pub classes: &'a [ClassGo<'a>],
    }

    #[derive(askama::Template)]
    #[template(path = "type_builder.go.j2", escape = "none")]
    pub struct TypeBuilderGo {}

    impl<'a> EnumGo<'a> {
        pub fn builder(&self) -> &str {
            if self.dynamic {
                "Builder"
            } else {
                "View"
            }
        }
    }

    impl<'a> ClassGo<'a> {
        pub fn builder(&self) -> &str {
            if self.dynamic {
                "Builder"
            } else {
                "View"
            }
        }
    }
}

pub(crate) fn render_type_builder_common(
    _enums: &[EnumGo],
    _classes: &[ClassGo],
    _pkg: &CurrentRenderPackage,
) -> Result<String, askama::Error> {
    use askama::Template;
    type_builder::TypeBuilderGo {}.render()
}

pub(crate) fn render_type_builder_enums(
    enums: &[EnumGo],
    _pkg: &CurrentRenderPackage,
) -> Result<String, askama::Error> {
    use askama::Template;
    type_builder::TypeBuilderEnumsGo { enums }.render()
}

pub(crate) fn render_type_builder_classes(
    classes: &[ClassGo],
    _pkg: &CurrentRenderPackage,
) -> Result<String, askama::Error> {
    use askama::Template;
    type_builder::TypeBuilderClassesGo { classes }.render()
}

/// A list of types in Go.
///
/// ```askama
/// package stream_types
///
/// import (
///     "encoding/json"
///     "fmt"
///
///     baml "github.com/boundaryml/baml/engine/language_client_go/pkg"
///     "github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
///
///     "{{ go_mod_name }}/baml_client/types"
/// )
///
/// {% for item in items -%}
/// {{ item.render()? }}
/// {%- endfor %}
/// ```
///
#[derive(askama::Template)]
#[template(in_doc = true, escape = "none", ext = "txt")]
struct GoStreamTypes<'ir, T: askama::Template> {
    items: &'ir [T],
    go_mod_name: &'ir str,
}

pub(crate) fn render_go_stream_types<T: askama::Template>(
    items: &[T],
    _pkg: &CurrentRenderPackage,
    go_mod_name: &str,
) -> Result<String, askama::Error> {
    use askama::Template;

    GoStreamTypes { items, go_mod_name }.render()
}

pub use class::{ClassGo, FieldGo};
pub use enums::EnumGo;
pub use type_aliases::TypeAliasGo;
pub use union::{UnionGo, VariantGo};
