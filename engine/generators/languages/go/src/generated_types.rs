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
        Ok(format!("{}{}", first_letter, rest))
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
/// type Checked[T any] baml.Checked[T]
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
///     flatbuffers "github.com/google/flatbuffers/go"
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

const STREAM_STATE_GO: &str = r#"
type StreamStateType string

const (
    StreamStatePending    StreamStateType = "Pending"
    StreamStateIncomplete StreamStateType = "Incomplete"
    StreamStateComplete   StreamStateType = "Complete"
)

// Values returns all allowed values for the AliasedEnum type.
func (StreamStateType) Values() []StreamStateType {
    return []StreamStateType{
        StreamStatePending,
        StreamStateIncomplete,
        StreamStateComplete,
    }
}

// IsValid checks whether the given AliasedEnum value is valid.
func (e StreamStateType) IsValid() bool {

    for _, v := range e.Values() {
        if e == v {
            return true
        }
    }
    return false

}

// MarshalJSON customizes JSON marshaling for AliasedEnum.
func (e StreamStateType) MarshalJSON() ([]byte, error) {
    if !e.IsValid() {
        return nil, fmt.Errorf("invalid StreamStateType: %q", e)
    }
    return json.Marshal(string(e))
}

// UnmarshalJSON customizes JSON unmarshaling for AliasedEnum.
func (e *StreamStateType) UnmarshalJSON(data []byte) error {
    var s string
    if err := json.Unmarshal(data, &s); err != nil {
        return err
    }
    *e = StreamStateType(s)
    if !e.IsValid() {
        return fmt.Errorf("invalid StreamStateType: %q", s)
    }
    return nil
}


type StreamState[T any] struct {
    Value T `json:"value"`
    State StreamStateType `json:"state"`    
}
"#;

/// A list of types in Go.
///
/// ```askama
/// package stream_types
///
/// import (
///     "encoding/json"
///     "fmt"
///
///     flatbuffers "github.com/google/flatbuffers/go"
///     baml "github.com/boundaryml/baml/engine/language_client_go/pkg"
///     "github.com/boundaryml/baml/engine/language_client_go/pkg/cffi"
/// )
///
/// {{ STREAM_STATE_GO }}
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
/// A list of types in Go.
///
/// ```askama
/// package stream_types
///
/// import (
///     "encoding/json"
///     "fmt"
///
///     flatbuffers "github.com/google/flatbuffers/go"
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
