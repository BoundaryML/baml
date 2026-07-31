//! Stable, content-derived symbol identity.
//!
//! A [`SymbolId`] names a declaration by *what it is* — package, namespace
//! path, name, kind, optional member — never by an interning order or a file
//! offset. Ids are serializable, lifetime-free, and resolvable in both
//! directions against any database revision, so they survive across
//! compilations and processes: committed artifacts diff on them, export
//! documents cross-link on them, and caches key on them.
//!
//! The string form uses a single-letter kind prefix (the
//! `DocumentationCommentId` idea), because BAML's type and value namespaces
//! are distinct — `class Foo` and `function Foo` can coexist, so a bare
//! dotted path is not injective:
//!
//! ```text
//! T:baml.time.Duration          type-space item (class/enum/interface/alias)
//! V:baml.json.parse             value-space item (function/global/test/…)
//! M:baml.time.Duration.abs     method (class, default, or required)
//! F:user.Point.x               field
//! E:user.Color.Red             enum variant
//! A:baml.Comparable.CompareError   associated type
//! ```
//!
//! The path is BAML's own dot syntax throughout. No member separator is
//! needed: the kind prefix decides the shape — for member kinds the last
//! segment is the member and the one before it the containing type.
//!
//! Impl blocks are unnamed and get their identity from the export layer
//! (interface head + for-type rendering), not from `SymbolId`.
//!
//! [`resolve`] is the human-path front door (`"baml.time.Duration.abs"`,
//! no prefixes); [`SymbolId::resolve`] is the precise, kind-directed one.

use std::{fmt, str::FromStr};

use baml_base::Name;
use serde::{Deserialize, Serialize};

use crate::{
    Db,
    handles::{FunctionOwner, Member, Namespace, Package, Symbol},
};

// ── SymbolId ─────────────────────────────────────────────────────────────────

/// The kind discriminant of a [`SymbolId`] — which lookup space the id names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdKind {
    /// Type-space item: class, enum, interface, type alias.
    Type,
    /// Value-space item: function, global, client, test, ….
    Value,
    /// A method member (class method, interface default, or required).
    Method,
    /// A field member of a class or interface.
    Field,
    /// An enum variant.
    Variant,
    /// An associated type of an interface.
    AssocType,
}

impl IdKind {
    fn prefix(self) -> char {
        match self {
            Self::Type => 'T',
            Self::Value => 'V',
            Self::Method => 'M',
            Self::Field => 'F',
            Self::Variant => 'E',
            Self::AssocType => 'A',
        }
    }

    fn from_prefix(c: char) -> Option<Self> {
        match c {
            'T' => Some(Self::Type),
            'V' => Some(Self::Value),
            'M' => Some(Self::Method),
            'F' => Some(Self::Field),
            'E' => Some(Self::Variant),
            'A' => Some(Self::AssocType),
            _ => None,
        }
    }

    fn is_member(self) -> bool {
        matches!(
            self,
            Self::Method | Self::Field | Self::Variant | Self::AssocType
        )
    }
}

/// A stable, content-derived symbol identity. See the module docs.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SymbolId {
    pub kind: IdKind,
    pub package: String,
    pub namespace: Vec<String>,
    /// The item's name; for a member id, the *containing type's* name.
    pub name: String,
    /// The member's name, for member kinds; `None` for item kinds.
    pub member: Option<String>,
}

impl fmt::Display for SymbolId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.kind.prefix(), self.package)?;
        for seg in &self.namespace {
            write!(f, ".{seg}")?;
        }
        write!(f, ".{}", self.name)?;
        if let Some(member) = &self.member {
            write!(f, ".{member}")?;
        }
        Ok(())
    }
}

/// A [`SymbolId`] string that does not parse; carries the offending input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidSymbolId(pub String);

impl fmt::Display for InvalidSymbolId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid symbol id: {}", self.0)
    }
}

impl std::error::Error for InvalidSymbolId {}

impl FromStr for SymbolId {
    type Err = InvalidSymbolId;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let invalid = || InvalidSymbolId(s.to_string());
        let (prefix, rest) = s.split_once(':').ok_or_else(invalid)?;
        let mut prefix_chars = prefix.chars();
        let kind = prefix_chars
            .next()
            .filter(|_| prefix_chars.next().is_none())
            .and_then(IdKind::from_prefix)
            .ok_or_else(invalid)?;

