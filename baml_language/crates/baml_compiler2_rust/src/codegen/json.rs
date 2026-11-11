//! When nothing observes calls (`vm.plain_calls()`: telemetry off, or low
//! with no policy), `baml.json.decode<T>` sites read through the module's
//! schema for `T`, and
//! `baml.json.stringify(value)` sites write through the schema of the
//! value's static type ([`crate::json_codec`]); a miss runs the call the
//! site always made. A level that observes calls sees `decode` and
//! `stringify` called as the interpreter calls them.
//!
//! Replaces, for a type with a schema, the VM's `_decode` and `_stringify`
//! on the success path: per-call plans, type substitution, boxed instance
//! building, and the walk that boxes every field it writes.

use baml_compiler2_mir::{Constant, Operand, Terminator, function_display_name};

use super::{Expr, Gen, Generator};
use crate::{analysis::Repr, json_codec::Direction, render::coerce};

/// The `omit_nulls` operand of a `stringify` call's options after the value
/// (`Some(None)` when omitted), or `None` when an option the schema does not
/// write (an `indent`) is passed.
fn encode_options<'m, 'db>(
    layout: Option<&baml_type::CallLayout>,
    rest: &'m [Operand<'db>],
) -> Option<Option<&'m Operand<'db>>> {
    let names: Vec<Option<&str>> = match layout {
        Some(layout) => layout.0.iter().skip(1).map(|slot| slot.as_ref().map(|name| name.as_str())).collect(),
        None if rest.is_empty() => Vec::new(),
        None => return None,
    };
    let passed = |at: usize| !matches!(rest.get(at), Some(Operand::Constant(Constant::OmittedArg)));
    let mut omit_nulls = None;
    for (at, name) in names.iter().enumerate() {
        match name {
            Some("naming") => {}
            Some("omit_nulls") if passed(at) => omit_nulls = Some(&rest[at]),
            Some("omit_nulls") => {}
            // The interpreter's text writer indents; this one writes compact.
            Some("indent") if !passed(at) => {}
            _ => return None,
        }
    }
    Some(omit_nulls)
}

/// An encode site: its value and `omit_nulls` operands and the schema of the
/// value's type.
pub(super) struct EncodeSite<'m, 'db> {
    value: &'m Operand<'db>,
    omit_nulls: Option<&'m Operand<'db>>,
    schema: String,
}

/// A decode site: its text and type operands and its type's schema.
pub(super) struct DecodeSite<'m, 'db> {
    text: &'m Operand<'db>,
    ty: &'m Operand<'db>,
    schema: String,
}

