//! BamlMap type alias.

use indexmap::IndexMap;

/// Type alias for map type - uses IndexMap for stable iteration order.
pub type BamlMap<K, V> = IndexMap<K, V>;
