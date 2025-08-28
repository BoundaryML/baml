use std::fmt::{self, Formatter};

use super::{type_meta, ConstraintLevel, TypeGeneric};
use crate::ir_type::UnionTypeViewGeneric;

/// ---------- 1. The helper that prints the *core* type string ----------
fn fmt_type_body<M>(ty: &TypeGeneric<M>, f: &mut Formatter<'_>) -> fmt::Result
where
    M: MetaSuffix,
{
    match ty {
        TypeGeneric::Top(_) => f.write_str("ANY"),
        TypeGeneric::Enum { name, .. } => write!(f, "{name}"),
        TypeGeneric::Class { name, mode, .. } => match mode {
            crate::StreamingMode::NonStreaming => write!(f, "{name}"),
            crate::StreamingMode::Streaming => write!(f, "Streaming.{name}"),
        },
        TypeGeneric::RecursiveTypeAlias { name, mode, .. } => match mode {
            crate::StreamingMode::NonStreaming => write!(f, "{name}"),
            crate::StreamingMode::Streaming => write!(f, "Streaming.{name}"),
        },
        TypeGeneric::Primitive(t, _) => write!(f, "{t}"),
        TypeGeneric::Literal(v, _) => write!(f, "{v}"),
        TypeGeneric::Union(choices, _) => {
            let view = choices.view();
            let res = match view {
                UnionTypeViewGeneric::Null => "null".to_owned(),
                UnionTypeViewGeneric::Optional(t) => format!("{t} | null"),
                UnionTypeViewGeneric::OneOf(types) => types
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" | "),
                UnionTypeViewGeneric::OneOfOptional(types) => {
                    let inner = types
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(" | ");
                    format!("{inner} | null")
                }
            };
            write!(f, "({res})")
        }
        TypeGeneric::Tuple(items, _) => write!(
            f,
            "({})",
            items
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        TypeGeneric::Map(k, v, _) => write!(f, "map<{k}, {v}>"),
        TypeGeneric::List(t, _) => write!(f, "{t}[]"),
        TypeGeneric::Arrow(arrow, _) => write!(
            f,
            "({}) -> {}",
            arrow
                .param_types
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", "),
            arrow.return_type
        ),
    }
}

/// ---------- 2. A tiny trait that says “add my meta-specific tags” ----------
pub trait MetaSuffix {
    /// Pushes *only* the extra suffixes this meta type needs.
    fn push_suffix(&self, buf: &mut String);
    fn constraints(&self) -> &[crate::Constraint];
}

/// • Non-streaming adds nothing
impl MetaSuffix for type_meta::NonStreaming {
    fn push_suffix(&self, _: &mut String) {}
    fn constraints(&self) -> &[crate::Constraint] {
        self.constraints.as_slice()
    }
}

/// • Streaming adds `done`/`with_state`
impl MetaSuffix for type_meta::Streaming {
    fn push_suffix(&self, buf: &mut String) {
        let type_meta::stream::StreamingBehavior { done, state } = self.streaming_behavior;
        if done {
            buf.push_str(" @stream.done");
        }
        if state {
            buf.push_str(" @stream.with_state");
        }
    }
    fn constraints(&self) -> &[crate::Constraint] {
        self.constraints.as_slice()
    }
}

/// • IR adds `done`/`not_null`/`with_state`
impl MetaSuffix for type_meta::IR {
    fn push_suffix(&self, buf: &mut String) {
        let type_meta::base::StreamingBehavior {
            done,
            needed,
            state,
        } = &self.streaming_behavior;
        if *done {
            buf.push_str(" @stream.done");
        }
        if *needed {
            buf.push_str(" @stream.not_null");
        }
        if *state {
            buf.push_str(" @stream.with_state");
        }
    }
    fn constraints(&self) -> &[crate::Constraint] {
        self.constraints.as_slice()
    }
}

/// ---------- 3. The one‐size-fits-all Display impl ----------
impl<M> std::fmt::Display for TypeGeneric<M>
where
    M: MetaSuffix, // meta knows how to add its tags
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        // a) print the body
        fmt_type_body(self, f)?;

        // b) build all suffixes
        let mut suffix = String::new();

        //   • constraints (same for every meta)
        for constraint in self.meta().constraints() {
            let lvl = match constraint.level {
                ConstraintLevel::Assert => "assert",
                ConstraintLevel::Check => "check",
            };
            let label = constraint
                .label
                .as_ref()
                .map(|l| format!("{l}, "))
                .unwrap_or_default();
            suffix.push_str(&format!(" @{lvl}({label}{{{{..}}}} )"));
        }

        //   • meta-specific ones
        self.meta().push_suffix(&mut suffix);

        // c) finally flush the suffix
        write!(f, "{suffix}")
    }
}
