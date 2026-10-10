//! `map<K, V>` for compiled BAML.
//!
//! A BAML map is a reference-semantics heap object that remembers insertion
//! order: the VM's `Object::Map` is an insertion-ordered table, and so is
//! [`Map`], a [`Shared`] handle over an [`IndexMap`]. Writing an existing key
//! replaces the value in place, deleting a key shifts the later entries up
//! (the VM's `shift_remove`), `keys()` and `values()` copy out in that
//! order, and `to_string` and JSON render it.
//!
//! Keys are `string`, `int` or `bool`, the key types the backend admits:
//! they compare by value on both backends, so [`IndexMap`]'s hashing stands
//! in for the VM's. (`float` keys and keys with user `Hash` / `Equals`
//! implementations are outside the subset.) Reading an absent key with
//! `m[k]` raises `baml.panics.MapKeyNotFound`; `get` returns `null`.
//!
//! JSON requires string keys, as it does on the VM: serializing or decoding
//! a map keyed by `int` or `bool` throws the VM's
//! `baml.json.SerializationError` / `DecodeError` with
//! [`STRING_MAP_REQUIRED`] as the message.

use std::hash::Hash;

use indexmap::IndexMap;
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, MapAccess, Visitor},
    ser::SerializeMap,
};

use crate::{
    Int63, Panic, Str, Thrown,
    handle::{Shared, shared},
    int_from_usize,
    render::ToBaml,
};

/// The VM's message when JSON meets a map whose keys are not strings.
pub const STRING_MAP_REQUIRED: &str = "JSON requires maps with string keys";

/// A key type the native map admits. Equality and hashing are the Rust
/// ones, which agree with the VM's value equality for these types.
pub trait MapKey: Hash + Eq + Clone + ToBaml {
    /// Whether the type is JSON's object-key type, `string`. The VM decides
    /// by the map's declared key type, so an empty `map<int, V>` has no JSON
    /// form either.
    const JSON_KEYS: bool;

    /// The key as a JSON object key; only called when [`Self::JSON_KEYS`].
    fn json_key(&self) -> &str;

    /// A key decoded from a JSON object key; only called when
    /// [`Self::JSON_KEYS`].
    fn from_json_key(key: String) -> Self;
}

impl MapKey for Str {
    const JSON_KEYS: bool = true;

    fn json_key(&self) -> &str {
        self.as_str()
    }

    fn from_json_key(key: String) -> Self {
        Str::from(key)
    }
}

impl MapKey for Int63 {
    const JSON_KEYS: bool = false;

    fn json_key(&self) -> &str {
        unreachable!("an int is not a JSON key")
    }

    fn from_json_key(_: String) -> Self {
        unreachable!("an int is not a JSON key")
    }
}

impl MapKey for bool {
    const JSON_KEYS: bool = false;

    fn json_key(&self) -> &str {
        unreachable!("a bool is not a JSON key")
    }

    fn from_json_key(_: String) -> Self {
        unreachable!("a bool is not a JSON key")
    }
}

/// A BAML `map<K, V>`: a handle with identity semantics over an
/// insertion-ordered table. `Clone` copies the handle, not the table.
pub struct Map<K, V>(Shared<IndexMap<K, V>>);

impl<K, V> Clone for Map<K, V> {
    #[inline]
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<K: std::fmt::Debug, V: std::fmt::Debug> std::fmt::Debug for Map<K, V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.0, f)
    }
}

impl<K, V> Map<K, V> {
    /// The handle this map is, for identity comparison and borrowing.
    #[inline]
    pub fn handle(&self) -> &Shared<IndexMap<K, V>> {
        &self.0
    }
}

/// Whether two maps are the same object.
#[inline]
pub fn ptr_eq<K, V>(left: &Map<K, V>, right: &Map<K, V>) -> bool {
    crate::handle::ptr_eq(&left.0, &right.0)
}

/// Allocate a map from its entries (`{ k: v, .. }`), in order; a repeated
/// key keeps its first position and takes the last value.
pub fn new<K: MapKey, V>(entries: Vec<(K, V)>) -> Map<K, V> {
    let mut table = IndexMap::with_capacity(entries.len());
    for (key, value) in entries {
        table.insert(key, value);
    }
    Map(shared(table))
}

/// `m.length()`.
#[inline]
pub fn len<K, V>(map: &Map<K, V>) -> Int63 {
    int_from_usize(map.0.borrow().len())
}

/// `m.has(k)`.
#[inline]
pub fn has<K: MapKey, V>(map: &Map<K, V>, key: &K) -> bool {
    map.0.borrow().contains_key(key)
}

