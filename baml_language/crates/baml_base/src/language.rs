//! Compiler-owned registry for built-in language constructs.
//!
//! This module contains semantic metadata shared by parsing, validation, and
//! editor tooling. Human-readable documentation lives in `baml_builtins2` and
//! is keyed by the stable names exposed here.

/// The argument shape accepted by a built-in schema attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaAttributeArguments {
    None,
    String { placeholder: &'static str },
}

/// Where a schema attribute is written. Attributes belong to declarations: a
/// member position takes `@name` trailing the member, a block position takes
/// `@@name` inside the declaration's body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributePosition {
    /// `@name` trailing a class field.
    ClassField,
    /// `@name` trailing an interface field.
    InterfaceField,
    /// `@name` trailing an enum variant.
    EnumVariant,
    /// `@@name` inside a class body.
    Class,
    /// `@@name` inside an enum body.
    Enum,
    /// `@@name` inside an interface body.
    Interface,
    /// `@@name` on a function, method, or interface method signature.
    Function,
}

impl AttributePosition {
    /// The prefix an attribute is written with at this position.
    pub const fn sigil(self) -> &'static str {
        match self {
            Self::ClassField | Self::InterfaceField | Self::EnumVariant => "@",
            Self::Class | Self::Enum | Self::Interface | Self::Function => "@@",
        }
    }

    /// The position as a diagnostic reads it, article included.
    pub const fn describe(self) -> &'static str {
        match self {
            Self::ClassField => "a class field",
            Self::InterfaceField => "an interface field",
            Self::EnumVariant => "an enum variant",
            Self::Class => "a class",
            Self::Enum => "an enum",
            Self::Interface => "an interface",
            Self::Function => "a function",
        }
    }
}

/// Semantic metadata for a built-in schema attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchemaAttributeSpec {
    pub name: &'static str,
    pub arguments: SchemaAttributeArguments,
    pub repeatable: bool,
    /// The positions the attribute has meaning at. Anywhere else it is an
    /// error: an attribute the compiler knows but does not read at a position
    /// would otherwise be accepted and silently ignored.
    pub positions: &'static [AttributePosition],
}

impl SchemaAttributeSpec {
    /// Render the attribute without its contextual `@`/`@@` prefix.
    pub fn signature(self) -> String {
        match self.arguments {
            SchemaAttributeArguments::None => self.name.to_string(),
            SchemaAttributeArguments::String { placeholder } => {
                format!(r#"{}("{placeholder}")"#, self.name)
            }
        }
    }

    pub fn allows(self, position: AttributePosition) -> bool {
        self.positions.contains(&position)
    }
}

// No schema attribute has meaning on an interface or its fields (a contract,
// not a concrete data type: nothing is ever parsed or rendered *as* one) or on
// a function. Those positions exist so that writing one there is an error
// rather than an accepted no-op.
const DATA_MEMBERS_AND_BLOCKS: &[AttributePosition] = &[
    AttributePosition::ClassField,
    AttributePosition::EnumVariant,
    AttributePosition::Class,
    AttributePosition::Enum,
];

const DATA_MEMBERS: &[AttributePosition] = &[
    AttributePosition::ClassField,
    AttributePosition::EnumVariant,
];

pub const SCHEMA_ATTRIBUTE_SPECS: &[SchemaAttributeSpec] = &[
    SchemaAttributeSpec {
        name: "description",
        arguments: SchemaAttributeArguments::String {
            placeholder: "text",
        },
        repeatable: false,
        positions: DATA_MEMBERS_AND_BLOCKS,
    },
    SchemaAttributeSpec {
        name: "alias",
        arguments: SchemaAttributeArguments::String {
            placeholder: "name",
        },
        repeatable: false,
        positions: DATA_MEMBERS_AND_BLOCKS,
    },
    SchemaAttributeSpec {
        name: "skip",
        arguments: SchemaAttributeArguments::None,
        repeatable: false,
        positions: DATA_MEMBERS,
    },
    // BEP-075: the streaming attributes are declaration attributes. A field's
    // `@stream.done` holds the field at its default until its value is
    // complete; a class's `@@stream.done` holds the whole object.
    SchemaAttributeSpec {
        name: "stream.done",
        arguments: SchemaAttributeArguments::None,
        repeatable: false,
        positions: &[AttributePosition::ClassField, AttributePosition::Class],
    },
    // The field has no default, so its class has no partial parse until the
    // field is present.
    SchemaAttributeSpec {
        name: "stream.must_exist",
        arguments: SchemaAttributeArguments::None,
        repeatable: false,
        positions: &[AttributePosition::ClassField],
    },
    // The JSON key of a field or variant, in `baml.json` and generated SDK
    // serialization. `@alias` stays the name a model is shown.
    SchemaAttributeSpec {
        name: "json.rename",
        arguments: SchemaAttributeArguments::String { placeholder: "key" },
        repeatable: false,
        positions: DATA_MEMBERS,
    },
    // A `map<string, json>` field that holds every key no other field took,
    // and writes them next to the other fields.
    SchemaAttributeSpec {
        name: "json.flatten",
        arguments: SchemaAttributeArguments::None,
        repeatable: false,
        positions: &[AttributePosition::ClassField],
    },
    // A `map<string, json>` field that makes its class the member of a union
    // that takes any object no other member's literals match. It holds the
    // whole object, and is all the class writes.
    SchemaAttributeSpec {
        name: "json.fallback",
        arguments: SchemaAttributeArguments::None,
        repeatable: false,
        positions: &[AttributePosition::ClassField],
    },
    // The class's optional fields holding `null` are left out when it is
    // written. A `json` field keeps its `null`, which is a value there.
    SchemaAttributeSpec {
        name: "json.omit_nulls",
        arguments: SchemaAttributeArguments::None,
        repeatable: false,
        positions: &[AttributePosition::Class],
    },
];

pub fn schema_attribute_spec(name: &str) -> Option<&'static SchemaAttributeSpec> {
    SCHEMA_ATTRIBUTE_SPECS.iter().find(|spec| spec.name == name)
}

