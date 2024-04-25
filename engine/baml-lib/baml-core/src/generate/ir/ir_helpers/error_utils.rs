use std::ops::Index;

/// Sorts a collection of strings based on their similarity to a given name.
///
/// # Parameters
/// - `name`: The reference name to compare against.
/// - `options`: A collection of strings to sort.
/// - `max_return`: The maximum number of results to return.
///
/// # Returns
/// A vector of strings from `options` that are similar to `name`, sorted by similarity.
pub fn sort_by_match<'a, I, T>(
    name: &str,
    options: &'a I,
    max_return: Option<usize>,
) -> Vec<&'a str>
where
    I: Index<usize, Output = T> + 'a,
    &'a I: IntoIterator<Item = &'a T>,
    T: AsRef<str> + 'a,
{
    // The maximum allowed distance for a string to be considered similar.
    const THRESHOLD: usize = 20;

    // Calculate distances and sort names by distance
    let mut name_distances = options
        .into_iter()
        .enumerate()
        .map(|(idx, n)| {
            (
                // Case insensitive comparison
                strsim::osa_distance(&n.as_ref().to_lowercase(), &name.to_lowercase()),
                idx,
            )
        })
        .collect::<Vec<_>>();

    name_distances.sort_by_key(|k| k.0);

    // Filter names based on the threshold
    let filtered_names = name_distances
        .iter()
        .filter(|&&(dist, _)| dist <= THRESHOLD)
        .map(|&(_, idx)| options.index(idx).as_ref());

    // Return either a limited or full set of filtered names
    match max_return {
        Some(max) => filtered_names.take(max).collect(),
        None => filtered_names.collect(),
    }
}

#[macro_export]
macro_rules! error_not_found {
    ($type:expr, $name:expr, $candidates:expr) => {{
        let suggestions = $crate::generate::ir::ir_helpers::error_utils::sort_by_match(
            $name,
            $candidates,
            Some(5),
        );
        match suggestions.len() {
            0 => anyhow::bail!("{} `{}` not found.", $type, $name),
            1 => {
                anyhow::bail!(
                    "{} `{}` not found. Did you mean: {}?",
                    $type,
                    $name,
                    suggestions[0]
                )
            }
            _ => {
                let suggestions = suggestions.join(", ");
                anyhow::bail!(
                    "{} `{}` not found. Did you mean one of: {}?",
                    $type,
                    $name,
                    suggestions
                )
            }
        }
    }};
}

#[macro_export]
macro_rules! error_unsupported {
    ($type:expr, $name:expr, $reason:expr) => {
        anyhow::bail!("Unsupported {} `{}`: {}", $type, $name, $reason)
    };
}