/// `m.get(k)`: the value, or `null` for an absent key.
#[inline]
pub fn get<K: MapKey, V: Clone>(map: &Map<K, V>, key: &K) -> Option<V> {
    map.0.borrow().get(key).cloned()
}

/// `m[k]`: the value, or `baml.panics.MapKeyNotFound` for an absent key.
#[inline]
pub fn index<K: MapKey, V: Clone>(map: &Map<K, V>, key: &K) -> Result<V, Thrown> {
    map.0.borrow().get(key).cloned().ok_or_else(key_not_found)
}

/// `m[k] = v` and `m.set(k, v)`: the previous value, or `null`. An existing
/// key keeps its position.
#[inline]
pub fn set<K: MapKey, V>(map: &Map<K, V>, key: K, value: V) -> Option<V> {
    map.0.borrow_mut().insert(key, value)
}

/// `m.delete(k)`: the removed value, or `null`. Later entries keep their
/// relative order.
#[inline]
pub fn delete<K: MapKey, V>(map: &Map<K, V>, key: &K) -> Option<V> {
    map.0.borrow_mut().shift_remove(key)
}

/// `m.keys()`: a fresh array of the keys in insertion order.
pub fn keys<K: MapKey, V>(map: &Map<K, V>) -> Shared<Vec<K>> {
    shared(map.0.borrow().keys().cloned().collect())
}

/// `m.values()`: a fresh array of the values in insertion order.
pub fn values<K, V: Clone>(map: &Map<K, V>) -> Shared<Vec<V>> {
    shared(map.0.borrow().values().cloned().collect())
}

/// `m.get_or_insert(k, v)`: the value under `k`, inserting `v` first when
/// the key is absent.
pub fn get_or_insert<K: MapKey, V: Clone>(map: &Map<K, V>, key: K, default: V) -> V {
    map.0.borrow_mut().entry(key).or_insert(default).clone()
}

/// `m.clear()`.
#[inline]
pub fn clear<K, V>(map: &Map<K, V>) {
    map.0.borrow_mut().clear();
}

#[cold]
fn key_not_found() -> Thrown {
    Thrown::Panic(Panic::MapKeyNotFound)
}

/// `{k: v, ..}` with every key and value rendered nested (strings quoted);
/// the empty map is `{}`.
impl<K: ToBaml, V: ToBaml> ToBaml for Map<K, V> {
    fn render(&self, out: &mut String, _nested: bool) {
        out.push('{');
        for (i, (key, value)) in self.0.borrow().iter().enumerate() {
            if i != 0 {
                out.push_str(", ");
            }
            key.render(out, true);
            out.push_str(": ");
            value.render(out, true);
        }
        out.push('}');
    }
}

/// A JSON object in insertion order; a map keyed by `int` or `bool` has no
/// JSON form and fails with [`STRING_MAP_REQUIRED`].
impl<K: MapKey, V: Serialize> Serialize for Map<K, V> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if !K::JSON_KEYS {
            return Err(serde::ser::Error::custom(STRING_MAP_REQUIRED));
        }
        let table = self.0.borrow();
        let mut object = serializer.serialize_map(Some(table.len()))?;
        for (key, value) in table.iter() {
            object.serialize_entry(key.json_key(), value)?;
        }
        object.end()
    }
}

