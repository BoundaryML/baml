//! Typed native leaves. Their Rust storage belongs to one program instance.
use crate::R;
use num_bigint::BigInt;
pub const BIGINT_DESC: u32 = 18;
pub const BYTES_BUFFER_DESC: u32 = 19;
pub const BYTES_DESC: u32 = 16;
pub const OPAQUE_DESC: u32 = 15;
pub const BIGINT_TAG: u64 = BIGINT_DESC as u64 + 3;
pub const BYTES_TAG: u64 = BYTES_DESC as u64 + 3;
pub const OPAQUE_TAG: u64 = OPAQUE_DESC as u64 + 3;
pub const BIGINT_LAYOUT: baml_rt_gc::Descriptor = baml_rt_gc::Descriptor {
    kind: baml_rt_gc::Kind::Opaque,
    size: 8,
    ptr_slots: (0, 0),
    tagged_slots: (0, 0),
    elem: Some(baml_rt_gc::Elements {
        offset: 24,
        layout: baml_rt_gc::ElementLayout::Scalar,
    }),
    type_id: BIGINT_TAG as u32,
    fields: &[],
    finalizer: None,
};
pub const BYTES_LAYOUT: baml_rt_gc::Descriptor = baml_rt_gc::Descriptor {
    kind: baml_rt_gc::Kind::Opaque,
    size: 32,
    ptr_slots: (8, 1),
    tagged_slots: (0, 0),
    elem: None,
    type_id: 19,
    fields: &[],
    finalizer: None,
};
pub const BYTES_BUFFER_LAYOUT: baml_rt_gc::Descriptor = baml_rt_gc::Descriptor {
    elem: Some(baml_rt_gc::Elements {
        offset: 8,
        layout: baml_rt_gc::ElementLayout::Scalar,
    }),
    type_id: BYTES_BUFFER_DESC + 3,
    ..BIGINT_LAYOUT
};
pub fn bigint(value: BigInt) -> R<usize> {
    let (sign, bytes) = value.to_bytes_le();
    let p = unsafe { crate::try_alloc(BIGINT_DESC, 24 + bytes.len().next_multiple_of(8))? };
    unsafe {
        p.add(8).cast::<usize>().write(bytes.len());
        p.add(16).cast::<i64>().write(match sign {
            num_bigint::Sign::Minus => -1,
            num_bigint::Sign::NoSign => 0,
            num_bigint::Sign::Plus => 1,
        });
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), p.add(24), bytes.len());
    }
    Ok(p as usize)
}
pub unsafe fn bigint_owned(value: usize) -> BigInt {
    let p = value as *const u8;
    unsafe {
        let sign = match p.add(16).cast::<i64>().read() {
            -1 => num_bigint::Sign::Minus,
            0 => num_bigint::Sign::NoSign,
            _ => num_bigint::Sign::Plus,
        };
        BigInt::from_bytes_le(
            sign,
            std::slice::from_raw_parts(p.add(24), p.add(8).cast::<usize>().read()),
        )
    }
}
pub fn bigint_literal(text: &str) -> R<usize> {
    bigint(text.parse().expect("compiler bigint literal"))
}
pub fn bytes(value: Vec<u8>) -> R<usize> {
    let p = unsafe { crate::try_alloc(BYTES_DESC, 32)? } as usize;
    unsafe {
        set_bytes(p, &value)?;
    }
    Ok(p)
}
unsafe fn set_bytes(value: usize, bytes: &[u8]) -> R<()> {
    let p = value as *mut u8;
    unsafe {
        let capacity = p.add(24).cast::<usize>().read();
        if bytes.len() > capacity {
            let next_capacity = bytes
                .len()
                .checked_next_power_of_two()
                .unwrap_or(bytes.len())
                .max(8);
            let buffer = crate::try_alloc(BYTES_BUFFER_DESC, 8 + next_capacity)?;
            baml_rt_gc::write_ptr(p, p.add(8).cast(), buffer as usize);
            p.add(24).cast::<usize>().write(next_capacity);
        }
        if !bytes.is_empty() {
            let buffer = p.add(8).cast::<*mut u8>().read();
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer.add(8), bytes.len());
        }
        p.add(16).cast::<usize>().write(bytes.len());
    }
    Ok(())
}
pub unsafe fn bytes_owned(value: usize) -> Vec<u8> {
    let p = value as *const u8;
    unsafe {
        let len = p.add(16).cast::<usize>().read();
        if len == 0 {
            vec![]
        } else {
            std::slice::from_raw_parts(p.add(8).cast::<*const u8>().read().add(8), len).to_vec()
        }
    }
}
pub unsafe fn with_bytes<T>(value: usize, call: impl FnOnce(&mut Vec<u8>) -> T) -> R<T> {
    let mut owned = unsafe { bytes_owned(value) };
    let result = call(&mut owned);
    unsafe {
        set_bytes(value, &owned)?;
    }
    Ok(result)
}

