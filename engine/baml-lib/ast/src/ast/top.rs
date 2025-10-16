use super::{
    assignment::Assignment,
    expr::{ExprFn, TopLevelAssignment},
    traits::WithSpan,
    Header, Identifier, Span, TemplateString, TypeExpressionBlock, ValueExprBlock, WithIdentifier,
};

/// Enum for distinguishing between top-level entries
#[derive(Debug, Clone)]
pub enum Top {
    /// An enum declaration.
    Enum(TypeExpressionBlock),
    /// A class declaration.
    Class(TypeExpressionBlock),
    /// A function declaration.
    Function(ValueExprBlock),
    /// Type alias expression.
    TypeAlias(Assignment),

    /// Clients to run.
    Client(ValueExprBlock),

    TemplateString(TemplateString),

    /// Generator.
    Generator(ValueExprBlock),

    TestCase(ValueExprBlock),

    RetryPolicy(ValueExprBlock),

    TopLevelAssignment(TopLevelAssignment),

    ExprFn(ExprFn),
}

impl Top {
    /// A string saying what kind of item this is.
    pub fn get_type(&self) -> &str {
        match self {
            Top::Enum(_) => "enum",
            Top::Class(_) => "class",
            Top::Function(_) => "function",
            Top::TypeAlias(_) => "type_alias",
            Top::Client(_) => "client<llm>",
            Top::TemplateString(_) => "template_string",
            Top::Generator(_) => "generator",
            Top::TestCase(_) => "test_case",
            Top::RetryPolicy(_) => "retry_policy",
            Top::TopLevelAssignment(_) => "assignment",
            Top::ExprFn(_) => "function",
        }
    }

    /// Try to interpret the item as an enum declaration.
    pub fn as_type_expression(&self) -> Option<&TypeExpressionBlock> {
        match self {
            Top::Enum(r#enum) => Some(r#enum),
            Top::Class(class) => Some(class),
            _ => None,
        }
    }

    pub fn as_value_exp(&self) -> Option<&ValueExprBlock> {
        match self {
            Top::Function(func) => Some(func),
            Top::Client(client) => Some(client),
            Top::Generator(gen) => Some(gen),
            Top::TestCase(test) => Some(test),
            Top::RetryPolicy(retry) => Some(retry),
            _ => None,
        }
    }

    pub fn as_type_alias_assignment(&self) -> Option<&Assignment> {
        match self {
            Top::TypeAlias(assignment) => Some(assignment),
            _ => None,
        }
    }

    pub fn as_template_string(&self) -> Option<&TemplateString> {
        match self {
            Top::TemplateString(t) => Some(t),
            _ => None,
        }
    }

    pub fn as_top_level_assignment(&self) -> Option<&TopLevelAssignment> {
        match self {
            Top::TopLevelAssignment(assignment) => Some(assignment),
            _ => None,
        }
    }

    pub fn as_expr_fn(&self) -> Option<&ExprFn> {
        match self {
            Top::ExprFn(expr_fn) => Some(expr_fn),
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
            Top::TypeAlias(x) => x.identifier(),
            Top::Client(x) => x.identifier(),
            Top::TemplateString(x) => x.identifier(),
            Top::Generator(x) => x.identifier(),
            Top::TestCase(x) => x.identifier(),
            Top::RetryPolicy(x) => x.identifier(),
            Top::TopLevelAssignment(x) => &x.stmt.identifier,
            Top::ExprFn(x) => &x.name,
        }
    }
}

impl WithSpan for Top {
    fn span(&self) -> &Span {
        match self {
            Top::Enum(en) => en.span(),
            Top::Class(class) => class.span(),
            Top::Function(func) => func.span(),
            Top::TypeAlias(alias) => alias.span(),
            Top::TemplateString(template) => template.span(),
            Top::Client(client) => client.span(),
            Top::Generator(gen) => gen.span(),
            Top::TestCase(test) => test.span(),
            Top::RetryPolicy(retry) => retry.span(),
            Top::TopLevelAssignment(asmnt) => &asmnt.stmt.span,
            Top::ExprFn(function) => &function.span,
        }
    }
}
