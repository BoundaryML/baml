use crate::{
    package::CurrentRenderPackage,
    r#type::{SerializeType, TypeTS},
};

mod filters {
    // This filter does not have extra arguments
    // pub fn exported_name(s: &str, _: &dyn askama::Values) -> askama::Result<String> {
    //     // make first letter uppercase
    //     let first_letter = s.chars().next().unwrap().to_uppercase();
    //     let rest = s[1..].to_string();
    //     Ok(format!("{}{}", first_letter, rest))
    // }
}

mod r#enum {

    /// An enum in TS.
    ///
    /// ```askama
    /// {%- if let Some(docstring) = docstring %}
    /// /**
    /// {{crate::utils::prefix_lines(docstring, " * ") }}
    ///  */
    /// {%- endif %}
    /// export enum {{name}} {
    ///   {%- for (value, docstring) in values %}
    ///   {%- if let Some(docstring) = docstring %}
    ///   /**
    ///   {{crate::utils::prefix_lines(docstring, " * ") }}
    ///    */
    ///   {%- endif %}
    ///   {{ value }} = "{{ value }}",
    ///   {%- endfor %}
    /// }
    ///
    /// ```
    #[derive(askama::Template)]
    #[template(in_doc = true, escape = "none", ext = "txt")]
    pub struct EnumTS {
        pub name: String,
        pub docstring: Option<String>,
        pub values: Vec<(String, Option<String>)>,
        pub dynamic: bool,
    }
}

mod class {
    use super::*;

    /// A class in TS.
    ///
    /// ```askama
    /// {%- if let Some(docstring) = docstring %}
    /// /**
    /// {{crate::utils::prefix_lines(docstring, " * ") }}
    ///  */
    /// {%- endif %}
    /// export interface {{name}} {
    ///   {%- for field in fields %}
    ///   {{- field.render()? }}
    ///   {%- endfor %}
    ///   {% if dynamic %}
    ///   [key: string]: any;
    ///   {%- endif %}
    /// }
    ///
    /// ```
    #[derive(askama::Template)]
    #[template(in_doc = true, escape = "none", ext = "txt")]
    pub struct ClassTS<'a> {
        pub name: String,
        pub docstring: Option<String>,
        pub fields: Vec<FieldTS<'a>>,
        pub dynamic: bool,
    }

    /// A field in a class.
    ///
    /// ```askama
    /// {% if let Some(docstring) = docstring %}
    ///   /**
    ///{{crate::utils::prefix_lines(docstring, "   * ") }}
    ///    */
    /// {%- endif %}
    ///   {{name}}{% if type.meta().is_optional() %}?{% endif %}: {{type.serialize_type(pkg)}}
    /// ```
    #[derive(askama::Template, Clone)]
    #[template(in_doc = true, escape = "none", ext = "txt")]
    pub struct FieldTS<'a> {
        pub docstring: Option<String>,
        pub name: String,
        pub r#type: TypeTS,
        pub pkg: &'a CurrentRenderPackage,
    }
    impl std::fmt::Debug for FieldTS<'_> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(
                f,
                "FieldTS {{docstring: {:?}, name: {}, type: {:?}, pkg: <<Mutex>> }}",
                self.docstring, self.name, self.r#type
            )
        }
    }
}

///
/// ```askama
/// import type { Image, Audio, Pdf, Video } from "@boundaryml/baml"
/// import type { Checked, Check } from "./types"
/// import type { {% for t in types %} {{ t }}{% if !loop.last %}, {% endif %}{% endfor %} } from "./types"
/// import type * as types from "./types"
///
/// /******************************************************************************
/// *
/// *  These types are used for streaming, for when an instance of a type
/// *  is still being built up and any of its fields is not yet fully available.
/// *
/// ******************************************************************************/
///
/// export interface StreamState<T> {
///   value: T
///   state: "Pending" | "Incomplete" | "Complete"
/// }
///
/// export namespace partial_types {
///   {%- for cls in classes %}
///     {%- if let Some(docstring) = cls.docstring %}
///     /**
///     {{crate::utils::prefix_lines(docstring, " * ") }}
///     */
///     {%- endif %}
///     export interface {{cls.name}} {
///     {%- for field in cls.fields %}
///         {{- field.render()?|indent(4, true) }}
///     {%- endfor %}
///     {%- if cls.dynamic %}
///       [key: string]: any;
///     {%- endif %}
///     }
///   {%- endfor %}
///   {%- for alias in type_aliases %}
///     {{- alias.render()? }}
///   {%- endfor %}
///   {%- for interface_alias in interface_aliases %}
///     {{- interface_alias.render()? }}
///   {%- endfor %}
/// }
/// ```
#[derive(askama::Template)]
#[template(in_doc = true, escape = "none", ext = "txt")]
struct PartialTypes<'a> {
    classes: &'a [ClassTS<'a>],
    types: &'a [String],
    type_aliases: &'a [TypeAliasTS<'a>],
    interface_aliases: &'a [TypeAliasInterfaceTS<'a>],
}

