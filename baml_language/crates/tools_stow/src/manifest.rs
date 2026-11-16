//! Reading dependency tables out of a `Cargo.toml`.
//!
//! Every check and the fixer find tables through [`table_paths`], so the plain
//! tables and the `target.<cfg>` ones are always treated alike.

use std::collections::BTreeSet;

use toml_edit::{DocumentMut, Item, TableLike};

use crate::config::Config;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DepKind {
    Normal,
    Dev,
    Build,
}

impl DepKind {
    const ALL: [DepKind; 3] = [DepKind::Normal, DepKind::Dev, DepKind::Build];

    fn key(self) -> &'static str {
        match self {
            DepKind::Normal => "dependencies",
            DepKind::Dev => "dev-dependencies",
            DepKind::Build => "build-dependencies",
        }
    }
}

/// Where a dependency table sits in a manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TablePath {
    pub(crate) kind: DepKind,
    pub(crate) target: Option<String>,
}

impl TablePath {
    /// The table's name as written in a manifest header.
    pub(crate) fn name(&self) -> String {
        match &self.target {
            None => self.kind.key().to_owned(),
            Some(target) => format!("target.'{target}'.{}", self.kind.key()),
        }
    }

    /// Whether the table holds regular (not dev or build) dependencies.
    pub(crate) fn is_regular(&self) -> bool {
        self.kind == DepKind::Normal
    }
}

#[derive(Debug)]
pub(crate) struct DepTable {
    pub(crate) path: TablePath,
    /// In file order.
    pub(crate) entries: Vec<DepEntry>,
}

#[derive(Debug)]
pub(crate) struct DepEntry {
    pub(crate) name: String,
    pub(crate) workspace: bool,
    pub(crate) features: BTreeSet<String>,
    pub(crate) internal: bool,
}

impl DepEntry {
    /// Sort key for the required order: internal first, then by name.
    pub(crate) fn order(&self) -> (bool, String, &str) {
        order(&self.name, self.internal)
    }
}

pub(crate) fn order(name: &str, internal: bool) -> (bool, String, &str) {
    (!internal, name.to_lowercase(), name)
}

pub(crate) fn parse(text: &str) -> Result<DocumentMut, String> {
    text.parse()
        .map_err(|error: toml_edit::TomlError| error.to_string())
}

/// The path of every dependency table present in the manifest.
pub(crate) fn table_paths(doc: &DocumentMut) -> Vec<TablePath> {
    let kinds_in = |parent: &dyn TableLike, target: Option<&str>| {
        DepKind::ALL
            .into_iter()
            .filter(|kind| parent.contains_key(kind.key()))
            .map(|kind| TablePath {
                kind,
                target: target.map(str::to_owned),
            })
            .collect::<Vec<_>>()
    };
    let targets = doc
        .get("target")
        .and_then(Item::as_table_like)
        .into_iter()
        .flat_map(TableLike::iter)
        .filter_map(|(target, item)| Some(kinds_in(item.as_table_like()?, Some(target))))
        .flatten();
    kinds_in(doc.as_table(), None)
        .into_iter()
        .chain(targets)
        .collect()
}

pub(crate) fn table_item_mut<'a>(
    doc: &'a mut DocumentMut,
    path: &TablePath,
) -> Option<&'a mut Item> {
    let parent: &mut dyn TableLike = match &path.target {
        None => doc.as_table_mut(),
        Some(target) => doc
            .get_mut("target")?
            .as_table_like_mut()?
            .get_mut(target)?
            .as_table_like_mut()?,
    };
    parent.get_mut(path.kind.key())
}

fn table_item<'a>(doc: &'a DocumentMut, path: &TablePath) -> Option<&'a Item> {
    let parent: &dyn TableLike = match &path.target {
        None => doc.as_table(),
        Some(target) => doc
            .get("target")?
            .as_table_like()?
            .get(target)?
            .as_table_like()?,
    };
    parent.get(path.kind.key())
}

pub(crate) fn dep_tables(doc: &DocumentMut, config: &Config) -> Vec<DepTable> {
    table_paths(doc)
        .into_iter()
        .map(|path| {
            let entries = table_item(doc, &path)
                .and_then(Item::as_table_like)
                .into_iter()
                .flat_map(TableLike::iter)
                .map(|(name, item)| DepEntry {
                    name: name.to_owned(),
                    workspace: is_workspace_entry(item),
                    features: features(item),
                    internal: config.namespace_of(name).is_some(),
                })
                .collect();
            DepTable { path, entries }
        })
        .collect()
}

pub(crate) fn is_workspace_entry(item: &Item) -> bool {
    item.as_table_like()
        .and_then(|table| table.get("workspace"))
        .and_then(Item::as_bool)
        == Some(true)
}

pub(crate) fn features(item: &Item) -> BTreeSet<String> {
    item.as_table_like()
        .and_then(|table| table.get("features"))
        .and_then(Item::as_array)
        .into_iter()
        .flatten()
        .filter_map(|feature| feature.as_str().map(str::to_owned))
        .collect()
}
