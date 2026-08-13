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
//! A method contributed by an `implements` block is addressed *through the
//! block*, in BAML's own qualified-path syntax:
//!
//! ```text
//! M:(int as baml.ops.Add<bigint>).add
//! M:(baml.time.Instant as baml.ops.Subtract<baml.time.Duration>).sub
//! ```
//!
//! The interface's arguments are load-bearing. One type may implement one
//! interface at several instantiations — that is what multi-RHS operator
//! overloading is — and each contributes a method under the same name, so
//! `M:baml.time.Duration.mul` cannot name `Multiply<int>`'s and
//! `Multiply<bigint>`'s both. Qualification is unconditional rather than
//! applied on collision: an id that gained a qualifier the day some unrelated
//! impl appeared would silently invalidate every cache keyed on it.
//!
//! This holds whether the block is free or written in a class body. An
//! *inherent* method keeps the plain path form.
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

/// What an id hangs off.
///
/// Two shapes, because two things can own a member. A named type is reached
/// by path; a method contributed by an `implements` block is reached *through
/// that block*, and the block is not a path — neither half of it need be
/// addressable as an item. `int` is a primitive with no namespace, and the
/// interface's arguments are types rather than names.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Owner {
    /// A named item in a package's namespace: `baml.time.Duration`.
    Path {
        package: String,
        namespace: Vec<String>,
        /// The item's name; for a member id, the *containing type's* name.
        name: String,
    },
    /// An impl block: `(int as baml.ops.Add<bigint>)`.
    ///
    /// This is BAML's own qualified-path syntax — the one that already writes
    /// `(Self as baml.Comparable).CompareError` — so the id stays readable and
    /// speakable, which is the point of a content-derived id.
    ///
    /// The interface's arguments are part of the identity, not decoration. A
    /// type may implement one interface at several instantiations; that is
    /// what multi-RHS operator overloading *is*, and `Add<int>` and
    /// `Add<bigint>` contribute different methods under the same name.
    Impl {
        /// The implementing type, canonically rendered: `int`.
        for_ty: String,
        /// The interface with its arguments: `baml.ops.Add<bigint>`.
        interface: String,
    },
}

impl fmt::Display for Owner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Path {
                package,
                namespace,
                name,
            } => {
                write!(f, "{package}")?;
                for seg in namespace {
                    write!(f, ".{seg}")?;
                }
                write!(f, ".{name}")
            }
            Self::Impl { for_ty, interface } => write!(f, "({for_ty} as {interface})"),
        }
    }
}

/// A stable, content-derived symbol identity. See the module docs.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SymbolId {
    pub kind: IdKind,
    pub owner: Owner,
    /// The member's name, for member kinds; `None` for item kinds.
    pub member: Option<String>,
}

impl fmt::Display for SymbolId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.kind.prefix(), self.owner)?;
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

        if rest.starts_with('(') {
            return Self::parse_impl_owned(s, kind, rest);
        }

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
            owner: Owner::Path {
                package,
                namespace: segments.into_iter().map(str::to_string).collect(),
                name,
            },
            member,
        })
    }
}

/// The impl block that contributes `function`, when one does.
///
/// A block written in a class body merges into the class, so the function's
/// owner is the class and the block has to be found by looking for it. An
/// inherent method belongs to no block and gets the plain path form.
fn contributing_impl<'db>(
    db: &'db dyn Db,
    function: crate::handles::Function<'db>,
) -> Option<crate::handles::Impl<'db>> {
    match function.owner(db)? {
        FunctionOwner::Impl(imp) => Some(imp),
        FunctionOwner::Class(class) => class
            .impls(db)
            .into_iter()
            .find(|imp| imp.methods(db).contains(&function)),
        FunctionOwner::Interface(_) => None,
    }
}

