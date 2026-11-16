//! The `stow.toml` schema (spec section 5) and the name logic built on it
//! (spec section 6).

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::glob::Glob;

/// Name parts that mark a crate's role and need no approved prefix.
const ROLE_SUFFIXES: [&str; 3] = ["types", "test", "tests"];

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Config {
    pub(crate) namespaces: Vec<Namespace>,
    pub(crate) dependency_rules: Vec<Rule>,
    pub(crate) ignore_crates: Vec<Glob>,
    pub(crate) graph: GraphConfig,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct GraphConfig {
    pub(crate) external_deps: Vec<String>,
    pub(crate) implied_deps: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Namespace {
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) approved_prefixes: Vec<String>,
    #[serde(default)]
    pub(crate) approved_suffixes: Vec<String>,
    #[serde(default)]
    pub(crate) test_crate_exceptions: Vec<String>,
    /// Folder name to package name.
    #[serde(default)]
    pub(crate) name_exceptions: BTreeMap<String, String>,
    #[serde(default)]
    pub(crate) dependency_rules: Vec<Rule>,
    #[serde(default)]
    pub(crate) link_crates: Vec<String>,
    #[serde(default)]
    pub(crate) embed_crates: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Rule {
    pub(crate) pattern: Pattern,
    #[serde(default)]
    pub(crate) allowed_crates: Vec<Glob>,
    #[serde(default)]
    pub(crate) allowed_prefixes: Vec<String>,
    #[serde(default)]
    pub(crate) regular_deps_only: bool,
    #[serde(default)]
    pub(crate) reason: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum Pattern {
    Simple(Glob),
    Select(Selection),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Selection {
    select: Glob,
    #[serde(default)]
    exclude: Vec<Glob>,
}

/// Why a package name breaks the naming convention (spec 7.1).
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum NamingError {
    NoNamespace,
    Word { word: String },
    Prefix { prefix: String, namespace: String },
    Structure { namespace: String },
    TooManyParts { namespace: String },
}

impl Config {
    pub(crate) fn from_toml(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|error| error.to_string())
    }

    pub(crate) fn from_metadata(value: toml::Value) -> Result<Self, String> {
        value.try_into().map_err(|error| error.to_string())
    }

    pub(crate) fn is_ignored(&self, package: &str) -> bool {
        self.ignore_crates.iter().any(|glob| glob.matches(package))
    }

    /// The namespace whose `<name>_` is the longest prefix of `package`, with
    /// the rest of the name.
    fn prefix_namespace<'a>(&self, package: &'a str) -> Option<(usize, &'a str)> {
        self.namespaces
            .iter()
            .enumerate()
            .filter_map(|(index, namespace)| {
                let tail = package
                    .strip_prefix(namespace.name.as_str())?
                    .strip_prefix('_')?;
                Some((index, tail))
            })
            .min_by_key(|(_, tail)| tail.len())
    }

    /// The namespace a package belongs to: by prefix, else by being the target
    /// of a name exception.
    pub(crate) fn namespace_of(&self, package: &str) -> Option<usize> {
        self.prefix_namespace(package)
            .map(|(index, _)| index)
            .or_else(|| {
                self.namespaces.iter().position(|namespace| {
                    namespace
                        .name_exceptions
                        .values()
                        .any(|name| name == package)
                })
            })
    }

    pub(crate) fn has_name_exception(&self, folder: &str, package: &str) -> bool {
        self.namespaces.iter().any(|namespace| {
            namespace
                .name_exceptions
                .get(folder)
                .is_some_and(|name| name == package)
        })
    }

    pub(crate) fn check_name(&self, package: &str) -> Result<(), NamingError> {
        let (index, tail) = self
            .prefix_namespace(package)
            .ok_or(NamingError::NoNamespace)?;
        let namespace = &self.namespaces[index];
        if namespace.approved_suffixes.iter().any(|s| s == tail) {
            return Ok(());
        }
        let approved = |word: &str| namespace.approved_prefixes.iter().any(|p| p == word);
        let is_word = |word: &str| !word.is_empty() && word.bytes().all(|b| b.is_ascii_lowercase());
        let name = || namespace.name.clone();

        match tail.split('_').collect::<Vec<_>>().as_slice() {
            [word] if is_word(word) || approved(word) => Ok(()),
            [word] => Err(NamingError::Word {
                word: (*word).to_owned(),
            }),
            [first, last] if approved(first) || ROLE_SUFFIXES.contains(last) => Ok(()),
            [first, _] => Err(NamingError::Prefix {
                prefix: (*first).to_owned(),
                namespace: name(),
            }),
            [first, _, last] if approved(first) && ROLE_SUFFIXES.contains(last) => Ok(()),
            [_, _, _] => Err(NamingError::Structure { namespace: name() }),
            _ => Err(NamingError::TooManyParts { namespace: name() }),
        }
    }

    /// The graph colour tag of a crate (spec 9.6).
    pub(crate) fn tag_of(&self, package: &str) -> Option<String> {
        let (index, tail) = self.prefix_namespace(package)?;
        let parts: Vec<&str> = tail.split('_').collect();
        let (first, last) = (*parts.first()?, *parts.last()?);
        let tagged = parts.len() >= 2
            && (self.namespaces[index]
                .approved_prefixes
                .iter()
                .any(|p| p == first)
                || ROLE_SUFFIXES.contains(&last));
        tagged.then(|| first.to_owned())
    }
}

impl Pattern {
    pub(crate) fn matches(&self, dependency: &str) -> bool {
        match self {
            Pattern::Simple(glob) => glob.matches(dependency),
            Pattern::Select(selection) => {
                selection.select.matches(dependency)
                    && !selection
                        .exclude
                        .iter()
                        .any(|glob| glob.matches(dependency))
            }
        }
    }
}

impl Rule {
    pub(crate) fn allows(&self, package: &str, config: &Config) -> bool {
        is_test_crate(package)
            || self.allowed_crates.iter().any(|glob| glob.matches(package))
            || self.allowed_prefixes.iter().any(|prefix| {
                config.namespaces.iter().any(|namespace| {
                    package
                        .strip_prefix(namespace.name.as_str())
                        .and_then(|rest| rest.strip_prefix('_'))
                        .and_then(|rest| rest.strip_prefix(prefix.as_str()))
                        .is_some_and(|rest| rest.is_empty() || rest.starts_with('_'))
                })
            })
    }
}

pub(crate) fn is_test_crate(package: &str) -> bool {
    test_crate_base(package).is_some()
}

/// The crate a test crate is paired with: `foo_tests` -> `foo`.
pub(crate) fn test_crate_base(package: &str) -> Option<&str> {
    package
        .strip_suffix("_tests")
        .or_else(|| package.strip_suffix("_test"))
}
