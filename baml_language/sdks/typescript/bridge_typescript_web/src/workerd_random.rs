//! Workerd-only `getrandom` backend backed by its virtual `/dev/random`.

#![allow(unsafe_code)] // getrandom's custom backend contract is an unsafe exported symbol.

use getrandom_04::Error;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(inline_js = r#"
import { closeSync, openSync, readSync } from "node:fs";

export function fillFromDevRandom(bytes) {
  const fd = openSync("/dev/random", "r");
  try {
    let offset = 0;
    while (offset < bytes.length) {
      const count = readSync(fd, bytes, offset, bytes.length - offset, null);
      if (count === 0) return offset;
      offset += count;
    }
    return offset;
  } finally {
    closeSync(fd);
  }
}
"#)]
extern "C" {
    #[wasm_bindgen(catch, js_name = fillFromDevRandom)]
    fn fill_from_dev_random(bytes: &mut [u8]) -> Result<u32, JsValue>;
}

/// Supplies the symbol required by getrandom's opt-in custom backend.
///
/// Workerd exposes `/dev/random`, but only returns bytes while executing on
/// behalf of a request. A short read is therefore an entropy failure rather
/// than permission to use a partially initialized buffer.
#[unsafe(no_mangle)]
unsafe extern "Rust" fn __getrandom_v03_custom(
    destination: *mut u8,
    length: usize,
) -> Result<(), Error> {
    // SAFETY: getrandom owns a valid, possibly-uninitialized buffer of exactly
    // `length` bytes for the duration of this call. Initialize it before
    // handing a mutable slice to wasm-bindgen.
    let bytes = unsafe {
        destination.write_bytes(0, length);
        std::slice::from_raw_parts_mut(destination, length)
    };
    let count = fill_from_dev_random(bytes).map_err(|_| Error::new_custom(1))?;
    if usize::try_from(count).ok() == Some(length) {
        Ok(())
    } else {
        Err(Error::new_custom(2))
    }
}
