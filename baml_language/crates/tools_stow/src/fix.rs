//! The fixer (spec section 8): manifest text in, manifest text out.

use std::collections::BTreeMap;

use toml_edit::{InlineTable, Item, Table, Value};

use crate::{config::Config, manifest, workspace::Workspace};

/// Keys that say where a dependency comes from; the workspace entry replaces them.
const SOURCE_KEYS: [&str; 7] = ["version", "path", "git", "branch", "tag", "rev", "registry"];
/// Keys Cargo accepts next to `workspace = true`.
const KEPT_KEYS: [&str; 3] = ["features", "optional", "default-features"];

/// Returns the fixed manifest, or `None` if nothing needs to change.
pub(crate) fn fix(
    text: &str,
    workspace: &Workspace,
    config: &Config,
) -> Result<Option<String>, String> {
    let mut doc = manifest::parse(text)?;
    let mut changed = false;
    for path in manifest::table_paths(&doc) {
        let Some(table) = manifest::table_item_mut(&mut doc, &path).and_then(Item::as_table_mut)
        else {
            continue;
        };
        changed |= convert(table, workspace);
        changed |= sort(table, config);
    }
    Ok(changed.then(|| doc.to_string()))
}

/// Rewrites entries that have a workspace definition to `workspace = true`.
fn convert(table: &mut Table, workspace: &Workspace) -> bool {
    let mut changed = false;
    for (name, item) in table.iter_mut() {
        if !workspace.deps.contains_key(name.get()) || manifest::is_workspace_entry(item) {
            continue;
        }
        if let Some(converted) = converted(item) {
            *item = converted;
            changed = true;
        }
    }
    changed
}

fn converted(item: &Item) -> Option<Item> {
    let convertible = |key: &str| SOURCE_KEYS.contains(&key) || KEPT_KEYS.contains(&key);
    match item {
        Item::Value(Value::String(version)) => {
            let mut entry = InlineTable::new();
            entry.insert("workspace", true.into());
            *entry.decor_mut() = version.decor().clone();
            Some(Item::Value(Value::InlineTable(entry)))
        }
        Item::Value(Value::InlineTable(old)) => {
            if !old.iter().all(|(key, _)| convertible(key)) {
                return None;
            }
            let mut entry = InlineTable::new();
            entry.insert("workspace", true.into());
            for (key, value) in old.iter().filter(|(key, _)| KEPT_KEYS.contains(key)) {
                let mut value = value.clone();
                value.decor_mut().clear();
                entry.insert(key, value);
            }
            *entry.decor_mut() = old.decor().clone();
            Some(Item::Value(Value::InlineTable(entry)))
        }
        Item::Table(old) => {
            if !old.iter().all(|(key, _)| convertible(key)) {
                return None;
            }
            let mut entry = old.clone();
            for key in SOURCE_KEYS {
                entry.remove(key);
            }
            entry.insert("workspace", toml_edit::value(true));
            Some(Item::Table(entry))
        }
        _ => None,
    }
}

/// Reorders a table's entries, each with its attached comments.
fn sort(table: &mut Table, config: &Config) -> bool {
    if !table.iter().all(|(_, item)| item.is_value()) {
        return false;
    }
    let order = |name: &str| {
        let (external, lowered, _) = manifest::order(name, config.namespace_of(name).is_some());
        (external, lowered, name.to_owned())
    };
    let names: Vec<String> = table.iter().map(|(name, _)| name.to_owned()).collect();
    if names.is_sorted_by_key(|name| order(name)) {
        return false;
    }

    // Blank lines and free-standing comments stay in their slot; the comment
    // lines directly above an entry travel with it.
    let (slots, attached): (Vec<String>, BTreeMap<String, String>) = names
        .iter()
        .map(|name| {
            let (detached, attached) = split_prefix(prefix(table, name));
            (detached.to_owned(), (name.clone(), attached.to_owned()))
        })
        .unzip();

    table.sort_values_by(|a, _, b, _| order(a.get()).cmp(&order(b.get())));

    let sorted: Vec<String> = table.iter().map(|(name, _)| name.to_owned()).collect();
    for (slot, name) in slots.iter().zip(&sorted) {
        if let Some(mut key) = table.key_mut(name) {
            key.leaf_decor_mut()
                .set_prefix(format!("{slot}{}", attached[name]));
        }
    }
    true
}

/// Everything between the previous line and this entry's key.
fn prefix<'a>(table: &'a Table, name: &str) -> &'a str {
    table
        .key(name)
        .and_then(|key| key.leaf_decor().prefix())
        .and_then(|raw| raw.as_str())
        .unwrap_or_default()
}

/// Splits a key prefix after its last blank line: what stays, what moves.
fn split_prefix(prefix: &str) -> (&str, &str) {
    let moving: usize = prefix
        .split_inclusive('\n')
        .rev()
        .take_while(|line| !(line.ends_with('\n') && line.trim().is_empty()))
        .map(str::len)
        .sum();
    prefix.split_at(prefix.len() - moving)
}
