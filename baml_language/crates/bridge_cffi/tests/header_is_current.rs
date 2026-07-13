//! Guards the committed C header (include/baml_cffi.h) against drift from the
//! exported FFI surface. The header is the frozen ABI contract shipped in the
//! baml-cpp release tarball; changes to it must show up in review as a diff.

use std::path::Path;

#[test]
fn committed_header_is_current() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let header_path = crate_dir.join("include/baml_cffi.h");

    let mut generated = Vec::new();
    cbindgen::generate(crate_dir)
        .expect("cbindgen failed to generate bindings from bridge_cffi sources")
        .write(&mut generated);
    let generated = String::from_utf8(generated).expect("cbindgen produced non-UTF-8 output");

    if std::env::var_os("BLESS").is_some() {
        std::fs::create_dir_all(header_path.parent().unwrap()).expect("failed to create include/");
        std::fs::write(&header_path, &generated).expect("failed to write include/baml_cffi.h");
        return;
    }

    let committed = std::fs::read_to_string(&header_path)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    assert_eq!(
        committed,
        generated.replace("\r\n", "\n"),
        "include/baml_cffi.h is stale; regenerate with \
         `BLESS=1 cargo test -p bridge_cffi --test header_is_current`"
    );
}
