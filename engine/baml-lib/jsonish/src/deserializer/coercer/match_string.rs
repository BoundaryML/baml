//! Contains reusable logic for matching string values against LLM output.
//!
//! Used mostly for matching enum variants or literal strings.

use std::{cmp::Ordering, collections::HashMap};

use anyhow::Result;
use baml_types::{TypeIR, TypeValue};

use super::ParsingContext;
use crate::{
    deserializer::{
        coercer::ParsingError,
        deserialize_flags::{DeserializerConditions, Flag},
        types::ValueWithFlags,
    },
    jsonish,
};

pub(super) fn matches_string_to_string(
    parsing_context: &ParsingContext,
    raw_value: &str,
    parse_into: &str,
) -> bool {
    match_string(
        parsing_context,
        &TypeIR::string(),
        Some(&crate::jsonish::Value::String(
            raw_value.to_string(),
            baml_types::CompletionState::Complete,
        )),
        &[(parse_into, vec![parse_into.to_string()])],
        false,
    )
    .is_ok()
}

/// Heuristic match of different possible values against an input string.
pub(super) fn match_string(
    parsing_context: &ParsingContext,
    target: &TypeIR,
    value: Option<&jsonish::Value>,
    // List of (name, [aliases]) tuples.
    candidates: &[(&str, Vec<String>)],
    allow_substring_match: bool,
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
        jsonish::Value::String(s, _) => s.clone(),
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
    if let Some(string_match) =
        string_match_strategy(match_context, candidates, &mut flags, allow_substring_match)
    {
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
    if let Some(string_match) = string_match_strategy(
        &match_context,
        &candidates,
        &mut flags,
        allow_substring_match,
    ) {
        return try_match_only_once(parsing_context, target, string_match, flags);
    }

    // Third attempt, case sensitive match without punctuation.
    if let Some(string_match) = string_match_strategy(
        &match_context,
        &candidates,
        &mut flags,
        allow_substring_match,
    ) {
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
    if let Some(string_match) = string_match_strategy(
        &match_context,
        &candidates,
        &mut flags,
        allow_substring_match,
    ) {
        return try_match_only_once(parsing_context, target, string_match, flags);
    }

    Err(parsing_context.error_unexpected_type(target, &value))
}

fn strip_punctuation(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect::<String>()
}

/// Remove accents from characters to enable fuzzy matching of unaccented input
/// against accented aliases/candidates.
fn remove_accents(s: &str) -> String {
    use unicode_normalization::UnicodeNormalization;

    // Handle ligatures separately since they're not combining marks
    let s = s
        .replace('ß', "ss")
        .replace('æ', "ae")
        .replace('Æ', "AE")
        .replace('ø', "o")
        .replace('Ø', "O")
        .replace('œ', "oe")
        .replace('Œ', "OE");

    s.nfkd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .collect()
}

/// Helper function to return a single string match result.
///
/// Multiple results will yield an error.
fn try_match_only_once(
    parsing_context: &ParsingContext<'_>,
    target: &TypeIR,
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
                .map(|(string, count)| format!("{string} ({count} times)")),
        ));
    };

    Ok((string_match.to_string(), target, flags).into())
}

