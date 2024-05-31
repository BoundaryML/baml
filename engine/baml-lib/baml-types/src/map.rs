#[cfg(feature = "stable_sort")]
use indexmap::IndexMap;
#[cfg(feature = "stable_sort")]
pub type Map<K, V> = IndexMap<K, V>;
#[cfg(not(feature = "stable_sort"))]
use std::collections::HashMap;
#[cfg(not(feature = "stable_sort"))]
pub type Map<K, V> = HashMap<K, V>;
