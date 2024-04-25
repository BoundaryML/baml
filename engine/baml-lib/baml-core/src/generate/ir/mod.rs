mod generate;
mod ir_helpers;
mod json_schema;
pub mod repr;
mod walker;

pub use generate::to_ir;
use internal_baml_schema_ast::ast;
pub use ir_helpers::{
    ClassWalker, ClientWalker, EnumWalker, FunctionWalker, IRHelper, RetryPolicyWalker,
    TemplateStringWalker,
};

pub(super) use json_schema::WithJsonSchema;
pub(super) use repr::IntermediateRepr;

// Add aliases for the IR types
pub type Enum = repr::Node<repr::Enum>;
pub type EnumValue = repr::Node<repr::EnumValue>;
pub type Class = repr::Node<repr::Class>;
pub(super) type FieldType = repr::FieldType;
pub(super) type Expression = repr::Expression;
pub(super) type Identifier = repr::Identifier;
pub(super) type TypeValue = ast::TypeValue;
pub type Function = repr::Node<repr::Function>;
#[allow(dead_code)]
pub(super) type FunctionV1 = repr::FunctionV1;
#[allow(dead_code)]
pub(super) type FunctionV2 = repr::FunctionV2;
pub(super) type FunctionArgs = repr::FunctionArgs;
pub(super) type Impl = repr::Node<repr::Implementation>;
pub type Client = repr::Node<repr::Client>;
pub type RetryPolicy = repr::Node<repr::RetryPolicy>;
pub type TemplateString = repr::Node<repr::TemplateString>;
pub(super) type TestCase = repr::Node<repr::TestCase>;
pub(super) type Walker<'db, I> = repr::Walker<'db, I>;

pub(super) type Prompt = repr::Prompt;
