//! Application-level coverage for native installation and configuration primitives.
use baml_tests::baml_test;
use bex_external_types::BexExternalValue;

#[tokio::test]
async fn toml_selector_edit_preserves_inline_table_and_comments() {
    let output = baml_test!(
        r##"
        function main() -> string {
            let doc = baml.toml.Table.parse("# project\nname = \"app\"\n[toolchain] # selection\nversion = \"old\" # keep this\nextra = true\n");
            let toolchain = doc.table("toolchain");
            toolchain.rename("version", "path");
            toolchain.set("path", "./cli");
            doc.to_string()
        }
    "##
    );
    let Ok(BexExternalValue::String(text)) = output.result else {
        panic!("{:?}", output.result)
    };
    assert!(text.contains("# project"));
    assert!(text.contains("[toolchain] # selection"));
    assert!(text.contains("path = \"./cli\" # keep this"));
    assert!(text.contains("extra = true"));
    assert!(!text.contains("version ="));
    let output = baml_test!(
        r##"
        function main() -> string {
            let doc = baml.toml.Table.parse("toolchain = { version = \"old\", extra = true } # inline\n");
            let toolchain = doc.table("toolchain");
            toolchain.rename("version", "channel");
            toolchain.set("channel", "canary");
            doc.to_string()
        }
    "##
    );
    let Ok(BexExternalValue::String(text)) = output.result else {
        panic!("{:?}", output.result)
    };
    assert!(text.contains("toolchain = {"));
    assert!(text.contains("# inline"));
    assert!(text.contains("extra = true"));
}

#[tokio::test]
async fn atomic_replacement_metadata_and_lock_lifecycle() {
    let tmp = tempfile::tempdir().unwrap();
    let path = serde_json::to_string(&tmp.path().join("config.toml").to_string_lossy()).unwrap();
    let lock = serde_json::to_string(&tmp.path().join("config.lock").to_string_lossy()).unwrap();
    let output = baml_test!(&format!(
        r##"
        function main() -> bool {{
            let guard = baml.fs.lock({lock}, 1000);
            let blocked = {{ baml.fs.lock({lock}, 0); false }} catch (e) {{ baml.errors.Io => true }};
            baml.fs.write_atomic({path}, b"old");
            baml.fs.write_atomic({path}, b"new");
            let meta = baml.fs.metadata({path});
            let content = baml.fs.read({path});
            guard.close();
            let next = baml.fs.lock({lock}, 0);
            next.close();
            blocked && meta.is_file && meta.modified_ms != null && content == "new"
        }}
    "##
    ));
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
    assert!(tmp.path().join("config.lock").exists());
    let entries: Vec<_> = std::fs::read_dir(tmp.path()).unwrap().collect();
    assert_eq!(entries.len(), 2, "temporary files must be removed");
}

