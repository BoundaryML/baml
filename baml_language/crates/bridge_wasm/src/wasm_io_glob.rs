//! WASM `baml.glob` namespace implementation via `WasmVfs` JS callbacks.
//!
//! `WasmIoGlob` implements `IoNamespaceGlob` and `IoClassGlobGlob` for WASM:
//!
//! - `new(pattern)` — stores the raw pattern string in the glob handle (no
//!   Regex compilation here to avoid linking the `regex` crate into WASM;
//!   the JS host handles matching via `readMany`).
//! - `Glob.scan(root)` — delegates to `WasmVfs.readMany(pattern)` which
//!   returns `[path, Uint8Array][]`; we extract just the paths.
//! - `Glob.matches(path)` — performs pure-Rust glob matching using the same
//!   `glob_to_regex` logic compiled for wasm32 (the `regex` crate works on
//!   wasm32-unknown-unknown).

use std::sync::Arc;

use sys_ops::io::{self, BexExternalValue, CallId, OpErrorKind, SysOpContext, SysOpOutput, owned};
use sys_types::BexHeap;
use wasm_bindgen::JsCast;

use crate::{send_wrapper::SendWrapper, wasm_fs::WasmVfs};

/// WASM implementation of `baml.glob` namespace + `Glob` class.
pub(crate) struct WasmIoGlob {
    vfs: SendWrapper<Arc<WasmVfs>>,
}

impl WasmIoGlob {
    /// Create a new `WasmIoGlob` from a shared VFS reference.
    pub(crate) fn new(vfs: Arc<WasmVfs>) -> Self {
        Self {
            vfs: SendWrapper::new(vfs),
        }
    }
}

// ============================================================================
// IoNamespaceGlob — `baml.glob.new(pattern)` creates a Glob handle.
// ============================================================================

