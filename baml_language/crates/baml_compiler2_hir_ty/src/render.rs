//! Spelling compile-time types from a viewpoint.
//!
//! A compile-time head ([`DeclName`]) carries the declaring root, not a name,
//! so nothing renders it without a viewpoint: which root is looking, and how
//! every root is spelled ([`Spelling`]). This module is the one place that
//! decision is made for the type checker's own output — diagnostics and
//! canonical dumps. Every other renderer of compile-time types (the IDE's
//! hover, describe) implements [`TyRenderStrategy`] with its own policy.

use std::borrow::Cow;

use baml_base::{SourceFile, SourceRoot};
use baml_compiler2_hir::package::{ROOT_SEGMENT, Spelling, edge_table, spelling};
use baml_type::{DeclName, Interface, LoweringTy, Name, Ty, TyRenderStrategy};

/// A viewpoint: the root looking (when there is one), the namespace in it
/// that is looking, the names it reaches its dependencies by, and the
/// spelling table.
///
/// Rendering *user-facing* (a viewer is set) spells a declaration as source
/// in the viewer's namespace writes it (`TYPE_SYSTEM.md` § Declarations,
/// Packages, and Names): a declaration of that namespace bare, any other
/// declaration of the viewer's package `root.`-qualified, every dependency
/// by the viewer's edge name for it, a declaration a dependency re-exports
/// by the path the re-export gives it — and a package the viewer has no
/// edge to by its provenance spelling ([`Spelling::of`]); synthetic effect
/// parameters show as `callback`. Rendering *canonically* (no viewer)
/// spells every package by its provenance spelling and every parameter
/// verbatim, which is what dumps and identity-bearing text expect.
#[derive(Clone)]
pub struct Viewpoint<'a> {
    spelling: &'a Spelling,
    viewer: Option<SourceRoot>,
    /// The namespace of the viewer's package that is looking: empty for the
    /// package's root namespace, which is also the view when no file is
    /// looking ([`user_facing`](Self::user_facing)).
    namespace: Cow<'a, [Name]>,
    /// The viewer's edge table — every package it reaches, by the name it
    /// reaches it under ([`edge_table`]); empty without a viewer.
    edges: &'a [(Name, SourceRoot)],
    /// The database the roots live in: the viewer's re-exports and every
    /// package's own name and edges are read from it.
    db: &'a dyn baml_compiler2_hir::Db,
}

/// A declaration that source in the viewer's package cannot write: its
/// package is neither the viewer nor one of the viewer's edges (a
/// transitive dependency's type reaching the viewer through a signature, a
/// foreign package behind a mount). The explicit outcome of
/// [`Viewpoint::source_path`]; never spelled by a stand-in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unspellable(pub DeclName);

/// A package the viewer has no edge to: the one outcome of
/// [`Viewpoint::package_segment`] a source path cannot be built from.
struct NoEdge;

impl<'a> Viewpoint<'a> {
    /// User-facing rendering as seen from `viewer`'s package, from its root
    /// namespace: the view of a reader addressing the package as a whole
    /// (a host, `baml describe`) rather than from one of its files.
    pub fn user_facing(db: &'a dyn baml_compiler2_hir::Db, viewer: SourceRoot) -> Self {
        Self::for_namespace(db, viewer, Cow::Borrowed(&[]))
    }

    /// User-facing rendering as seen from `file`: its package and its
    /// namespace, so every declaration spells as source in that file writes
    /// it. The viewpoint of a diagnostic reported in `file`.
    pub fn for_file(db: &'a dyn baml_compiler2_hir::Db, file: SourceFile) -> Self {
        let package = baml_compiler2_hir::file_package::file_package(db, file);
        Self::for_namespace(db, package.root, Cow::Owned(package.namespace_path))
    }

    /// User-facing rendering as seen from `namespace` of `viewer`'s package:
    /// [`for_file`](Self::for_file) for a reader whose namespace is known
    /// without a file in hand.
    pub fn for_namespace(
        db: &'a dyn baml_compiler2_hir::Db,
        viewer: SourceRoot,
        namespace: Cow<'a, [Name]>,
    ) -> Self {
        Self {
            spelling: spelling(db),
            viewer: Some(viewer),
            namespace,
            edges: edge_table(db, viewer),
            db,
        }
    }

    /// Canonical rendering: every package spelled, nothing elided.
    pub fn canonical(db: &'a dyn baml_compiler2_hir::Db) -> Self {
        Self {
            spelling: spelling(db),
            viewer: None,
            namespace: Cow::Borrowed(&[]),
            edges: &[],
            db,
        }
    }

    /// The path of a declaration of the viewer's own package: bare when it
    /// belongs to the viewing namespace, else `root.<ns>.<Name>` — the two
    /// ways source names an item of its own package, and nothing else.
    fn own_path(&self, decl: &DeclName) -> String {
        if decl.namespace() == self.namespace.as_ref() {
            decl.name().to_string()
        } else {
            join_path(Some(ROOT_SEGMENT), decl)
        }
    }

