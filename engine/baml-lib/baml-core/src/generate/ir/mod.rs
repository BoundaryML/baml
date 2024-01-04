mod generate;
mod repr;

pub use generate::to_ir;
pub(super) use repr::IntermediateRepr;

// Add aliases for the IR types
pub(super) type Enum = repr::Node<repr::Enum>;
pub(super) type Class = repr::Node<repr::Class>;
pub(super) type FieldType = repr::FieldType;
pub(super) type Expression = repr::Expression;
pub(super) type Function = repr::Node<repr::Function>;
pub(super) type FunctionArgs = repr::FunctionArgs;
