//! Contains reusable logic for matching string values against LLM output.
//!
//! Used mostly for matching enum variants or literal strings.

use std::cmp::Ordering;

use anyhow::Result;
use baml_types::FieldType;

use crate::{
    deserializer::{
        coercer::ParsingError,
        deserialize_flags::{DeserializerConditions, Flag},
        types::ValueWithFlags,
    },
    jsonish,
};

use super::ParsingContext;

pub(super) fn match_string(
    parsing_context: &ParsingContext,
    target: &FieldType,
    value: Option<&jsonish::Value>,
    candidates: &[(&str, Vec<String>)],
) -> Result<ValueWithFlags<String>, ParsingError> {
    // Get rid of nulls.
    let value = match value {
        None | Some(jsonish::Value::Null) => {
            return Err(parsing_context.error_unexpected_null(target));
        }
        Some(v) => v,
    };

    let mut flags = DeserializerConditions::new();

    let jsonish_string = match value {
        jsonish::Value::String(s) => s.clone(),
        jsonish::Value::AnyOf(_, s) => {
            flags.add_flag(Flag::ObjectToString(value.clone()));
            s.clone()
        }
        v => {
            flags.add_flag(Flag::ObjectToString(v.clone()));
            format!("{v}")
        }
    };

    let match_context = jsonish_string.trim();

    if let Some(string_match) = string_match_strategy(&match_context, &candidates, &mut flags) {
        return try_match_only_once(parsing_context, target, string_match, flags);
    }

    // Strip punctuation and try again.
    let match_context = strip_punctuation(match_context);

    let candidates = candidates
        .iter()
        .map(|(candidate, valid_values)| {
            (
                *candidate,
                valid_values.iter().map(|v| strip_punctuation(v)).collect(),
            )
        })
        .collect::<Vec<_>>();

    if let Some(string_match) = string_match_strategy(&match_context, &candidates, &mut flags) {
        return try_match_only_once(parsing_context, target, string_match, flags);
    }

    Err(parsing_context.error_unexpected_type(target, &value))
}

fn strip_punctuation(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect::<String>()
}

fn try_match_only_once(
    parsing_context: &ParsingContext<'_>,
    target: &FieldType,
    string_match: &str,
    flags: DeserializerConditions,
) -> Result<ValueWithFlags<String>, ParsingError> {
    if let Some(mismatch) = flags.flags.iter().find_map(|f| match f {
        Flag::StrMatchOneFromMany(options) => Some(options),
        _ => None,
    }) {
        return Err(parsing_context.error_too_many_matches(
            target,
            mismatch
                .iter()
                .map(|(count, string)| format!("{} ({} times)", string, count)),
        ));
    };

    Ok((string_match.to_string(), flags).into())
}

fn string_match_strategy<'c>(
    value_str: &str,
    candidates: &'c [(&'c str, Vec<String>)],
    flags: &mut DeserializerConditions,
) -> Option<&'c str> {
    // Try and look for an exact match against valid values.
    for (candidate, valid_values) in candidates {
        // Consider adding a flag for case insensitive match.
        if valid_values
            .iter()
            .any(|v| v.eq_ignore_ascii_case(value_str))
        {
            // We did nothing fancy, so no extra flags.
            return Some(candidate);
        }
    }

    // Now find all the candidates which occur in the value, by frequency.
    let mut result = candidates
        .iter()
        .filter_map(|(variant, valid_names)| {
            // Check how many counts of the variant are in the value.
            let match_count_pos = valid_names
                .iter()
                .filter_map(|valid_name| {
                    let matches = value_str.match_indices(valid_name);
                    // Return (count, first_idx)
                    matches.fold(None, |acc, (idx, _)| match acc {
                        Some((count, prev_idx)) => Some((count + 1, prev_idx)),
                        None => Some((1, idx)),
                    })
                })
                .reduce(|a, b| match a.0.cmp(&b.0) {
                    // Return the one with more matches.
                    Ordering::Less => b,
                    Ordering::Greater => a,
                    // Return the one that matches earlier
                    Ordering::Equal => match a.1.cmp(&b.1) {
                        Ordering::Less => a,
                        _ => b,
                    },
                });
            match_count_pos.map(|(count, pos)| (count, pos, variant))
        })
        .collect::<Vec<_>>();

    // Sort by max count, then min pos.
    result.sort_by(|a, b| match a.0.cmp(&b.0) {
        Ordering::Less => Ordering::Greater,
        Ordering::Greater => Ordering::Less,
        Ordering::Equal => a.1.cmp(&b.1),
    });

    // Filter for max count.
    let max_count = result.first().map(|r| r.0).unwrap_or(0);
    result.retain(|r| r.0 == max_count);

    // Return the best match if there is one.
    if let Some((_, _, candidate)) = result.first() {
        flags.add_flag(Flag::SubstringMatch(value_str.into()));

        // Add flag for multiple matches.
        if result.len() > 1 {
            flags.add_flag(Flag::StrMatchOneFromMany(
                result
                    .iter()
                    .map(|(count, _, candidate)| ((*count) as usize, candidate.to_string()))
                    .collect(),
            ));
        }

        return Some(candidate);
    }

    // No match found.
    None
}