/// The schema attributes that have meaning at `position`, in table order.
pub fn schema_attribute_specs_at(
    position: AttributePosition,
) -> impl Iterator<Item = &'static SchemaAttributeSpec> {
    SCHEMA_ATTRIBUTE_SPECS
        .iter()
        .filter(move |spec| spec.allows(position))
}

/// Presentation-neutral identity and signature for a well-known client
/// configuration key. Provider-specific validation remains owned by the
/// client-options/type schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientConfigKeySpec {
    pub name: &'static str,
    pub signature: &'static str,
}

pub const CLIENT_CONFIG_KEY_SPECS: &[ClientConfigKeySpec] = &[
    ClientConfigKeySpec {
        name: "provider",
        signature: "provider <name>",
    },
    ClientConfigKeySpec {
        name: "options",
        signature: "options { ... }",
    },
    ClientConfigKeySpec {
        name: "model",
        signature: "model <name>",
    },
    ClientConfigKeySpec {
        name: "http",
        signature: "http { ... }",
    },
    ClientConfigKeySpec {
        name: "request_timeout_ms",
        signature: "request_timeout_ms <milliseconds>",
    },
    ClientConfigKeySpec {
        name: "retry_policy",
        signature: "retry_policy <name>",
    },
];

pub fn client_config_key_spec(name: &str) -> Option<&'static ClientConfigKeySpec> {
    CLIENT_CONFIG_KEY_SPECS
        .iter()
        .find(|spec| spec.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_attribute_lookup_and_signatures() {
        assert_eq!(
            schema_attribute_spec("alias").map(|spec| spec.signature()),
            Some(r#"alias("name")"#.to_string())
        );
        assert_eq!(
            schema_attribute_spec("skip").map(|spec| spec.signature()),
            Some("skip".to_string())
        );
        assert_eq!(
            schema_attribute_spec("stream.done").map(|spec| spec.signature()),
            Some("stream.done".to_string())
        );
        assert!(schema_attribute_spec("stream.with_state").is_none());
    }

    #[test]
    fn schema_attribute_positions() {
        let names_at = |position| {
            schema_attribute_specs_at(position)
                .map(|spec| spec.name)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            names_at(AttributePosition::ClassField),
            [
                "description",
                "alias",
                "skip",
                "stream.done",
                "stream.must_exist",
                "json.rename",
                "json.flatten",
                "json.fallback"
            ]
        );
        assert_eq!(
            names_at(AttributePosition::EnumVariant),
            ["description", "alias", "skip", "json.rename"]
        );
        assert_eq!(
            names_at(AttributePosition::Class),
            ["description", "alias", "stream.done", "json.omit_nulls"]
        );
        assert_eq!(names_at(AttributePosition::Enum), ["description", "alias"]);
        assert!(names_at(AttributePosition::Interface).is_empty());
        assert!(names_at(AttributePosition::Function).is_empty());
        assert!(names_at(AttributePosition::InterfaceField).is_empty());
    }

    #[test]
    fn client_config_key_lookup() {
        assert_eq!(
            client_config_key_spec("request_timeout_ms").map(|spec| spec.signature),
            Some("request_timeout_ms <milliseconds>")
        );
        assert!(client_config_key_spec("temperature").is_none());
    }
}
