//! The per-declaration comparison `BexEngine::hotswap_patch` runs on source.
//!
//! BEP-053's semantic hashes are not built, so a declaration is compared by its
//! tokens: trivia (whitespace and plain comments) never counts as a change.
//! What reaches the running program does, though it is written like
//! formatting or a comment: the text of string and prompt literals, `///`
//! docs, which render into prompts, and `baml:` directives such as a `$trace`
//! hook (BEP-087). Declarations that share a key (an implementation is keyed
//! by bare names) are fingerprinted together, in an order that does not
//! depend on the files', so an edit to either one is seen.

use std::collections::BTreeSet;

use baml_syntax::{
    index::descendants,
    nodes::{
        AnyClassMember, AnyImplMember, AnyItem, FileTree, FunctionDef, ImplementsBlock,
        ImplementsFor, Name,
    },
    token::{Ident, TokenKind, TokenSet as _},
    tree::{AnyNode as _, AnyNodeId, Id},
};
use bex_vm_types::DeclFingerprints;

/// The fingerprint of every declaration in `files`, by project-relative path.
pub fn declaration_fingerprints<'a>(
    files: impl Iterator<Item = (&'a str, &'a str)>,
) -> DeclFingerprints {
    let mut fingerprints = DeclFingerprints::default();
    for (path, text) in files {
        let parsed = baml_parser::parse(text);
        let tree = &parsed.tree;
        let file = File {
            tree,
            namespace: namespace_of(path),
        };
        for &item in tree.list(tree.get(tree.root()).items) {
            file.declaration(item, &mut fingerprints);
        }
    }
    fingerprints
}

/// `a.b.` for a file under `ns_a/ns_b/`, empty at the package root: the
/// compiler's own derivation, so a file named `ns_x.baml` is no namespace.
fn namespace_of(path: &str) -> String {
    let mut prefix = String::new();
    for name in baml_compiler2_hir::file_package::namespace_path(std::path::Path::new(path)) {
        prefix.push_str(name.as_str());
        prefix.push('.');
    }
    prefix
}

/// The last segment of a dotted name with any type arguments removed.
fn simple_name(text: &str) -> String {
    let text: String = text.split_whitespace().collect();
    let base = text.split('<').next().unwrap_or(&text);
    base.rsplit('.').next().unwrap_or(base).to_string()
}

/// Separates the declarations that share a key; no token contains it.
const SHARED: char = '\u{0}';

/// Add `value` under `key`, beside anything another declaration put there,
/// sorted, so the order files are read in never matters.
fn insert(fingerprints: &mut DeclFingerprints, key: String, value: String) {
    fingerprints
        .decls
        .entry(key)
        .and_modify(|existing| {
            let mut parts: Vec<&str> = existing.split(SHARED).collect();
            parts.push(&value);
            parts.sort_unstable();
            *existing = parts.join(&SHARED.to_string());
        })
        .or_insert(value);
}

struct File<'t> {
    tree: &'t FileTree,
    namespace: String,
}