/// Heuristic string match algorithm.
///
/// The algorithm is case sensitive so for case insensitive matches it must
/// receive lowercase strings. This algorithm will first try to look for exact
/// matches in the input string, if it doesn't find any it will look for
/// substring matches and return the one with the most matches. Whether that is
/// an ambiguous match or not is up to the caller to decide.
fn string_match_strategy<'c>(
    value_str: &str,
    candidates: &'c [(&'c str, Vec<String>)],
    flags: &mut DeserializerConditions,
    allow_substring_match: bool,
) -> Option<&'c str> {
    log::debug!("string_match_strategy: {value_str}");
    log::debug!(
        "candidates:\n{}",
        candidates
            .iter()
            .map(|(c, v)| format!(
                "{c} -> {}",
                v.iter()
                    .map(|v| format!("\"{v}\""))
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
            .collect::<Vec<_>>()
            .join("\n")
    );
    // Strategy 1: Try exact case-sensitive match
    for (candidate, valid_values) in candidates {
        if valid_values.iter().any(|v| v == value_str) {
            // No flags since we found an exact match.
            return Some(candidate);
        }
    }

    // Strategy 2: Try unaccented case-sensitive match
    let unaccented_value_str = remove_accents(value_str);
    for (candidate, valid_values) in candidates {
        if valid_values
            .iter()
            .any(|v| remove_accents(v) == unaccented_value_str)
        {
            // No flags since we found an exact match.
            return Some(candidate);
        }
    }

    if !allow_substring_match {
        return None;
    }

    // (start_index, end_index, valid_name, variant)
    // TODO: Consider using a struct with named fields instead of a 4-tuple.
    let mut all_matches: Vec<(usize, usize, &'c str, &'c str)> = Vec::new();

    // Look for substrings of valid values
    for (variant, valid_names) in candidates {
        for valid_name in valid_names {
            for (start_idx, _) in value_str.match_indices(valid_name) {
                let end_idx = start_idx + valid_name.len();
                all_matches.push((start_idx, end_idx, valid_name, variant));
            }
        }
    }

    // No substring match at all for any variant, early return.
    if all_matches.is_empty() {
        // Try to see if we can find any substring matches in the unaccented
        // value string.
        for (variant, valid_names) in candidates {
            for valid_name in valid_names {
                let unaccented_valid_name = remove_accents(valid_name);
                for (start_idx, _) in unaccented_value_str.match_indices(&unaccented_valid_name) {
                    let end_idx = start_idx + valid_name.len();
                    all_matches.push((start_idx, end_idx, valid_name, variant));
                }
            }
        }
    }

    // No substring match at all for any variant, early return.
    if all_matches.is_empty() {
        return None;
    }

    // Sort by position and length
    all_matches.sort_by(|a, b| {
        match a.0.cmp(&b.0) {
            Ordering::Equal => b.1.cmp(&a.1), // Longer first
            ordering => ordering,             // Less or Greater stays the same
        }
    });

    // Filter out overlapping matches
    let mut filtered_matches = Vec::new();
    let mut last_end = 0;

    for current_match in all_matches {
        if current_match.0 >= last_end {
            // No overlap with previous match
            last_end = current_match.1;
            filtered_matches.push(current_match);
        }
    }

    // Count occurrences of each variant in non-overlapping matches.
    // (count, variant)
    let mut variant_counts = HashMap::<&'c str, usize>::new();
    for (_, _, _, variant) in &filtered_matches {
        if let Some(count) = variant_counts.get_mut(*variant) {
            // Increment count if variant already exists.
            *count += 1;
        } else {
            // Add new variant.
            variant_counts.insert(variant, 1);
        }
    }

    // Return the best match if there is one
    if let Some((best_match, max_count)) = variant_counts
        .iter()
        .max_by(|(_, count_a), (_, count_b)| count_a.cmp(count_b))
    {
        flags.add_flag(Flag::SubstringMatch(value_str.into()));

        // Find all variants with the same count
        let ties: Vec<_> = variant_counts
            .iter()
            .filter(|(_, count)| *count == max_count)
            .map(|(variant, count)| (variant.to_string(), *count))
            .collect();

        // If there are multiple matches, add a flag
        if ties.len() > 1 {
            flags.add_flag(Flag::StrMatchOneFromMany(ties));
        }

        return Some(best_match);
    }

    // No match found.
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remove_accents_etude() {
        assert_eq!(remove_accents("étude"), "etude");
    }

    #[test]
    fn test_remove_accents_francais() {
        assert_eq!(remove_accents("français"), "francais");
    }

    #[test]
    fn test_remove_accents_espanol() {
        assert_eq!(remove_accents("Español"), "Espanol");
    }

    #[test]
    fn test_remove_accents_portugues() {
        assert_eq!(remove_accents("português"), "portugues");
    }

    #[test]
    fn test_remove_accents_medium() {
        assert_eq!(remove_accents("médium"), "medium");
    }

    #[test]
    fn test_remove_accents_grun() {
        assert_eq!(remove_accents("Grün"), "Grun");
    }

    #[test]
    fn test_remove_accents_uber() {
        assert_eq!(remove_accents("Über"), "Uber");
    }

    #[test]
    fn test_remove_accents_strasse() {
        assert_eq!(remove_accents("Straße"), "Strasse");
    }

    #[test]
    fn test_remove_accents_stadt() {
        assert_eq!(remove_accents("Stadt"), "Stadt");
    }

    #[test]
    fn test_remove_accents_ae_ligature() {
        assert_eq!(remove_accents("æ"), "ae");
        assert_eq!(remove_accents("Æ"), "AE");
    }

    #[test]
    fn test_remove_accents_o_ligature() {
        assert_eq!(remove_accents("ø"), "o");
        assert_eq!(remove_accents("Ø"), "O");
    }

    #[test]
    fn test_remove_accents_oe_ligature() {
        assert_eq!(remove_accents("œ"), "oe");
        assert_eq!(remove_accents("Œ"), "OE");
    }

    #[test]
    fn test_remove_accents_danish_word() {
        assert_eq!(remove_accents("København"), "Kobenhavn");
    }

    #[test]
    fn test_remove_accents_french_word() {
        assert_eq!(remove_accents("cœur"), "coeur");
        assert_eq!(remove_accents("œuvre"), "oeuvre");
    }

    #[test]
    fn test_remove_accents_mixed_ligatures() {
        assert_eq!(
            remove_accents("Straße ældre øl œuvre"),
            "Strasse aeldre ol oeuvre"
        );
    }

    #[test]
    fn test_remove_accents_non_alphanumeric() {
        // It correctly leaves non-alphanumeric ASCII and other scripts alone, but converts ligatures
        assert_eq!(
            remove_accents("ß, æ, ø, œ, こんにちは, 🦄"),
            "ss, ae, o, oe, こんにちは, 🦄"
        );
    }
}