        let mut segments: Vec<&str> = rest.split('.').collect();
        // Member ids need `pkg.Type.member`; item ids need `pkg.Name`.
        let min = if kind.is_member() { 3 } else { 2 };
        if segments.len() < min || segments.iter().any(|seg| seg.is_empty()) {
            return Err(invalid());
        }
        let member = kind
            .is_member()
            .then(|| segments.pop().unwrap_or_else(|| unreachable!()).to_string());
        let name = segments.pop().unwrap_or_else(|| unreachable!()).to_string();
        let package = segments.remove(0).to_string();
        Ok(Self {
            kind,
            package,
            namespace: segments.into_iter().map(str::to_string).collect(),
            name,
            member,
        })
    }
}

impl SymbolId {
    /// The id of a named item or method. `None` for impls (unnamed — the
    /// export layer identifies them structurally) and for free-impl methods
    /// (their identity lives under that impl id).
    pub fn of_symbol(db: &dyn Db, symbol: Symbol<'_>) -> Option<Self> {
        // A method's id nests under its owner type rather than the namespace.
        if let Symbol::Function(function) = symbol {
            match function.owner(db) {
                Some(FunctionOwner::Class(class)) => {
                    return Some(Self::member_id(
                        db,
                        Symbol::Class(class),
                        IdKind::Method,
                        &function.name(db),
                    ));
                }
                Some(FunctionOwner::Interface(iface)) => {
                    return Some(Self::member_id(
                        db,
                        Symbol::Interface(iface),
                        IdKind::Method,
                        &function.name(db),
                    ));
                }
                Some(FunctionOwner::Impl(_)) => return None,
                None => {}
            }
        }

        let kind = match symbol {
            Symbol::Class(_) | Symbol::Enum(_) | Symbol::Interface(_) | Symbol::TypeAlias(_) => {
                IdKind::Type
            }
            Symbol::Function(_)
            | Symbol::TemplateString(_)
            | Symbol::Client(_)
            | Symbol::Test(_)
            | Symbol::RetryPolicy(_)
            | Symbol::Global(_) => IdKind::Value,
            Symbol::Impl(_) => return None,
        };
        let name = symbol.name(db)?;
        let pkg = baml_compiler2_hir::file_package::file_package(db, symbol.file(db));
        Some(Self {
            kind,
            package: pkg.package.to_string(),
            namespace: pkg.namespace_path.iter().map(ToString::to_string).collect(),
            name: name.to_string(),
            member: None,
        })
    }

    /// The id of a member of `owner`.
    pub fn of_member(db: &dyn Db, owner: Symbol<'_>, member: Member<'_>) -> Option<Self> {
        let kind = match member {
            Member::Method(_) | Member::RequiredMethod(_) => IdKind::Method,
            Member::Field(_) => IdKind::Field,
            Member::Variant(_) => IdKind::Variant,
            Member::AssocType(_) => IdKind::AssocType,
        };
        Some(Self::member_id(db, owner, kind, &member.name(db)))
    }

    fn member_id(db: &dyn Db, owner: Symbol<'_>, kind: IdKind, member: &Name) -> Self {
        let owner_name = owner
            .name(db)
            .unwrap_or_else(|| unreachable!("member owners are named items"));
        let pkg = baml_compiler2_hir::file_package::file_package(db, owner.file(db));
        Self {
            kind,
            package: pkg.package.to_string(),
            namespace: pkg.namespace_path.iter().map(ToString::to_string).collect(),
            name: owner_name.to_string(),
            member: Some(member.to_string()),
        }
    }

