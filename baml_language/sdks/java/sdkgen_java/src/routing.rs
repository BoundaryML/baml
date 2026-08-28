//! Routing: turns a `baml_codegen_types::Name` into the Java package
//! path (under `baml_sdk/`) where that symbol's generated source lives.
//! Single source of truth for per-symbol placement; `to_source_code`
//! uses this to place per-symbol files, and later phases reuse it when
//! resolving cross-package type references.
//!
//! The namespace logic is identical to the TS emitter
//! (`sdkgen_typescript_node/src/routing.rs`); the structural difference
//! is that Java has no per-directory barrel file — a package exists by
//! virtue of its directory, so routing only ever produces directories,
//! never an `index.*` filename.
//!
//! The `pkg` field in the codegen-facing `Name` is the literal package
//! name from HIR — `"user"` for project files, `"baml"` for stdlib,
//! `"<vendor>"` for declared external packages.
//!
//! `"baml"` routes under `baml/`, anything else under `vendor/<pkg>/`.
//!
//! PPIR `$stream` partial-output classes do **not** get a separate package:
//! `$` is a valid Java identifier character, so a partial model keeps its
//! `$stream`-suffixed name and is emitted beside its base type in the same
//! package. Routing is therefore independent of the `$stream`
//! suffix and of the symbol kind.
//!
//! Unlike TS (where `sanitize_module_segment` is a no-op), Java package
//! segments MUST avoid Java's reserved words — the `void` fixture
//! namespace is a real occurrence. Keyword segments get a `$` suffix
//! (`void` → `void$`), the same escape the conventions doc uses for
//! symbol-level collisions (see
//! `sdks/agent-docs/bridge-ref/ref-java-codegen-conventions.md`).

use baml_codegen_types::Name;

/// Package path under `baml_sdk/`. Empty segments means the root
/// package (`baml_sdk` itself).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct PackagePath {
    pub(crate) segments: Vec<String>,
}

impl PackagePath {
    /// Fully-qualified Java package name, e.g. `baml_sdk.lorem`.
    pub(crate) fn java_package(&self) -> String {
        let mut out = String::from("baml_sdk");
        for seg in &self.segments {
            out.push('.');
            out.push_str(seg);
        }
        out
    }
}

/// Java reserved words (JLS 3.9) plus the `true` / `false` / `null`
/// literals and the reserved identifier `_` — none of these may appear
/// as a package segment or simple identifier.
pub(crate) const JAVA_KEYWORDS: &[&str] = &[
    "abstract",
    "assert",
    "boolean",
    "break",
    "byte",
    "case",
    "catch",
    "char",
    "class",
    "const",
    "continue",
    "default",
    "do",
    "double",
    "else",
    "enum",
    "extends",
    "final",
    "finally",
    "float",
    "for",
    "goto",
    "if",
    "implements",
    "import",
    "instanceof",
    "int",
    "interface",
    "long",
    "native",
    "new",
    "package",
    "private",
    "protected",
    "public",
    "return",
    "short",
    "static",
    "strictfp",
    "super",
    "switch",
    "synchronized",
    "this",
    "throw",
    "throws",
    "transient",
    "try",
    "void",
    "volatile",
    "while",
    "true",
    "false",
    "null",
    "_",
];

/// Method names a generated Java method must not use:
/// * `java.lang.Object`'s final methods (`wait`/`notify`/`notifyAll`/
///   `getClass`) — an override cannot compile (`wait() in Process cannot
///   override wait() in Object`) and a `static` one cannot hide them;
/// * `equals`/`hashCode`/`toString` — every generated class already declares
///   them (deep-equality + display), so a user method with the same name
///   either collides outright or overloads them confusingly.
///
/// Escaped in method position with the same `$` suffix as keywords.
pub(crate) const JAVA_OBJECT_FINAL_METHODS: &[&str] = &[
    "wait",
    "notify",
    "notifyAll",
    "getClass",
    "equals",
    "hashCode",
    "toString",
];

/// Sanitize a *method* name: the keyword escape plus the
/// `java.lang.Object` final-method escape (`wait` → `wait$`). The runtime
/// binding keeps the BAML name; only the Java-visible identifier shifts.
pub(crate) fn java_method_identifier(seg: &str) -> String {
    if JAVA_OBJECT_FINAL_METHODS.contains(&seg) {
        return format!("{seg}$");
    }
    java_identifier(seg)
}

