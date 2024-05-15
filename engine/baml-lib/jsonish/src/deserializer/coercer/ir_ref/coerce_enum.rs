use anyhow::Result;
use internal_baml_core::ir::{EnumValueWalker, EnumWalker, FieldType};

use crate::deserializer::{
    coercer::{ParsingError, TypeCoercer},
    deserialize_flags::{DeserializerConditions, Flag},
    types::BamlValueWithFlags,
};

use super::ParsingContext;

fn candidates<'a>(
    enm: &'a EnumWalker<'a>,
    ctx: &ParsingContext,
) -> Result<Vec<(EnumValueWalker<'a>, Vec<String>)>> {
    enm.walk_values()
        .filter_map(|v| match v.skip(ctx.env) {
            Ok(true) => return None,
            Ok(false) => match v.valid_values(ctx.env) {
                Ok(valid_values) => Some(Ok((v, valid_values))),
                Err(e) => return Some(Err(e)),
            },
            Err(e) => return Some(Err(e)),
        })
        .collect()
}

impl TypeCoercer for EnumWalker<'_> {
    fn coerce(
        &self,
        ctx: &ParsingContext,
        target: &FieldType,
        value: Option<&crate::jsonish::Value>,
    ) -> Result<BamlValueWithFlags, ParsingError> {
        let value = match value {
            None | Some(crate::jsonish::Value::Null) => {
                // If the value is None, we can't parse it.
                return Err(ctx.error_unexpected_null(target));
            }
            Some(v) => v,
        };

        let mut flags = DeserializerConditions::new();

        let context = match value {
            crate::jsonish::Value::String(s) => s.clone(),
            crate::jsonish::Value::AnyOf(_, s) => {
                flags.add_flag(Flag::ObjectToString(value.clone()));
                s.clone()
            }
            v => {
                flags.add_flag(Flag::ObjectToString(v.clone()));
                format!("{}", v)
            }
        };

        let candidates = match candidates(self, ctx) {
            Ok(c) => c,
            Err(e) => return Err(ctx.error_internal(e)),
        };

        let context = context.trim();

        if let Some(e) = enum_match_strategy(&context, &candidates, &mut flags) {
            if let Some(mismatch) = flags.flags.iter().find_map(|f| match f {
                Flag::EnumOneFromMany(options) => Some(options),
                _ => None,
            }) {
                return Err(ctx.error_too_many_matches(
                    target,
                    mismatch
                        .iter()
                        .map(|(count, e)| format!("{} ({} times)", e, count)),
                ));
            }

            return Ok(BamlValueWithFlags::Enum(
                self.name().into(),
                (e.name().into(), flags).into(),
            ));
        }

        // Try to strip punctuation and try again.
        let context = strip_punctuation(context);
        let candidates = candidates
            .iter()
            .map(|(e, valid_values)| {
                (
                    *e,
                    valid_values.iter().map(|v| strip_punctuation(v)).collect(),
                )
            })
            .collect::<Vec<_>>();

        if let Some(e) = enum_match_strategy(&context, &candidates, &mut flags) {
            if let Some(mismatch) = flags.flags.iter().find_map(|f| match f {
                Flag::EnumOneFromMany(options) => Some(options),
                _ => None,
            }) {
                return Err(ctx.error_too_many_matches(target, mismatch.iter().map(|(_, e)| e)));
            }
            return Ok(BamlValueWithFlags::Enum(
                self.name().into(),
                (e.name().into(), flags).into(),
            ));
        }

        Err(ctx.error_unexpected_type(target, &value))
    }
}

fn strip_punctuation(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect::<String>()
}

fn enum_match_strategy<'a>(
    value_str: &str,
    candidates: &'a Vec<(EnumValueWalker<'a>, Vec<String>)>,
    flags: &mut DeserializerConditions,
) -> Option<&'a EnumValueWalker<'a>> {
    // Try and look for a value that matches the value.
    // First search for exact matches
    for (e, valid_values) in candidates {
        // Consider adding a flag for case insensitive match.
        if valid_values
            .iter()
            .any(|v| v.eq_ignore_ascii_case(value_str))
        {
            // We did nothing fancy, so no extra flags.
            return Some(e);
        }
    }

    // Now find all the enums which occur in the value, by frequency.
    let mut result = candidates
        .iter()
        .filter_map(|(e, valid_names)| {
            // Check how many counts of the enum are in the value.
            let match_count_pos = valid_names
                .iter()
                .filter_map(|v| {
                    let matches = value_str.match_indices(v);
                    // Return (count, first_idx)
                    matches.fold(None, |acc, (idx, _)| match acc {
                        Some((count, prev_idx)) => Some((count + 1, prev_idx)),
                        None => Some((1, idx)),
                    })
                })
                .reduce(|a, b| match a.0.cmp(&b.0) {
                    // Return the one with more matches.
                    std::cmp::Ordering::Less => b,
                    std::cmp::Ordering::Greater => a,
                    // Return the one that matches earlier
                    std::cmp::Ordering::Equal => match a.1.cmp(&b.1) {
                        std::cmp::Ordering::Less => a,
                        _ => b,
                    },
                });
            match_count_pos.map(|(count, pos)| (count, pos, e))
        })
        .collect::<Vec<_>>();

    // Sort by max count, then min pos.
    result.sort_by(|a, b| match a.0.cmp(&b.0) {
        std::cmp::Ordering::Less => std::cmp::Ordering::Greater,
        std::cmp::Ordering::Greater => std::cmp::Ordering::Less,
        std::cmp::Ordering::Equal => a.1.cmp(&b.1),
    });

    // Filter for max count.
    let max_count = result.first().map(|r| r.0).unwrap_or(0);
    result.retain(|r| r.0 == max_count);

    // Return the best match if there is one.
    if let Some((_, _, e)) = result.first() {
        flags.add_flag(Flag::SubstringMatch(value_str.into()));

        if result.len() > 1 {
            // Let remaining options are:

            flags.add_flag(Flag::EnumOneFromMany(
                result
                    .iter()
                    .map(|(count, _, e)| ((*count) as usize, e.name().into()))
                    .collect(),
            ));
        }

        return Some(e);
    }

    None
}
