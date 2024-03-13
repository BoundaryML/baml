mod generate;
mod json_schema;
mod repr;
mod walker;

pub use generate::to_ir;
use internal_baml_schema_ast::ast;
pub(super) use json_schema::WithJsonSchema;
pub(super) use repr::IntermediateRepr;

// Add aliases for the IR types
pub(super) type Enum = repr::Node<repr::Enum>;
pub(super) type Class = repr::Node<repr::Class>;
pub(super) type FieldType = repr::FieldType;
pub(super) type Expression = repr::Expression;
pub(super) type Identifier = repr::Identifier;
pub(super) type TypeValue = ast::TypeValue;
pub(super) type Function = repr::Node<repr::Function>;
pub(super) type FunctionArgs = repr::FunctionArgs;
pub(super) type Impl = repr::Node<repr::Implementation>;
pub(super) type Client = repr::Node<repr::Client>;
pub(super) type TestCase = repr::Node<repr::TestCase>;
pub(super) type Walker<'db, I> = repr::Walker<'db, I>;
pub(super) type Prompt = repr::Prompt;
