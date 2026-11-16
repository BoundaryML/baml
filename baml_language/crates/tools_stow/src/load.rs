//! The reading half of the IO shell: turns the file system and `cargo
//! metadata` into a [`Config`] and a [`Workspace`].

use std::{
    io::ErrorKind,
    path::{Path, PathBuf},
};

use cargo_metadata::{Metadata, MetadataCommand};
use toml_edit::{DocumentMut, Item, TableLike};

use crate::{
    config::Config,
    error::{Error, Result},
    manifest,
    workspace::{Crate, Workspace, WorkspaceDep},
};

pub(crate) const CONFIG_FILE: &str = "stow.toml";

/// The nearest `Cargo.toml` at or above `start`.
pub(crate) fn locate(start: &Path) -> Result<PathBuf> {
    start
        .ancestors()
        .map(|dir| dir.join("Cargo.toml"))
        .find(|manifest| manifest.is_file())
        .ok_or_else(|| Error::NoManifest(start.to_owned()))
}

pub(crate) fn metadata(manifest: &Path) -> Result<Metadata> {
    Ok(MetadataCommand::new()
        .manifest_path(manifest)
        .no_deps()
        .exec()?)
}

pub(crate) fn read(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(|source| Error::Read {
        path: path.to_owned(),
        source,
    })
}

fn parse_error(path: &Path) -> impl FnOnce(String) -> Error + '_ {
    move |message| Error::Parse {
        path: path.to_owned(),
        message,
    }
}

pub(crate) fn config(root: &Path) -> Result<Config> {
    let path = root.join(CONFIG_FILE);
    match std::fs::read_to_string(&path) {
        Ok(text) => return Config::from_toml(&text).map_err(parse_error(&path)),
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(source) => return Err(Error::Read { path, source }),
    }

    let path = root.join("Cargo.toml");
    let table: toml::Table = read(&path)?
        .parse()
        .map_err(|error: toml::de::Error| parse_error(&path)(error.to_string()))?;
    let embedded = ["workspace", "metadata", "stow"]
        .into_iter()
        .try_fold(&toml::Value::Table(table), |value, key| value.get(key))
        .cloned();
    match embedded {
        Some(value) => Config::from_metadata(value).map_err(parse_error(&path)),
        None => Err(Error::NoConfig(root.to_owned())),
    }
}

pub(crate) fn workspace(metadata: &Metadata, config: &Config) -> Result<Workspace> {
    let root = metadata.workspace_root.as_std_path();
    let root_manifest = document(&root.join("Cargo.toml"))?;
    let section = |key: &str| {
        root_manifest
            .get("workspace")
            .and_then(Item::as_table_like)
            .and_then(|workspace| workspace.get(key))
    };

    let member_roots = section("members")
        .and_then(Item::as_array)
        .into_iter()
        .flatten()
        .filter_map(|member| member.as_str().map(member_root))
        .collect();

    let deps = section("dependencies")
        .and_then(Item::as_table_like)
        .into_iter()
        .flat_map(TableLike::iter)
        .map(|(key, item)| {
            let package = item
                .as_table_like()
                .and_then(|table| table.get("package"))
                .and_then(Item::as_str)
                .unwrap_or(key);
            let dep = WorkspaceDep {
                package: package.to_owned(),
                features: manifest::features(item),
            };
            (key.to_owned(), dep)
        })
        .collect();

    let mut crates = metadata
        .workspace_packages()
        .into_iter()
        .filter(|package| !config.is_ignored(&package.name))
        .map(|package| {
            let path = package.manifest_path.as_std_path();
            let dir = path.parent().unwrap_or(root);
            let name = package.name.to_string();
            Ok(Crate {
                folder: dir
                    .file_name()
                    .map(|folder| folder.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                dir: relative(root, dir),
                manifest: relative(root, path),
                namespace: config.namespace_of(&name),
                tables: manifest::dep_tables(&document(path)?, config),
                name,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    crates.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(Workspace {
        member_roots,
        deps,
        crates,
    })
}

fn document(path: &Path) -> Result<DocumentMut> {
    manifest::parse(&read(path)?).map_err(parse_error(path))
}

/// `path` relative to `root` with `/` separators, or unchanged if outside it.
fn relative(root: &Path, path: &Path) -> String {
    match path.strip_prefix(root) {
        Ok(inside) => inside
            .components()
            .map(|part| part.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"),
        Err(_) => path.to_string_lossy().into_owned(),
    }
}

/// The directory part of a `members` entry before its first wildcard segment.
fn member_root(member: &str) -> String {
    member
        .split(['/', '\\'])
        .take_while(|segment| !segment.contains(['*', '?', '[']))
        .filter(|segment| !segment.is_empty() && *segment != ".")
        .collect::<Vec<_>>()
        .join("/")
}
