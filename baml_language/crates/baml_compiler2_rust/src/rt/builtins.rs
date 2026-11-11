use super::{BTreeSet, Gen, no};

pub(super) fn emit(out: &mut String, builtins: &BTreeSet<String>) -> Gen<()> {
    if !builtins.is_empty() {
        let (mut declarations, io, _) =
            baml_builtins2_codegen::extract_native_builtins().map_err(no)?;
        declarations.extend(io);
        out.push_str(&baml_builtins2_codegen::generate_rt(
            &declarations,
            builtins,
        )?);
    }
    Ok(())
}

impl super::Emitter<'_, '_> {
    pub(super) fn native_closure_adapter(&self) -> Gen<String> {
        use super::{Repr, registry};
        let object = self.object;
        let name = self.plan.index.builtin_path(object);
        let info = self.plan.index.info(object);
        let frame = self.plan.linked_function_frame(object)?;
        let mut prelude = String::new();
        let mut values = Vec::new();
        for (i, t) in info.param_types.iter().enumerate() {
            let repr = self.plan.field_repr(&t.substitute_symbolic(&frame))?;
            let raw = if i == 0 {
                "if rt::closures::is_bound(env){rt::closures::load(rt::closures::capture(env,0))}else{args[0]}".to_owned()
            } else {
                format!(
                    "args[if rt::closures::is_bound(env){{{}}}else{{{i}}}]",
                    i - 1
                )
            };
            let value = self.closure_argument(raw, repr)?;
            prelude.push_str(&format!("let a{i}={value};"));
            values.push((format!("a{i}"), repr));
        }
        let mut types = Vec::new();
        if !super::callbacks::native(name)
            && let Some(frame) = self.plan.function_frames.get(&object)
        {
            for (_, ty) in frame {
                let realized = baml_compiler2_mir::RealizedTy::try_from(ty.clone())
                    .map_err(|_| no("builtin frame"))?;
                types.push(Repr::Type(
                    self.plan
                        .type_code(&baml_compiler2_mir::TyTemplate::from(realized))?,
                ));
            }
        }
        let (expression, repr) = if name == "baml.Array.length" && !values[0].1.growing() {
            (format!("*((rt::nonnull(a0)?+8) as *const i64)"), Repr::Int)
        } else if let Some(call) = registry::call(self, name, &types, &values) {
            call?
        } else {
            match name {
            "baml.String.char_count" => ("rt_string_char_count(&rt::strings::Cx::current(),rt::strings::Str::from_raw(a0))".into(),Repr::Int),
            "baml.String.slice" => ("rt_string_slice(&mut rt::strings::Cx::current(),rt::strings::Str::from_raw(a0),a1,a2)?.as_raw()".into(),Repr::Str),
            "baml.String.to_code_points" => (format!("rt_code_points(&mut rt::strings::Cx::current(),rt::strings::Str::from_raw(a0),{})?",self.lists[&Repr::ListInt]),Repr::ListInt),
            "baml._to_string_default" if values.len()==1 && values[0].1==Repr::Int => ("rt_int_to_string(&mut rt::strings::Cx::current(),a0)?.as_raw()".into(),Repr::Str),
            _ => return Err(no(format!("builtin function value {name}"))),
        }
        };
        let expression = self.coerce((expression, repr), Repr::Tagged)?;
        let cxt = super::exceptions::context_type(self.plan.error_contexts);
        Ok(format!(
            "static O{object}:std::sync::atomic::AtomicU8=std::sync::atomic::AtomicU8::new(0);unsafe fn closure{object}(env:usize,_site:usize,args:&[rt::Dyn])->R<rt::Dyn>{{unsafe{{let cx={cxt}::current();{prelude}Ok({expression})}}}}\n"
        ))
    }
}
