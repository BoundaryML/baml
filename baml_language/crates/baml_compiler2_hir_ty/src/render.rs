//! Spelling compile-time types from a viewpoint.
//!
//! A compile-time head ([`DeclName`]) carries the declaring root, not a name,
//! so nothing renders it without a viewpoint: which root is looking, and how
//! every root is spelled ([`Spelling`]). This module is the one place that
//! decision is made for the type checker's own output — diagnostics and
//! canonical dumps. Every other renderer of compile-time types (the IDE's
//! hover, describe) implements [`TyRenderStrategy`] with its own policy.

use baml_base::SourceRoot;
use baml_compiler2_hir::package::{Spelling, spelling};
use baml_type::{DeclName, Interface, LoweringTy, Name, Ty, TyRenderStrategy};

/// A viewpoint: the root looking (when there is one) and the spelling table.
///
/// Rendering *user-facing* (a viewer is set) elides the viewer's own
/// package and shows synthetic effect parameters as `callback`; rendering
/// *canonically* (no viewer) spells every package and every parameter
/// verbatim, which is what dumps and identity-bearing text expect.
#[derive(Clone, Copy)]
pub struct Viewpoint<'a> {
    spelling: &'a Spelling,
    viewer: Option<SourceRoot>,
}

impl<'a> Viewpoint<'a> {
    /// User-facing rendering as seen from `viewer`'s package.
    pub fn user_facing(db: &'a dyn baml_compiler2_ppir::Db, viewer: SourceRoot) -> Self {
        Self {
            spelling: spelling(db),
            viewer: Some(viewer),
        }
    }

    /// Canonical rendering: every package spelled, nothing elided.
    pub fn canonical(db: &'a dyn baml_compiler2_ppir::Db) -> Self {
        Self {
            spelling: spelling(db),
            viewer: None,
        }
    }

    /// A viewpoint over an explicit spelling table (user-facing when `viewer`
    /// is set), for callers that hold the table outside a database.
    pub fn over(spelling: &'a Spelling, viewer: Option<SourceRoot>) -> Self {
        Self { spelling, viewer }
    }

    /// The root looking, if this is a user-facing viewpoint.
    pub fn viewer(&self) -> Option<SourceRoot> {
        self.viewer
    }

    /// The spelling table this viewpoint renders through.
    pub fn spelling(&self) -> &'a Spelling {
        self.spelling
    }

    /// The dotted path of a declaration: `ns.Name` inside the viewer's own
    /// package, `pkg.ns.Name` elsewhere.
    pub fn path(&self, decl: &DeclName) -> String {
        let mut parts: Vec<&str> = Vec::new();
        if self.viewer != Some(decl.root()) {
            parts.push(self.spelling.of(decl.root()).as_str());
        }
        parts.extend(decl.namespace().iter().map(Name::as_str));
        parts.push(decl.name().as_str());
        parts.join(".")
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
