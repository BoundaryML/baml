//! Enum schemas at map JSON boundaries. Validation precedes materialization.
use crate::{Dyn, json_lists::EnumVariant};
use serde_json::Value;

pub struct Schema {
    pub tag: u64,
    pub name: &'static str,
    pub variants: &'static [EnumVariant],
    pub aliased: bool,
    pub variant: Option<usize>,
}
impl Schema {
    fn key(&self, variant: &EnumVariant) -> &'static str {
        if self.aliased {
            variant.alias
        } else {
            variant.name
        }
    }
    pub(crate) fn check(&self, value: &Value) -> Result<(), String> {
        if let Some(index) = self.variant {
            let variant = &self.variants[index];
            if self.aliased && variant.skip {
                return Err(format!(
                    "enum variant `{}.{}` is `@skip`, so JSON has no name for it",
                    self.name, variant.name
                ));
            }
            let expected = self.key(variant);
            if value.as_str() != Some(expected) {
                return Err(format!("expected enum variant `{expected}`"));
            }
        }
        let actual = value.as_str().ok_or("expected enum variant string")?;
        if self
            .variants
            .iter()
            .any(|v| !(self.aliased && v.skip) && self.key(v) == actual)
        {
            Ok(())
        } else {
            Err(format!(
                "unknown variant `{actual}` for enum `{}`",
                self.name
            ))
        }
    }
    pub(crate) fn empty(&self) -> Option<Value> {
        // Private typed-default token. JSON numbers fail `check`, so this can
        // only originate from a skipped class field's empty value.
        self.variant
            .or_else(|| self.variants.iter().position(|v| !v.skip))
            .map(Value::from)
    }
    pub(crate) fn build(&self, value: &Value) -> Dyn {
        // A skipped class field is materialized from its typed default, which
        // may itself be a skipped enum variant. Its parent checked the input.
        let index = value.as_u64().map(|n| n as usize).unwrap_or_else(|| {
            self.variants
                .iter()
                .position(|v| {
                    !(self.aliased && v.skip)
                        && self.key(v) == value.as_str().expect("validated enum string")
                })
                .expect("validated enum variant")
        });
        Dyn {
            tag: self.tag,
            payload: index as u64,
        }
    }
}
