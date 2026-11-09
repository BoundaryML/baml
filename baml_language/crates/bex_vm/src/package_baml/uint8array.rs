use bex_vm_types::Value;

use super::{BamlClassUint8Array, PackageBamlImpl};
use crate::errors::{VmBamlError, VmRustFnError};

impl BamlClassUint8Array for PackageBamlImpl {
    fn length(uint8array: &[u8]) -> i64 {
        bex_vm_types::bytes::length(uint8array)
    }
    fn at(uint8array: &[u8], index: i64) -> Option<i64> {
        bex_vm_types::bytes::at(uint8array, index)
    }
    fn push(uint8array: &mut Vec<u8>, item: i64) -> i64 {
        bex_vm_types::bytes::push(uint8array, item)
    }
    fn pop(uint8array: &mut Vec<u8>) -> Option<i64> {
        bex_vm_types::bytes::pop(uint8array)
    }
    fn concat(uint8array: &[u8], other: &[u8]) -> Vec<u8> {
        bex_vm_types::bytes::concat(uint8array, other)
    }
    fn includes(uint8array: &[u8], item: i64) -> bool {
        bex_vm_types::bytes::includes(uint8array, item)
    }
    fn index_of(uint8array: &[u8], item: i64) -> Option<i64> {
        bex_vm_types::bytes::index_of(uint8array, item)
    }
    fn reverse(uint8array: &[u8]) -> Vec<u8> {
        bex_vm_types::bytes::reverse(uint8array)
    }
    fn slice(uint8array: &[u8], start: i64, end: i64) -> Vec<u8> {
        bex_vm_types::bytes::slice(uint8array, start, end)
    }
    fn zeroes(size: i64) -> Result<Vec<u8>, VmRustFnError> {
        bex_vm_types::bytes::zeroes(size)
    }
    fn from_array(array: &[Value]) -> Result<Vec<u8>, VmRustFnError> {
        let integers = array
            .iter()
            .enumerate()
            .map(|(i, v)| {
                v.as_int().ok_or_else(|| {
                    VmRustFnError::from(VmBamlError::InvalidArgument {
                        message: format!("Element at index {i} is not an `int`"),
                    })
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        bex_vm_types::bytes::from_array(&integers)
    }
    fn to_array(uint8array: &[u8]) -> Vec<Value> {
        bex_vm_types::bytes::to_array(uint8array)
            .into_iter()
            .map(Value::int)
            .collect()
    }
    fn from_hex(hex: &bex_str::BexStr) -> Result<Vec<u8>, VmRustFnError> {
        bex_vm_types::bytes::from_hex(hex)
    }
    fn to_hex(uint8array: &[u8]) -> bex_str::BexStr {
        bex_vm_types::bytes::to_hex(uint8array)
    }
    fn from_base64(base64_str: &bex_str::BexStr) -> Result<Vec<u8>, VmRustFnError> {
        bex_vm_types::bytes::from_base64(base64_str)
    }
    fn to_base64(uint8array: &[u8]) -> bex_str::BexStr {
        bex_vm_types::bytes::to_base64(uint8array)
    }
    fn _to_string_impl(uint8array: &[u8]) -> bex_str::BexStr {
        bex_vm_types::bytes::_to_string_impl(uint8array)
    }
    fn sort(uint8array: &mut Vec<u8>) -> () {
        bex_vm_types::bytes::sort(uint8array)
    }
}