/// The byte index just past the `)` closing the parenthesis at index 0.
///
/// Counted rather than searched: a for-type may itself be parenthesized —
/// `((Self as baml.Comparable).CompareError as baml.FromJson)` — so the first
/// `)` is routinely the wrong one.
fn matching_paren(text: &str) -> Option<usize> {
    debug_assert!(text.starts_with('('));
    let mut depth = 0usize;
    for (index, ch) in text.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

/// The byte index of the ` as ` that separates the two halves, ignoring any
/// nested inside a parenthesized projection.
///
/// Parens alone are enough to nest by: BAML's only other ` as ` is the
/// qualified-path form, which is always parenthesized. Angle brackets need no
/// tracking, which is just as well — `->` in a function type would break naive
/// `<`/`>` counting.
fn split_as(inner: &str) -> Option<usize> {
    const AS: &str = " as ";
    let mut depth = 0usize;
    for (index, ch) in inner.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ if depth == 0 && inner[index..].starts_with(AS) => return Some(index),
            _ => {}
        }
    }
    None
}

impl SymbolId {
    /// Parse the impl-owned form: `(int as baml.ops.Add<bigint>).add`.
    fn parse_impl_owned(whole: &str, kind: IdKind, rest: &str) -> Result<Self, InvalidSymbolId> {
        let invalid = || InvalidSymbolId(whole.to_string());
        let close = matching_paren(rest).ok_or_else(invalid)?;
        let inner = &rest[1..close];
        let at = split_as(inner).ok_or_else(invalid)?;
        let for_ty = inner[..at].trim().to_string();
        let interface = inner[at + " as ".len()..].trim().to_string();
        if for_ty.is_empty() || interface.is_empty() {
            return Err(invalid());
        }

        let tail = &rest[close + 1..];
        // An impl block owns methods and nothing else: no fields, no variants,
        // no associated types. A bare `(T as I)` with no member names the
        // block itself, which is the impl record's own id.
        let member = match (kind, tail) {
            (IdKind::Method, _) if !tail.is_empty() => {
                let name = tail.strip_prefix('.').ok_or_else(invalid)?;
                if name.is_empty() || name.contains('.') {
                    return Err(invalid());
                }
                Some(name.to_string())
            }
            (_, "") => None,
            _ => return Err(invalid()),
        };
        if kind.is_member() && member.is_none() {
            return Err(invalid());
        }
        Ok(Self {
            kind,
            owner: Owner::Impl { for_ty, interface },
            member,
        })
    }
}

impl SymbolId {
    /// The id of a named item or method.
    ///
    /// A method contributed by an `implements` block is addressed through that
    /// block, free or in-body alike; an inherent method and an interface's own
    /// default take the plain path form. `None` only for an impl block itself,
    /// which is not a named item — the export layer gives it a structural id.
    pub fn of_symbol(db: &dyn Db, symbol: Symbol<'_>) -> Option<Self> {
        // A method's id nests under its owner type rather than the namespace.
        if let Symbol::Function(function) = symbol {
            // An impl-contributed method is addressed through its block, even
            // when the block is written in the class body and the method is
            // therefore also a class method. Addressing it on the class alone
            // is what collided: `Duration` implementing both `Multiply<int>`
            // and `Multiply<bigint>` contributes two methods named `mul`, and
            // `M:baml.time.Duration.mul` cannot name both.
            if let Some(imp) = contributing_impl(db, function) {
                return Self::impl_member_id(db, imp, &function.name(db));
            }
            match function.owner(db) {
                Some(FunctionOwner::Class(class)) => {
                    return Self::member_id(
                        db,
                        Symbol::Class(class),
                        IdKind::Method,
                        &function.name(db),
                    );
                }
                Some(FunctionOwner::Interface(iface)) => {
                    return Self::member_id(
                        db,
                        Symbol::Interface(iface),
                        IdKind::Method,
                        &function.name(db),
                    );
                }
                Some(FunctionOwner::Impl(_)) | None => {}
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
            owner: Owner::Path {
                package: pkg.package.to_string(),
                namespace: pkg.namespace_path.iter().map(ToString::to_string).collect(),
                name: name.to_string(),
            },
            member: None,
        })
    }

