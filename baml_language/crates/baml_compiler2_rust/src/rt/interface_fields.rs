//! Interface field links resolve to offsets in each concrete class layout.
use super::*;
pub(super) struct Feature;
impl registry::Feature for Feature {
    fn value(
        &self,
        e: &Emitter<'_, '_>,
        value: &Rvalue<'_>,
        expected: Repr,
    ) -> Option<Gen<(String, Repr)>> {
        let Rvalue::VirtualFieldAccess {
            iface,
            receiver,
            field_index,
            ..
        } = value
        else {
            return None;
        };
        Some((|| {
            let receiver = e.coerce(e.operand(receiver)?, Repr::Tagged)?;
            let mut code = format!("{{let receiver={receiver};match receiver.tag{{");
            for (class, field) in super::dispatch::field_targets(e.db, e.plan, iface, *field_index)?
            {
                let Some(layout) = e.plan.layouts.get(&class) else {
                    continue;
                };
                let repr = layout.fields[field];
                let raw = format!("(*(receiver.payload as *const C{class})).f{field}");
                let raw = if repr == Repr::Bool {
                    format!("({raw}!=0)")
                } else {
                    raw
                };
                let value = e.coerce((raw, repr), expected)?;
                write!(code, "{}=>{value},", e.descriptors[&class] + 3).unwrap();
            }
            let fail = e.fallible(&format!(
                "Err::<{},_>(rt::Error::internal(\"invalid interface field receiver\".into()))",
                expected.rust()
            ));
            write!(code, "_=>{fail}}}}}").unwrap();
            Ok((code, expected))
        })())
    }
    fn statement(
        &self,
        e: &mut Emitter<'_, '_>,
        statement: &StatementKind<'_>,
    ) -> Option<Gen<String>> {
        let StatementKind::VirtualFieldStore {
            iface,
            receiver,
            field_index,
            value,
            ..
        } = statement
        else {
            return None;
        };
        Some((|| {
            let receiver = e.coerce(e.operand(receiver)?, Repr::Tagged)?;
            let (value, repr) = e.operand(value)?;
            let mut code =
                format!("{{let value={value};let receiver={receiver};match receiver.tag{{");
            for (class, field) in super::dispatch::field_targets(e.db, e.plan, iface, *field_index)?
            {
                let Some(layout) = e.plan.layouts.get(&class) else {
                    continue;
                };
                let target = layout.fields[field];
                let value = e.coerce(("value".into(), repr), target)?;
                let slot = format!("&raw mut (*(receiver.payload as *mut C{class})).f{field}");
                let store = if target.pointer() {
                    format!("baml_rt_gc::write_ptr(receiver.payload as *mut u8,{slot},value);")
                } else if matches!(target, Repr::Tagged | Repr::Dyn(_)) {
                    format!("rt::write_dyn(receiver.payload as *mut u8,{slot},value);")
                } else {
                    format!("({slot}).write(value as _);")
                };
                write!(
                    code,
                    "{}=>{{let value={value};{store}}},",
                    e.descriptors[&class] + 3
                )
                .unwrap();
            }
            let fail = e.fallible(
                "Err::<(),_>(rt::Error::internal(\"invalid interface field receiver\".into()))",
            );
            write!(code, "_=>{fail}}}}}").unwrap();
            Ok(code)
        })())
    }
}
