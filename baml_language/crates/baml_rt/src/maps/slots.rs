//! Storage ABI for map entries, independent of list implementation.
use crate::{
    Dyn, R,
    strings::{Cx, Str},
};
use baml_rt_gc::write_ptr;

/// # Safety
/// Layout constants must describe every managed pointer in Self. Zeroed storage
/// must be traceable, and stores must use the pointer write barrier.
pub unsafe trait Slot: Copy {
    const POINTER: bool = false;
    const TAGGED: bool = false;
    unsafe fn store(_owner: *mut u8, target: *mut Self, value: Self) {
        unsafe { target.write(value) };
    }
}
unsafe impl Slot for i64 {}
unsafe impl Slot for f64 {}
unsafe impl Slot for bool {}
unsafe impl Slot for Dyn {
    const TAGGED: bool = true;
    unsafe fn store(owner: *mut u8, target: *mut Self, value: Self) {
        unsafe { crate::write_dyn(owner, target, value) };
    }
}
unsafe impl Slot for Str {
    const POINTER: bool = true;
    unsafe fn store(owner: *mut u8, target: *mut Self, value: Self) {
        unsafe { write_ptr(owner, target.cast(), value.as_raw()) };
    }
}
/// A statically tagged pointer slot (classes, containers, closures, futures).
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct Pointer<const TAG: u64> {
    pointer: usize,
    owner: std::marker::PhantomData<std::rc::Rc<()>>,
}
impl<const TAG: u64> Pointer<TAG> {
    /// # Safety
    /// The pointer must be null or a live value of TAG in the active instance;
    /// handles and copies obey that instance's lifetime and rooting contract.
    pub unsafe fn from_raw(pointer: usize) -> Self {
        Self {
            pointer,
            owner: std::marker::PhantomData,
        }
    }
    pub fn as_raw(self) -> usize {
        self.pointer
    }
}
unsafe impl<const TAG: u64> Slot for Pointer<TAG> {
    const POINTER: bool = true;
    unsafe fn store(owner: *mut u8, target: *mut Self, value: Self) {
        unsafe { write_ptr(owner, target.cast(), value.pointer) };
    }
}
/// Built-in keys propagate rope-flattening failures. User-defined keys use the
/// callback-taking map operations so BAML hash/equality can allocate and yield.
pub trait Key: Slot {
    fn hash(self, cx: &Cx) -> R<u64>;
    fn equals(self, other: Self, cx: &Cx) -> R<bool>;
}
fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for &byte in bytes {
        hash = (hash ^ byte as u64).wrapping_mul(0x100000001b3);
    }
    hash
}
impl Key for Str {
    fn hash(self, cx: &Cx) -> R<u64> {
        Ok(hash_bytes(self.try_as_str(cx)?.as_bytes()))
    }
    fn equals(self, other: Self, cx: &Cx) -> R<bool> {
        Ok(self.try_as_str(cx)? == other.try_as_str(cx)?)
    }
}
impl Key for i64 {
    fn hash(self, _: &Cx) -> R<u64> {
        Ok(hash_bytes(&self.to_ne_bytes()))
    }
    fn equals(self, other: Self, _: &Cx) -> R<bool> {
        Ok(self == other)
    }
}
impl Key for bool {
    fn hash(self, _: &Cx) -> R<u64> {
        Ok(self as u64)
    }
    fn equals(self, other: Self, _: &Cx) -> R<bool> {
        Ok(self == other)
    }
}
impl Key for f64 {
    fn hash(self, _: &Cx) -> R<u64> {
        Ok({
            hash_bytes(
                &(if self.is_nan() {
                    f64::NAN
                } else if self == 0.0 {
                    0.0
                } else {
                    self
                })
                .to_bits()
                .to_ne_bytes(),
            )
        })
    }
    fn equals(self, other: Self, _: &Cx) -> R<bool> {
        Ok(bex_vm_types::float_order::eq(self, other))
    }
}

#[derive(Clone, Copy)]
pub struct Null;
unsafe impl Slot for Null {}
impl Key for Null {
    fn hash(self, _: &Cx) -> R<u64> {
        Ok(0)
    }
    fn equals(self, _: Self, _: &Cx) -> R<bool> {
        Ok(true)
    }
}

/// Scalar unions keep their discriminants in the entry. In particular, integer
/// one, float one and boolean true are distinct keys, as in the VM.
impl Key for Dyn {
    fn hash(self, cx: &Cx) -> R<u64> {
        Ok({
            let hash = match self.tag {
                0 => (self.payload as i64).hash(cx)?,
                1 => (self.payload != 0).hash(cx)?,
                2 => Null.hash(cx)?,
                3 => unsafe { Str::from_raw(self.payload as usize) }.hash(cx)?,
                crate::FLOAT_TAG => f64::from_bits(self.payload).hash(cx)?,
                _ => panic!("non-scalar map key requires BAML hash/equality callbacks"),
            };
            hash.rotate_left(7) ^ self.tag
        })
    }
    fn equals(self, other: Self, cx: &Cx) -> R<bool> {
        Ok({
            if self.tag != other.tag {
                return Ok(false);
            }
            match self.tag {
                0 | 1 | 2 => self.payload == other.payload,
                3 => unsafe { Str::from_raw(self.payload as usize) }
                    .equals(unsafe { Str::from_raw(other.payload as usize) }, cx)?,
                crate::FLOAT_TAG => {
                    f64::from_bits(self.payload).equals(f64::from_bits(other.payload), cx)?
                }
                _ => panic!("non-scalar map key requires BAML hash/equality callbacks"),
            }
        })
    }
}