pub use bex_vm_types::bigint as bigint_body;
fn operand(value: crate::Dyn) -> BigInt {
    match value.tag {
        0 => BigInt::from(value.payload as i64),
        BIGINT_TAG => unsafe { bigint_owned(value.payload as usize) },
        _ => unreachable!("typed bigint operand"),
    }
}
pub fn compare(l: crate::Dyn, r: crate::Dyn, op: &str) -> R<bool> {
    let l = operand(l);
    let r = operand(r);
    Ok(match op {
        "==" => l == r,
        "!=" => l != r,
        "<" => l < r,
        "<=" => l <= r,
        ">" => l > r,
        ">=" => l >= r,
        _ => unreachable!(),
    })
}
pub fn binary(left: crate::Dyn, right: crate::Dyn, op: &str) -> R<usize> {
    use bex_vm_types::errors::VmPanic;
    use num_bigint::Sign;
    let l = operand(left);
    let r = operand(right);
    let alloc = |message| crate::language::from_panic(VmPanic::AllocFailure { message });
    let result = match op {
        "+" => l + r,
        "-" => l - r,
        "*" => {
            let bits = l.bits().saturating_add(r.bits());
            if bits > baml_type::MAX_BIGINT_BITS {
                return Err(alloc(format!(
                    "bigint mul: result of bigint multiplication would require ~{bits} bits (limit: {})",
                    baml_type::MAX_BIGINT_BITS
                )));
            }
            l * r
        }
        "/" | "%" => {
            if r == BigInt::ZERO {
                let p = unsafe { crate::try_alloc(9, 24)? };
                unsafe { crate::write_dyn(p, p.add(8).cast(), left) };
                let value = l.to_string();
                return Err(Box::new(crate::Error {
                    telemetry_caught: None,
                    telemetry_history: Default::default(),
                    thrown_value: None,
                    exit_code: None,
                    context: 0,
                    cause: 0,
                    trace: None,
                    diagnostic_value: None,
                    value: p as usize,
                    class: "baml.panics.DivisionByZero",
                    message: format!("division by zero: {l} / 0"),
                    frames: vec![],
                    telemetry_at: None,
                    telemetry_raise: None,
                    display_value: Some(format!(
                        "baml.panics.DivisionByZero {{dividend: {value}}}"
                    )),
                }));
            }
            if op == "/" { l / r } else { l % r }
        }
        "&" => l & r,
        "|" => l | r,
        "^" => l ^ r,
        "<<" | ">>" => {
            let name = if op == "<<" { "shl" } else { "shr" };
            if r.sign() == Sign::Minus {
                return Err(crate::language::from_panic(VmPanic::NegativeBitShift {
                    message: format!("bigint {name}: negative shift count ({r})"),
                }));
            }
            let shift = usize::try_from(&r);
            if op == "<<" {
                let shift = shift.map_err(|_| {
                    alloc(format!(
                        "bigint shl: shift count ({r}) does not fit in usize"
                    ))
                })?;
                let bits = l.bits().saturating_add(shift as u64);
                if bits > baml_type::MAX_BIGINT_BITS {
                    return Err(alloc(format!(
                        "bigint shl: result of {l} << {shift} would require ~{bits} bits (limit: {})",
                        baml_type::MAX_BIGINT_BITS
                    )));
                }
                l << shift
            } else {
                match shift {
                    Ok(n) => l >> n,
                    Err(_) if l.sign() == Sign::Minus => BigInt::from(-1),
                    Err(_) => BigInt::ZERO,
                }
            }
        }
        _ => return Err(crate::Error::internal(format!("bigint operator {op}"))),
    };
    bigint(result)
}
pub use bex_vm_types::bytes as bytes_body;
pub fn optional_int(value: Option<i64>) -> crate::Dyn {
    value.map_or(crate::Dyn { tag: 2, payload: 0 }, |v| crate::Dyn {
        tag: 0,
        payload: v as u64,
    })
}
pub unsafe fn int_list_owned(value: usize) -> Vec<i64> {
    let cx = unsafe { crate::strings::Cx::current() };
    let list = unsafe { crate::lists::List::<i64>::from_raw(value) };
    (0..list.len(&cx))
        .map(|i| list.at(&cx, i).unwrap())
        .collect()
}
pub fn int_list(desc: u32, values: Vec<i64>) -> R<usize> {
    let mut cx = unsafe { crate::strings::Cx::current() };
    let list = unsafe { crate::lists::List::<i64>::new(&mut cx, desc)? };
    for v in values {
        list.push(&mut cx, v)?;
    }
    Ok(list.as_raw())
}
pub unsafe fn byte_index(value: usize, index: i64) -> R<i64> {
    let bytes = unsafe { bytes_owned(value) };
    bytes_body::at(&bytes, index).ok_or_else(|| crate::language::index_error(index, bytes.len()))
}
pub unsafe fn byte_set(value: usize, index: i64, item: i64) -> R<()> {
    unsafe {
        with_bytes(value, |bytes| {
            let len = bytes.len();
            let resolved = if index < 0 { index + len as i64 } else { index };
            if let Ok(i) = usize::try_from(resolved)
                && i < len
            {
                bytes[i] = item as u8;
                Ok(())
            } else {
                Err(crate::language::index_error(index, len))
            }
        })?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    static D: &[baml_rt_gc::Descriptor] = &{
        let mut descriptors = [crate::STRING_DESC; 20];
        descriptors[9] = crate::DIV_DESC;
        descriptors[BIGINT_DESC as usize] = BIGINT_LAYOUT;
        descriptors[16] = BYTES_LAYOUT;
        descriptors[15] = crate::opaque::descriptor(18);
        descriptors[BYTES_BUFFER_DESC as usize] = BYTES_BUFFER_LAYOUT;
        descriptors
    };
    #[test]
    fn native_leaf_sizes_and_mutable_aliases_survive_gc_stress() {
        const POINTERS: [bool; 23] = {
            let mut p = [true; 23];
            p[0] = false;
            p[1] = false;
            p[2] = false;
            p
        };
        let mut instance = crate::Instance::new(D, &POINTERS);
        instance.set_stress(true);
        unsafe {
            instance.run_owned(|| -> R<()> {
                let retained = bigint_literal("123456789012345678901234567890")?;
                let buffer = bytes(vec![0, 255])?;
                for n in 0usize..130 {
                    let value = -(BigInt::from(1u32) << n);
                    let p = bigint(value.clone())?;
                    assert_eq!(bigint_owned(p), value);
                    with_bytes(buffer, |b| {
                        b.push(n as u8);
                    })?;
                    assert_eq!(
                        bigint_owned(retained).to_string(),
                        "123456789012345678901234567890"
                    );
                }
                assert_eq!(bytes_owned(buffer).len(), 132);
                assert_eq!(byte_index(buffer, -1)?, 129);
                byte_set(buffer, -1, 258)?;
                assert_eq!(byte_index(buffer, -1)?, 2);
                let zero = bigint(BigInt::ZERO)?;
                assert_eq!(bigint_owned(zero), BigInt::ZERO);
                let dividend = bigint_literal("99")?;
                let error = binary(
                    crate::Dyn {
                        tag: BIGINT_TAG,
                        payload: dividend as u64,
                    },
                    crate::Dyn {
                        tag: BIGINT_TAG,
                        payload: zero as u64,
                    },
                    "/",
                )
                .unwrap_err();
                assert_eq!(
                    error.display_value.as_deref(),
                    Some("baml.panics.DivisionByZero {dividend: 99}")
                );
                let stored = ((error.value + 8) as *const crate::Dyn).read();
                assert_eq!(stored.tag, BIGINT_TAG);
                assert_eq!(bigint_owned(stored.payload as usize), BigInt::from(99));
                Ok(())
            })
        }
        .unwrap();
    }
}

use crate::strings::Cx;
use baml_rt_gc::{Descriptor, Kind};
use std::rc::Rc;
pub const DESC: Descriptor = Descriptor {
    kind: Kind::Opaque,
    size: 8,
    ptr_slots: (0, 0),
    tagged_slots: (0, 0),
    elem: None,
    type_id: 18,
    fields: &[],
    finalizer: None,
};
/// # Safety
/// The payload must not contain managed pointers. Its handle's descriptor has
/// no pointer slots; the collector cannot trace Rust-owned payload fields.
pub unsafe fn new<T: std::any::Any>(cx: &mut Cx, payload: T) -> R<usize> {
    let object = cx.alloc(15, 8)? as usize;
    unsafe {
        (&mut *crate::HEAP.get()).attach_native(object, Rc::new(payload));
    }
    Ok(object)
}
/// # Safety
/// `object` is a live handle of T in the active instance.
pub unsafe fn get<T: std::any::Any>(object: usize) -> Rc<T> {
    unsafe { (&*crate::HEAP.get()).native(object) }
}
