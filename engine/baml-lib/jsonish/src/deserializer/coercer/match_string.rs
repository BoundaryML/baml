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

/// Heuristic match of different possible values against an input string.
pub(super) fn match_string(
    parsing_context: &ParsingContext,
    target: &FieldType,
    value: Option<&jsonish::Value>,
    // List of (name, [aliases]) tuples.
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

    // Grab context.
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

    // Trim whitespaces.
    let match_context = jsonish_string.trim();

    // First attempt, case sensitive match ignoring possible pucntuation.
    if let Some(string_match) = string_match_strategy(&match_context, &candidates, &mut flags) {
        return try_match_only_once(parsing_context, target, string_match, flags);
    }

    // Strip punctuation and try again.
    let match_context = strip_punctuation(match_context);

    // TODO: If the candidates don't contain any punctuation themselves there's
    // no point in removing the punctuation from the input string and running
    // the entire algorithm again because it should've already matched the
    // substrings in the previous attempt. This can be optimized.
    let mut candidates = Vec::from_iter(candidates.iter().map(|(candidate, valid_values)| {
        let stripped_valid_values = valid_values.iter().map(|v| strip_punctuation(v)).collect();
        (*candidate, stripped_valid_values)
    }));

    // Second attempt, case sensitive match without punctuation.
    if let Some(string_match) = string_match_strategy(&match_context, &candidates, &mut flags) {
        return try_match_only_once(parsing_context, target, string_match, flags);
    }

    // Last hope, case insensitive match without punctuation. This could yield
    // wrong results since the name of a candidate could appear as a "normal"
    // word used by the LLM to explain the output.
    let match_context = match_context.to_lowercase();

    // TODO: Consider adding a flag for case insensitive match.
    candidates.iter_mut().for_each(|(_, valid_values)| {
        valid_values.iter_mut().for_each(|v| *v = v.to_lowercase());
    });

    // There goes our last hope :)
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

/// Helper function to return a single string match result.
///
/// Multiple results will yield an error.
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
                .map(|(count, string)| format!("{string} ({count} times)")),
        ));
    };

    Ok((string_match.to_string(), flags).into())
}

/// Heuristic string match algorithm.
///
/// The algorithm is case sensitive so for case insensitive matches it must
/// recieve lowercase strings. This algorithm will first try to look for exact
/// matches in the input string, if it doesn't find any it will look for
/// substring matches and return the one with the most matches. Whether that is
/// an ambigous match or not is up to the caller to decide.
fn string_match_strategy<'c>(
    value_str: &str,
    candidates: &'c [(&'c str, Vec<String>)],
    flags: &mut DeserializerConditions,
) -> Option<&'c str> {
    // Try and look for an exact match against valid values.
    for (candidate, valid_values) in candidates {
        if valid_values.iter().any(|v| v == value_str) {
            // We did nothing fancy, so no extra flags.
            return Some(candidate);
        }
    }

    // Now find all the candidates which occur in the value, by frequency.
    let mut result = Vec::from_iter(candidates.iter().filter_map(|(variant, valid_names)| {
        // Check how many counts of the variant are in the value.
        let match_count_pos = valid_names
            .iter()
            .filter_map(|valid_name| {
                // Match ocurrences of valid name.
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
    }));

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
