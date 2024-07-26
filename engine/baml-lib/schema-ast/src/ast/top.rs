use super::{
    traits::WithSpan, Identifier, Span, TemplateString, TypeExpression, ValueExp, WithIdentifier,
};

/// Enum for distinguishing between top-level entries
#[derive(Debug, Clone)]
pub enum Top {
    /// An enum declaration
    Enum(TypeExpression),
    // A class declaration
    Class(TypeExpression),
    // A function declaration
    Function(ValueExp),

    // Clients to run
    Client(ValueExp),

    TemplateString(TemplateString),

    // Generator
    Generator(ValueExp),

    TestCase(ValueExp),

    RetryPolicy(ValueExp),
}

impl Top {
    /// A string saying what kind of item this is.
    pub fn get_type(&self) -> &str {
        match self {
            // Top::CompositeType(_) => "composite type",
            Top::Enum(_) => "enum",
            Top::Class(_) => "class",
            Top::Function(_) => "function",
            Top::Client(_) => "client<llm>",
            Top::TemplateString(_) => "template_string",
            Top::Generator(_) => "generator",
            Top::TestCase(_) => "test_case",
            Top::RetryPolicy(_) => "retry_policy",
        }
    }

    /// Try to interpret the item as an enum declaration.
    pub fn as_type_expression(&self) -> Option<&TypeExpression> {
        match self {
            Top::Enum(r#enum) => Some(r#enum),
            Top::Class(class) => Some(class),
            _ => None,
        }
    }

    pub fn as_value_exp(&self) -> Option<&ValueExp> {
        match self {
            Top::Function(func) => Some(func),
            Top::Client(client) => Some(client),
            Top::Generator(gen) => Some(gen),
            Top::TestCase(test) => Some(test),
            Top::RetryPolicy(retry) => Some(retry),
            _ => None,
        }
    }

    pub fn as_template_string(&self) -> Option<&TemplateString> {
        match self {
            Top::TemplateString(t) => Some(t),
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
            Top::Function(x) => x.identifier(),
            Top::Client(x) => x.identifier(),
            Top::TemplateString(x) => x.identifier(),
            Top::Generator(x) => x.identifier(),
            Top::TestCase(x) => x.identifier(),
            Top::RetryPolicy(x) => x.identifier(),
        }
    }
}

impl WithSpan for Top {
    fn span(&self) -> &Span {
        match self {
            Top::Enum(en) => en.span(),
            Top::Class(class) => class.span(),
            Top::Function(func) => func.span(),
            Top::TemplateString(template) => template.span(),
            Top::Client(client) => client.span(),
            Top::Generator(gen) => gen.span(),
            Top::TestCase(test) => test.span(),
            Top::RetryPolicy(retry) => retry.span(),
        }
    }
}