impl File<'_> {
    /// The tokens of `node` that mean something, leaving out the `skip` child
    /// nodes: everything but trivia, so a doc, a directive and a literal's text
    /// stay as written.
    fn text_of(&self, node: AnyNodeId, skip: &[AnyNodeId]) -> String {
        let tokens = self.tree.tokens();
        let skipped: Vec<_> = skip
            .iter()
            .map(|&child| self.tree.arenas().span(child).as_range())
            .collect();
        self.tree
            .arenas()
            .span(node)
            .iter()
            .filter(|idx| !tokens.kind(*idx).is_trivia())
            .filter(|idx| !skipped.iter().any(|range| range.contains(idx)))
            // A comment's trailing spaces are formatting; a literal's are text.
            .map(|idx| match tokens.kind(idx) {
                TokenKind::DocComment | TokenKind::DirectiveComment => {
                    self.tree.token_text(idx).trim_end()
                }
                _ => self.tree.token_text(idx),
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The names `node` refers to, leaving out the `skip` child nodes:
    /// identifiers, which a literal's text and a prompt's never are, though a
    /// `${...}` interpolation's are.
    fn refs_of(&self, node: AnyNodeId, skip: &[AnyNodeId], refs: &mut BTreeSet<String>) {
        let tokens = self.tree.tokens();
        let skipped: Vec<_> = skip
            .iter()
            .map(|&child| self.tree.arenas().span(child).as_range())
            .collect();
        refs.extend(
            self.tree
                .arenas()
                .span(node)
                .iter()
                .filter(|idx| Ident::KINDS.contains(tokens.kind(*idx)))
                .filter(|idx| !skipped.iter().any(|range| range.contains(idx)))
                .map(|idx| self.tree.token_text(idx).to_string()),
        );
    }

    fn insert_decl(
        &self,
        fingerprints: &mut DeclFingerprints,
        key: String,
        node: AnyNodeId,
        skip: &[AnyNodeId],
    ) {
        self.refs_of(
            node,
            skip,
            fingerprints.refs.entry(key.clone()).or_default(),
        );
        // The namespace tells apart declarations that share a key.
        let text = format!("{}\u{1}{}", self.namespace, self.text_of(node, skip));
        insert(fingerprints, key, text);
    }

    fn insert_function(
        &self,
        fingerprints: &mut DeclFingerprints,
        key: String,
        id: Id<FunctionDef>,
    ) {
        // The runtime widens a `throws` literal to its primitive, so the
        // declared clause is compared here, as written.
        if let Some(clause) = self.tree.get(id).throws {
            insert(
                fingerprints,
                format!("throws {key}"),
                self.text_of(AnyNodeId::of(clause), &[]),
            );
        }
        self.insert_decl(fingerprints, key, AnyNodeId::of(id), &[]);
    }

    fn name(&self, name: Id<Name>) -> &str {
        self.tree.token_text(self.tree.get(name).token.idx)
    }

    fn function_name(&self, id: Id<FunctionDef>) -> Option<&str> {
        self.tree.get(id).name.ok().map(|name| self.name(name))
    }

    /// The bare name of a type as written, such as an implementation's
    /// interface or target.
    fn type_name<T: baml_syntax::tree::AnyNode>(
        &self,
        ty: Result<T, baml_syntax::tree::Missing>,
    ) -> Option<String> {
        let node = ty.ok()?.any_node_id()?;
        Some(simple_name(&self.text_of(node, &[])))
    }

    fn declaration(&self, item: AnyItem, fingerprints: &mut DeclFingerprints) {
        let ns = &self.namespace;
        let tree = self.tree;
        let node = item
            .any_node_id()
            .unwrap_or_else(|| unreachable!("every alternative of `AnyItem` is a node"));
        let named = |name: Option<Id<Name>>, kind: &str, fingerprints: &mut DeclFingerprints| {
            if let Some(name) = name {
                let key = format!("{kind} {ns}{}", self.name(name));
                self.insert_decl(fingerprints, key, node, &[]);
            }
        };
        match item {
            AnyItem::Function(id) => {
                if let Some(name) = self.function_name(id) {
                    self.insert_function(fingerprints, format!("fn {ns}{name}"), id);
                }
            }
            AnyItem::Class(id) => self.class(id, fingerprints),
            AnyItem::Enum(id) => named(tree.get(id).name.ok(), "enum", fingerprints),
            AnyItem::Interface(id) => named(tree.get(id).name.ok(), "interface", fingerprints),
            AnyItem::TypeAlias(id) => named(tree.get(id).name.ok(), "type", fingerprints),
            AnyItem::ClientValue(id) => named(Some(tree.get(id).name), "client", fingerprints),
            // A top-level `let` or `const`. Its key is `const` either way,
            // which is how a patch names a `let` it must initialize.
            AnyItem::Let(id) => {
                let name = tree
                    .get(tree.get(id).stmt)
                    .pattern
                    .ok()
                    .and_then(|pattern| {
                        descendants(tree, pattern.any_node_id()?).find_map(AnyNodeId::cast::<Name>)
                    });
                named(name, "const", fingerprints);
            }
            AnyItem::ImplementsFor(id) => self.implements_for(id, fingerprints),
            // Tests declare nothing the running program reaches; removed
            // syntax and recovered text do not compile.
            AnyItem::TypeBinding(_)
            | AnyItem::Test(_)
            | AnyItem::Removed(_)
            | AnyItem::Bogus(_) => {}
        }
    }

    fn class(&self, id: Id<baml_syntax::nodes::ClassDef>, fingerprints: &mut DeclFingerprints) {
        let tree = self.tree;
        let class_def = tree.get(id);
        let Ok(name) = class_def.name else {
            return;
        };
        let class = self.name(name).to_string();
        let ns = &self.namespace;
        let members = tree.list(class_def.members);
        // Methods and implementations have keys of their own.
        let own: Vec<AnyNodeId> = members
            .iter()
            .filter(|member| {
                matches!(
                    member,
                    AnyClassMember::Method(_) | AnyClassMember::Implements(_)
                )
            })
            .filter_map(baml_syntax::tree::AnyNode::any_node_id)
            .collect();
        self.insert_decl(
            fingerprints,
            format!("class {ns}{class}"),
            AnyNodeId::of(id),
            &own,
        );
        for member in members {
            match *member {
                AnyClassMember::Method(method) => {
                    if let Some(name) = self.function_name(method) {
                        self.insert_function(
                            fingerprints,
                            format!("method {ns}{class}.{name}"),
                            method,
                        );
                    }
                }
                AnyClassMember::Implements(block) => {
                    self.implements_block(block, &class, fingerprints);
                }
                _ => {}
            }
        }
    }

    fn implements_block(
        &self,
        id: Id<ImplementsBlock>,
        class: &str,
        fingerprints: &mut DeclFingerprints,
    ) {
        let block = self.tree.get(id);
        if let Some(interface) = self.type_name(block.target) {
            self.implementation(
                &interface,
                class,
                AnyNodeId::of(id),
                block.members,
                fingerprints,
            );
        }
    }

    fn implements_for(&self, id: Id<ImplementsFor>, fingerprints: &mut DeclFingerprints) {
        let implements = self.tree.get(id);
        if let (Some(interface), Some(target)) = (
            self.type_name(implements.target),
            self.type_name(implements.self_ty),
        ) {
            self.implementation(
                &interface,
                &target,
                AnyNodeId::of(id),
                implements.members,
                fingerprints,
            );
        }
    }

    /// An implementation's header, and each of its methods under its own key.
    fn implementation(
        &self,
        interface: &str,
        target: &str,
        node: AnyNodeId,
        members: baml_syntax::tree::List<AnyImplMember>,
        fingerprints: &mut DeclFingerprints,
    ) {
        let members = self.tree.list(members);
        let methods: Vec<Id<FunctionDef>> = members
            .iter()
            .filter_map(|member| match member {
                AnyImplMember::Method(method) => Some(*method),
                _ => None,
            })
            .collect();
        let skip: Vec<AnyNodeId> = methods
            .iter()
            .map(|&method| AnyNodeId::of(method))
            .collect();
        self.insert_decl(
            fingerprints,
            format!("impl {interface} for {target}"),
            node,
            &skip,
        );
        for method in methods {
            if let Some(name) = self.function_name(method) {
                self.insert_function(
                    fingerprints,
                    format!("implmethod {interface} for {target}.{name}"),
                    method,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::declaration_fingerprints;

    fn fingerprints(files: &[(&str, &str)]) -> std::collections::BTreeMap<String, String> {
        declaration_fingerprints(files.iter().copied()).decls
    }

    /// The keys whose fingerprint differs between `before` and `after`.
    fn changed(before: &str, after: &str) -> Vec<String> {
        let (before, after) = (
            fingerprints(&[("main.baml", before)]),
            fingerprints(&[("main.baml", after)]),
        );
        before
            .keys()
            .chain(after.keys())
            .filter(|key| before.get(*key) != after.get(*key))
            .cloned()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    #[test]
    fn what_the_program_sees_is_a_change_and_formatting_is_not() {
        let source = r#"
class C {
    /// the customer id
    id string
    /// baml:other_directive
    function m(self) -> string { "a b" }
}
const Words = ["a b"];
//baml:$trace=decide_a
function f() -> string { `x y` }
"#;
        // Whitespace inside literals, docs, and either directive spelling.
        for (from, to, key) in [
            ("[\"a b\"]", "[\"a  b\"]", "const Words"),
            ("`x y`", "`x  y`", "fn f"),
            ("`x y`", "`x y `", "fn f"),
            ("decide_a", "decide_b", "fn f"),
            ("the customer id", "the account id", "class C"),
            ("{ \"a b\" }", "{ \"a  b\" }", "method C.m"),
            ("baml:other_directive", "baml:renamed", "method C.m"),
        ] {
            assert_eq!(
                changed(source, &source.replacen(from, to, 1)),
                [key],
                "{from:?}"
            );
        }
        // Comments and formatting outside literals are not.
        let reformatted = source
            .replace("-> string {", "->   string\n{")
            .replace("const Words", "// a note\nconst Words");
        assert_eq!(changed(source, &reformatted), Vec::<String>::new());
    }

    /// Two implementations under one key: exchanging their bodies is a change.
    #[test]
    fn a_shared_key_keeps_each_part_with_its_namespace() {
        let root = |body: &str| {
            format!(
                "interface Show {{ function show(self) -> string; }}\nclass Foo {{ n int }}\nimplement Show for Foo {{ function show(self) -> string {{ \"{body}\" }} }}\n"
            )
        };
        let nested = |body: &str| {
            format!(
                "class Foo {{ n int }}\nimplement root.Show for Foo {{ function show(self) -> string {{ \"{body}\" }} }}\n"
            )
        };
        let before = fingerprints(&[("main.baml", &root("a")), ("ns_a/a.baml", &nested("b"))]);
        let after = fingerprints(&[("main.baml", &root("b")), ("ns_a/a.baml", &nested("a"))]);
        let key = "implmethod Show for Foo.show";
        assert_ne!(before[key], after[key]);
    }

    /// Two implementations under one key, read in either order.
    #[test]
    fn a_shared_key_does_not_depend_on_file_order() {
        let root = "interface Show { function show(self) -> string; }\nclass Foo { n int }\nimplement Show for Foo { function show(self) -> string { \"root\" } }\n";
        let nested = "class Foo { n int }\nimplement root.Show for Foo { function show(self) -> string { \"nested\" } }\n";
        assert_eq!(
            fingerprints(&[("ns_a.baml", root), ("ns_a/a.baml", nested)]),
            fingerprints(&[("ns_a/a.baml", nested), ("ns_a.baml", root)]),
        );
    }

    fn keys(source: &str) -> Vec<String> {
        declaration_fingerprints(std::iter::once(("main.baml", source)))
            .decls
            .into_keys()
            .collect()
    }

    #[test]
    fn bindings_are_keyed_by_name_and_tests_by_nothing() {
        let source = r#"
const Limit = 3;
let Greeting = "hi";
function f() -> int { 1 }
test "a local const is not a declaration" {
    const x = f();
    assert(x == 1);
}
"#;
        assert_eq!(keys(source), ["const Greeting", "const Limit", "fn f"]);
    }
}
