//! Stable syntax node addressing for incremental compilation.
//!
//! Provides content-based IDs that remain stable when:
//! - Items are reordered
//! - Items are added/removed elsewhere in the file
//! - Whitespace or comments change
//! - Function bodies change (as long as the function signature is the same)
//!
//! Based on rust-analyzer's `AstId` system:
//! - Content-based hashing (name + kind)
//! - Collision handling via index disambiguator
//! - Breadth-first tree traversal for parent-before-child ordering

use baml_syntax::{BamlLanguage, SyntaxKind, SyntaxNode, ast};
use rowan::ast::{AstNode, SyntaxNodePtr};
use rustc_hash::{FxBuildHasher, FxHashMap};
use std::hash::{BuildHasher, Hash, Hasher};

/// Type alias for BAML syntax node pointer
type BamlSyntaxNodePtr = SyntaxNodePtr<BamlLanguage>;

/// Stable ID for a syntax node within a file.
///
/// The ID is based on the item's name and kind, making it stable across:
/// - Reordering items
/// - Adding/removing other items
/// - Changing whitespace/comments
/// - Changing function bodies (name unchanged)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileAstId {
    /// Content-based hash (name + kind)
    hash: u16,

    /// Disambiguator for hash collisions
    index: u16,

    /// Item kind
    kind: ItemKind,
}

impl FileAstId {
    /// Create a new `FileAstId`.
    pub fn new(hash: u16, index: u16, kind: ItemKind) -> Self {
        Self { hash, index, kind }
    }

    /// Get the hash component.
    pub fn hash(&self) -> u16 {
        self.hash
    }

    /// Get the index component.
    pub fn index(&self) -> u16 {
        self.index
    }

    /// Get the kind component.
    pub fn kind(&self) -> ItemKind {
        self.kind
    }
}

/// Kind of item that can have an `AstId`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ItemKind {
    Function,
    Class,
    Enum,
    TypeAlias,
    Client,
    Test,
}

/// Bidirectional map between `FileAstId` and `SyntaxNodePtr`.
///
/// This provides stable addressing into the syntax tree:
/// - Given a syntax node, get its stable ID
/// - Given a stable ID, get the syntax node pointer
#[derive(Debug, Default, PartialEq, Eq)]
pub struct AstIdMap {
    /// Arena storing (ptr, id) pairs
    arena: Vec<(BamlSyntaxNodePtr, FileAstId)>,

    /// Map: `SyntaxNodePtr` → arena index
    ptr_to_idx: FxHashMap<BamlSyntaxNodePtr, usize>,

    /// Map: `FileAstId` → arena index
    id_to_idx: FxHashMap<FileAstId, usize>,
}

impl AstIdMap {
    /// Build `AstIdMap` from syntax tree using breadth-first traversal.
    ///
    /// Breadth-first order ensures parents get lower indices than children,
    /// which helps with ID stability when items are added/removed.
    pub fn from_source(root: &SyntaxNode) -> Self {
        let mut map = AstIdMap::default();
        let mut index_counters = FxHashMap::<(ItemKind, u16), u16>::default();

        // Breadth-first traversal
        let mut curr_layer = vec![root.clone()];
        let mut next_layer = Vec::new();

        while !curr_layer.is_empty() {
            for node in curr_layer.drain(..) {
                // Check if this is a top-level item we should track
                if let Some((kind, name)) = extract_item_info(&node) {
                    let hash = compute_hash(&name, kind);

                    // Get disambiguator index for this (kind, hash) pair
                    let index = *index_counters.entry((kind, hash)).or_insert(0);
                    index_counters.insert((kind, hash), index + 1);

                    let ast_id = FileAstId { hash, index, kind };
                    let ptr = SyntaxNodePtr::new(&node);

                    let idx = map.arena.len();
                    map.arena.push((ptr, ast_id));
                    map.ptr_to_idx.insert(ptr, idx);
                    map.id_to_idx.insert(ast_id, idx);
                }

                // Add direct children to next layer
                // (Don't recurse into descendants - we only want top-level items)
                next_layer.extend(node.children());
            }
            std::mem::swap(&mut curr_layer, &mut next_layer);
        }

        map
    }

    /// Get `AstId` for a syntax node pointer.
    pub fn ast_id(&self, ptr: &BamlSyntaxNodePtr) -> Option<FileAstId> {
        self.ptr_to_idx.get(ptr).map(|&idx| self.arena[idx].1)
    }

    /// Get syntax node pointer from `AstId`.
    pub fn get(&self, ast_id: FileAstId) -> Option<BamlSyntaxNodePtr> {
        self.id_to_idx.get(&ast_id).map(|&idx| self.arena[idx].0)
    }