    /// How an impl block is written inside an id: `(int as baml.ops.Add<bigint>)`.
    ///
    /// The one renderer, shared with the export layer's block ids, so the two
    /// can never disagree about what identifies an impl.
    pub fn impl_owner(db: &dyn Db, imp: crate::handles::Impl<'_>) -> Option<Owner> {
        let data = crate::facts::impl_data(db, imp.loc())?;
        let iface: crate::Interface<'_> = data.interface.into();
        let mut interface = iface.qualified_name(db).render_dotted(false);
        if !data.interface_args.is_empty() {
            let args: Vec<String> = data
                .interface_args
                .iter()
                .map(|arg| crate::display::TyDisplayFormat::Canonical.render(arg))
                .collect();
            interface = format!("{interface}<{}>", args.join(", "));
        }
        Some(Owner::Impl {
            for_ty: crate::display::TyDisplayFormat::Canonical.render(&data.for_ty_pattern),
            interface,
        })
    }

    /// The id of a method reached through `imp`.
    fn impl_member_id(db: &dyn Db, imp: crate::handles::Impl<'_>, member: &Name) -> Option<Self> {
        Some(Self {
            kind: IdKind::Method,
            owner: Self::impl_owner(db, imp)?,
            member: Some(member.to_string()),
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
        Self::member_id(db, owner, kind, &member.name(db))
    }

    /// `None` when the owner has no name — an impl block, which is identified
    /// structurally rather than by path, so its members cannot be addressed
    /// this way.
    fn member_id(db: &dyn Db, owner: Symbol<'_>, kind: IdKind, member: &Name) -> Option<Self> {
        let owner_name = owner.name(db)?;
        let pkg = baml_compiler2_hir::file_package::file_package(db, owner.file(db));
        Some(Self {
            kind,
            owner: Owner::Path {
                package: pkg.package.to_string(),
                namespace: pkg.namespace_path.iter().map(ToString::to_string).collect(),
                name: owner_name.to_string(),
            },
            member: Some(member.to_string()),
        })
    }

    /// Resolve this id against a database. Kind-directed: a `T:` id only
    /// finds type-space items, a `V:` id only value-space items, and member
    /// ids look inside their containing type.
    pub fn resolve<'db>(&self, db: &'db dyn Db) -> Option<Resolved<'db>> {
        let (package_name, namespace_path, name) = match &self.owner {
            Owner::Path {
                package,
                namespace,
                name,
            } => (package, namespace, name),
            // Impl-owned ids are found by matching the rendered block, since
            // neither half is reachable through a namespace: the for-type may
            // be a primitive and the interface arguments are types.
            Owner::Impl { .. } => {
                let member_name = self.member.as_deref()?;
                let imp = crate::handles::project_impls(db)
                    .into_iter()
                    .find(|imp| Self::impl_owner(db, *imp).as_ref() == Some(&self.owner))?;
                // `all_methods`, so the defaults a block inherits resolve too.
                //
                // For those, `of_symbol` gives back the *interface's* path id
                // rather than this one, and that asymmetry is deliberate: the
                // two answer different questions. `M:baml.iter.Iterator.chain`
                // says where the code is written — once. `M:(T[] as
                // baml.iter.Iterator).chain` says how it is reached, and
                // thirteen implementors reach the same declaration. A
                // declaration has one id; an access path has one per block,
                // which is why the export carries both (`id`, `declared_by`).
                // Rejecting the access path here would leave the record the
                // export publishes unresolvable.
                let method = imp
                    .all_methods(db)
                    .into_iter()
                    .find(|m| m.function.name(db).as_str() == member_name)?;
                return Some(Resolved::Member(
                    Symbol::Impl(imp),
                    Member::Method(method.function),
                ));
            }
        };
        let package = Package::named_checked(db, package_name)?;
        let namespace = package.namespace(db, namespace_path)?;
        match self.kind {
            IdKind::Type => namespace.type_named(db, name).map(Resolved::Symbol),
            IdKind::Value => namespace.value_named(db, name).map(Resolved::Symbol),
            IdKind::Method | IdKind::Field | IdKind::Variant | IdKind::AssocType => {
                let owner = namespace.type_named(db, name)?;
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
                // `continue`, not `?`: a miss here must fall back to a shorter
                // namespace prefix, or a name that is both a namespace and a
                // type in its parent (`baml.json`) would never resolve as the
                // type. Matches the `[item, member]` arm below.
                if let Some(symbol) = namespace
                    .type_named(db, item)
                    .or_else(|| namespace.value_named(db, item))
                {
                    return Some(Resolved::Symbol(symbol));
                }
                continue;
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
