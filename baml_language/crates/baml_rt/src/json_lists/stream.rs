//! Direct typed decoding for scalar, class and list schemas. A structural
//! mismatch returns to the diagnostic decoder; syntax was checked first.
use super::*;
use bex_vm_types::json_text::{Reader, Stop};
enum Failure {
    Mismatch,
    Runtime(Box<crate::Error>),
}
impl From<Stop> for Failure {
    fn from(_: Stop) -> Self {
        Self::Mismatch
    }
}
impl From<Box<crate::Error>> for Failure {
    fn from(e: Box<crate::Error>) -> Self {
        Self::Runtime(e)
    }
}
type Fast<T> = Result<T, Failure>;
fn required<T>(v: Option<T>) -> Fast<T> {
    v.ok_or(Failure::Mismatch)
}
fn store(pointer: *mut u8, field: &Field, value: Dyn) {
    unsafe {
        let slot = pointer.add(field.offset);
        match field.slot {
            SlotKind::Tagged => crate::write_dyn(pointer, slot.cast(), value),
            SlotKind::Pointer => write_ptr(pointer, slot.cast(), value.payload as usize),
            SlotKind::Scalar => slot.cast::<u64>().write(value.payload),
        }
    }
}
unsafe fn build(cx: &mut Cx, r: &mut Reader<'_>, ty: &Ty) -> Fast<Dyn> {
    Ok(match ty {
        Ty::Int | Ty::LiteralInt(_) => {
            let n = required(r.number()?.as_i64().and_then(baml_type::Int63::new))?.get();
            if matches!(ty,Ty::LiteralInt(w) if *w!=n) {
                return Err(Failure::Mismatch);
            }
            Dyn {
                tag: 0,
                payload: n as u64,
            }
        }
        Ty::Float | Ty::LiteralFloat(_) => {
            let n = required(r.number()?.as_f64())?;
            if matches!(ty,Ty::LiteralFloat(w) if (n-*w).abs()>=f64::EPSILON) {
                return Err(Failure::Mismatch);
            }
            Dyn {
                tag: crate::FLOAT_TAG,
                payload: n.to_bits(),
            }
        }
        Ty::Bool | Ty::LiteralBool(_) => {
            let b = r.bool()?;
            if matches!(ty,Ty::LiteralBool(w) if *w!=b) {
                return Err(Failure::Mismatch);
            }
            Dyn {
                tag: 1,
                payload: b as u64,
            }
        }
        Ty::Null => {
            r.null()?;
            Dyn { tag: 2, payload: 0 }
        }
        Ty::String | Ty::Literal(_) => {
            let text = r.string()?;
            if matches!(ty,Ty::Literal(w) if *w!=text) {
                return Err(Failure::Mismatch);
            }
            Dyn {
                tag: 3,
                payload: cx.string(text)?.as_raw() as u64,
            }
        }
        Ty::Union(members) => {
            // These two common shapes have an unambiguous scalar decision,
            // including the VM's preference for int over float.
            if members.len() == 2 && members.iter().any(|m| matches!(m.ty, Ty::Null)) {
                let null = r.peek() == Some(b'n');
                let member = members
                    .iter()
                    .find(|m| matches!(m.ty, Ty::Null) == null)
                    .unwrap();
                unsafe { build(cx, r, member.ty) }?
            } else if members.iter().all(|m| matches!(m.ty, Ty::Int | Ty::Float)) {
                let mark = r.mark();
                let number = r.number()?;
                let integer = number.as_i64().and_then(baml_type::Int63::new).is_some();
                let selected = members
                    .iter()
                    .find(|m| integer && matches!(m.ty, Ty::Int))
                    .or_else(|| members.iter().find(|m| matches!(m.ty, Ty::Float)))
                    .ok_or(Failure::Mismatch)?;
                r.reset(mark);
                unsafe { build(cx, r, selected.ty) }?
            } else {
                return Err(Failure::Mismatch);
            }
        }
        Ty::Class {
            desc,
            bytes,
            fields,
            ..
        } => {
            if fields.iter().any(|f| f.skip) {
                return Err(Failure::Mismatch);
            }
            r.open_object()?;
            let pointer = unsafe { crate::try_alloc(*desc, *bytes) }?;
            let mut seen = SmallVec::<[bool; 16]>::new();
            seen.resize(fields.len(), false);
            let mut first = true;
            while let Some(key) = r.next_key(first)? {
                first = false;
                let index = fields.iter().position(|f| f.name == key);
                if let Some(i) = index {
                    let value = unsafe { build(cx, r, fields[i].ty) }?;
                    store(pointer, &fields[i], value);
                    seen[i] = true;
                } else {
                    r.skip_value()?;
                }
            }
            for (field, present) in fields.iter().zip(seen) {
                if !present {
                    if !field.ty.nullable() {
                        return Err(Failure::Mismatch);
                    }
                    store(pointer, field, Dyn { tag: 2, payload: 0 });
                }
            }
            Dyn {
                tag: *desc as u64 + 3,
                payload: pointer as u64,
            }
        }
        Ty::List {
            desc,
            element,
            slot,
        } => {
            unsafe fn fill<T: Slot>(
                cx: &mut Cx,
                r: &mut Reader<'_>,
                desc: u32,
                element: &Ty,
                convert: impl Fn(Dyn) -> T,
            ) -> Fast<usize> {
                r.open_array()?;
                let list = unsafe { List::<T>::new(cx, desc) }?;
                let mut i = 0;
                while r.next_element(i == 0)? {
                    let value = unsafe { build(cx, r, element) }?;
                    list.push(cx, convert(value))?;
                    i += 1;
                    if i & 255 == 0 {
                        cx.poll()?;
                    }
                }
                Ok(list.as_raw())
            }
            let pointer = unsafe {
                match slot {
                    SlotKind::Tagged => fill::<Dyn>(cx, r, *desc, element, |v| v),
                    SlotKind::Pointer => {
                        fill::<Ptr>(cx, r, *desc, element, |v| Ptr::from_raw(v.payload as usize))
                    }
                    SlotKind::Scalar => match element {
                        Ty::Float | Ty::LiteralFloat(_) => {
                            fill::<f64>(cx, r, *desc, element, |v| f64::from_bits(v.payload))
                        }
                        Ty::Bool | Ty::LiteralBool(_) => {
                            fill::<bool>(cx, r, *desc, element, |v| v.payload != 0)
                        }
                        _ => fill::<i64>(cx, r, *desc, element, |v| v.payload as i64),
                    },
                }
            }?;
            Dyn {
                tag: *desc as u64 + 3,
                payload: pointer as u64,
            }
        }
        _ => return Err(Failure::Mismatch),
    })
}
pub(super) unsafe fn decode(cx: &mut Cx, source: &str, ty: &Ty) -> R<Option<Dyn>> {
    match unsafe { build(cx, &mut Reader::new(source), ty) } {
        Ok(v) => Ok(Some(v)),
        Err(Failure::Mismatch) => Ok(None),
        Err(Failure::Runtime(e)) => Err(e),
    }
}