    /// The name the viewer reaches `root` by, if it has an edge to it.
    fn edge_name(&self, root: SourceRoot) -> Option<&Name> {
        self.edges
            .iter()
            .find(|(_, target)| *target == root)
            .map(|(name, _)| name)
    }

    /// The package segment of `decl`'s path from this viewpoint: none inside
    /// the viewer's own package, the viewer's edge name where it has one —
    /// unless an item of the viewing namespace is spelled like that edge, in
    /// which case the name reads the item and the package is out of reach
    /// from here (`TYPE_SYSTEM.md` § Declarations, Packages, and Names) — the
    /// provenance spelling when no viewer is looking (canonical), and
    /// [`NoEdge`] for a package a viewer reaches no other way — which source
    /// in the viewer's package cannot write.
    fn package_segment(&self, root: SourceRoot) -> Result<Option<&str>, NoEdge> {
        if self.viewer == Some(root) {
            return Ok(None);
        }
        match (self.edge_name(root), self.viewer) {
            (Some(name), Some(_)) if self.edge_is_shadowed(name) => Err(NoEdge),
            (Some(name), _) => Ok(Some(name.as_str())),
            (None, Some(_)) => Err(NoEdge),
            (None, None) => Ok(Some(self.spelling.of(root).as_str())),
        }
    }

    /// Whether an item of the viewing namespace is spelled like the viewer's
    /// edge `name`: source there reads the name as the item, so the edge is
    /// out of reach — for the package it leads to and for everything that
    /// package re-exports alike (`TYPE_SYSTEM.md` § Declarations, Packages,
    /// and Names).
    fn edge_is_shadowed(&self, name: &Name) -> bool {
        self.viewer.is_some_and(|viewer| {
            baml_compiler2_hir::namespace::namespace_declares(
                self.db,
                viewer,
                &self.namespace,
                name,
            )
        })
    }

    /// The dotted path of `decl` as source in the viewer's namespace writes
    /// it: bare for a declaration of that namespace, `root.ns.Name` for the
    /// rest of the viewer's package, `edge.ns.Name` for a dependency,
    /// `edge.ns.Name` by the re-export's path for a declaration a dependency
    /// re-exports. [`Unspellable`] when the viewer reaches the declaration no
    /// such way — and for every declaration when no viewer is looking: a
    /// canonical rendering is for display, and nothing writes it as source.
    pub fn source_path(&self, decl: &DeclName) -> Result<String, Unspellable> {
        if self.viewer.is_none() {
            return Err(Unspellable(decl.clone()));
        }
        match self.package_segment(decl.root()) {
            Ok(None) => Ok(self.own_path(decl)),
            Ok(Some(package)) => Ok(join_path(Some(package), decl)),
            Err(NoEdge) => self
                .reexport_path(decl)
                .ok_or_else(|| Unspellable(decl.clone())),
        }
    }

    /// The path a re-export reachable through one of the viewer's edges
    /// gives `decl`, if any: `edge.ns.Name` as the re-export spells it. An
    /// edge an item of the viewing namespace shadows reaches nothing.
    fn reexport_path(&self, decl: &DeclName) -> Option<String> {
        self.edges.iter().find_map(|(edge, target)| {
            if self.edge_is_shadowed(edge) {
                return None;
            }
            let (namespace, name) =
                crate::package_interface::reexport_paths(self.db, *target).get(decl)?;
            Some(
                std::iter::once(edge.as_str())
                    .chain(namespace.iter().map(Name::as_str))
                    .chain(std::iter::once(name.as_str()))
                    .collect::<Vec<_>>()
                    .join("."),
            )
        })
    }