    /// Get `AstId` by name and kind (helper for interning).
    ///
    /// This is used during item interning to find the `AstId` for a function/class/etc.
    /// by name when we don't have the `SyntaxNodePtr`.
    pub fn find_by_name(&self, name: &str, kind: ItemKind) -> Option<FileAstId> {
        let target_hash = compute_hash(name, kind);

        // Look through arena for matching name/kind
        // In practice this is fast because:
        // 1. We can filter by kind and hash first
        // 2. Most files have < 100 items
        self.arena
            .iter()
            .map(|(_, id)| *id)
            .find(|id| id.kind == kind && id.hash == target_hash)
    }

    /// Get all items in the map.
    pub fn items(&self) -> impl Iterator<Item = (BamlSyntaxNodePtr, FileAstId)> + '_ {
        self.arena.iter().map(|(ptr, id)| (*ptr, *id))
    }

    /// Number of items in the map.
    pub fn len(&self) -> usize {
        self.arena.len()
    }

    /// Check if the map is empty.
    pub fn is_empty(&self) -> bool {
        self.arena.is_empty()
    }
}

/// Extract item info (kind and name) from a syntax node.
fn extract_item_info(node: &SyntaxNode) -> Option<(ItemKind, String)> {
    match node.kind() {
        SyntaxKind::FUNCTION_DEF => {
            let func = ast::FunctionDef::cast(node.clone())?;
            let name = func.name()?.text().to_string();
            Some((ItemKind::Function, name))
        }
        SyntaxKind::CLASS_DEF => {
            let class = ast::ClassDef::cast(node.clone())?;
            let name = class.name()?.text().to_string();
            Some((ItemKind::Class, name))
        }
        SyntaxKind::ENUM_DEF => {
            let enum_def = ast::EnumDef::cast(node.clone())?;
            // EnumDef doesn't have a name() method, extract manually
            let name = enum_def
                .syntax()
                .children_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .find(|t| t.kind() == SyntaxKind::WORD)?
                .text()
                .to_string();
            Some((ItemKind::Enum, name))
        }
        SyntaxKind::TYPE_ALIAS_DEF => {
            let alias = ast::TypeAliasDef::cast(node.clone())?;
            // Extract name manually
            let name = alias
                .syntax()
                .children_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .find(|t| t.kind() == SyntaxKind::WORD)?
                .text()
                .to_string();
            Some((ItemKind::TypeAlias, name))
        }
        SyntaxKind::CLIENT_DEF => {
            let client = ast::ClientDef::cast(node.clone())?;
            // Extract name manually
            let name = client
                .syntax()
                .children_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .find(|t| t.kind() == SyntaxKind::WORD)?
                .text()
                .to_string();
            Some((ItemKind::Client, name))
        }
        SyntaxKind::TEST_DEF => {
            let test = ast::TestDef::cast(node.clone())?;
            // Extract name manually
            let name = test
                .syntax()
                .children_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .find(|t| t.kind() == SyntaxKind::WORD)?
                .text()
                .to_string();
            Some((ItemKind::Test, name))
        }
        _ => None,
    }
}

/// Compute a stable hash for an item based on its name and kind.
///
/// Uses `FxHash` for speed, takes upper 16 bits for better entropy.
fn compute_hash(name: &str, kind: ItemKind) -> u16 {
    let mut hasher = FxBuildHasher.build_hasher();
    name.hash(&mut hasher);
    kind.hash(&mut hasher);
    let hash = hasher.finish();
    // Take upper 16 bits (better entropy than lower bits)
    (hash >> 48) as u16
}

#[cfg(test)]
mod tests {
    use super::ItemKind; // Import for tests
    use super::*;
    use baml_base::FileId;
    use baml_lexer::lex_lossless;
    use baml_parser::parse_file;

    fn parse(text: &str) -> SyntaxNode {
        let tokens = lex_lossless(text, FileId::new(0));
        let (green, _errors) = parse_file(&tokens);
        SyntaxNode::new_root(green)
    }

    #[test]
    fn test_empty_file() {
        let tree = parse("");
        let map = AstIdMap::from_source(&tree);
        assert_eq!(map.len(), 0);
        assert!(map.is_empty());
    }

    #[test]
    fn test_single_function() {
        let tree = parse("function Foo() -> int { return 1; }");
        let map = AstIdMap::from_source(&tree);

        assert_eq!(map.len(), 1);

        // Find the function by name
        let foo_id = map.find_by_name("Foo", ItemKind::Function);
        assert!(foo_id.is_some());

        let foo_id = foo_id.unwrap();
        assert_eq!(foo_id.kind(), ItemKind::Function);
        assert_eq!(foo_id.index(), 0); // First function with this hash
    }

    #[test]
    fn test_multiple_functions() {
        let tree = parse(
            r#"
            function Foo() -> int { return 1; }
            function Bar() -> int { return 2; }
            function Baz() -> int { return 3; }
        "#,
        );
        let map = AstIdMap::from_source(&tree);

        assert_eq!(map.len(), 3);

        let foo_id = map.find_by_name("Foo", ItemKind::Function).unwrap();
        let bar_id = map.find_by_name("Bar", ItemKind::Function).unwrap();
        let baz_id = map.find_by_name("Baz", ItemKind::Function).unwrap();

        // All should have different IDs
        assert_ne!(foo_id, bar_id);
        assert_ne!(foo_id, baz_id);
        assert_ne!(bar_id, baz_id);
    }

