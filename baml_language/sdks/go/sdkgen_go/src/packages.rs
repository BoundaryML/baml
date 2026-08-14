//! One generated Go package per BAML package.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use baml_base::Name;
use baml_codegen_types::SymbolPool;

use crate::{names::GoPackageName, rendering::is_protected_go_identifier};

#[derive(Clone, Debug)]
pub(crate) struct GoPackage {
    baml_name: Name,
    go_name: GoPackageName,
    relative_dir: PathBuf,
}

impl GoPackage {
    pub(crate) fn baml_name(&self) -> &Name {
        &self.baml_name
    }

    pub(crate) fn go_name(&self) -> &GoPackageName {
        &self.go_name
    }

    pub(crate) fn file(&self, filename: &str) -> PathBuf {
        self.relative_dir.join(filename)
    }

    pub(crate) fn import_path(&self, sdk_import_path: &str) -> String {
        if self.relative_dir.as_os_str().is_empty() {
            sdk_import_path.to_string()
        } else {
            format!("{sdk_import_path}/{}", slash_path(&self.relative_dir))
        }
    }
}

pub(crate) struct GoPackages {
    packages: BTreeMap<Name, GoPackage>,
}

impl GoPackages {
    pub(crate) fn for_pool(pool: &SymbolPool) -> Self {
        let mut baml_names = pool
            .keys()
            .map(|name| name.package().clone())
            .collect::<BTreeSet<_>>();
        baml_names.insert(Name::new("user"));

        // `project_package_name` has already escaped every shared protected
        // identifier. Only the fixed user-package name remains unavailable.
        let reserved = BTreeSet::from(["baml_sdk".to_string()]);
        let mut groups = BTreeMap::<String, Vec<Name>>::new();
        for baml_name in baml_names {
            if baml_name.as_str() != "user" {
                groups
                    .entry(project_package_name(&baml_name))
                    .or_default()
                    .push(baml_name);
            }
        }

        let mut packages = BTreeMap::from([(
            Name::new("user"),
            GoPackage {
                baml_name: Name::new("user"),
                go_name: GoPackageName::new("baml_sdk"),
                relative_dir: PathBuf::new(),
            },
        )]);
        let mut used = reserved.clone();
        for (base, baml_names) in groups {
            let collides = baml_names.len() > 1 || reserved.contains(&base);
            for baml_name in baml_names {
                let base_candidate = if collides {
                    format!("{base}_{}", short_hash(baml_name.as_str()))
                } else {
                    base.clone()
                };
                let mut candidate = base_candidate.clone();
                for suffix in 2.. {
                    if used.insert(candidate.clone()) {
                        break;
                    }
                    candidate = format!("{base_candidate}_{suffix}");
                }
                let go_name = GoPackageName::new(&candidate);
                let relative_dir = if baml_name.as_str() == "baml" {
                    PathBuf::from(&candidate)
                } else {
                    PathBuf::from("packages").join(&candidate)
                };
                packages.insert(
                    baml_name.clone(),
                    GoPackage {
                        baml_name,
                        go_name,
                        relative_dir,
                    },
                );
            }
        }
        Self { packages }
    }

    pub(crate) fn get(&self, baml_name: &Name) -> &GoPackage {
        self.packages
            .get(baml_name)
            .expect("BAML package was not registered for Go generation")
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &GoPackage> {
        self.packages.values()
    }
}

fn project_package_name(name: &Name) -> String {
    let mut result = String::new();
    let mut separator = false;
    for character in name.as_str().chars() {
        if character.is_ascii_alphanumeric() || character == '_' {
            if separator && !result.ends_with('_') {
                result.push('_');
            }
            result.push(character.to_ascii_lowercase());
            separator = false;
        } else {
            separator = true;
        }
    }
    if result.is_empty() {
        result.push_str("baml_package");
    } else if result.starts_with(|character: char| character.is_ascii_digit()) {
        result.insert_str(0, "baml_");
    } else if result.starts_with('_') {
        result.insert_str(0, "baml");
    }
    while crate::names::is_go_keyword(&result)
        || is_protected_go_identifier(&result)
        || is_special_go_directory(&result)
    {
        result.push('_');
    }
    result
}

fn is_special_go_directory(value: &str) -> bool {
    matches!(value, "internal" | "vendor" | "testdata")
}

fn slash_path(path: &Path) -> String {
    path.iter()
        .map(|segment| segment.to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn short_hash(value: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in value.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")[..8].to_string()
}

#[cfg(test)]
mod tests {
    use baml_codegen_types::{Class, Name as SymbolName, Origin, Symbol};

    use super::*;

    fn class(package: &str) -> (SymbolName, Symbol) {
        let name = SymbolName::new(Name::new(package), vec![], Name::new("Thing"));
        let symbol = Symbol::Class(Class {
            name: name.clone(),
            generic_params: vec![],
            docstring: None,
            properties: vec![],
            static_methods: vec![],
            instance_methods: vec![],
            origin: Origin {
                source_file_path: "types.baml".to_string(),
                span_start: 0,
            },
        });
        (name, symbol)
    }

    #[test]
    fn routes_one_go_package_per_baml_package() {
        let pool = SymbolPool::from([class("user"), class("baml"), class("acme-api")]);
        let packages = GoPackages::for_pool(&pool);

        assert_eq!(
            packages.get(&Name::new("user")).go_name().as_str(),
            "baml_sdk"
        );
        assert_eq!(
            packages.get(&Name::new("user")).file("types.go"),
            PathBuf::from("types.go")
        );
        assert_eq!(
            packages.get(&Name::new("baml")).file("types.go"),
            PathBuf::from("baml/types.go")
        );
        assert_eq!(
            packages.get(&Name::new("acme-api")).file("types.go"),
            PathBuf::from("packages/acme_api/types.go")
        );
    }

    #[test]
    fn package_names_avoid_import_aliases_and_normalization_collisions() {
        let second_order_collision = format!("foo_bar_{}", short_hash("foo-bar"));
        let pool = SymbolPool::from([
            class("context"),
            class("string"),
            class("err_"),
            class("nil"),
            class("init"),
            class("main"),
            class("foo-bar"),
            class("foo_bar"),
            class(&second_order_collision),
            class("internal"),
            class("_hidden"),
        ]);
        let packages = GoPackages::for_pool(&pool);

        assert_ne!(
            packages.get(&Name::new("context")).go_name().as_str(),
            "context"
        );
        assert_ne!(
            packages.get(&Name::new("string")).go_name().as_str(),
            "string"
        );
        for protected in ["err_", "nil", "init", "main"] {
            assert_ne!(
                packages.get(&Name::new(protected)).go_name().as_str(),
                protected
            );
        }
        assert_ne!(
            packages.get(&Name::new("foo-bar")).go_name(),
            packages.get(&Name::new("foo_bar")).go_name()
        );
        assert_ne!(
            packages.get(&Name::new("foo-bar")).go_name(),
            packages.get(&Name::new(&second_order_collision)).go_name()
        );
        assert_eq!(
            packages.get(&Name::new("internal")).go_name().as_str(),
            "internal_"
        );
        assert_eq!(
            packages.get(&Name::new("_hidden")).go_name().as_str(),
            "baml_hidden"
        );
    }
}