impl io::IoNamespaceGlob for WasmIoGlob {
    fn new(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        pattern: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::glob::Glob> {
        // Store the raw pattern string as the handle. This avoids the full
        // glob-to-regex compilation at construction time; scan() delegates to
        // the JS host, and matches() compiles inline at call time.
        let handle: Arc<dyn std::any::Any + Send + Sync> = Arc::new(pattern);
        SysOpOutput::ok(owned::glob::Glob { _handle: handle })
    }
}

// ============================================================================
// IoClassGlobGlob — `Glob.scan` and `Glob.matches` method implementations.
// ============================================================================

impl io::IoClassGlobGlob for WasmIoGlob {
    fn scan(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        glob: owned::glob::Glob,
        _root: BexExternalValue,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<String>> {
        // Downcast the handle to get the raw pattern string.
        let pattern = match glob._handle.clone().downcast::<String>() {
            Ok(p) => (*p).clone(),
            Err(_) => {
                return SysOpOutput::err(OpErrorKind::Other(
                    "Invalid glob handle: expected String pattern".into(),
                ));
            }
        };
        let (re, negated) = match glob_to_regex(&pattern) {
            Ok(glob) => glob,
            Err(e) => return SysOpOutput::err(OpErrorKind::Other(e)),
        };

        // Delegate to WasmVfs.readMany which accepts a glob string and returns
        // `[string, Uint8Array][]`. Filter with the same Rust matcher used by
        // matches() so the host and native implementations agree on patterns.
        match self.vfs.vfs_read_many(&pattern) {
            Ok(arr) => {
                let paths: Vec<String> = arr
                    .iter()
                    .filter_map(|item| {
                        // Each item is [path: string, data: Uint8Array]
                        let pair: js_sys::Array = item.dyn_into().ok()?;
                        pair.get(0).as_string()
                    })
                    .filter(|path| glob_regex_matches(&re, negated, path))
                    .collect();
                SysOpOutput::ok(paths)
            }
            Err(e) => {
                let msg = e.as_string().unwrap_or_else(|| format!("{e:?}"));
                SysOpOutput::err(OpErrorKind::Other(msg))
            }
        }
    }

    fn matches(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        glob: owned::glob::Glob,
        path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<bool> {
        let pattern = match glob._handle.clone().downcast::<String>() {
            Ok(p) => (*p).clone(),
            Err(_) => {
                return SysOpOutput::err(OpErrorKind::Other(
                    "Invalid glob handle: expected String pattern".into(),
                ));
            }
        };

        // Compile the glob to a regex at call time. The `regex` crate is
        // available on wasm32-unknown-unknown so this works in the browser.
        match glob_to_regex(&pattern) {
            Ok((re, negated)) => SysOpOutput::ok(glob_regex_matches(&re, negated, &path)),
            Err(e) => SysOpOutput::err(OpErrorKind::Other(e)),
        }
    }
}

// ============================================================================
// Inline glob-to-regex converter for WASM.
//
// Duplicated from `sys_native::glob_utils` to avoid a cross-target dependency
// on `sys_native` (which has `wasm_support = false` in its CI config and uses
// tokio + walkdir). For a future cleanup, move `glob_utils` to a shared crate
// that both `sys_native` and `bridge_wasm` can depend on.
// ============================================================================

fn glob_regex_matches(re: &regex::Regex, negated: bool, path: &str) -> bool {
    let matched = re.is_match(path);
    if negated { !matched } else { matched }
}

fn glob_to_regex(glob: &str) -> Result<(regex::Regex, bool), String> {
    let (negated, glob) = if let Some(rest) = glob.strip_prefix('!') {
        (true, rest)
    } else {
        (false, glob)
    };

    let mut re = String::from("^");
    let bytes = glob.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if i + 1 < bytes.len() => {
                let ch = bytes[i + 1] as char;
                re.push_str(&regex::escape(&ch.to_string()));
                i += 2;
            }
            b'*' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                re.push_str(".*");
                i += 2;
                // Consume trailing slash after `**`
                if i < bytes.len() && bytes[i] == b'/' {
                    i += 1;
                }
            }
            b'*' => {
                re.push_str("[^/]*");
                i += 1;
            }
            b'?' => {
                re.push_str("[^/]");
                i += 1;
            }
            b'[' => {
                let start = i;
                i += 1;
                let mut class = String::from("[");
                if i < bytes.len() && (bytes[i] == b'^' || bytes[i] == b'!') {
                    class.push('^');
                    i += 1;
                }
                if i < bytes.len() && bytes[i] == b']' {
                    class.push(']');
                    i += 1;
                }
                while i < bytes.len() && bytes[i] != b']' {
                    class.push(bytes[i] as char);
                    i += 1;
                }
                if i < bytes.len() {
                    class.push(']');
                    i += 1;
                    re.push_str(&class);
                } else {
                    re.push_str(&regex::escape(
                        String::from_utf8_lossy(&bytes[start..]).as_ref(),
                    ));
                }
            }
            b'{' => {
                if let Some((alts, next)) = parse_brace_alternation(bytes, i + 1) {
                    re.push_str(&alts);
                    i = next;
                } else {
                    re.push_str("\\{");
                    i += 1;
                }
            }
            ch => {
                let c = ch as char;
                push_regex_literal(&mut re, c);
                i += 1;
            }
        }
    }
    re.push('$');

    let regex =
        regex::Regex::new(&re).map_err(|e| format!("Invalid glob pattern '{glob}': {e}"))?;
    Ok((regex, negated))
}

fn parse_brace_alternation(bytes: &[u8], mut i: usize) -> Option<(String, usize)> {
    let mut alts = String::from("(?:");
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if i + 1 < bytes.len() => {
                push_regex_literal(&mut alts, bytes[i + 1] as char);
                i += 2;
            }
            b'{' => {
                let (nested, next) = parse_brace_alternation(bytes, i + 1)?;
                alts.push_str(&nested);
                i = next;
            }
            b'}' => {
                alts.push(')');
                return Some((alts, i + 1));
            }
            b',' => {
                alts.push('|');
                i += 1;
            }
            ch => {
                push_regex_literal(&mut alts, ch as char);
                i += 1;
            }
        }
    }
    None
}

fn push_regex_literal(re: &mut String, c: char) {
    if ".+^${}()|[]\\".contains(c) {
        re.push('\\');
    }
    re.push(c);
}
