//! The erased `__union_*` operator drivers over primitive operands. Each
//! case is the operation the operator syntax lowers to for those types.
use crate::{Dyn, FLOAT_TAG, R, strings::Cx};

fn float(value: Dyn) -> Option<f64> {
    match value.tag {
        0 => Some(value.payload as i64 as f64),
        FLOAT_TAG => Some(f64::from_bits(value.payload)),
        _ => None,
    }
}

/// `left op right` (or `-left` for `"neg"`) selected by runtime operand types.
pub fn erased(left: Dyn, right: Dyn, op: &str, bigint: Option<u64>) -> R<Dyn> {
    let int = |v: i64| Dyn {
        tag: 0,
        payload: v as u64,
    };
    let is_bigint = |v: Dyn| bigint.is_some_and(|tag| v.tag == tag && v.payload != 0);
    if op == "neg" {
        return Ok(match left.tag {
            0 => int(crate::sub(0, left.payload as i64)?),
            FLOAT_TAG => Dyn {
                tag: FLOAT_TAG,
                payload: (-f64::from_bits(left.payload)).to_bits(),
            },
            _ if is_bigint(left) => Dyn {
                tag: left.tag,
                payload: crate::native::bigint(-unsafe {
                    crate::native::bigint_owned(left.payload as usize)
                })? as u64,
            },
            _ => return Err(unresolved(op)),
        });
    }
    if is_bigint(left) || is_bigint(right) {
        if !(left.tag == 0 || is_bigint(left)) || !(right.tag == 0 || is_bigint(right)) {
            return Err(unresolved(op));
        }
        return Ok(Dyn {
            tag: bigint.unwrap(),
            payload: crate::native::binary(left, right, op)? as u64,
        });
    }
    if left.tag == 0 && right.tag == 0 {
        let (l, r) = (left.payload as i64, right.payload as i64);
        return Ok(int(match op {
            "+" => crate::add(l, r)?,
            "-" => crate::sub(l, r)?,
            "*" => crate::mul(l, r)?,
            "/" => crate::div(l, r)?,
            "%" => crate::rem(l, r)?,
            _ => return Err(unresolved(op)),
        }));
    }
    if let (Some(l), Some(r)) = (float(left), float(right)) {
        let value = match op {
            "+" => l + r,
            "-" => l - r,
            "*" => l * r,
            "/" => l / r,
            "%" => l % r,
            _ => return Err(unresolved(op)),
        };
        return Ok(Dyn {
            tag: FLOAT_TAG,
            payload: value.to_bits(),
        });
    }
    if op == "+" && left.tag == 3 && right.tag == 3 && left.payload != 0 && right.payload != 0 {
        let joined = unsafe {
            let mut cx = Cx::current();
            crate::strings::concat(
                &mut cx,
                &[
                    crate::strings::Str::from_raw(left.payload as usize),
                    crate::strings::Str::from_raw(right.payload as usize),
                ],
            )
        }?;
        return Ok(Dyn {
            tag: 3,
            payload: joined.as_raw() as u64,
        });
    }
    Err(unresolved(op))
}

fn unresolved(op: &str) -> Box<crate::Error> {
    crate::Error::internal(format!("unresolved erased operator `{op}`"))
}
