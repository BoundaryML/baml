//! Shared test fixtures and utilities for codegen crates.
//!
//! This crate provides:
//! - Test helper functions (`ty`, `name`) for constructing types
//! - Macros (`class!`, `function!`, `r#enum!`, `type_alias!`, `object_pool!`) for ergonomic test setup
//! - Common fixture functions that can be reused across all language-specific codegen crates
//!
//! # Usage
//!
//! ```rust,ignore
//! use baml_codegen_tests::fixtures;
//!
//! let pool = fixtures::simple_class();
//! let output = to_source_code(&pool, &PathBuf::from("."));
//! ```

pub use baml_codegen_types::{
    Class, ClassProperty, Enum, EnumVariant, Function, FunctionArgument, Name, Namespace, Object,
    ObjectPool, Ty, TypeAlias,
};

/// Test helpers and DSL macros.
#[macro_use]
pub mod builders;

// Re-export types that might be needed by `define_fixtures!` expanded code
// or by consumers of this crate
pub use builders::*;

define_fixtures! {
    @dollar $;
    /// Empty pool - no types defined.
    empty => {
        object_pool! {}
    },

    /// Comprehensive test for mixed complex types (Classes, Enums, Unions, Maps, Lists).
    mixed_complex_types => {
        object_pool! {
            // Primitives & Literals
            class KitchenSink {
                id: "int", // Primitives
                name: "string",
                score: "float",
                active: "bool",
                nothing: "null",
                status: "string", // Literal types map to string/int in basic Ty unless Enum
                priority: "int",
                tags: "string[]", // Arrays
                numbers: "int[]",
                matrix: "int[][]",
                metadata: "map<string, string>", // Maps
                scores: "map<string, float>",
                description: "string?", // Optional
                notes: "string | null",
                data: "string | int | DataObject", // Unions
                result: "Success | Error", // Union of classes
                user: "User", // Nested classes
                items: "Item[]",
                config: "Configuration",
            },
            class DataObject { type: "string", value: "map<string, string>" },
            class Success { type: "string", data: "map<string, string>" },
            class Error { type: "string", message: "string", code: "int" },

            class User { id: "int", profile: "UserProfile", settings: "map<string, Setting>" },
            class UserProfile { name: "string", email: "string", bio: "string?", links: "string[]" },
            class Setting { key: "string", value: "string | int | bool", metadata: "map<string, string>?" },

            class Item { id: "int", name: "string", variants: "Variant[]", attributes: "map<string, string | int | float | bool>" },
            class Variant { sku: "string", price: "float", stock: "int", options: "map<string, string>" },

            class Configuration {
                version: "string",
                features: "Feature[]",
                environments: "map<string, Environment>",
                rules: "Rule[]",
            },
            class Feature { name: "string", enabled: "bool", config: "map<string, string | int | bool>?", dependencies: "string[]" },
            class Environment { name: "string", url: "string", variables: "map<string, string>", secrets: "map<string, string>?" },
            class Rule { id: "int", name: "string", condition: "Condition", actions: "Action[]", priority: "int" },
            class Condition { type: "string", conditions: "(Condition | SimpleCondition)[]" },
            class SimpleCondition { field: "string", operator: "string", value: "string | int | float | bool" },
            class Action { type: "string", parameters: "map<string, string | int | bool>", async_: "bool" },

            class UltraComplex {
                tree: "Node",
                widgets: "Widget[]",
                data: "ComplexData?",
                response: "UserResponse",
                assets: "Asset[]",
            },
            class Node {
                id: "int",
                type: "string",
                value: "string | int | Node[] | map<string, Node>",
                metadata: "NodeMetadata?",
            },
            class NodeMetadata { created: "string", modified: "string", tags: "string[]", attributes: "map<string, string | int | bool | null>" },
            class Widget { type: "string", button: "ButtonWidget?", text: "TextWidget?", img: "ImageWidget?", container: "ContainerWidget?" },
            class ButtonWidget { label: "string", action: "string", style: "map<string, string>" },
            class TextWidget { content: "string", format: "string", style: "map<string, string>" },
            class ImageWidget { alt: "string", dimensions: "Dimensions" },
            class Dimensions { width: "int", height: "int" },
            class ContainerWidget { layout: "string", children: "Widget[]", style: "map<string, string>" },

            class ComplexData { primary: "PrimaryData", secondary: "SecondaryData?", tertiary: "TertiaryData | null" },
            class PrimaryData { values: "(string | int | float)[]", mappings: "map<string, map<string, string>>", flags: "bool[]" },
            class SecondaryData { records: "Record[]", index: "map<string, Record>" },
            class Record { id: "int", data: "map<string, string | int | bool | null>", related: "Record[]?" },
            class TertiaryData { raw: "string", parsed: "map<string, string>?", valid: "bool" },

            class UserResponse { status: "string", data: "User?", error: "ErrorDetail?", metadata: "ResponseMetadata" },
            class ErrorDetail { code: "string", message: "string", details: "map<string, string>?" },
            class ResponseMetadata { timestamp: "string", requestId: "string", duration: "int", retries: "int" },

            class Asset { id: "int", type: "string", metadata: "AssetMetadata", tags: "string[]" },
            class AssetMetadata { filename: "string", size: "int", mimeType: "string", uploaded: "string", checksum: "string" },

            fn TestKitchenSink(input: "string") -> "KitchenSink",
            fn TestUltraComplex(input: "string") -> "UltraComplex",
            fn TestRecursiveComplexity(input: "string") -> "Node",
        }
    },

    /// Tests for semantic streaming features.
    semantic_streaming => {
        object_pool! {
            class SemanticContainer {
                sixteen_digit_number: "int",
                string_with_twenty_words: "stream_state<string>",
                class_1: "ClassWithoutDone",
                class_2: "ClassWithBlockDone",
                class_done_needed: "stream_state<ClassWithBlockDone>",
                class_needed: "stream_state<ClassWithoutDone>",
                three_small_things: "SmallThing[]",
                final_string: "string",
            },
            class ClassWithoutDone {
                i_16_digits: "int",
                s_20_words: "stream_state<string>",
            },
            class ClassWithBlockDone {
                i_16_digits: "int",
                s_20_words: "string",
            },
            class SmallThing {
                i_16_digits: "stream_state<int>",
                i_8_digits: "int",
            },
            // Simulate the generated stream types namespace versions
            class "stream_types.SemanticContainer" {
                sixteen_digit_number: "int",
                string_with_twenty_words: "stream_state<string>",
                class_1: "stream_types.ClassWithoutDone", // Stream version referencing stream version
                class_2: "ClassWithBlockDone", // References types version (if block done)
                class_done_needed: "stream_state<ClassWithBlockDone>",
                class_needed: "stream_state<stream_types.ClassWithoutDone>",
                three_small_things: "stream_types.SmallThing[]", // Stream version list
                final_string: "string",
            },
            class "stream_types.ClassWithoutDone" {
                 i_16_digits: "int",
                 s_20_words: "stream_state<string>",
            },
             class "stream_types.SmallThing" {
                i_16_digits: "stream_state<int>",
                i_8_digits: "int",
            },

            fn MakeSemanticContainer() -> "SemanticContainer",
            fn MakeClassWithBlockDone() -> "ClassWithBlockDone",
            fn MakeClassWithExternalDone() -> "stream_state<ClassWithoutDone>",
        }
    },

    /// Extended union types tests.
    union_types_extended => {
        object_pool! {
            fn union_simple(u: "string | int") -> "bool",
            fn union_complex(u: "User | Company | string") -> "void",
            fn union_in_list(l: "(string | int)[]") -> "void",
            fn union_return() -> "string | int",

            class User { name: "string" },
            class Company { name: "string", industry: "string" },
            class Container {
               items: "(string | int | bool)[]",
               matrix: "(string | int)[][]",
               optional_union: "(string | int)?",
            },
        }
    },

    /// Map types suite.
    map_types => {
        object_pool! {
            fn map_string_int(m: "map<string, int>") -> "map<string, string>",
            fn nested_map(m: "map<string, map<string, int>>") -> "void",
            fn map_of_arrays(m: "map<string, int[]>") -> "void",
            class MapContainer {
                simple: "map<string, int>",
                nested: "map<string, map<string, string>>",
                array_val: "map<string, string[]>",
                union_val: "map<string, string | int>",
            },
        }
    },

    /// Literal types.
    literal_types => {
        object_pool! {
            class Literals {
                priority_1: "'1'",
                priority_2: "'2'",
                priority_3: "'3'",
                status_draft: "'draft'",
                status_published: "'published'",
                count: "42",
                enabled: "true",
                disabled: "false",
            },
        }
    },
}