pub fn render_partial_types(
    classes: &[ClassTS],
    types: &[String],
    type_aliases: &[TypeAliasTS],
    interface_aliases: &[TypeAliasInterfaceTS],
) -> Result<String, askama::Error> {
    use askama::Template;

    PartialTypes {
        classes,
        types,
        type_aliases,
        interface_aliases,
    }
    .render()
}

mod type_aliases {
    use super::*;

    /// A type alias in TS.
    ///
    /// ```askama
    /// {% if let Some(docstring) = docstring -%}
    /// /**
    ///  {{crate::utils::prefix_lines(docstring, " * ") }}
    ///  */
    /// {%- endif %}
    /// export type {{ name }} = {{ target_type.serialize_type(pkg) }}
    ///
    /// ```
    #[derive(askama::Template)]
    #[template(in_doc = true, escape = "none", ext = "txt")]
    pub struct TypeAliasTS<'a> {
        pub name: String,
        pub target_type: TypeTS,
        pub docstring: Option<String>,
        pub pkg: &'a CurrentRenderPackage,
    }

    /// A type alias converted to an interface to break circular references.
    ///
    /// ```askama
    /// {% if let Some(docstring) = docstring -%}
    /// /**
    ///  {{crate::utils::prefix_lines(docstring, " * ") }}
    ///  */
    /// {%- endif %}
    /// export interface {{ name }} {
    ///   [key: string]: {{ value_type.serialize_type(pkg) }}
    /// }
    ///
    /// ```
    #[derive(askama::Template)]
    #[template(in_doc = true, escape = "none", ext = "txt")]
    pub struct TypeAliasInterfaceTS<'a> {
        pub name: String,
        pub value_type: TypeTS,
        pub docstring: Option<String>,
        pub pkg: &'a CurrentRenderPackage,
    }
}
/// A list of types in TS.
///
/// ```askama
/// import type { Image, Audio, Pdf, Video } from "@boundaryml/baml"
/// /**
///  * Recursively partial type that can be null.
///  *
///  * @deprecated Use types from the `partial_types` namespace instead, which provides type-safe partial implementations
///  * @template T The type to make recursively partial.
///  */
/// export type RecursivePartialNull<T> = T extends object
///     ? { [P in keyof T]?: RecursivePartialNull<T[P]> }
///     : T | null;
///
/// export interface Checked<T,CheckName extends string = string> {
///     value: T,
///     checks: Record<CheckName, Check>,
/// }
///
/// export interface Check {
///     name: string,
///     expr: string
///     status: "succeeded" | "failed"
/// }
///
/// export function all_succeeded<CheckName extends string>(checks: Record<CheckName, Check>): boolean {
///     return get_checks(checks).every(check => check.status === "succeeded")
/// }
///
/// export function get_checks<CheckName extends string>(checks: Record<CheckName, Check>): Check[] {
///     return Object.values(checks)
/// }
///
/// {%- for e in enums %}
/// {{- e.render()? }}
/// {%- endfor %}
///
/// {%- for cls in classes -%}
/// {{- cls.render()? -}}
/// {%- endfor -%}
///
/// {%- for alias in type_aliases %}
/// {{- alias.render()? }}
/// {%- endfor %}
/// {%- for interface_alias in interface_aliases %}
/// {{- interface_alias.render()? }}
/// {%- endfor %}
/// ```
#[derive(askama::Template)]
#[template(in_doc = true, escape = "none", ext = "txt")]
struct TsTypes<'ir> {
    enums: &'ir [EnumTS],
    classes: &'ir [ClassTS<'ir>],
    type_aliases: &'ir [TypeAliasTS<'ir>],
    interface_aliases: &'ir [TypeAliasInterfaceTS<'ir>],
}

pub(crate) fn render_ts_types(
    enums: &[EnumTS],
    classes: &[ClassTS],
    type_aliases: &[TypeAliasTS],
    interface_aliases: &[TypeAliasInterfaceTS],
    _pkg: &CurrentRenderPackage,
) -> Result<String, askama::Error> {
    use askama::Template;

    let ts_types: TsTypes = TsTypes {
        enums,
        classes,
        type_aliases,
        interface_aliases,
    };
    ts_types.render()
}

#[derive(askama:: Template)]
#[template(path = "type_builder.ts.j2", escape = "none")]
struct TypeBuilder<'a> {
    classes: &'a [ClassTS<'a>],
    enums: &'a [EnumTS],
}

pub(crate) fn render_type_builder(
    classes: &[ClassTS],
    enums: &[EnumTS],
) -> Result<String, askama::Error> {
    use askama::Template;

    TypeBuilder { classes, enums }.render()
}

pub use class::{ClassTS, FieldTS};
pub use r#enum::EnumTS;
// pub use union::{UnionTS, VariantTS};
pub use type_aliases::{TypeAliasInterfaceTS, TypeAliasTS};
