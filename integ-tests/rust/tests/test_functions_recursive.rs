//! Recursive types tests - ported from test_functions_recursive_test.go
//!
//! Tests for recursive type handling including:
//! - Simple linked lists
//! - Mutually recursive trees
//! - Aliases pointing to recursive types
//! - Classes pointing to recursive classes via aliases

use rust::baml_client::sync_client::B;
use rust::baml_client::types::*;

/// Test simple linked list
#[test]
fn test_simple_linked_list() {
    let input = vec![1, 2, 3, 4, 5];
    let result = B.BuildLinkedList.call(&input);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let list = result.unwrap();
    assert_eq!(list.len, 5, "Expected list length of 5");
    assert!(list.head.is_some(), "Expected non-empty list head");
}

/// Test mutually recursive tree
#[test]
fn test_mutually_recursive_tree() {
    let input = BinaryNode {
        data: 1,
        left: Some(Box::new(BinaryNode {
            data: 2,
            left: None,
            right: None,
        })),
        right: Some(Box::new(BinaryNode {
            data: 3,
            left: None,
            right: None,
        })),
    };
    let result = B.BuildTree.call(&input);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let tree = result.unwrap();
    assert_eq!(tree.data, 1, "Expected root data of 1");
}

/// Test alias pointing to recursive type
#[test]
fn test_alias_pointing_to_recursive_type() {
    let input = LinkedListAliasNode {
        value: 10,
        next: Some(Box::new(LinkedListAliasNode {
            value: 20,
            next: Some(Box::new(LinkedListAliasNode {
                value: 30,
                next: None,
            })),
        })),
    };
    let result = B.AliasThatPointsToRecursiveType.call(&input);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    assert_eq!(output.value, 10, "Expected first value of 10");
}

/// Test class pointing to recursive class through alias
#[test]
fn test_class_pointing_to_recursive_class_through_alias() {
    let input = ClassToRecAlias {
        list: LinkedListAliasNode {
            value: 100,
            next: Some(Box::new(LinkedListAliasNode {
                value: 200,
                next: None,
            })),
        },
    };
    let result = B.ClassThatPointsToRecursiveClassThroughAlias.call(&input);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
}

/// Test recursive class with alias indirection
#[test]
fn test_recursive_class_with_alias_indirection() {
    let input = NodeWithAliasIndirection {
        value: 1,
        next: Some(Box::new(NodeWithAliasIndirection {
            value: 2,
            next: Some(Box::new(NodeWithAliasIndirection {
                value: 3,
                next: None,
            })),
        })),
    };
    let result = B.RecursiveClassWithAliasIndirection.call(&input);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let output = result.unwrap();
    assert_eq!(output.value, 1, "Expected first value of 1");
}

/// Test returning JSON entries (recursive structure)
#[test]
fn test_return_json_entry() {
    let result = B.ReturnJsonEntry.call("nested json structure");
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
}

/// Test deep recursive structure
#[test]
fn test_deep_recursive_structure() {
    // Build a deep linked list
    let mut current: Option<Box<LinkedListAliasNode>> = None;
    for i in (1..=10).rev() {
        current = Some(Box::new(LinkedListAliasNode {
            value: i,
            next: current.map(|n| *n).map(Box::new),
        }));
    }

    let input = LinkedListAliasNode {
        value: current.as_ref().unwrap().value,
        next: current.unwrap().next,
    };

    let result = B.AliasThatPointsToRecursiveType.call(&input);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
}

/// Test complex tree structure
#[test]
fn test_complex_tree_structure() {
    let input = BinaryNode {
        data: 1,
        left: Some(Box::new(BinaryNode {
            data: 2,
            left: Some(Box::new(BinaryNode {
                data: 4,
                left: None,
                right: None,
            })),
            right: Some(Box::new(BinaryNode {
                data: 5,
                left: None,
                right: None,
            })),
        })),
        right: Some(Box::new(BinaryNode {
            data: 3,
            left: Some(Box::new(BinaryNode {
                data: 6,
                left: None,
                right: None,
            })),
            right: Some(Box::new(BinaryNode {
                data: 7,
                left: None,
                right: None,
            })),
        })),
    };

    let result = B.BuildTree.call(&input);
    assert!(result.is_ok(), "Expected successful call, got {:?}", result);
    let tree = result.unwrap();
    assert_eq!(tree.data, 1, "Expected root data of 1");
}