    #[test]
    fn test_ast_id_stable_when_reordering() {
        let tree1 = parse(
            r#"
            function Foo() -> int { return 1; }
            function Bar() -> int { return 2; }
        "#,
        );
        let map1 = AstIdMap::from_source(&tree1);
        let foo_id1 = map1.find_by_name("Foo", ItemKind::Function).unwrap();

        // Reorder functions
        let tree2 = parse(
            r#"
            function Bar() -> int { return 2; }
            function Foo() -> int { return 1; }
        "#,
        );
        let map2 = AstIdMap::from_source(&tree2);
        let foo_id2 = map2.find_by_name("Foo", ItemKind::Function).unwrap();

        // Foo's ID should be the same (name-based, not position-based)
        assert_eq!(foo_id1, foo_id2);
    }

    #[test]
    fn test_ast_id_stable_when_adding_function() {
        let tree1 = parse(
            r#"
            function Foo() -> int { return 1; }
            function Bar() -> int { return 2; }
        "#,
        );
        let map1 = AstIdMap::from_source(&tree1);
        let foo_id1 = map1.find_by_name("Foo", ItemKind::Function).unwrap();
        let bar_id1 = map1.find_by_name("Bar", ItemKind::Function).unwrap();

        // Add function in the middle
        let tree2 = parse(
            r#"
            function Foo() -> int { return 1; }
            function Baz() -> int { return 3; }
            function Bar() -> int { return 2; }
        "#,
        );
        let map2 = AstIdMap::from_source(&tree2);
        let foo_id2 = map2.find_by_name("Foo", ItemKind::Function).unwrap();
        let bar_id2 = map2.find_by_name("Bar", ItemKind::Function).unwrap();

        // Existing functions should have same IDs
        assert_eq!(foo_id1, foo_id2);
        assert_eq!(bar_id1, bar_id2);
    }

    #[test]
    fn test_ast_id_stable_when_changing_body() {
        let tree1 = parse(
            r#"
            function Foo() -> int { return 1; }
        "#,
        );
        let map1 = AstIdMap::from_source(&tree1);
        let foo_id1 = map1.find_by_name("Foo", ItemKind::Function).unwrap();

        // Change body (but not signature)
        let tree2 = parse(
            r#"
            function Foo() -> int { return 999; }
        "#,
        );
        let map2 = AstIdMap::from_source(&tree2);
        let foo_id2 = map2.find_by_name("Foo", ItemKind::Function).unwrap();

        // ID should be the same (based on name, not body)
        assert_eq!(foo_id1, foo_id2);
    }

    #[test]
    fn test_different_item_kinds() {
        let tree = parse(
            r#"
            function Foo() -> int { return 1; }
            class Foo { x int }
            enum Foo { A B }
        "#,
        );
        let map = AstIdMap::from_source(&tree);

        assert_eq!(map.len(), 3);

        let func_id = map.find_by_name("Foo", ItemKind::Function).unwrap();
        let class_id = map.find_by_name("Foo", ItemKind::Class).unwrap();
        let enum_id = map.find_by_name("Foo", ItemKind::Enum).unwrap();

        // Same name but different kinds should have different IDs
        assert_ne!(func_id, class_id);
        assert_ne!(func_id, enum_id);
        assert_ne!(class_id, enum_id);
    }

    #[test]
    fn test_hash_collision_handling() {
        // Create a scenario where we might have collisions
        // (In practice, 16-bit hashes make this unlikely with < 256 items)
        let tree = parse(
            r#"
            function A() -> int { return 1; }
            function B() -> int { return 2; }
            function C() -> int { return 3; }
            function D() -> int { return 4; }
            function E() -> int { return 5; }
        "#,
        );
        let map = AstIdMap::from_source(&tree);

        assert_eq!(map.len(), 5);

        // Even if hashes collide, indices should disambiguate
        let ids: Vec<_> = ["A", "B", "C", "D", "E"]
            .iter()
            .map(|name| map.find_by_name(name, ItemKind::Function).unwrap())
            .collect();

        // All IDs should be unique
        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                assert_ne!(ids[i], ids[j], "IDs should be unique");
            }
        }
    }

    #[test]
    fn test_roundtrip_ptr_to_id_to_ptr() {
        let tree = parse("function Foo() -> int { return 1; }");
        let map = AstIdMap::from_source(&tree);

        // Find the function node
        let func_node = tree
            .children()
            .find(|n| n.kind() == SyntaxKind::FUNCTION_DEF)
            .expect("function node must exist");

        let ptr = SyntaxNodePtr::new(&func_node);

        // Roundtrip: ptr -> id -> ptr
        let ast_id = map.ast_id(&ptr).expect("must have ast_id");
        let ptr2 = map.get(ast_id).expect("must resolve back to ptr");

        assert_eq!(ptr, ptr2);
    }
}
