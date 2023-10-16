use crate::ast::{
    traits::WithSpan, Class, Client, Enum, Function, GeneratorConfig, Identifier, Span,
};

use super::Variant;

/// Enum for distinguishing between top-level entries
#[derive(Debug, Clone)]
pub enum Top {
    /// An enum declaration
    Enum(Enum),
    // A class declaration
    Class(Class),
    // A function declaration
    Function(Function),

    // Clients to run
    Client(Client),

    // Variant to run
    Variant(Variant),

    // Generator
    Generator(GeneratorConfig),
}

impl Top {
    /// A string saying what kind of item this is.
    pub fn get_type(&self) -> &str {
        match self {
            // Top::CompositeType(_) => "composite type",
            Top::Enum(_) => "enum",
            Top::Class(_) => "class",
            Top::Function(_) => "function",
            Top::Client(m) if m.is_llm() => "client<llm>",
            Top::Client(_) => "client<?>",
            Top::Variant(v) if v.is_llm() => "variant<llm>",
            Top::Variant(_) => "variant<?>",
            Top::Generator(_) => "generator",
        }
    }

    /// The name of the item.
    pub fn identifier(&self) -> &Identifier {
        match self {
            // Top::CompositeType(ct) => &ct.name,
            Top::Enum(x) => &x.name,
            Top::Class(x) => &x.name,
            Top::Function(x) => &x.name,
            Top::Client(x) => &x.name,
            Top::Variant(x) => &x.name,
            Top::Generator(x) => &x.name,
        }
    }

    /// The name of the item.
    pub fn name(&self) -> &str {
        &self.identifier().name
    }

    /// Try to interpret the item as an enum declaration.
    pub fn as_enum(&self) -> Option<&Enum> {
        match self {
            Top::Enum(r#enum) => Some(r#enum),
            _ => None,
        }
    }

    pub fn as_class(&self) -> Option<&Class> {
        match self {
            Top::Class(class) => Some(class),
            _ => None,
        }
    }

    pub fn as_function(&self) -> Option<&Function> {
        match self {
            Top::Function(func) => Some(func),
            _ => None,
        }
    }

    pub fn as_client(&self) -> Option<&Client> {
        match self {
            Top::Client(client) => Some(client),
            _ => None,
        }
    }

    pub fn as_generator(&self) -> Option<&GeneratorConfig> {
        match self {
            Top::Generator(gen) => Some(gen),
            _ => None,
        }
    }

    pub fn as_variant(&self) -> Option<&Variant> {
        match self {
            Top::Variant(variant) => Some(variant),
            _ => None,
        }
    }
}

impl WithSpan for Top {
    fn span(&self) -> Span {
        match self {
            Top::Enum(en) => en.span(),
            Top::Class(class) => class.span(),
            Top::Function(func) => func.span(),
            Top::Client(client) => client.span(),
            Top::Variant(variant) => variant.span(),
            Top::Generator(gen) => gen.span(),
        }
    }
}
