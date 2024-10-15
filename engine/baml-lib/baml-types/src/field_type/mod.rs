use crate::BamlMediaType;
use crate::Constraint;
use crate::ConstraintLevel;

mod builder;

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub enum TypeValue {
    String,
    Int,
    Float,
    Bool,
    // Char,
    Null,
    Media(BamlMediaType),
}
impl TypeValue {
    pub fn from_str(s: &str) -> Option<TypeValue> {
        match s {
            "string" => Some(TypeValue::String),
            "int" => Some(TypeValue::Int),
            "float" => Some(TypeValue::Float),
            "bool" => Some(TypeValue::Bool),
            "null" => Some(TypeValue::Null),
            "image" => Some(TypeValue::Media(BamlMediaType::Image)),
            "audio" => Some(TypeValue::Media(BamlMediaType::Audio)),
            _ => None,
        }
    }
}
impl std::fmt::Display for TypeValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TypeValue::String => write!(f, "string"),
            TypeValue::Int => write!(f, "int"),
            TypeValue::Float => write!(f, "float"),
            TypeValue::Bool => write!(f, "bool"),
            TypeValue::Null => write!(f, "null"),
            TypeValue::Media(BamlMediaType::Image) => write!(f, "image"),
            TypeValue::Media(BamlMediaType::Audio) => write!(f, "audio"),
        }
    }
}

/// Subset of [`crate::BamlValue`] allowed for literal type definitions.
#[derive(serde::Serialize, Debug, Clone, PartialOrd, Ord, PartialEq, Eq)]
pub enum LiteralValue {
    String(String),
    Int(i64),
    Bool(bool),
}

impl LiteralValue {
    pub fn literal_base_type(&self) -> FieldType {
        match self {
            Self::String(_) => FieldType::string(),
            Self::Int(_) => FieldType::int(),
            Self::Bool(_) => FieldType::bool(),
        }
    }
}

impl std::fmt::Display for LiteralValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LiteralValue::String(str) => write!(f, "\"{str}\""),
            LiteralValue::Int(int) => write!(f, "{int}"),
            LiteralValue::Bool(bool) => write!(f, "{bool}"),
        }
    }
}

/// FieldType represents the type of either a class field or a function arg.
#[derive(serde::Serialize, Debug, Clone, PartialEq)]
pub enum FieldType {
    Primitive(TypeValue),
    Enum(String),
    Literal(LiteralValue),
    Class(String),
    List(Box<FieldType>),
    Map(Box<FieldType>, Box<FieldType>),
    Union(Vec<FieldType>),
    Tuple(Vec<FieldType>),
    Optional(Box<FieldType>),
    Constrained{ base: Box<FieldType>, constraints: Vec<Constraint> },
}

// Impl display for FieldType
impl std::fmt::Display for FieldType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FieldType::Enum(name) | FieldType::Class(name) => {
                write!(f, "{}", name)
            }
            FieldType::Primitive(t) => write!(f, "{}", t),
            FieldType::Literal(v) => write!(f, "{}", v),
            FieldType::Union(choices) => {
                write!(
                    f,
                    "({})",
                    choices
                        .iter()
                        .map(|t| t.to_string())
                        .collect::<Vec<_>>()
                        .join(" | ")
                )
            }
            FieldType::Tuple(choices) => {
                write!(
                    f,
                    "({})",
                    choices
                        .iter()
                        .map(|t| t.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            FieldType::Map(k, v) => write!(f, "map<{}, {}>", k.to_string(), v.to_string()),
            FieldType::List(t) => write!(f, "{}[]", t.to_string()),
            FieldType::Optional(t) => write!(f, "{}?", t.to_string()),
            FieldType::Constrained{base,..} => base.fmt(f),
        }
    }
}

impl FieldType {
    pub fn is_primitive(&self) -> bool {
        match self {
            FieldType::Primitive(_) => true,
            FieldType::Optional(t) => t.is_primitive(),
            FieldType::List(t) => t.is_primitive(),
            FieldType::Constrained{base,..} => base.is_primitive(),
            _ => false,
        }
    }

    pub fn is_optional(&self) -> bool {
        match self {
            FieldType::Optional(_) => true,
            FieldType::Primitive(TypeValue::Null) => true,
            FieldType::Union(types) => types.iter().any(FieldType::is_optional),
            FieldType::Constrained{base,..} => base.is_optional(),
            _ => false,
        }
    }

    pub fn is_null(&self) -> bool {
        match self {
            FieldType::Primitive(TypeValue::Null) => true,
            FieldType::Optional(t) => t.is_null(),
            FieldType::Constrained{base,..} => base.is_null(),
            _ => false,
        }
    }

    /// Eliminate the `FieldType::Constrained` variant by searching for it, and stripping
    /// it off of its base type, returning a tulpe of the base type and any constraints found
    /// (if called on an argument that is not Constrained, the returned constraints Vec is
    /// empty).
    ///
    /// If the function encounters directly nested Constrained types,
    /// (i.e. `FieldType::Constrained { base: FieldType::Constrained { .. }, .. } `)
    /// then the constraints of the two levels will be combined into a single vector.
    /// So, we always return a base type that is not FieldType::Constrained.
    pub fn distribute_constraints(self: &FieldType) -> (&FieldType, Vec<Constraint>) {

        match self {
            // Check the first level to see if it's constrained.
            FieldType::Constrained { base, constraints } => {
                match base.as_ref() {
                    // If so, we must check the second level to see if we need to combine
                    // constraints across levels.
                    // The recursion here means that arbitrarily nested `FieldType::Constrained`s
                    // will be collapsed before the function returns.
                    FieldType::Constrained{..} => {
                        let (sub_base, sub_constraints) = base.as_ref().distribute_constraints();
                        let combined_constraints = vec![constraints.clone(), sub_constraints].into_iter().flatten().collect();
                        (sub_base, combined_constraints)
                    },
                    _ => (base, constraints.clone()),
                }
            },
            _ => (self, Vec::new()),
        }
    }

    pub fn has_constraints(&self) -> bool {
        let (_, constraints) = self.distribute_constraints();
        !constraints.is_empty()
    }

    pub fn has_checks(&self) -> bool {
        let (_, constraints) = self.distribute_constraints();
        constraints.iter().any(|Constraint{level,..}| level == &ConstraintLevel::Check)
    }

}

#[cfg(test)]
mod tests {
    use crate::{Constraint, ConstraintLevel, JinjaExpression};
    use super::*;


    #[test]
    fn test_nested_constraint_distribution() {
        fn mk_constraint(s: &str) -> Constraint {
            Constraint { level: ConstraintLevel::Assert, expression: JinjaExpression(s.to_string()), label: Some(s.to_string()) }
        }

        let input = FieldType::Constrained {
            constraints: vec![mk_constraint("a")],
            base: Box::new(FieldType::Constrained {
                constraints: vec![mk_constraint("b")],
                base: Box::new(FieldType::Constrained {
                    constraints: vec![mk_constraint("c")],
                    base: Box::new(FieldType::Primitive(TypeValue::Int)),
                })
            })
        };

        let expected_base = FieldType::Primitive(TypeValue::Int);
        let expected_constraints = vec![mk_constraint("a"),mk_constraint("b"), mk_constraint("c")];

        let (base, constraints) = input.distribute_constraints();

        assert_eq!(base, &expected_base);
        assert_eq!(constraints, expected_constraints);
    }
}