    /// How a package the viewer has no edge to is shown — display only, never
    /// source ([`source_path`](Self::source_path) refuses it) — following
    /// Rust, which prints such a crate's own name: the package's declared
    /// name when it has one; an unnamed package shows the name its nearest
    /// dependent reaches it by (the last hop of the shortest edge path from
    /// the viewer), the one spelling some manifest actually wrote. With no
    /// edge path at all, the provenance spelling.
    pub fn display_package(&self, root: SourceRoot) -> Cow<'a, str> {
        if let Some(own) = root.self_name(self.db) {
            return Cow::Owned(own.to_string());
        }
        self.viewer
            .and_then(|viewer| baml_compiler2_hir::package::edge_path(self.db, viewer, root))
            .and_then(|hops| hops.last().map(|(_, edge, _)| Cow::Owned(edge.to_string())))
            .unwrap_or_else(|| Cow::Borrowed(self.spelling.of(root).as_str()))
    }

    /// `ty` as source in the viewer's package writes it, or the first
    /// declaration in it that this package cannot name.
    pub fn source_text(&self, ty: &Ty) -> Result<String, Unspellable> {
        let strategy = SourceSpelling {
            viewpoint: self,
            unspellable: std::cell::Cell::new(None),
        };
        let text = ty.render_with(&strategy);
        match strategy.unspellable.into_inner() {
            Some(decl) => Err(Unspellable(decl)),
            None => Ok(text),
        }
    }

    /// The root looking, if this is a user-facing viewpoint.
    pub fn viewer(&self) -> Option<SourceRoot> {
        self.viewer
    }

    /// The package prefix of a path from this viewpoint, for display: `None`
    /// inside the viewer's own package (elided — an own-package path then
    /// follows [`path`](Self::path)'s rule: bare in the viewing namespace,
    /// `root.`-qualified elsewhere), the viewer's edge name for a dependency,
    /// and the display name of a package the viewer has no edge to
    /// ([`display_package`](Self::display_package); a diagnostic must still
    /// name it). Callers assembling dotted paths of their own take it
    /// directly.
    pub fn package_prefix(&self, root: SourceRoot) -> Option<Cow<'_, str>> {
        match self.package_segment(root) {
            Ok(package) => package.map(Cow::Borrowed),
            Err(NoEdge) => Some(self.display_package(root)),
        }
    }

    /// The dotted path of a declaration for display: bare for a declaration
    /// of the viewing namespace, `root.ns.Name` for the rest of the viewer's
    /// package, `edge.ns.Name` for a dependency or for a declaration a
    /// dependency re-exports (by the re-export's path), and the display name
    /// of a package the viewer reaches no other way
    /// ([`display_package`](Self::display_package); a diagnostic must still
    /// name it).
    pub fn path(&self, decl: &DeclName) -> String {
        match self.package_segment(decl.root()) {
            Ok(None) => self.own_path(decl),
            Ok(Some(package)) => join_path(Some(package), decl),
            Err(NoEdge) => self
                .reexport_path(decl)
                .unwrap_or_else(|| join_path(Some(&self.display_package(decl.root())), decl)),
        }
    }
}

fn join_path(package: Option<&str>, decl: &DeclName) -> String {
    package
        .into_iter()
        .chain(decl.namespace().iter().map(Name::as_str))
        .chain(std::iter::once(decl.name().as_str()))
        .collect::<Vec<_>>()
        .join(".")
}

/// The strategy behind [`Viewpoint::source_text`]: spells every head as
/// source would, recording the first head source cannot spell.
struct SourceSpelling<'v, 'a> {
    viewpoint: &'v Viewpoint<'a>,
    unspellable: std::cell::Cell<Option<DeclName>>,
}

impl TyRenderStrategy<DeclName> for SourceSpelling<'_, '_> {
    fn qtn(&self, qtn: &DeclName) -> String {
        match self.viewpoint.source_path(qtn) {
            Ok(path) => path,
            Err(Unspellable(decl)) => {
                // Record the first one and keep it. `take` would empty the
                // cell on every later head, so an even number of unspellable
                // heads left it `None` and the render reported success.
                let recorded = self.unspellable.take();
                self.unspellable.set(recorded.or(Some(decl)));
                self.viewpoint.path(qtn)
            }
        }
    }

    fn type_var(&self, name: &Name) -> String {
        self.viewpoint.type_var(name)
    }
}

impl TyRenderStrategy<DeclName> for Viewpoint<'_> {
    fn qtn(&self, qtn: &DeclName) -> String {
        self.path(qtn)
    }

    fn type_var(&self, name: &Name) -> String {
        // A synthetic effect parameter (`__effect_param_N`) is an
        // implementation detail of effect-polymorphic callbacks; show it as
        // `callback` in user-facing output.
        if self.viewer.is_some() && baml_type::is_synthetic_effect_param(name) {
            "callback".to_string()
        } else {
            name.to_string()
        }
    }
}

/// Something that spells itself from a viewpoint.
pub trait Spell {
    fn spell(&self, viewpoint: &Viewpoint<'_>) -> String;
}

impl Spell for DeclName {
    fn spell(&self, viewpoint: &Viewpoint<'_>) -> String {
        viewpoint.path(self)
    }
}

impl Spell for Ty {
    fn spell(&self, viewpoint: &Viewpoint<'_>) -> String {
        self.render_with(viewpoint)
    }
}

impl Spell for LoweringTy {
    fn spell(&self, viewpoint: &Viewpoint<'_>) -> String {
        self.render_with(viewpoint)
    }
}

impl Spell for Interface {
    fn spell(&self, viewpoint: &Viewpoint<'_>) -> String {
        self.to_ty().render_with(viewpoint)
    }
}

impl<T: Spell> Spell for &T {
    fn spell(&self, viewpoint: &Viewpoint<'_>) -> String {
        (**self).spell(viewpoint)
    }
}

impl<T: Spell> Spell for Box<T> {
    fn spell(&self, viewpoint: &Viewpoint<'_>) -> String {
        (**self).spell(viewpoint)
    }
}