impl<'a, 'db> Generator<'a, 'db> {
    /// `terminator` as a decode site, when it calls `baml.json.decode` with
    /// a type argument that has a schema.
    pub(super) fn decode_site<'m>(
        &self,
        terminator: &'m Terminator<'db>,
    ) -> Option<DecodeSite<'m, 'db>> {
        let Terminator::Call {
            callee: Operand::Constant(Constant::Function(func)),
            args,
            ntypeargs: 1,
            ..
        } = terminator
        else {
            return None;
        };
        if function_display_name(self.db, *func) != "baml.json.decode" {
            return None;
        }
        let [ty, text, ..] = args.as_slice() else {
            return None;
        };
        let template = self.type_arg_template(ty)?;
        let anchored = self.index.anchor(template)?;
        let schema = self
            .json
            .borrow_mut()
            .schema_for(self.index, &anchored, Direction::Decode)?;
        Some(DecodeSite { text, ty, schema })
    }

    /// `terminator` as an encode site, when it calls `baml.json.stringify`
    /// on a value whose static type has a schema, with no `indent`.
    pub(super) fn encode_site<'m>(
        &self,
        terminator: &'m Terminator<'db>,
    ) -> Option<EncodeSite<'m, 'db>> {
        let Terminator::Call {
            callee: Operand::Constant(Constant::Function(func)),
            args,
            ntypeargs: 0,
            argument_layout,
            ..
        } = terminator
        else {
            return None;
        };
        if function_display_name(self.db, *func) != "baml.json.stringify" {
            return None;
        }
        let (value, rest) = args.split_first()?;
        let omit_nulls = encode_options(argument_layout.as_ref(), rest)?;
        let (Operand::Copy(baml_compiler2_mir::Place::Local(local))
        | Operand::Move(baml_compiler2_mir::Place::Local(local))) = value
        else {
            return None;
        };
        let realized =
            baml_compiler2_mir::RealizedTy::try_from(self.body.locals[local.0].ty.clone()).ok()?;
        let template = baml_compiler2_mir::TyTemplate::from(realized);
        let anchored = self.index.anchor(&template)?;
        let schema = self
            .json
            .borrow_mut()
            .schema_for(self.index, &anchored, Direction::Encode)?;
        Some(EncodeSite { value, omit_nulls, schema })
    }

    /// The static type a type-argument operand loads, when it is static
    /// (not one the frame binds).
    fn type_arg_template(
        &self,
        operand: &Operand<'db>,
    ) -> Option<&'a baml_compiler2_mir::TyTemplate> {
        use baml_compiler2_mir::{Place, Rvalue, StatementKind};
        let (Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local))) = operand
        else {
            return None;
        };
        if !self.analysis.static_types.contains_key(&local.0) {
            return None;
        }
        let body: &'a baml_compiler2_mir::MirFunctionBody<'db> = self.body;
        body.blocks
            .iter()
            .flat_map(|block| &block.statements)
            .find_map(|statement| match &statement.kind {
                StatementKind::Assign {
                    destination: Place::Local(assigned),
                    value: Rvalue::LoadType(template),
                } if assigned == local => Some(template),
                _ => None,
            })
    }

    /// Emit decode site `block`: the schema's decode, and the call the site
    /// made (`fallback`) when it misses.
    pub(super) fn decode_through_schema(
        &mut self,
        block: baml_compiler2_mir::BlockId,
        site: DecodeSite<'_, 'db>,
        fallback: impl FnOnce(&mut Self) -> Gen<()>,
    ) -> Gen<()> {
        let Some(Terminator::Call { destination, .. }) = &self.body.blocks[block.0].terminator
        else {
            unreachable!("a decode site is a call");
        };
        let text = self.value_operand(block, site.text)?;
        let ty = self.type_operand("call", site.ty)?;
        let store = self.store(block, destination, &Expr::atom("decoded", Repr::Value))?;
        self.emit_statement(&format!(
            "if let Some(decoded) = (if vm.plain_calls() {{ bex_vm::compiled_rt::json::decode_json(vm, {text}, &{ty}, &{}) }} else {{ None }}) {{ {store} }} else {{",
            site.schema
        ));
        fallback(self)?;
        self.emit_statement("}");
        Ok(())
    }

    /// Emit encode site `block`: the schema's encode, and the call the site
    /// made (`fallback`) when it misses.
    pub(super) fn encode_through_schema(
        &mut self,
        block: baml_compiler2_mir::BlockId,
        site: EncodeSite<'_, 'db>,
        fallback: impl FnOnce(&mut Self) -> Gen<()>,
    ) -> Gen<()> {
        let Some(Terminator::Call { destination, .. }) = &self.body.blocks[block.0].terminator
        else {
            unreachable!("an encode site is a call");
        };
        let value = self.value_operand(block, site.value)?;
        let omit_nulls = match site.omit_nulls {
            Some(operand) => {
                let operand = self.operand(block, operand)?;
                coerce(&operand.code, operand.repr, Repr::Bool)
            }
            None => "false".into(),
        };
        let store = self.store(block, destination, &Expr::atom("encoded", Repr::Value))?;
        self.emit_statement(&format!(
            "if let Some(encoded) = (if vm.plain_calls() {{ bex_vm::compiled_rt::json::encode_json(vm, {value}, &{}, {omit_nulls}) }} else {{ None }}) {{ {store} }} else {{",
            site.schema
        ));
        fallback(self)?;
        self.emit_statement("}");
        Ok(())
    }
}
