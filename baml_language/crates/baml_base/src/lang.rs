//! The stdlib packages, by language identity.
//!
//! Every package is an anonymous [`SourceRoot`]; names live on dependency
//! edges. The stdlib's packages are nevertheless known to the compiler itself
//! — `baml` declares the builtin companions and the panic classes, `reflect`
//! the sealed reflection views, `ai` what an LLM function lowers to, a
//! provider package the client a `"provider/model"` shorthand constructs —
//! the way rustc knows `core` and `std`. The compiler refers to them by
//! [`LangPackage`], and the database records which root each one was
//! installed at in a [`LangRoots`]. The manifest name is consulted exactly
//! once, at install; nothing downstream compares a package name string
//! except to read back a name the compiler itself wrote
//! ([`LangPackage::from_manifest_name`]).
//!
//! The compiler's own desugars name these packages. A desugar lowers to a
//! source-shaped path (`spawn { .. }` becomes a call of
//! `baml.spawn.Plan.new`) marked [`PathOrigin::Compiler`], and HIR reads its
//! first segment as the package by identity: a parameter called `ai` does
//! not capture it, and inside the `baml` package itself — where no edge is
//! called `baml` — it still means the package.

#[cfg(doc)]
use crate::PathOrigin;
use crate::files::SourceRoot;

/// A stdlib package, by identity.
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
    /// The test harness: a `test` block lowers to a registrar over its
    /// `TestCollector`.
    Testing,
    /// Assertions for test bodies.
    Assert,
    /// A provider package: the client classes a `"provider/model"` shorthand
    /// constructs.
    Openai,
    /// See [`Self::Openai`].
    Anthropic,
    /// See [`Self::Openai`].
    Typesafeai,
    /// See [`Self::Openai`].
    Google,
    /// See [`Self::Openai`].
    Aws,
    /// See [`Self::Openai`].
    Vercel,
    /// See [`Self::Openai`].
    ClaudeCode,
}

impl LangPackage {
    /// Every stdlib package, in declaration order: `ALL[i] as usize == i`.
    pub const ALL: [Self; 14] = [
        Self::Baml,
        Self::Reflect,
        Self::Ai,
        Self::Log,
        Self::Trace,
        Self::Testing,
        Self::Assert,
        Self::Openai,
        Self::Anthropic,
        Self::Typesafeai,
        Self::Google,
        Self::Aws,
        Self::Vercel,
        Self::ClaudeCode,
    ];

    /// The package's `[package].name` in the stdlib manifests — the ONE
    /// spelling the installer matches to find the root, and the segment a
    /// compiler-written path spells it by.
    pub const fn manifest_name(self) -> &'static str {
        match self {
            Self::Baml => "baml",
            Self::Reflect => "reflect",
            Self::Ai => "ai",
            Self::Log => "log",
            Self::Trace => "trace",
            Self::Testing => "testing",
            Self::Assert => "assert",
            Self::Openai => "openai",
            Self::Anthropic => "anthropic",
            Self::Typesafeai => "typesafeai",
            Self::Google => "google",
            Self::Aws => "aws",
            Self::Vercel => "vercel",
            Self::ClaudeCode => "claude_code",
        }
    }

    /// The package a compiler-written path's first segment spells
    /// ([`PathOrigin::Compiler`]): the inverse of [`Self::manifest_name`].
    /// `None` names no stdlib package — a desugar wrote a path into a
    /// package that does not exist, which is a compiler bug.
    #[must_use]
    pub fn from_manifest_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|package| package.manifest_name() == name)
    }

    /// Whether `name` is some language package's manifest name. `const`, so
    /// [`compiler_path!`](crate::compiler_path) rejects a misspelled package
    /// when the compiler is built rather than when the path is read.
    pub const fn is_manifest_name(name: &str) -> bool {
        let mut i = 0;
        while i < Self::ALL.len() {
            if str_eq(Self::ALL[i].manifest_name(), name) {
                return true;
            }
            i += 1;
        }
        false
    }
}

