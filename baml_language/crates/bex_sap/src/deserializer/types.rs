use std::{borrow::Cow, collections::HashSet};

use serde_json::json;

use super::{
    coercer::ParsingError,
    deserialize_flags::{DeserializerConditions, Flag},
    score::WithScore,
};
use crate::{
    baml_value::{BamlValue, BamlValueWithMeta, ValueWithMeta},
    sap_model::{TyResolvedRef, TyWithMeta, TypeAnnotations, TypeIdent, TypeName},
};

/// Metadata on values produced by the deserializer.
#[derive(Clone)]
pub struct DeserializerMeta<'s, 'v, 't, N: TypeIdent>
where
    's: 'v,
{
    pub flags: DeserializerConditions<'s, 'v, 't, N>,
    /// The type that was deserialized to produce this value.
    ///
    /// May also be a subtype of the expected type.
    pub ty: TyWithMeta<TyResolvedRef<'t, N>, &'t TypeAnnotations<'t, N>>,
}
impl<'t, N: TypeIdent> DeserializerMeta<'_, '_, 't, N> {
    pub fn new(
        ty: TyWithMeta<impl Into<TyResolvedRef<'t, N>>, &'t TypeAnnotations<'t, N>>,
    ) -> Self {
        Self {
            flags: DeserializerConditions::new(),
            ty: ty.map_ty(Into::into),
        }
    }
}

pub type ValueWithFlags<'s, 'v, 't, T, N> = ValueWithMeta<T, DeserializerMeta<'s, 'v, 't, N>>;
pub type BamlValueWithFlags<'s, 'v, 't, N> =
    ValueWithFlags<'s, 'v, 't, BamlValue<'s, 'v, 't, N>, N>;

impl<N: TypeIdent> serde::Serialize for BamlValueWithFlags<'_, '_, '_, N> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.value.serialize(serializer)
    }
}

impl<N: TypeIdent> std::fmt::Debug for BamlValueWithFlags<'_, '_, '_, N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.value {
            BamlValue::String(s) => f.debug_tuple("String").field(&s.value).finish(),
            BamlValue::Int(i) => f.debug_tuple("Int").field(&i.value).finish(),
            BamlValue::Bigint(bi) => f
                .debug_tuple("Bigint")
                .field(&bi.value.to_string())
                .finish(),
            BamlValue::Float(fl) => f.debug_tuple("Float").field(&fl.value).finish(),
            BamlValue::Bool(b) => f.debug_tuple("Bool").field(&b.value).finish(),
            BamlValue::Array(arr) => f
                .debug_struct("List")
                .field("type", &self.meta.ty.type_name().as_ref())
                .field("flags", &self.meta.flags)
                .field("items", &arr.value)
                .finish(),
            BamlValue::Map(map) => f
                .debug_struct("Map")
                .field("type", &self.meta.ty.type_name().as_ref())
                .field("flags", &self.meta.flags)
                .field("entries", &map.value)
                .finish(),
            BamlValue::Enum(e) => f
                .debug_struct("Enum")
                .field("name", &e.name.to_string())
                .field("value", &e.value)
                .field("flags", &self.meta.flags)
                .finish(),
            BamlValue::Class(c) => f
                .debug_struct("Class")
                .field("name", &c.name.to_string())
                .field("flags", &self.meta.flags)
                .field("fields", &c.value)
                .finish(),
            BamlValue::Null(_) => f
                .debug_struct("Null")
                .field("flags", &self.meta.flags)
                .finish(),
            BamlValue::Media(_) => f
                .debug_struct("Media")
                .field("flags", &self.meta.flags)
                .finish(),
            BamlValue::StreamState(_) => f
                .debug_struct("StreamState")
                .field("flags", &self.meta.flags)
                .finish(),
        }
    }
}

impl<'s, 'v, 't, N: TypeIdent> BamlValueWithFlags<'s, 'v, 't, N> {
    pub fn is_composite(&self) -> bool {
        matches!(
            &self.value,
            BamlValue::Array(_) | BamlValue::Map(_) | BamlValue::Class(_) | BamlValue::Media(_)
        )
    }

    pub fn score(&self) -> i32 {
        let base = self.meta.flags.score();
        match &self.value {
            BamlValue::Array(arr) => {
                base + arr
                    .value
                    .iter()
                    .map(crate::baml_value::ValueWithMeta::score)
                    .sum::<i32>()
            }
            BamlValue::Map(map) => base + map.value.iter().map(|(_, v)| v.score()).sum::<i32>(),
            BamlValue::Class(cls) => base + cls.value.iter().map(|(_, v)| v.score()).sum::<i32>(),
            _ => base,
        }
    }

    pub fn conditions(&self) -> &DeserializerConditions<'s, 'v, 't, N> {
        &self.meta.flags
    }
}