/// Sanitize a name segment into a legal Java identifier. Reserved
/// words get the conventions doc's `$` suffix escape (`void` →
/// `void$`, `new` → `new$`); any character that is not a valid Java
/// identifier part is replaced with `_`. BAML identifiers are already
/// close to Java's grammar, so in practice this is the keyword escape
/// plus a defensive character filter.
pub(crate) fn java_identifier(seg: &str) -> String {
    if JAVA_KEYWORDS.contains(&seg) {
        return format!("{seg}$");
    }
    let mut out = String::with_capacity(seg.len());
    for (i, ch) in seg.chars().enumerate() {
        let valid = if i == 0 {
            ch.is_alphabetic() || ch == '_' || ch == '$'
        } else {
            ch.is_alphanumeric() || ch == '_' || ch == '$'
        };
        out.push(if valid { ch } else { '_' });
    }
    out
}

/// Route a pool entry to its package directory (under `baml_sdk/`).
///
/// Routing depends only on the symbol's package + namespace path. The
/// A `$stream` suffix on a PPIR partial-output class does not influence
/// placement: the partial model lives beside its final-output type.
pub(crate) fn route(name: &Name) -> PackagePath {
    let mut segs: Vec<String> = Vec::new();

    match name.package().as_str() {
        "user" => {}
        "baml" => segs.push("baml".to_string()),
        other => {
            segs.push("vendor".to_string());
            segs.push(java_identifier(other));
        }
    }

    for seg in name.namespace() {
        segs.push(java_identifier(seg.as_str()));
    }

    PackagePath { segments: segs }
}

#[cfg(test)]
mod tests {
    use baml_base::Name as BaseName;

    use super::*;

    fn name(pkg: &str, ns: &[&str], n: &str) -> Name {
        Name::new(
            BaseName::new(pkg),
            ns.iter().map(|s| BaseName::new(*s)).collect(),
            BaseName::new(n),
        )
    }

    #[test]
    fn user_no_ns_routes_to_root_package() {
        let pp = route(&name("user", &[], "Foo"));
        assert!(pp.segments.is_empty());
        assert_eq!(pp.java_package(), "baml_sdk");
    }

    #[test]
    fn user_with_ns_routes_under_ns() {
        let pp = route(&name("user", &["lorem"], "Resume"));
        assert_eq!(pp.segments, vec!["lorem".to_string()]);
        assert_eq!(pp.java_package(), "baml_sdk.lorem");
    }

    #[test]
    fn vendor_routes_under_vendor_pkg() {
        let pp = route(&name("aws", &["s3"], "Bucket"));
        assert_eq!(
            pp.segments,
            vec!["vendor".to_string(), "aws".to_string(), "s3".to_string()]
        );
        assert_eq!(pp.java_package(), "baml_sdk.vendor.aws.s3");
    }

    #[test]
    fn baml_routes_under_baml() {
        let pp = route(&name("baml", &["http"], "Response"));
        assert_eq!(pp.segments, vec!["baml".to_string(), "http".to_string()]);
    }

    #[test]
    fn stream_class_routes_to_base_package() {
        let pp = route(&name("user", &["lorem"], "Resume$stream"));
        assert_eq!(pp.segments, vec!["lorem".to_string()]);
    }

    #[test]
    fn user_deeper_ns() {
        let pp = route(&name("user", &["a", "b"], "Thing"));
        assert_eq!(pp.segments, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn keyword_namespace_segment_is_escaped() {
        // The `void` fixture namespace: `void` is a Java reserved word,
        // so the package segment escapes to `void$`.
        let pp = route(&name("user", &["void"], "VoidProbe"));
        assert_eq!(pp.segments, vec!["void$".to_string()]);
        assert_eq!(pp.java_package(), "baml_sdk.void$");
    }

    #[test]
    fn keyword_vendor_package_is_escaped() {
        let pp = route(&name("new", &[], "Thing"));
        assert_eq!(pp.segments, vec!["vendor".to_string(), "new$".to_string()]);
    }

    #[test]
    fn java_identifier_escapes_and_filters() {
        assert_eq!(java_identifier("void"), "void$");
        assert_eq!(java_identifier("new"), "new$");
        assert_eq!(java_identifier("helpers"), "helpers");
        assert_eq!(java_identifier("Resume$stream"), "Resume$stream");
        assert_eq!(java_identifier("has space"), "has_space");
        assert_eq!(java_identifier("_"), "_$");
    }

    #[test]
    fn object_final_methods_are_escaped_in_method_position() {
        // `baml.sys.Process.wait` — `wait()` cannot override the final
        // `java.lang.Object.wait()`, and a static `wait()` cannot hide it.
        assert_eq!(java_method_identifier("wait"), "wait$");
        assert_eq!(java_method_identifier("notify"), "notify$");
        // Ordinary methods and keywords behave like `java_identifier`.
        assert_eq!(java_method_identifier("close"), "close");
        assert_eq!(java_method_identifier("new"), "new$");
        // Only method position escapes: fields/classes named `wait` are legal.
        // A value class with such a field therefore declares `wait` as the
        // FIELD and `wait$()` as its accessor (see `render_class`).
        assert_eq!(java_identifier("wait"), "wait");
    }
}
