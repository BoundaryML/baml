//! Inline fast paths for the hottest container builtins.
//!
//! Each returns `None`, before touching the container, unless the receiver
//! is a list or a map with a string key. Generated code then makes the
//! builtin call the helper stands in for, so every other case, and every
//! error, keeps the builtin's behavior.

use bex_vm_types::{BexStr, Object, Value, map_string_hash, types::Map};

use crate::vm::BexVm;

impl BexVm {
    /// `Array.push`: the new length.
    #[inline]
    pub fn list_push(&mut self, list: Value, item: Value) -> Option<Value> {
        self.list_push_in::<false>(list, item)
    }

    /// [`Self::list_push`] on a list escape analysis proved thread-local,
    /// without its lock.
    #[inline]
    pub fn list_push_local(&mut self, list: Value, item: Value) -> Option<Value> {
        self.list_push_in::<true>(list, item)
    }

    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn list_push_in<const THREAD_LOCAL: bool>(
        &mut self,
        list: Value,
        item: Value,
    ) -> Option<Value> {
        let Object::Array(array) = self.get_object(list.as_object_ptr()?) else {
            return None;
        };
        let debt = self.tlab.alloc_debt();
        // SAFETY (`THREAD_LOCAL`): codegen calls the `_local` entry points
        // only where escape analysis proved no other thread can reach the list.
        #[allow(unsafe_code)]
        let mut guard = if THREAD_LOCAL {
            unsafe { array.lock_mut_thread_local(debt) }
        } else {
            array.lock_mut(debt)
        };
        guard.push(item);
        #[allow(clippy::cast_possible_wrap)]
        let len = guard.len() as i64;
        drop(guard);
        // As the builtin call does, so growth reaches the collector as soon.
        self.settle();
        Some(Value::int(len))
    }

    /// `Map.get` for a string key: `Some(None)` when the key is absent.
    #[allow(clippy::option_option, reason = "see `map_get_str_in`")]
    #[inline]
    pub fn map_get_str(&self, map: Value, key: Value) -> Option<Option<Value>> {
        self.map_get_str_in::<false>(map, key)
    }

    /// [`Self::map_get_str`] on a thread-local map, without its lock.
    #[allow(clippy::option_option, reason = "see `map_get_str_in`")]
    #[inline]
    pub fn map_get_str_local(&self, map: Value, key: Value) -> Option<Option<Value>> {
        self.map_get_str_in::<true>(map, key)
    }

    #[allow(
        clippy::inline_always,
        clippy::option_option,
        reason = "the outer `None` declines to the builtin, as every helper here does"
    )]
    #[inline(always)]
    fn map_get_str_in<const THREAD_LOCAL: bool>(
        &self,
        map: Value,
        key: Value,
    ) -> Option<Option<Value>> {
        let (map, text) = self.string_keyed(map, key)?;
        let hash = map_string_hash(text.as_str());
        // SAFETY (`THREAD_LOCAL`): as for `list_push_in`.
        #[allow(unsafe_code)]
        let entries = if THREAD_LOCAL {
            unsafe { map.lock_thread_local() }
        } else {
            map.lock()
        };
        Some(entries.get_hashed(hash, |candidate| self.is_string(*candidate, text)))
    }

    /// `Map.set` for a string key: the value replaced, or null.
    #[inline]
    pub fn map_set_str(&mut self, map: Value, key: Value, value: Value) -> Option<Value> {
        self.map_set_str_in::<false>(map, key, value)
    }

    /// [`Self::map_set_str`] on a thread-local map, without its lock.
    #[inline]
    pub fn map_set_str_local(&mut self, map: Value, key: Value, value: Value) -> Option<Value> {
        self.map_set_str_in::<true>(map, key, value)
    }

    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn map_set_str_in<const THREAD_LOCAL: bool>(
        &mut self,
        map: Value,
        key: Value,
        value: Value,
    ) -> Option<Value> {
        let (container, text) = self.string_keyed(map, key)?;
        let hash = map_string_hash(text.as_str());
        let debt = self.tlab.alloc_debt();
        // SAFETY (`THREAD_LOCAL`): as for `list_push_in`.
        #[allow(unsafe_code)]
        let mut entries = if THREAD_LOCAL {
            unsafe { container.lock_mut_thread_local(debt) }
        } else {
            container.lock_mut(debt)
        };
        let previous = entries.set_hashed(
            hash,
            |candidate| self.is_string(*candidate, text),
            key,
            value,
        );
        drop(entries);
        self.settle();
        Some(previous.unwrap_or(Value::NULL))
    }

    /// The map behind `map` and the text of `key`, when `key` is a string.
    #[inline]
    fn string_keyed(&self, map: Value, key: Value) -> Option<(&Map, &BexStr)> {
        let Object::Map(map) = self.get_object(map.as_object_ptr()?) else {
            return None;
        };
        Some((map, self.as_string(&key).ok()?))
    }

    /// String keys compare by contents, as the `Map` builtins compare them.
    #[inline]
    fn is_string(&self, value: Value, text: &BexStr) -> bool {
        self.as_string(&value)
            .is_ok_and(|candidate| candidate == text)
    }
}

#[cfg(test)]
mod tests {
    use bex_vm_types::{RealizedTy, Value};

    use crate::vm::tests::test_vm;

    #[test]
    fn push_appends_through_either_path_and_declines_other_receivers() {
        let mut vm = test_vm(Vec::new());
        let list = vm.new_array(RealizedTy::Int, vec![Value::int(1)]);
        assert_eq!(vm.list_push(list, Value::int(2)), Some(Value::int(2)));
        assert_eq!(vm.list_push_local(list, Value::int(3)), Some(Value::int(3)));
        assert_eq!(
            vm.load_array_element(list, Value::int(-1)).unwrap(),
            Value::int(3)
        );
        let map = vm.new_map(RealizedTy::String, RealizedTy::Int);
        assert_eq!(vm.list_push(map, Value::int(1)), None);
        assert_eq!(vm.list_push(Value::int(1), Value::int(1)), None);
    }

    #[test]
    fn string_keys_compare_by_contents() {
        let mut vm = test_vm(Vec::new());
        let map = vm.new_map(RealizedTy::String, RealizedTy::Int);
        let key = vm.string_value("ab");
        let same = vm.string_value("ab");
        assert_eq!(vm.map_get_str(map, key), Some(None));
        assert_eq!(vm.map_set_str(map, key, Value::int(1)), Some(Value::NULL));
        assert_eq!(
            vm.map_set_str_local(map, same, Value::int(2)),
            Some(Value::int(1))
        );
        assert_eq!(vm.map_get_str_local(map, key), Some(Some(Value::int(2))));
        assert_eq!(vm.container_len(map).unwrap(), Value::int(1));
    }

    /// The builtin decides everything but a map with a string key.
    #[test]
    fn maps_decline_other_keys_and_receivers() {
        let mut vm = test_vm(Vec::new());
        let map = vm.new_map(RealizedTy::String, RealizedTy::Int);
        let key = vm.string_value("a");
        let list = vm.new_array(RealizedTy::Int, Vec::new());
        assert_eq!(vm.map_get_str(map, Value::int(1)), None);
        assert_eq!(vm.map_set_str(map, Value::int(1), Value::int(1)), None);
        assert_eq!(vm.map_get_str(list, key), None);
        assert_eq!(vm.map_set_str(list, key, Value::int(1)), None);
        assert_eq!(vm.container_len(map).unwrap(), Value::int(0));
    }
}
