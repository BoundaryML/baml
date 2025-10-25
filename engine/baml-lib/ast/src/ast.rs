mod argument;
mod assignment;
mod attribute;

mod comment;
mod config;

pub mod expr;
mod expression;
mod field;
mod stmt;

mod app;
mod identifier;
mod indentation_type;
mod newline_type;

mod template_string;
mod top;
mod traits;
mod type_builder_block;
mod type_expression_block;
mod value_expression_block;

mod baml_vis;
pub use app::App;
pub use argument::{Argument, ArgumentId, ArgumentsList};
pub use assignment::Assignment;
pub use attribute::{Attribute, AttributeContainer, AttributeId};
pub use baml_vis::{diagram_generator, mermaid_debug::MermaidDiagramGenerator};
pub use config::ConfigBlockProperty;
pub use expr::{ExprFn, TopLevelAssignment};
pub use expression::{
    BinaryOperator, ClassConstructor, ClassConstructorField, Expression, ExpressionBlock,
    RawString, UnaryOperator,
};
pub use field::{Field, FieldArity, FieldType};
pub use identifier::{Identifier, RefIdentifier};
pub use indentation_type::IndentationType;
pub use internal_baml_diagnostics::Span;
pub use newline_type::NewlineType;
pub use stmt::{
    AssertStmt, AssignOp, AssignOpStmt, AssignStmt, BreakStmt, CForLoopStmt, ContinueStmt,
    ExprStmt, ForLoopStmt, Header, LetStmt, ReturnStmt, Stmt, WatchNotifyStmt, WatchOptionsStmt,
    WhileStmt,
};
pub use template_string::TemplateString;
pub use top::Top;
pub use traits::{WithAttributes, WithDocumentation, WithIdentifier, WithName, WithSpan};
pub use type_builder_block::{TypeBuilderBlock, TypeBuilderEntry, DYNAMIC_TYPE_NAME_PREFIX};
pub use type_expression_block::{FieldId, SubType, TypeExpressionBlock};
pub use value_expression_block::{BlockArg, BlockArgs, ValueExprBlock, ValueExprBlockType};

pub(crate) use self::comment::Comment;

/// AST representation of the Baml source code.
///
/// This module is used internally to represent an AST (Abstract Syntax Tree).
/// The AST's nodes can be used during validation.
///
/// The AST is not validated, also fields and attributes are not resolved. Every
/// node is annotated with its location in the text representation.
#[derive(Debug, Clone)]
pub struct Ast {
    /// All function defs, class defs, enum defs, etc.
    pub tops: Vec<Top>,
}

impl Default for Ast {
    fn default() -> Self {
        Self::new()
    }
}

