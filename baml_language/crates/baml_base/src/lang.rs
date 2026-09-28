//! The stdlib packages the compiler knows by language identity.
//!
//! Every package is an anonymous [`SourceRoot`]; names live on dependency
//! edges. A handful of stdlib packages are nevertheless special to the
//! compiler itself — `baml` declares the builtin companions and the panic
//! classes, `reflect` the sealed reflection views, and so on — the way rustc
//! knows `core` and `std`. The compiler refers to those by [`LangPackage`],
//! and the database records which root each one was installed at in a
//! [`LangRoots`]. The manifest name is consulted exactly once, at install;
//! nothing downstream compares a package name string.

use crate::files::SourceRoot;

/// A stdlib package the compiler special-cases by identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LangPackage {
    /// The core package: builtin companion classes, `baml.panics`, `baml.ops`.
    Baml,
    /// Reflection: `reflect.Type`, `reflect.AnyFunction`, `reflect.AnyClass`
    /// and the sealed `reflect.<kind>.Type` views.
    Reflect,
    /// LLM functions and clients; the `client:` desugar and prompt lowering
    /// name its declarations.
    Ai,
    /// The `log` package, whose `info`/`debug`/`warn`/`error` are the
    /// compiler intrinsics MIR lowers to log statements.
    Log,
    /// Invocation tracing options accepted by the reserved `$trace` argument.
    Trace,
}

impl LangPackage {
    pub const ALL: [Self; 5] = [Self::Baml, Self::Reflect, Self::Ai, Self::Log, Self::Trace];

    /// The package's `[package].name` in the stdlib manifests — the ONE
    /// spelling the installer matches to find the root.
    pub const fn manifest_name(self) -> &'static str {
        match self {
            Self::Baml => "baml",
            Self::Reflect => "reflect",
            Self::Ai => "ai",
            Self::Log => "log",
            Self::Trace => "trace",
        }
    }
}

/// Where the language packages are installed. A package is absent when the
/// stdlib (or that package) is not installed in the database, in which case
/// every identity test against it is `false`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct LangRoots {
    baml: Option<SourceRoot>,
    reflect: Option<SourceRoot>,
    ai: Option<SourceRoot>,
    log: Option<SourceRoot>,
    trace: Option<SourceRoot>,
}

impl LangRoots {
    /// The root `package` is installed at, if any.
    #[must_use]
    pub fn get(self, package: LangPackage) -> Option<SourceRoot> {
        match package {
            LangPackage::Baml => self.baml,
            LangPackage::Reflect => self.reflect,
            LangPackage::Ai => self.ai,
            LangPackage::Log => self.log,
            LangPackage::Trace => self.trace,
        }
    }

    /// Whether `root` is the installed `package`.
    #[must_use]
    pub fn is(self, package: LangPackage, root: SourceRoot) -> bool {
        self.get(package) == Some(root)
    }

    /// This table with `package` installed at `root`.
    #[must_use]
    pub fn with(mut self, package: LangPackage, root: SourceRoot) -> Self {
        let slot = match package {
            LangPackage::Baml => &mut self.baml,
            LangPackage::Reflect => &mut self.reflect,
            LangPackage::Ai => &mut self.ai,
            LangPackage::Log => &mut self.log,
            LangPackage::Trace => &mut self.trace,
        };
        *slot = Some(root);
        self
    }
}
