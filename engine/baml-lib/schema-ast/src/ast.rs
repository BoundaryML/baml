mod argument;
mod attribute;

mod comment;
mod config;

mod expression;
mod field;

mod identifier;
mod indentation_type;
mod newline_type;

mod template_string;
mod top;
mod traits;
mod type_expression_block;
mod value_expression_block;
pub(crate) use self::comment::Comment;

pub use argument::{ArgumentId, Argument, ArgumentsList};
pub use attribute::{Attribute, AttributeContainer, AttributeId};
pub use config::ConfigBlockProperty;
pub use expression::{Expression, RawString};
pub use field::{Field, FieldArity, FieldType};
pub use identifier::{Identifier, RefIdentifier};
pub use indentation_type::IndentationType;
pub use internal_baml_diagnostics::Span;
pub use newline_type::NewlineType;
pub use template_string::TemplateString;
pub use top::Top;
pub use traits::{WithAttributes, WithDocumentation, WithIdentifier, WithName, WithSpan};
pub use type_expression_block::{FieldId, SubType, TypeExpressionBlock};
pub use value_expression_block::{
    BlockArg, BlockArgs, ValueExprBlock, ValueExprBlockType,
};

/// AST representation of a prisma schema.
///
/// This module is used internally to represent an AST. The AST's nodes can be used
/// during validation of a schema, especially when implementing custom attributes.
///
/// The AST is not validated, also fields and attributes are not resolved. Every node is
/// annotated with its location in the text representation.
/// Basically, the AST is an object oriented representation of the datamodel's text.
/// Schema = Datamodel + Generators + Datasources
#[derive(Debug)]
pub struct SchemaAst {
    /// All models, enums, composite types, datasources, generators and type aliases.
    pub tops: Vec<Top>,
}

impl Default for SchemaAst {
    fn default() -> Self {
        Self::new()
    }
}

impl SchemaAst {
    pub fn new() -> Self {
        SchemaAst { tops: Vec::new() }
    }

    /// Iterate over all the top-level items in the schema.
    pub fn iter_tops(&self) -> impl Iterator<Item = (TopId, &Top)> {
        self.tops
            .iter()
            .enumerate()
            .map(|(top_idx, top)| (top_idx_to_top_id(top_idx, top), top))
    }

    /// Iterate over all the generator blocks in the schema.
    pub fn generators(&self) -> impl Iterator<Item = &ValueExprBlock> {
        self.tops.iter().filter_map(|top| {
            if let Top::Generator(gen) = top {
                Some(gen)
            } else {
                None
            }
        })
    }
}

/// An opaque identifier for an enum in a schema AST.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeExpId(u32);

impl From<u32> for TypeExpId {
    fn from(id: u32) -> Self {
        TypeExpId(id)
    }
}

impl std::ops::Index<TypeExpId> for SchemaAst {
    type Output = TypeExpressionBlock;

    fn index(&self, index: TypeExpId) -> &Self::Output {
        self.tops[index.0 as usize]
            .as_type_expression()
            .expect("expected type expression")
    }
}

/// An opaque identifier for a model in a schema AST. Use the
/// `schema[model_id]` syntax to resolve the id to an `ast::Model`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ValExpId(u32);
impl std::ops::Index<ValExpId> for SchemaAst {
    type Output = ValueExprBlock;

    fn index(&self, index: ValExpId) -> &Self::Output {
        let idx = index.0;
        let var = &self.tops[idx as usize];

        var.as_value_exp().expect("expected value expression")
    }
}

/// An opaque identifier for a model in a schema AST. Use the
/// `schema[model_id]` syntax to resolve the id to an `ast::Model`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TemplateStringId(u32);
impl std::ops::Index<TemplateStringId> for SchemaAst {
    type Output = TemplateString;

    fn index(&self, index: TemplateStringId) -> &Self::Output {
        self.tops[index.0 as usize].as_template_string().unwrap()
    }
}

/// An identifier for a top-level item in a schema AST. Use the `schema[top_id]`
/// syntax to resolve the id to an `ast::Top`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TopId {
    /// An enum declaration
    Enum(TypeExpId),

    // A class declaration
    Class(TypeExpId),

    // A function declaration
    Function(ValExpId),

    // A client declaration
    Client(ValExpId),

    // A generator declaration
    Generator(ValExpId),

    // Template Strings
    TemplateString(TemplateStringId),

    // A config block
    TestCase(ValExpId),

    RetryPolicy(ValExpId),
}

impl TopId {
    /// Try to interpret the top as an enum.
    pub fn as_enum_id(self) -> Option<TypeExpId> {
        match self {
            TopId::Enum(id) => Some(id),
            _ => None,
        }
    }

    /// Try to interpret the top as a class.
    pub fn as_class_id(self) -> Option<TypeExpId> {
        match self {
            TopId::Class(id) => Some(id),
            _ => None,
        }
    }

    /// Try to interpret the top as a function.
    pub fn as_function_id(self) -> Option<ValExpId> {
        match self {
            TopId::Function(id) => Some(id),
            _ => None,
        }
    }

    pub fn as_client_id(self) -> Option<ValExpId> {
        match self {
            TopId::Client(id) => Some(id),
            _ => None,
        }
    }

    pub fn as_template_string_id(self) -> Option<TemplateStringId> {
        match self {
            TopId::TemplateString(id) => Some(id),
            _ => None,
        }
    }

    pub fn as_retry_policy_id(self) -> Option<ValExpId> {
        match self {
            TopId::RetryPolicy(id) => Some(id),
            _ => None,
        }
    }

    pub fn as_test_case_id(self) -> Option<ValExpId> {
        match self {
            TopId::TestCase(id) => Some(id),
            _ => None,
        }
    }
}

impl std::ops::Index<TopId> for SchemaAst {
    type Output = Top;

    fn index(&self, index: TopId) -> &Self::Output {
        let idx = match index {
            TopId::Enum(TypeExpId(idx)) => idx,
            TopId::Class(TypeExpId(idx)) => idx,
            TopId::Function(ValExpId(idx)) => idx,
            TopId::TemplateString(TemplateStringId(idx)) => idx,
            TopId::Client(ValExpId(idx)) => idx,
            TopId::Generator(ValExpId(idx)) => idx,
            TopId::TestCase(ValExpId(idx)) => idx,
            TopId::RetryPolicy(ValExpId(idx)) => idx,
        };

        &self.tops[idx as usize]
    }
}

fn top_idx_to_top_id(top_idx: usize, top: &Top) -> TopId {
    match top {
        Top::Enum(_) => TopId::Enum(TypeExpId(top_idx as u32)),
        Top::Class(_) => TopId::Class(TypeExpId(top_idx as u32)),
        Top::Function(_) => TopId::Function(ValExpId(top_idx as u32)),
        Top::Client(_) => TopId::Client(ValExpId(top_idx as u32)),
        Top::TemplateString(_) => TopId::TemplateString(TemplateStringId(top_idx as u32)),
        Top::Generator(_) => TopId::Generator(ValExpId(top_idx as u32)),
        Top::TestCase(_) => TopId::TestCase(ValExpId(top_idx as u32)),
        Top::RetryPolicy(_) => TopId::RetryPolicy(ValExpId(top_idx as u32)),
    }
}