impl Ast {
    pub fn new() -> Self {
        Ast { tops: Vec::new() }
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

impl From<u32> for ValExpId {
    fn from(id: u32) -> Self {
        ValExpId(id)
    }
}

impl From<u32> for ExprFnId {
    fn from(id: u32) -> Self {
        ExprFnId(id)
    }
}

impl std::ops::Index<TypeExpId> for Ast {
    type Output = TypeExpressionBlock;

    fn index(&self, index: TypeExpId) -> &Self::Output {
        self.tops[index.0 as usize]
            .as_type_expression()
            .expect("expected type expression")
    }
}

/// An opaque identifier for a type alias in a schema AST.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeAliasId(u32);

impl std::ops::Index<TypeAliasId> for Ast {
    type Output = Assignment;

    fn index(&self, index: TypeAliasId) -> &Self::Output {
        self.tops[index.0 as usize]
            .as_type_alias_assignment()
            .expect("expected type expression")
    }
}

/// An opaque identifier for a top-level assignment in a schema AST.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TopLevelAssignmentId(u32);

impl std::ops::Index<TopLevelAssignmentId> for Ast {
    type Output = TopLevelAssignment;

    fn index(&self, index: TopLevelAssignmentId) -> &Self::Output {
        self.tops[index.0 as usize]
            .as_top_level_assignment()
            .expect("expected top level assignment")
    }
}

/// An opaque identifier for an expression function in a schema AST.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExprFnId(u32);

impl std::ops::Index<ExprFnId> for Ast {
    type Output = ExprFn;

    fn index(&self, index: ExprFnId) -> &Self::Output {
        self.tops[index.0 as usize]
            .as_expr_fn()
            .expect("expected expression function")
    }
}

/// An opaque identifier for a model in a schema AST. Use the
/// `schema[model_id]` syntax to resolve the id to an `ast::Model`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ValExpId(u32);
impl std::ops::Index<ValExpId> for Ast {
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
impl std::ops::Index<TemplateStringId> for Ast {
    type Output = TemplateString;

    fn index(&self, index: TemplateStringId) -> &Self::Output {
        self.tops[index.0 as usize].as_template_string().unwrap()
    }
}

/// An identifier for a top-level item in a schema AST. Use the `schema[top_id]`
/// syntax to resolve the id to an `ast::Top`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TopId {
    /// An enum declaration.
    Enum(TypeExpId),

    /// A class declaration.
    Class(TypeExpId),

    /// A function declaration.
    Function(ValExpId),

    /// A type alias declaration.
    TypeAlias(TypeAliasId),

    /// A client declaration.
    Client(ValExpId),

    /// A generator declaration.
    Generator(ValExpId),

    /// Template Strings.
    TemplateString(TemplateStringId),

    /// A config block.
    TestCase(ValExpId),

    RetryPolicy(ValExpId),

    /// A top-level assignment.
    TopLevelAssignment(TopLevelAssignmentId),

    /// A function declaration.
    ExprFn(ExprFnId),
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

    /// Try to interpret the top as a type alias.
    pub fn as_type_alias_id(self) -> Option<TypeAliasId> {
        match self {
            TopId::TypeAlias(id) => Some(id),
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

    pub fn as_generator_id(self) -> Option<ValExpId> {
        match self {
            TopId::Generator(id) => Some(id),
            _ => None,
        }
    }

    pub fn as_template_string_id(self) -> Option<TemplateStringId> {
        match self {
            TopId::TemplateString(id) => Some(id),
            _ => None,
        }
    }

    pub fn as_toplevel_assignment_id(self) -> Option<TopLevelAssignmentId> {
        match self {
            TopId::TopLevelAssignment(id) => Some(id),
            _ => None,
        }
    }

    pub fn as_expr_fn_id(self) -> Option<ExprFnId> {
        match self {
            TopId::ExprFn(id) => Some(id),
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
impl std::ops::Index<TopId> for Ast {
    type Output = Top;

    fn index(&self, index: TopId) -> &Self::Output {
        let idx = match index {
            TopId::Enum(TypeExpId(idx)) => idx,
            TopId::Class(TypeExpId(idx)) => idx,
            TopId::TypeAlias(TypeAliasId(idx)) => idx,
            TopId::Function(ValExpId(idx)) => idx,
            TopId::TemplateString(TemplateStringId(idx)) => idx,
            TopId::Client(ValExpId(idx)) => idx,
            TopId::Generator(ValExpId(idx)) => idx,
            TopId::TestCase(ValExpId(idx)) => idx,
            TopId::RetryPolicy(ValExpId(idx)) => idx,
            TopId::TopLevelAssignment(TopLevelAssignmentId(idx)) => idx,
            TopId::ExprFn(ExprFnId(idx)) => idx,
        };

        &self.tops[idx as usize]
    }
}

fn top_idx_to_top_id(top_idx: usize, top: &Top) -> TopId {
    match top {
        Top::Enum(_) => TopId::Enum(TypeExpId(top_idx as u32)),
        Top::Class(_) => TopId::Class(TypeExpId(top_idx as u32)),
        Top::Function(_) => TopId::Function(ValExpId(top_idx as u32)),
        Top::TypeAlias(_) => TopId::TypeAlias(TypeAliasId(top_idx as u32)),
        Top::Client(_) => TopId::Client(ValExpId(top_idx as u32)),
        Top::TemplateString(_) => TopId::TemplateString(TemplateStringId(top_idx as u32)),
        Top::Generator(_) => TopId::Generator(ValExpId(top_idx as u32)),
        Top::TestCase(_) => TopId::TestCase(ValExpId(top_idx as u32)),
        Top::RetryPolicy(_) => TopId::RetryPolicy(ValExpId(top_idx as u32)),
        Top::TopLevelAssignment(_) => {
            TopId::TopLevelAssignment(TopLevelAssignmentId(top_idx as u32))
        }
        Top::ExprFn(_) => TopId::ExprFn(ExprFnId(top_idx as u32)),
    }
}
