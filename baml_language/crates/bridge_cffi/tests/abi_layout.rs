use std::{
    collections::BTreeMap,
    fs,
    mem::{align_of, offset_of, size_of},
    path::{Path, PathBuf},
    process::Command,
};

use bridge_cffi::{
    BamlApiV1, BamlBridgeInfoV1, BamlCffiHandleType, BamlCffiMediaKind, BamlCffiStatus,
    BridgeLanguage, Buffer,
};

fn test_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests")
}

fn include_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("include")
}

fn scratch_dir() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "baml-cffi-abi-layout-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("create ABI test scratch directory");
    path
}

fn compiler(cpp: bool) -> Command {
    let variable = if cpp { "CXX" } else { "CC" };
    let fallback = if cfg!(windows) {
        "cl.exe"
    } else if cpp {
        "c++"
    } else {
        "cc"
    };
    Command::new(std::env::var_os(variable).unwrap_or_else(|| fallback.into()))
}

fn run_checked(command: &mut Command, context: &str) {
    let output = command.output().unwrap_or_else(|error| {
        panic!("failed to run {context}: {error}");
    });
    assert!(
        output.status.success(),
        "{context} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn compile_c_layout(source: &Path, executable: &Path) {
    let mut command = compiler(false);
    if cfg!(windows) {
        command
            .arg("/nologo")
            .arg("/std:c11")
            .arg("/W4")
            .arg("/WX")
            .arg(format!("/I{}", include_dir().display()))
            .arg(format!("/I{}", test_dir().display()))
            .arg(source)
            .arg(format!("/Fe:{}", executable.display()));
    } else {
        command
            .arg("-std=c11")
            .arg("-Wall")
            .arg("-Wextra")
            .arg("-Werror")
            .arg("-pedantic")
            .arg("-I")
            .arg(include_dir())
            .arg("-I")
            .arg(test_dir())
            .arg(source)
            .arg("-o")
            .arg(executable);
    }
    run_checked(&mut command, "strict C ABI layout compile");
}

fn compile_cpp_assertions(source: &Path, object: &Path) {
    let mut command = compiler(true);
    if cfg!(windows) {
        command
            .arg("/nologo")
            .arg("/std:c++17")
            .arg("/EHsc")
            .arg("/W4")
            .arg("/WX")
            .arg(format!("/I{}", include_dir().display()))
            .arg(format!("/I{}", test_dir().display()))
            .arg("/c")
            .arg(source)
            .arg(format!("/Fo:{}", object.display()));
    } else {
        command
            .arg("-std=c++17")
            .arg("-Wall")
            .arg("-Wextra")
            .arg("-Werror")
            .arg("-pedantic")
            .arg("-I")
            .arg(include_dir())
            .arg("-I")
            .arg(test_dir())
            .arg("-c")
            .arg(source)
            .arg("-o")
            .arg(object);
    }
    run_checked(&mut command, "strict C++ ABI assertion compile");
}

fn parse_layout(stdout: &[u8]) -> BTreeMap<String, usize> {
    String::from_utf8(stdout.to_vec())
        .expect("C layout probe output must be UTF-8")
        .lines()
        .map(|line| {
            let (key, value) = line.split_once('=').expect("layout line must contain '='");
            (
                key.to_string(),
                value.parse::<usize>().expect("layout value must be usize"),
            )
        })
        .collect()
}

#[test]
fn rust_c_and_cpp_agree_on_the_complete_v1_abi() {
    let scratch = scratch_dir();
    let executable = scratch.join(if cfg!(windows) {
        "abi_layout.exe"
    } else {
        "abi_layout"
    });
    compile_c_layout(&test_dir().join("abi_layout.c"), &executable);
    compile_cpp_assertions(
        &test_dir().join("abi_layout.cpp"),
        &scratch.join(if cfg!(windows) {
            "abi_layout_cpp.obj"
        } else {
            "abi_layout_cpp.o"
        }),
    );

    let output = Command::new(&executable)
        .output()
        .expect("run C ABI layout probe");
    assert!(output.status.success(), "C ABI layout probe must succeed");
    let actual = parse_layout(&output.stdout);

    let mut expected = BTreeMap::new();
    macro_rules! size_align {
        ($name:literal, $type:ty) => {
            expected.insert(concat!("size.", $name).to_string(), size_of::<$type>());
            expected.insert(concat!("align.", $name).to_string(), align_of::<$type>());
        };
    }
    macro_rules! field {
        ($name:literal, $type:ty, $field:ident) => {
            expected.insert(
                concat!("offset.", $name, ".", stringify!($field)).to_string(),
                offset_of!($type, $field),
            );
        };
    }

    size_align!("BamlCffiStatus", BamlCffiStatus);
    size_align!("BamlBridgeLanguage", BridgeLanguage);
    size_align!("BamlCffiMediaKind", BamlCffiMediaKind);
    size_align!("BamlCffiHandleType", BamlCffiHandleType);
    size_align!("BamlBuffer", Buffer);
    field!("BamlBuffer", Buffer, ptr);
    field!("BamlBuffer", Buffer, len);
    size_align!("BamlBridgeInfoV1", BamlBridgeInfoV1);
    field!("BamlBridgeInfoV1", BamlBridgeInfoV1, struct_size);
    field!("BamlBridgeInfoV1", BamlBridgeInfoV1, language);
    field!("BamlBridgeInfoV1", BamlBridgeInfoV1, sdk_version);
    field!("BamlBridgeInfoV1", BamlBridgeInfoV1, sdk_version_len);
    field!("BamlBridgeInfoV1", BamlBridgeInfoV1, bridge_runtime_name);
    field!(
        "BamlBridgeInfoV1",
        BamlBridgeInfoV1,
        bridge_runtime_name_len
    );
    field!("BamlBridgeInfoV1", BamlBridgeInfoV1, bridge_runtime_version);
    field!(
        "BamlBridgeInfoV1",
        BamlBridgeInfoV1,
        bridge_runtime_version_len
    );
    size_align!("BamlApiV1", BamlApiV1);
    field!("BamlApiV1", BamlApiV1, abi_version);
    field!("BamlApiV1", BamlApiV1, struct_size);
    field!("BamlApiV1", BamlApiV1, version);
    field!("BamlApiV1", BamlApiV1, initialize_runtime_from_bytecode);
    field!("BamlApiV1", BamlApiV1, free_buffer);
    field!("BamlApiV1", BamlApiV1, register_callback);
    field!("BamlApiV1", BamlApiV1, call_function);
    field!("BamlApiV1", BamlApiV1, new_function_call);
    field!("BamlApiV1", BamlApiV1, cancel_function_call);
    field!("BamlApiV1", BamlApiV1, register_host_dispatch_callback);
    field!("BamlApiV1", BamlApiV1, register_host_release_callback);
    field!("BamlApiV1", BamlApiV1, complete_host_call);
    field!("BamlApiV1", BamlApiV1, handle_clone);
    field!("BamlApiV1", BamlApiV1, handle_release);
    field!("BamlApiV1", BamlApiV1, media_from_url);
    field!("BamlApiV1", BamlApiV1, media_from_file);
    field!("BamlApiV1", BamlApiV1, media_from_base64);
    field!("BamlApiV1", BamlApiV1, media_url);
    field!("BamlApiV1", BamlApiV1, media_file);
    field!("BamlApiV1", BamlApiV1, media_base64);
    field!("BamlApiV1", BamlApiV1, media_mime_type);
    field!("BamlApiV1", BamlApiV1, register_bridge);
    field!(
        "BamlApiV1",
        BamlApiV1,
        register_unhandled_spawn_error_callback
    );
    field!("BamlApiV1", BamlApiV1, shutdown_runtime);
    field!(
        "BamlApiV1",
        BamlApiV1,
        initialize_runtime_from_bytecode_with_metadata
    );

    assert_eq!(actual, expected, "C and Rust ABI layouts differ");
    assert_eq!(BamlCffiStatus::Ok as u32, 0);
    assert_eq!(BamlCffiStatus::InvalidHandle as u32, 1);
    assert_eq!(BamlCffiStatus::TypeMismatch as u32, 2);
    assert_eq!(BamlCffiStatus::UnsupportedHandleType as u32, 3);
    assert_eq!(BamlCffiStatus::InternalError as u32, 4);
    assert_eq!(BamlCffiStatus::UnexpectedNullptr as u32, 5);
    assert_eq!(BridgeLanguage::NodeJs as u32, 1);
    assert_eq!(BridgeLanguage::Python as u32, 2);
    assert_eq!(BridgeLanguage::Go as u32, 3);
    assert_eq!(BridgeLanguage::Rust as u32, 4);
    assert_eq!(BridgeLanguage::CSharp as u32, 5);
    assert_eq!(BridgeLanguage::Java as u32, 7);
    assert_eq!(BridgeLanguage::Swift as u32, 8);
    assert_eq!(BridgeLanguage::Web as u32, 9);

    let _ = fs::remove_dir_all(scratch);
}