    /// Resolve this id against a database. Kind-directed: a `T:` id only
    /// finds type-space items, a `V:` id only value-space items, and member
    /// ids look inside their containing type.
    pub fn resolve<'db>(&self, db: &'db dyn Db) -> Option<Resolved<'db>> {
        let package = Package::named_checked(db, &self.package)?;
        let namespace = package.namespace(db, &self.namespace)?;
        match self.kind {
            IdKind::Type => namespace.type_named(db, &self.name).map(Resolved::Symbol),
            IdKind::Value => namespace.value_named(db, &self.name).map(Resolved::Symbol),
            IdKind::Method | IdKind::Field | IdKind::Variant | IdKind::AssocType => {
                let owner = namespace.type_named(db, &self.name)?;
                let member_name = self.member.as_deref()?;
                let member = owner.member_named(db, member_name)?;
                let matches = matches!(
                    (self.kind, member),
                    (
                        IdKind::Method,
                        Member::Method(_) | Member::RequiredMethod(_)
                    ) | (IdKind::Field, Member::Field(_))
                        | (IdKind::Variant, Member::Variant(_))
                        | (IdKind::AssocType, Member::AssocType(_))
                );
                matches.then_some(Resolved::Member(owner, member))
            }
        }
    }
}

// ── Human-path resolution ────────────────────────────────────────────────────

/// What a path resolved to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolved<'db> {
    Package(Package<'db>),
    Namespace(Namespace<'db>),
    Symbol(Symbol<'db>),
    /// A member, paired with the symbol that contains it.
    Member(Symbol<'db>, Member<'db>),
}

/// Resolve a human-written dotted path — `"baml.time.Duration"`,
/// `"String.split"`, `"root.helpers.greet"`.
///
/// Package routing: a leading `root.` forces the user package; a leading
/// builtin package name (`baml.`, `assert.`, …) selects it; otherwise the
/// user package is tried first, then `baml` (so unqualified builtin names
/// like `"String"` or `"json.parse"` resolve). Within a package, the longest
/// namespace prefix wins, the next segment is the item (types before
/// values), and one trailing segment drills into a member.
pub fn resolve<'db>(db: &'db dyn Db, path: &str) -> Option<Resolved<'db>> {
    let segments: Vec<&str> = path.split('.').collect();
    if segments.iter().any(|seg| seg.is_empty()) {
        return None;
    }

    let builtin_packages = baml_builtins2::stdlib_package_names();
    let (package_name, rest): (&str, &[&str]) = match segments.first() {
        Some(&"root") => ("user", &segments[1..]),
        Some(first) if builtin_packages.contains(first) => (first, &segments[1..]),
        _ => ("user", &segments[..]),
    };

    let attempt = |package_name: &str, rest: &[&str]| -> Option<Resolved<'db>> {
        let package = Package::named_checked(db, package_name)?;
        resolve_in_package(db, package, rest)
    };
    attempt(package_name, rest).or_else(|| {
        // Unqualified builtin fallback: `String.split`, `Comparable`.
        if package_name == "user" && segments.first() != Some(&"root") {
            attempt("baml", &segments)
        } else {
            None
        }
    })
}

fn resolve_in_package<'db>(
    db: &'db dyn Db,
    package: Package<'db>,
    segments: &[&str],
) -> Option<Resolved<'db>> {
    if segments.is_empty() {
        return Some(Resolved::Package(package));
    }

    // Longest namespace prefix wins; the packages' namespace set is closed,
    // so this is a simple backwards scan (including the whole path as a
    // namespace, for bare-namespace targets like `baml.json`).
    for split in (0..=segments.len()).rev() {
        let (ns_path, item_path) = segments.split_at(split);
        let Some(namespace) = package.namespace(
            db,
            &ns_path.iter().map(ToString::to_string).collect::<Vec<_>>(),
        ) else {
            continue;
        };
        match item_path {
            [] => return Some(Resolved::Namespace(namespace)),
            [item] => {
                let symbol = namespace
                    .type_named(db, item)
                    .or_else(|| namespace.value_named(db, item))?;
                return Some(Resolved::Symbol(symbol));
            }
            [item, member] => {
                // `String.split` — implicit member drill-in.
                if let Some(symbol) = namespace
                    .type_named(db, item)
                    .or_else(|| namespace.value_named(db, item))
                    && let Some(found) = symbol.member_named(db, member)
                {
                    return Some(Resolved::Member(symbol, found));
                }
                continue;
            }
            _ => continue,
        }
    }
    None
}