#[tokio::test]
async fn hash_and_path_primitives() {
    let output = baml_test!(
        r##"
        function main() -> bool {
            let cwd = baml.sys.current_dir();
            baml.crypto.sha256(b"abc") == "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad" &&
                baml.path.is_absolute(cwd) && baml.path.absolute(".") == cwd &&
                baml.path.absolute("note", base = "notes") == baml.path.join([cwd, "notes", "note"]) &&
                baml.path.parent(baml.path.join([cwd, "child"])) == cwd
        }
    "##
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn archive_roundtrip_rejects_traversal_and_nonempty_destinations() {
    let tmp = tempfile::tempdir().unwrap();
    let good = "UEsDBBQAAAAIAE4AQ11y/dtsBAAAAAIAAAAHAAAAYmluL2NsaWP4DwBQSwECFAMUAAAACABOAENdcv3bbAQAAAACAAAABwAAAAAAAAAAAAAAgAEAAAAAYmluL2NsaVBLBQYAAAAAAQABADUAAAApAAAAAAA=";
    let malicious = "UEsDBBQAAAAIAC0AQ12m/eq1CQAAAAcAAAAOAAAALi4vZXNjYXBlZC50eHTLLy0pzkxJBQBQSwECFAMUAAAACAAtAENdpv3qtQkAAAAHAAAADgAAAAAAAAAAAAAAgAEAAAAALi4vZXNjYXBlZC50eHRQSwUGAAAAAAEAAQA8AAAANQAAAAAA";
    let destination = serde_json::to_string(&tmp.path().join("archive").to_string_lossy()).unwrap();
    let rejected = serde_json::to_string(&tmp.path().join("rejected").to_string_lossy()).unwrap();
    let escaped = serde_json::to_string(&tmp.path().join("escaped.txt").to_string_lossy()).unwrap();
    let output = baml_test!(&format!(
        r#"
        function main() -> bool {{
            let archive = uint8array.from_base64("{good}");
            let bytes = baml.archive.read_file(archive, "zip", "bin/cli");
            baml.archive.extract(archive, "zip", {destination});
            let overwrite_rejected = {{ baml.archive.extract(archive, "zip", {destination}); false }} catch (e) {{ baml.errors.Io => true }};
            let traversal_rejected = {{ baml.archive.extract(uint8array.from_base64("{malicious}"), "zip", {rejected}); false }} catch (e) {{ baml.errors.Io => true }};
            bytes == b"\x00\xff" && baml.fs.exists(baml.path.join([{destination}, "bin", "cli"])) &&
                overwrite_rejected && traversal_rejected && !baml.fs.exists({escaped})
        }}
    "#
    ));
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
#[cfg(unix)]
async fn absolute_paths_preserve_symlink_parent_semantics() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("real/inner")).unwrap();
    std::fs::write(tmp.path().join("real/cli"), "selected").unwrap();
    std::fs::write(tmp.path().join("cli"), "different").unwrap();
    std::os::unix::fs::symlink("real/inner", tmp.path().join("link")).unwrap();
    let path = serde_json::to_string(&tmp.path().join("link/../cli").to_string_lossy()).unwrap();
    let output = baml_test!(&format!(
        r#"
        function main() -> string {{ baml.fs.read(baml.path.absolute({path})) }}
    "#
    ));
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("selected".into()))
    );
}

#[tokio::test]
#[cfg(unix)]
async fn absolute_paths_preserve_symlink_parents_for_new_files() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("real/inner")).unwrap();
    std::os::unix::fs::symlink("real/inner", tmp.path().join("link")).unwrap();
    let base = serde_json::to_string(&tmp.path().to_string_lossy()).unwrap();
    let output = baml_test!(&format!(
        r#"
        function main() -> bool {{
            let destination = baml.path.absolute("link/../new", base = {base});
            baml.fs.write(destination, "created");
            baml.fs.read(destination) == "created"
        }}
    "#
    ));
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("real/new")).unwrap(),
        "created"
    );
    assert!(!tmp.path().join("new").exists());
}

#[tokio::test]
async fn archives_accept_runtime_format_values() {
    let tmp = tempfile::tempdir().unwrap();
    let root = serde_json::to_string(&tmp.path().to_string_lossy()).unwrap();
    let output = baml_test!(&format!(
        r#"
        function format(compressed: bool) -> "tar.gz" | "zip" {{
            if (compressed) {{ "tar.gz" }} else {{ "zip" }}
        }}
        function main() -> bool {{
            let zip = uint8array.from_base64("UEsDBBQAAAAIAE4AQ11y/dtsBAAAAAIAAAAHAAAAYmluL2NsaWP4DwBQSwECFAMUAAAACABOAENdcv3bbAQAAAACAAAABwAAAAAAAAAAAAAAgAEAAAAAYmluL2NsaVBLBQYAAAAAAQABADUAAAApAAAAAAA=");
            let tar = uint8array.from_base64("H4sIAAAAAAAC/+3NTQpAYBgE4O8obkBKzoOVkoWf8/Oykj0lz7OZaTbT9mPeDX16UhHqqjoz3DOUl37sdRlTVqQXrPPSTHGZfmpLAAAAAAAAAAAAfNAOmyzhUQAoAAA=");
            for (let compressed in [false, true]) {{
                let data = if (compressed) {{ tar }} else {{ zip }};
                let selected = format(compressed);
                let destination = baml.path.join([{root}, if (compressed) {{ "tar" }} else {{ "zip" }}]);
                if (baml.archive.read_file(data, selected, "bin/cli") != b"\x00\xff") {{ return false; }}
                baml.archive.extract(data, selected, destination);
                let file = baml.fs.open(baml.path.join([destination, "bin", "cli"]), "r");
                defer {{ file.close(); }}
                if (file.bytes() != b"\x00\xff") {{ return false; }}
            }}
            true
        }}
    "#
    ));
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}
