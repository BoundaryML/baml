//! `string.slice` without a call, where the answer is a slice object.

use baml_compiler2_mir::{Constant, Operand};
use cranelift_codegen::ir::{self, InstBuilder, condcodes::IntCC};

use super::{Translator, WORD, array_access, imm, internal, offset, slot_access};
use crate::{CompileError, layout, natives::Native, types::NativeTy};

/// The native this is the common case of.
pub(super) const STRING_SLICE: &str = "baml.String.slice";

impl<'db> Translator<'_, 'db> {
    /// `string.slice(start, end)`: make the slice object here when the
    /// string is all ASCII, the bounds need no resolving (`0 <= start <=
    /// end <= length`), and the part is long enough for a slice and not the
    /// whole string. Anything else calls the native, which decides the
    /// same way.
    pub(super) fn inline_string_slice(
        &mut self,
        native: &'static Native,
        arguments: &[Operand<'db>],
        ntypeargs: usize,
    ) -> Result<(ir::Value, NativeTy<'db>), CompileError> {
        let [string, start, end] = &arguments[ntypeargs..] else {
            return Err(internal("a checked string slice has two bounds"));
        };
        // What bounds written as numbers settle here.
        let written = |bound: &Operand<'db>| match bound {
            Operand::Constant(Constant::Int(value)) => Some(*value),
            _ => None,
        };
        let (ordered, long_enough) = match (written(start), written(end)) {
            (Some(start), Some(end)) => {
                if start < 0 || end < start || end - start < imm(layout::SLICE_MIN_BYTES) {
                    return self.native_call_out_of_line(native, arguments, ntypeargs);
                }
                (true, true)
            }
            _ => (false, false),
        };
        let string = self.operand_as(string, &NativeTy::String)?;
        let start = self.operand_as(start, &NativeTy::Int)?;
        let end = self.operand_as(end, &NativeTy::Int)?;

        let call = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.append_block_param(done, WORD);
        self.builder.set_cold_block(call);

        let lengths =
            self.builder
                .ins()
                .load(WORD, array_access(), string, offset(layout::STRING_LENGTHS));
        let chars = self
            .builder
            .ins()
            .ushr_imm(lengths, i64::from(layout::STRING_CHAR_COUNT_SHIFT));
        let bytes = self.builder.ins().band_imm(lengths, 0xFFFF_FFFF);
        let count = self.builder.ins().isub(end, start);
        let shortest = self.word_const(layout::SLICE_MIN_BYTES);
        // A negative bound is a large unsigned one, so it fails here too.
        let mut refusals = vec![(IntCC::NotEqual, chars, bytes)];
        if !ordered {
            refusals.push((IntCC::UnsignedGreaterThan, start, end));
        }
        refusals.push((IntCC::UnsignedGreaterThan, end, bytes));
        if !long_enough {
            refusals.push((IntCC::UnsignedLessThan, count, shortest));
        }
        refusals.push((IntCC::Equal, count, bytes));
        for (condition, left, right) in refusals {
            let refused = self.builder.ins().icmp(condition, left, right);
            let next = self.builder.create_block();
            self.builder.ins().brif(refused, call, &[], next, &[]);
            self.builder.seal_block(next);
            self.builder.switch_to_block(next);
        }
        self.builder.seal_block(call);

        let slice = self.allocate(layout::SLICE_SIZE)?;
        // Read after the allocation, which can move the string. It has at
        // least a slice's shortest text, which makes it as large as a
        // slice: the two words read of a string that is not one are its
        // own text, and are not used.
        let header = self.builder.ins().uload32(slot_access(), string, 0);
        let holds_text = self
            .builder
            .ins()
            .icmp_imm(IntCC::Equal, header, imm(layout::TAG_STRING));
        let its_parent =
            self.builder
                .ins()
                .load(WORD, slot_access(), string, offset(layout::SLICE_PARENT));
        let its_offset =
            self.builder
                .ins()
                .load(WORD, slot_access(), string, offset(layout::SLICE_OFFSET));
        let parent = self.builder.ins().select(holds_text, string, its_parent);
        let from = self.builder.ins().iadd(its_offset, start);
        let from = self.builder.ins().select(holds_text, start, from);
        let shifted = self
            .builder
            .ins()
            .ishl_imm(count, i64::from(layout::STRING_CHAR_COUNT_SHIFT));
        let lengths = self.builder.ins().bor(count, shifted);
        let slice_type = self.word_const(layout::TAG_STRING_SLICE);
        let ins = self.builder.ins();
        ins.store(slot_access(), slice_type, slice, 0);
        let ins = self.builder.ins();
        ins.store(
            array_access(),
            lengths,
            slice,
            offset(layout::STRING_LENGTHS),
        );
        let ins = self.builder.ins();
        ins.store(slot_access(), parent, slice, offset(layout::SLICE_PARENT));
        let ins = self.builder.ins();
        ins.store(slot_access(), from, slice, offset(layout::SLICE_OFFSET));
        self.builder.ins().jump(done, &[slice.into()]);

        self.builder.switch_to_block(call);
        let (called, produced) = self.native_call_out_of_line(native, arguments, ntypeargs)?;
        self.builder.ins().jump(done, &[called.into()]);

        self.builder.seal_block(done);
        self.builder.switch_to_block(done);
        let result = self.builder.block_params(done)[0];
        self.builder.declare_value_needs_stack_map(result);
        Ok((result, produced))
    }
}