/// `a == b` for `const` evaluation.
const fn str_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// A written type spelling as it appears in a stdlib native KEY: every path
/// headed by `root.` is spelled under `package`, the stdlib package's fixed
/// name, so `root.TypeView` written inside `reflect` keys as
/// `reflect.TypeView`. A native key names a `$rust_function` body for the
/// codegen's dispatch tables, and both producers — the codegen extractor
/// reading source text and the MIR lowering reading the HIR type ref — spell
/// the `{iface}$for${target}` segment through this one function so the two
/// cannot drift. The segment is otherwise the written text verbatim — a
/// string literal in a type argument included, whatever it contains.
///
/// A key is not a name: nothing resolves it, and `Function.name` renders the
/// canonical display form. Spelling the package here only keeps the key —
/// and the Rust identifiers the codegen derives from it — reading as the
/// package rather than as a namespace called `root`.
#[must_use]
pub fn native_key_spelling(package: &str, written: &str) -> String {
    const ROOT: &str = "root.";
    let mut out = String::with_capacity(written.len());
    let mut rest = written;
    let mut path_start = true;
    while !rest.is_empty() {
        if path_start && rest.starts_with(ROOT) {
            out.push_str(package);
            out.push('.');
            rest = &rest[ROOT.len()..];
            path_start = false;
            continue;
        }
        if let Some(literal) = string_literal_prefix(rest) {
            out.push_str(literal);
            rest = &rest[literal.len()..];
            path_start = false;
            continue;
        }
        let c = rest
            .chars()
            .next()
            .unwrap_or_else(|| unreachable!("non-empty"));
        path_start = !(c.is_alphanumeric() || c == '_' || c == '.');
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

/// The string literal `text` starts with, closing quote included, if it
/// starts with one: `"…"` with backslash escapes. An unterminated literal
/// runs to the end of `text`.
fn string_literal_prefix(text: &str) -> Option<&str> {
    let mut chars = text.char_indices();
    if chars.next()?.1 != '"' {
        return None;
    }
    let mut escaped = false;
    for (index, c) in chars {
        match (escaped, c) {
            (true, _) => escaped = false,
            (false, '\\') => escaped = true,
            (false, '"') => return Some(&text[..=index]),
            (false, _) => {}
        }
    }
    Some(text)
}

/// Where the stdlib packages are installed. A package is absent when the
/// stdlib (or that package) is not installed in the database, in which case
/// every identity test against it is `false`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct LangRoots {
    /// Indexed by the package's position in [`LangPackage::ALL`].
    roots: [Option<SourceRoot>; LangPackage::ALL.len()],
}

impl LangRoots {
    /// The root `package` is installed at, if any.
    #[must_use]
    pub fn get(self, package: LangPackage) -> Option<SourceRoot> {
        self.roots[package as usize]
    }

    /// Whether `root` is the installed `package`.
    #[must_use]
    pub fn is(self, package: LangPackage, root: SourceRoot) -> bool {
        self.get(package) == Some(root)
    }

    /// This table with `package` installed at `root`.
    #[must_use]
    pub fn with(mut self, package: LangPackage, root: SourceRoot) -> Self {
        self.roots[package as usize] = Some(root);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::{LangPackage, native_key_spelling};

    #[test]
    fn a_native_key_spells_root_as_the_package() {
        assert_eq!(
            native_key_spelling("reflect", "root.TypeView$for$Type"),
            "reflect.TypeView$for$Type"
        );
        assert_eq!(
            native_key_spelling("baml", "root.io.Read$for$root.fs.File"),
            "baml.io.Read$for$baml.fs.File"
        );
        assert_eq!(
            native_key_spelling("baml", "Add<root.time.Duration>"),
            "Add<baml.time.Duration>"
        );
        // Only a path HEAD is `root`: an item or namespace of that name is
        // left alone, and so is a bare `root`.
        assert_eq!(native_key_spelling("baml", "sub.root.Foo"), "sub.root.Foo");
        assert_eq!(native_key_spelling("baml", "rooted.Foo"), "rooted.Foo");
        assert_eq!(native_key_spelling("baml", "root"), "root");
        // A string literal is not a path, whatever it says.
        assert_eq!(
            native_key_spelling("baml", r#"Tag<"root.x", root.Foo>"#),
            r#"Tag<"root.x", baml.Foo>"#
        );
        assert_eq!(
            native_key_spelling("baml", r#"Tag<"a\"root.x", root.Foo>"#),
            r#"Tag<"a\"root.x", baml.Foo>"#
        );
    }

    #[test]
    fn every_package_is_at_its_own_index_and_round_trips_its_name() {
        for (index, package) in LangPackage::ALL.into_iter().enumerate() {
            assert_eq!(package as usize, index);
            assert_eq!(
                LangPackage::from_manifest_name(package.manifest_name()),
                Some(package)
            );
        }
        assert_eq!(LangPackage::from_manifest_name("user"), None);
    }
}
