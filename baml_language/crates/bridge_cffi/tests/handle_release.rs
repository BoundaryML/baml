//! `baml_handle_release` is a C ABI entry point: the last release of a key
//! runs the entry's destructor, and a panic there must be reported as a
//! status rather than unwinding into the caller.
use std::sync::Arc;

use bridge_ctypes::{CffiHandleTableEntry, HANDLE_TABLE};

#[test]
fn resource_cleanup_panic_stays_inside_handle_release_abi() {
    struct PanicOnDrop;
    impl Drop for PanicOnDrop {
        fn drop(&mut self) {
            panic!("synthetic resource cleanup failure");
        }
    }
    let key = HANDLE_TABLE.insert(
        CffiHandleTableEntry::try_from(bex_project::BexExternalValue::RustData(Arc::new(
            PanicOnDrop,
        )))
        .unwrap(),
    );
    assert_eq!(
        unsafe { bridge_cffi::baml_handle_release(key) },
        bridge_cffi::BamlCffiStatus::InternalError,
    );
    assert!(HANDLE_TABLE.resolve(key).is_none());
}
