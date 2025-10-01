use baml_types::{BamlValueWithMeta, Completion, Constraint, ResponseCheck, TypeIR};

pub enum EmitBamlValue {
    Value(BamlValueWithMeta<EmitValueMetadata>),
    Block(String),
}

/// The BamlValueWithMeta metadata for a
/// BamlValue in an event.
pub struct EmitValueMetadata {
    pub constraints: Vec<Constraint>,
    pub response_checks: Vec<ResponseCheck>,
    pub completion: Completion,
    pub r#type: TypeIR,
}

pub struct EmitEvent {
    value: EmitBamlValue,
}
