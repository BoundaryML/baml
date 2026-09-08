//! Spelling compile-time types from a viewpoint.
//!
//! A compile-time head ([`DeclName`]) carries the declaring root, not a name,
//! so nothing renders it without a viewpoint: which root is looking, and how
//! every root is spelled ([`Spelling`]). This module is the one place that
//! decision is made for the type checker's own output — diagnostics and
//! canonical dumps. Every other renderer of compile-time types (the IDE's
//! hover, describe) implements [`TyRenderStrategy`] with its own policy.

use baml_base::{Dependency, SourceRoot};
use baml_compiler2_hir::package::{Spelling, spelling};
use baml_type::{DeclName, Interface, LoweringTy, Name, Ty, TyRenderStrategy};

/// A viewpoint: the root looking (when there is one), the names it reaches
/// its dependencies by, and the spelling table.
///
/// Rendering *user-facing* (a viewer is set) spells the viewer's own
/// package bare, every dependency by the viewer's edge name for it — what
/// source in that package writes — and a package the viewer has no edge to
/// by its provenance spelling ([`Spelling::of`]); synthetic effect
/// parameters show as `callback`. Rendering *canonically* (no viewer)
/// spells every package by its provenance spelling and every parameter
/// verbatim, which is what dumps and identity-bearing text expect.
#[derive(Clone, Copy)]
pub struct Viewpoint<'a> {
    spelling: &'a Spelling,
    viewer: Option<SourceRoot>,
    /// The viewer's dependency edges; empty without a viewer or a database.
    edges: &'a [Dependency],
}

/// A declaration that source in the viewer's package cannot write: its
/// package is neither the viewer nor one of the viewer's edges (a
/// transitive dependency's type reaching the viewer through a signature, a
/// foreign package behind a mount). The explicit outcome of
/// [`Viewpoint::source_path`]; never spelled by a stand-in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unspellable(pub DeclName);

impl<'a> Viewpoint<'a> {
    /// User-facing rendering as seen from `viewer`'s package.
    pub fn user_facing(db: &'a dyn baml_compiler2_ppir::Db, viewer: SourceRoot) -> Self {
        Self {
            spelling: spelling(db),
            viewer: Some(viewer),
            edges: viewer.dependencies(db),
        }
    }

    /// Canonical rendering: every package spelled, nothing elided.
    pub fn canonical(db: &'a dyn baml_compiler2_ppir::Db) -> Self {
        Self {
            spelling: spelling(db),
            viewer: None,
            edges: &[],
        }
    }

    /// A viewpoint over an explicit spelling table (user-facing when `viewer`
    /// is set, with no edges), for callers that hold the table outside a
    /// database.
    pub fn over(spelling: &'a Spelling, viewer: Option<SourceRoot>) -> Self {
        Self {
            spelling,
            viewer,
            edges: &[],
        }
    }

    /// The name the viewer reaches `root` by, if it has an edge to it.
    fn edge_name(&self, root: SourceRoot) -> Option<&'a Name> {
        self.edges
            .iter()
            .find(|edge| edge.root == root)
            .map(|edge| &edge.name)
    }

    /// The package segment of `decl`'s path from this viewpoint: none inside
    /// the viewer's own package, the viewer's edge name where it has one,
    /// else the provenance spelling — which source in the viewer's package
    /// cannot write, so [`source_path`](Self::source_path) refuses it.
    fn package_segment(&self, root: SourceRoot) -> Result<Option<&'a str>, Unspellable> {
        if self.viewer == Some(root) {
            return Ok(None);
        }
        match self.edge_name(root) {
            Some(name) => Ok(Some(name.as_str())),
            None if self.viewer.is_some() => Err(Unspellable(DeclName::in_root(
                root,
                Vec::new(),
                Name::new(""),
            ))),
            None => Ok(Some(self.spelling.of(root).as_str())),
        }
    }

    /// The dotted path of `decl` as source in the viewer's package writes
    /// it: bare or `ns.Name` inside the viewer's package, `edge.ns.Name` for
    /// a dependency. [`Unspellable`] when the viewer has no edge to the
    /// declaring package.
    pub fn source_path(&self, decl: &DeclName) -> Result<String, Unspellable> {
        let package = self
            .package_segment(decl.root())
            .map_err(|_| Unspellable(decl.clone()))?;
        Ok(join_path(package, decl))
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

    /// The spelling table this viewpoint renders through.
    pub fn spelling(&self) -> &'a Spelling {
        self.spelling
    }

    /// The dotted path of a declaration for display: `ns.Name` inside the
    /// viewer's own package, `edge.ns.Name` for a dependency, and the
    /// provenance spelling for a package the viewer has no edge to (a
    /// diagnostic must still name it).
    pub fn path(&self, decl: &DeclName) -> String {
        let package = match self.package_segment(decl.root()) {
            Ok(package) => package,
            Err(_) => Some(self.spelling.of(decl.root()).as_str()),
        };
        join_path(package, decl)
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
                if self.unspellable.take().is_none() {
                    self.unspellable.set(Some(decl));
                }
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