impl<'de, K: MapKey, V: Deserialize<'de>> Deserialize<'de> for Map<K, V> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct MapVisitor<K, V>(std::marker::PhantomData<(K, V)>);

        impl<'de, K: MapKey, V: Deserialize<'de>> Visitor<'de> for MapVisitor<K, V> {
            type Value = Map<K, V>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("expected object")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Map<K, V>, A::Error> {
                let mut table = IndexMap::with_capacity(access.size_hint().unwrap_or(0));
                while let Some(key) = access.next_key::<String>()? {
                    let value = access.next_value()?;
                    table.insert(K::from_json_key(key), value);
                }
                Ok(Map(shared(table)))
            }
        }

        if !K::JSON_KEYS {
            return Err(de::Error::custom(STRING_MAP_REQUIRED));
        }
        deserializer.deserialize_map(MapVisitor(std::marker::PhantomData))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{array, json, render::to_std, string::from_literal};

    fn int(value: i64) -> Int63 {
        Int63::new(value).unwrap()
    }

    fn s(text: &'static str) -> Str {
        from_literal(text)
    }

    #[test]
    fn operations_keep_insertion_order_and_share_the_table() {
        let m = new(vec![(s("b"), int(2)), (s("a"), int(1)), (s("b"), int(3))]);
        assert_eq!(len(&m), int(2));
        assert_eq!(get(&m, &s("b")), Some(int(3)));
        assert_eq!(get(&m, &s("zz")), None);
        assert!(has(&m, &s("a")));
        assert_eq!(set(&m, s("a"), int(10)), Some(int(1)));
        assert_eq!(set(&m, s("c"), int(4)), None);
        let alias = m.clone();
        assert!(ptr_eq(&m, &alias));
        assert_eq!(delete(&alias, &s("b")), Some(int(3)));
        assert_eq!(delete(&alias, &s("b")), None);
        assert_eq!(*keys(&m).borrow(), vec![s("a"), s("c")]);
        assert_eq!(*values(&m).borrow(), vec![int(10), int(4)]);
        assert_eq!(get_or_insert(&m, s("a"), int(0)), int(10));
        assert_eq!(get_or_insert(&m, s("d"), int(5)), int(5));
        assert_eq!(len(&m), int(3));
        clear(&m);
        assert_eq!(len(&alias), int(0));
        assert!(!ptr_eq(&m, &new(Vec::new())));
    }

    #[test]
    fn subscript_of_an_absent_key_is_the_vm_panic() {
        let m = new(vec![(int(1), s("one"))]);
        assert_eq!(index(&m, &int(1)).unwrap(), s("one"));
        let thrown = index(&m, &int(2)).unwrap_err();
        assert_eq!(
            thrown.render_readable(),
            r#"baml.panics.MapKeyNotFound {key: "(unknown)"}"#
        );
        assert_eq!(thrown.exit_code(), 1);
    }

    #[test]
    fn renders_like_the_vm() {
        let m = new(vec![(s("a"), int(1)), (s("b \"q\""), int(2))]);
        assert_eq!(to_std(&m), r#"{"a": 1, "b \"q\"": 2}"#);
        let ints = new(vec![(int(2), s("x")), (int(-1), s("y"))]);
        assert_eq!(to_std(&ints), r#"{2: "x", -1: "y"}"#);
        let bools = new(vec![(true, array::new(vec![int(1)]))]);
        assert_eq!(to_std(&bools), "{true: [1]}");
        let empty: Map<Str, Int63> = new(Vec::new());
        assert_eq!(to_std(&empty), "{}");
        let nested = new(vec![(s("m"), new(vec![(s("k"), s("v"))]))]);
        assert_eq!(to_std(&nested), r#"{"m": {"k": "v"}}"#);
    }

    #[test]
    fn json_round_trips_string_keys_in_order() {
        let m = new(vec![(s("b"), int(2)), (s("a"), int(1))]);
        let text = json::to_string(&m).unwrap();
        assert_eq!(text.as_str(), r#"{"b":2,"a":1}"#);
        let back: Map<Str, Int63> = json::deserialize(&text).unwrap();
        assert_eq!(*keys(&back).borrow(), vec![s("b"), s("a")]);
        assert_eq!(get(&back, &s("a")), Some(int(1)));
        let nested: Map<Str, Shared<Vec<Str>>> =
            json::deserialize(&Str::from(r#"{"x": ["1"], "y": []}"#)).unwrap();
        assert_eq!(len(&nested), int(2));
        let empty: Map<Str, Int63> = json::deserialize(&Str::from("{}")).unwrap();
        assert_eq!(len(&empty), int(0));
    }

    #[test]
    fn json_refuses_non_string_keys_like_the_vm() {
        let ints = new(vec![(int(1), s("x"))]);
        let thrown = json::to_string(&ints).unwrap_err();
        assert_eq!(
            thrown.render_readable(),
            r#"baml.json.SerializationError {message: "JSON requires maps with string keys", path: "", reason: "unserializable"}"#
        );
        // The declared key type decides, as on the VM: an empty map too.
        let bools: Map<bool, Int63> = new(Vec::new());
        assert_eq!(
            json::to_string(&bools).unwrap_err().class_fqn(),
            "baml.json.SerializationError"
        );
        let thrown = json::deserialize::<Map<Int63, Str>>(&Str::from(r#"{"1": "x"}"#)).unwrap_err();
        assert_eq!(
            thrown.render_readable(),
            r#"baml.json.DecodeError {message: "JSON requires maps with string keys", path: ""}"#
        );
        let thrown = json::deserialize::<Map<Str, Int63>>(&Str::from("[1]")).unwrap_err();
        assert_eq!(thrown.class_fqn(), "baml.json.DecodeError");
    }
}
