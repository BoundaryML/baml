//! Maps: literals, `map[key]` read and written, and the natives that are a
//! probe and little else.
//!
//! A probe is a leaf of the runtime that returns the address of the key's
//! value, or zero. The address is inside the map's entries, so it is used
//! at once, before anything that can call or allocate, and is never a
//! value the collector is told about.

use baml_compiler2_mir::Operand;
use cranelift_codegen::ir::{self, AbiParam, InstBuilder, condcodes::IntCC, types};

use super::{
    Panic, Translator, WORD, array_access, internal, offset, payload_of, slot_access, slot_words,
};
use crate::{
    CompileError,
    check::native_type_arguments,
    layout,
    natives::{MAP_INDEX, Native},
    types::{NativeTy, narrowed},
};

impl<'db> Translator<'_, 'db> {
    /// The key and value types of a map type.
    fn map_types(ty: &NativeTy<'db>) -> Result<(NativeTy<'db>, NativeTy<'db>), CompileError> {
        match narrowed(ty.clone()) {
            NativeTy::Map(key, value) => Ok((*key, *value)),
            _ => Err(internal("a checked map operation is on a map")),
        }
    }

    /// The key the local `index` holds, as a map of type `map_ty` holds
    /// its keys.
    pub(super) fn map_key(
        &mut self,
        map_ty: &NativeTy<'db>,
        index: baml_compiler2_mir::Local,
    ) -> Result<ir::Value, CompileError> {
        let (key_ty, _) = Self::map_types(map_ty)?;
        let found = self.local_ty(index)?;
        let key = self.read_place(&baml_compiler2_mir::Place::Local(index))?;
        Ok(self.coerce(key, &found, &key_ty))
    }

    /// Call a leaf of the runtime: it neither allocates nor throws.
    fn map_leaf(
        &mut self,
        symbol: &str,
        params: &[ir::Type],
        result: ir::Type,
        arguments: &[ir::Value],
    ) -> Result<ir::Value, CompileError> {
        let params = params.iter().map(|ty| AbiParam::new(*ty)).collect();
        let callee = self.import(symbol, params, vec![AbiParam::new(result)])?;
        let call = self.builder.ins().call(callee, arguments);
        Ok(self.builder.inst_results(call)[0])
    }

    /// The address of the value of `key` in `map`, or zero.
    fn map_probe(
        &mut self,
        map: ir::Value,
        key: ir::Value,
        key_ty: &NativeTy<'db>,
    ) -> Result<ir::Value, CompileError> {
        match key_ty {
            NativeTy::String => {
                // The hash a string caches in its header, computed the
                // first time it is asked for.
                let high = self.builder.ins().uload32(slot_access(), key, 4);
                let cached = self
                    .builder
                    .ins()
                    .band_imm(high, i64::from(layout::STRING_HASH_MASK));
                let cached = self.builder.ins().ireduce(types::I32, cached);
                let compute = self.builder.create_block();
                let probe = self.builder.create_block();
                self.builder.append_block_param(probe, types::I32);
                self.builder.set_cold_block(compute);
                self.builder
                    .ins()
                    .brif(cached, probe, &[cached.into()], compute, &[]);
                self.builder.seal_block(compute);
                self.builder.switch_to_block(compute);
                let computed = self.map_leaf("baml_rt_str_hash", &[WORD], types::I32, &[key])?;
                self.builder.ins().jump(probe, &[computed.into()]);
                self.builder.seal_block(probe);
                self.builder.switch_to_block(probe);
                let hash = self.builder.block_params(probe)[0];
                self.map_leaf(
                    "baml_rt_map_find_str",
                    &[WORD, WORD, types::I32],
                    WORD,
                    &[map, key, hash],
                )
            }
            // The runtime makes a `float` key canonical, and hashes an
            // enum's variant by the position its object holds. A key
            // whose type is a type parameter is of whichever key type its
            // tag says.
            NativeTy::Float | NativeTy::Enum(_) | NativeTy::Unknown => {
                let (tag, key) = slot_words(&mut self.builder, key_ty, key);
                self.map_leaf(
                    "baml_rt_map_find",
                    &[WORD, WORD, WORD],
                    WORD,
                    &[map, tag, key],
                )
            }
            _ => {
                let key = payload_of(&mut self.builder, key, key_ty);
                self.map_leaf("baml_rt_map_find_int", &[WORD, WORD], WORD, &[map, key])
            }
        }
    }

    /// `map[key]`: the key's value, or a panic.
    pub(super) fn map_index(
        &mut self,
        map: ir::Value,
        map_ty: &NativeTy<'db>,
        key: ir::Value,
    ) -> Result<ir::Value, CompileError> {
        let (key_ty, value_ty) = Self::map_types(map_ty)?;
        let slot = self.map_probe(map, key, &key_ty)?;
        let missing = self.builder.ins().icmp_imm(IntCC::Equal, slot, 0);
        self.panic_if(missing, Panic::MapKeyNotFound)?;
        Ok(self.slot_load(&value_ty, slot, 0))
    }

    /// `map[key] = value`: one probe, and an insertion that does not
    /// search again when it finds nothing.
    pub(super) fn map_store(
        &mut self,
        map: ir::Value,
        map_ty: &NativeTy<'db>,
        key: ir::Value,
        value: ir::Value,
    ) -> Result<(), CompileError> {
        let (key_ty, value_ty) = Self::map_types(map_ty)?;
        let slot = self.map_probe(map, key, &key_ty)?;
        let replace = self.builder.create_block();
        let insert = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.ins().brif(slot, replace, &[], insert, &[]);
        self.builder.seal_block(replace);
        self.builder.seal_block(insert);
        self.builder.switch_to_block(replace);
        self.slot_store(&value_ty, value, slot, 0);
        self.builder.ins().jump(done, &[]);
        self.builder.switch_to_block(insert);
        self.map_runtime_set(
            "baml_rt_map_insert_new",
            map,
            &key_ty,
            &value_ty,
            key,
            value,
        )?;
        self.builder.ins().jump(done, &[]);
        self.builder.seal_block(done);
        self.builder.switch_to_block(done);
        Ok(())
    }

    /// Call one of the runtime's two ways to set a key: the key and the
    /// value go as the two words a slot of each one's type holds.
    fn map_runtime_set(
        &mut self,
        symbol: &str,
        map: ir::Value,
        key_ty: &NativeTy<'db>,
        value_ty: &NativeTy<'db>,
        key: ir::Value,
        value: ir::Value,
    ) -> Result<(), CompileError> {
        let callee = self.import(symbol, vec![AbiParam::new(WORD); 6], vec![])?;
        // The runtime can collect, which moves storage.
        self.forget_arrays();
        let mutator = self.mutator();
        let key = slot_words(&mut self.builder, key_ty, key);
        let value = slot_words(&mut self.builder, value_ty, value);
        self.builder
            .ins()
            .call(callee, &[mutator, map, key.0, key.1, value.0, value.1]);
        Ok(())
    }

    /// Whether two maps of string keys hold the same keys with equal
    /// values.
    pub(super) fn map_equal(
        &mut self,
        ty: &NativeTy<'db>,
        left: ir::Value,
        right: ir::Value,
    ) -> Result<ir::Value, CompileError> {
        Self::map_types(ty)?;
        self.map_leaf("baml_rt_map_eq", &[WORD, WORD], types::I8, &[left, right])
    }

    /// A map literal: the map and room for its entries in one allocation,
    /// then each entry set in turn, so that a key written twice keeps its
    /// first place and its last value.
    pub(super) fn map_literal(
        &mut self,
        ty: &NativeTy<'db>,
        entries: &[(Operand<'db>, Operand<'db>)],
    ) -> Result<ir::Value, CompileError> {
        let (key_ty, value_ty) = Self::map_types(ty)?;
        let entries = entries
            .iter()
            .map(|(key, value)| {
                Ok((
                    self.operand_as(key, &key_ty)?,
                    self.operand_as(value, &value_ty)?,
                ))
            })
            .collect::<Result<Vec<_>, CompileError>>()?;
        let word = AbiParam::new(WORD);
        let callee = self.import("baml_rt_map_new", vec![word, word], vec![word])?;
        self.forget_arrays();
        let mutator = self.mutator();
        let count = self.word_const(entries.len());
        let call = self.builder.ins().call(callee, &[mutator, count]);
        let map = self.builder.inst_results(call)[0];
        self.builder.declare_value_needs_stack_map(map);
        for (key, value) in entries {
            self.map_runtime_set("baml_rt_map_set", map, &key_ty, &value_ty, key, value)?;
        }
        Ok(map)
    }

    /// How many keys a map has.
    pub(super) fn map_length(&mut self, map: ir::Value) -> ir::Value {
        self.builder
            .ins()
            .uload32(array_access(), map, offset(layout::MAP_LEN))
    }

    /// The natives of `Map` that are compiled here rather than called:
    /// `length` is a load, and `has` and `get` a probe.
    pub(super) fn inline_map_native(
        &mut self,
        native: &'static Native,
        arguments: &[Operand<'db>],
        ntypeargs: usize,
    ) -> Result<Option<(ir::Value, NativeTy<'db>)>, CompileError> {
        if !matches!(
            native.path,
            "baml.Map.length" | "baml.Map.has" | "baml.Map.get" | MAP_INDEX
        ) {
            return Ok(None);
        }
        let (signature, _) = native.callable().map_err(internal)?;
        let types = native_type_arguments(self.db, self.body, native, arguments, ntypeargs)
            .map_err(internal)?;
        let produced = signature.result.native(self.db, &types);
        let map_ty = NativeTy::Map(Box::new(types[0].clone()), Box::new(types[1].clone()));
        let map = self.operand_as(&arguments[ntypeargs], &map_ty)?;
        if native.path == "baml.Map.length" {
            return Ok(Some((self.map_length(map), produced)));
        }
        let key = self.operand_as(&arguments[ntypeargs + 1], &types[0])?;
        if native.path == MAP_INDEX {
            let value = self.map_index(map, &map_ty, key)?;
            return Ok(Some((value, produced)));
        }
        let slot = self.map_probe(map, key, &types[0])?;
        if native.path == "baml.Map.has" {
            let found = self.builder.ins().icmp_imm(IntCC::NotEqual, slot, 0);
            return Ok(Some((found, produced)));
        }
        // The value as a pair, or `null` for a key the map has not.
        let read = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.append_block_param(done, WORD);
        self.builder.append_block_param(done, WORD);
        let zero = self.builder.ins().iconst(WORD, 0);
        self.builder
            .ins()
            .brif(slot, read, &[], done, &[zero.into(), zero.into()]);
        self.builder.seal_block(read);
        self.builder.switch_to_block(read);
        let found = self.slot_load(&types[1], slot, 0);
        let (tag, payload) = slot_words(&mut self.builder, &types[1], found);
        self.builder.ins().jump(done, &[tag.into(), payload.into()]);
        self.builder.seal_block(done);
        self.builder.switch_to_block(done);
        let (tag, payload) = (
            self.builder.block_params(done)[0],
            self.builder.block_params(done)[1],
        );
        let value = self.pair(tag, payload);
        self.builder.declare_value_needs_stack_map(value);
        Ok(Some((value, produced)))
    }
}