impl<'s, 'v, 't, N: TypeIdent> From<BamlValueWithFlags<'s, 'v, 't, N>>
    for BamlValueWithMeta<'s, 'v, 't, Vec<Flag<'s, 'v, 't, N>>, N>
{
    fn from(baml_value: BamlValueWithFlags<'s, 'v, 't, N>) -> Self {
        baml_value.map_meta(|meta| meta.flags.flags)
    }
}

pub trait ParsingErrorToUiJson {
    fn to_ui_json(&self) -> serde_json::Value;
}

impl ParsingErrorToUiJson for ParsingError {
    fn to_ui_json(&self) -> serde_json::Value {
        json!({
            if self.scope.is_empty() {
                "<root>".to_string()
            } else {
                self.scope.join(".")
            }: self.reason,
            "causes": self.causes.iter().map(ParsingErrorToUiJson::to_ui_json).collect::<Vec<_>>(),
        })
    }
}

#[allow(clippy::must_use_candidate)]
impl<'s, 'v, 't, T, N: TypeIdent> ValueWithFlags<'s, 'v, 't, T, N> {
    #[must_use]
    pub fn with_flag(mut self, flag: Flag<'s, 'v, 't, N>) -> Self {
        self.meta.flags.add_flag(flag);
        self
    }

    #[must_use]
    pub fn with_flags(mut self, flags: impl IntoIterator<Item = Flag<'s, 'v, 't, N>>) -> Self {
        self.meta.flags.flags.extend(flags);
        self
    }

    pub fn add_flag(&mut self, flag: Flag<'s, 'v, 't, N>) {
        self.meta.flags.add_flag(flag);
    }
}

impl<N: TypeIdent> BamlValueWithFlags<'_, '_, '_, N> {
    pub(super) fn r#type(&self) -> Cow<'static, str> {
        match &self.value {
            BamlValue::String(..) => Cow::Borrowed("String"),
            BamlValue::Int(..) => Cow::Borrowed("Int"),
            BamlValue::Bigint(..) => Cow::Borrowed("Bigint"),
            BamlValue::Float(..) => Cow::Borrowed("Float"),
            BamlValue::Bool(..) => Cow::Borrowed("Bool"),
            BamlValue::Array(arr) => {
                #[allow(clippy::redundant_closure_for_method_calls)]
                let inner = arr
                    .value
                    .iter()
                    .map(|i| i.r#type())
                    .collect::<HashSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>()
                    .join(" | ");
                Cow::Owned(format!("List[{}:{inner}]", arr.value.len()))
            }
            BamlValue::Map(..) => Cow::Borrowed("Map"),
            BamlValue::Enum(e) => Cow::Owned(format!("Enum {}", e.name)),
            BamlValue::Class(c) => Cow::Owned(format!("Class {}", c.name)),
            BamlValue::Null(..) => Cow::Borrowed("Null"),
            BamlValue::Media(..) => Cow::Borrowed("Image"),
            BamlValue::StreamState(..) => Cow::Borrowed("StreamState"),
        }
    }
}

impl<N: TypeIdent> std::fmt::Display for BamlValueWithFlags<'_, '_, '_, N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (Score: {}): ", self.r#type(), self.score())?;
        match &self.value {
            BamlValue::String(s) => {
                write!(f, "{}", s.value)?;
            }
            BamlValue::Int(i) => {
                write!(f, "{}", i.value)?;
            }
            BamlValue::Bigint(bi) => {
                write!(f, "{}", bi.value)?;
            }
            BamlValue::Float(fl) => {
                write!(f, "{}", bex_vm_types::format_float(fl.value))?;
            }
            BamlValue::Bool(b) => {
                write!(f, "{}", b.value)?;
            }
            BamlValue::Array(arr) => {
                writeln!(f)?;
                for (idx, item) in arr.value.iter().enumerate() {
                    writeln!(f, "  {idx}: {}", item.to_string().replace('\n', "  \n"))?;
                }
            }
            BamlValue::Map(map) => {
                writeln!(f)?;
                for (key, val) in &map.value {
                    writeln!(f, "{key}: {val}")?;
                }
            }
            BamlValue::Enum(e) => {
                write!(f, "{}", e.value)?;
            }
            BamlValue::Class(cls) => {
                writeln!(f)?;
                for (k, v) in &cls.value {
                    writeln!(f, "  KV {}", k.to_string().replace('\n', "\n  "))?;
                    writeln!(f, "  {}", v.to_string().replace('\n', "\n  "))?;
                }
            }
            BamlValue::Null(_) => {
                write!(f, "null")?;
            }
            BamlValue::Media(_) => {
                write!(f, "Media")?;
            }
            BamlValue::StreamState(_) => {
                write!(f, "StreamState")?;
            }
        }
        if !self.meta.flags.flags.is_empty() {
            write!(
                f,
                "\n  {}",
                self.meta.flags.to_string().replace('\n', "\n  ")
            )?;
        }
        Ok(())
    }
}
