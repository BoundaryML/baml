use super::{
    traits::WithSpan, Class, Client, Configuration, Enum, Function, GeneratorConfig, Identifier,
    Span, Variant, WithIdentifier,
};

/// Enum for distinguishing between top-level entries
#[derive(Debug, Clone)]
pub enum Top {
    /// An enum declaration
    Enum(Enum),
    // A class declaration
    Class(Class),
    // A function declaration
    FunctionOld(Function),
    Function(Function),

    // Clients to run
    Client(Client),

    // Variant to run
    Variant(Variant),

    // Abritrary config (things with names and key-value pairs where keys are known)
    Config(Configuration),

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
            Top::FunctionOld(_) => "function[deprecated]",
            Top::Function(_) => "function",
            Top::Client(m) if m.is_llm() => "client<llm>",
            Top::Client(_) => "client<?>",
            Top::Variant(v) if v.is_llm() => "impl<llm>",
            Top::Variant(_) => "impl<?>",
            Top::Generator(_) => "generator",
            Top::Config(c) => c.get_type(),
        }
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

    pub fn as_function_old(&self) -> Option<&Function> {
        match self {
            Top::FunctionOld(func) => Some(func),
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

    pub fn as_configurations(&self) -> Option<&Configuration> {
        match self {
            Top::Config(config) => Some(config),
            _ => None,
        }
    }
}

impl WithIdentifier for Top {
    /// The name of the item.
    fn identifier(&self) -> &Identifier {
        match self {
            // Top::CompositeType(ct) => &ct.name,
            Top::Enum(x) => x.identifier(),
            Top::Class(x) => x.identifier(),
            Top::Function(x) | Top::FunctionOld(x) => x.identifier(),
            Top::Client(x) => x.identifier(),
            Top::Variant(x) => x.identifier(),
            Top::Generator(x) => x.identifier(),
            Top::Config(x) => x.identifier(),
        }
    }
}

impl WithSpan for Top {
    fn span(&self) -> &Span {
        match self {
            Top::Enum(en) => en.span(),
            Top::Class(class) => class.span(),
            Top::Function(func) | Top::FunctionOld(func) => func.span(),
            Top::Client(client) => client.span(),
            Top::Variant(variant) => variant.span(),
            Top::Generator(gen) => gen.span(),
            Top::Config(config) => config.span(),
        }
    }
}
