use serde::{Deserialize, Serialize};

mod constraint;
pub mod expr;
mod map;
mod media;
mod minijinja;
pub mod tracing;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FunctionFlavor {
    Llm,
    Expr,
}

pub mod baml_value;
mod generator;
pub mod ir_type;
mod value_expr;

pub use baml_value::{BamlValue, BamlValueWithMeta, Completion, CompletionState};
pub use constraint::*;
pub use generator::{GeneratorDefaultClientMode, GeneratorOutputType};
pub use ir_type::{
    type_meta, Arrow, HasType, LiteralValue, StreamingMode, ToUnionName, TypeIR, TypeValue,
    UnionType, UnionTypeView,
};
pub use map::Map as BamlMap;
pub use media::{BamlMedia, BamlMediaContent, BamlMediaType, MediaBase64, MediaFile, MediaUrl};
pub use minijinja::JinjaExpression;
pub use value_expr::{
    ApiKeyWithProvenance, EvaluationContext, GetEnvVar, Resolvable, ResolvedValue, StringOr,
    TemplateStringRenderer, UnresolvedValue,
};
