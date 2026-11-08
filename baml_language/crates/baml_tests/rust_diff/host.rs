//! Isolated host checks while the shared probe project awaits initializer
//! support. These do not replace or alter any shared differential fixture.
use super::*;

#[test]
#[ignore = "builds generated Rust crates with cargo"]
fn rt_host_boundary() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.parent().unwrap().parent().unwrap();
    let build = Path::new(env!("CARGO_TARGET_TMPDIR")).join("rt-host");
    std::fs::create_dir_all(&build).unwrap();
    let reader = telemetry_worker::build(&build, workspace);
    let only = std::env::var("BAML_RT_HOST_CASES").ok();
    let mut exercised = std::collections::BTreeSet::new();
    for (name, source, json) in [
        (
            "scalars",
            r#"function main(n:int,label:string,yes:bool,x:float)->string { if(yes){`${label}:${n}:${x}`}else{label} }"#,
            Some(r#"{"n":5,"label":"ok","yes":true,"x":1.25}"#),
        ),
        (
            "class",
            r#"class Reply {text:string,number:float} function main(value:Reply)->Reply { value.number+=0.5;value }"#,
            Some(r#"{"value":{"text":"copied","number":1.25,"ignored":true}}"#),
        ),
        (
            "math",
            r#"function main()->int { (4.0).sqrt().itrunc()+int.parse("3")+(7).count_ones() }"#,
            None,
        ),
        (
            "float_capture",
            r#"/// baml:$trace=trace.span
function Calc(x:float)->float { x.sqrt() }
function main()->int { Calc(4.0).itrunc() }"#,
            None,
        ),
        (
            "logs",
            r#"function main()->int { log.info("one");log.warn("two");baml.io.println("logged");7 }"#,
            None,
        ),
        (
            "nullable",
            r#"function main()->string? { baml.env.get("BAML_RT_HOST_MISSING") }"#,
            None,
        ),
        (
            "env_ready",
            r#"function main()->string? { baml.env.get("BAML_RT_HOST_VALUE") }"#,
            None,
        ),
        (
            "void",
            r#"function main()->void { baml.io.println("void") }"#,
            None,
        ),
        (
            "hook",
            r#"/// baml:$trace=trace.span
function main()->int { 7 }"#,
            None,
        ),
        (
            "float_order",
            r#"function main()->int {
    let nan=float.nan();
    if(nan==nan && nan>float.inf() && (1.0/0.0).is_infinite() && (0.0/0.0).is_nan()) { 7 } else { 9 }
}"#,
            None,
        ),
        (
            "parse_error",
            r#"function main()->int { int.parse("bad") }"#,
            None,
        ),
        (
            "overflow_error",
            "function main()->int { (-4611686018427387904).abs() }",
            None,
        ),
        (
            "invalid_math",
            "function main()->int { (-1).isqrt() }",
            None,
        ),
        (
            "caught_parse",
            r#"function main()->string {
    int.parse("bad").to_string() catch (e) {
        baml.errors.ParseError {message} => message
    }
}"#,
            None,
        ),
        (
            "fs",
            r#"function main()->int {
    let n=baml.fs.write("files/first.txt","héllo");
    let exists=baml.fs.exists("files/first.txt");
    let text=baml.fs.read("files/first.txt");
    let size=baml.fs.size("files/first.txt");
    baml.fs.rename("files/first.txt","files/second.txt");
    let absolute=baml.fs.canonicalize("files/second.txt").char_count();
    baml.fs.remove("files/second.txt");
    baml.fs.remove_dir("files");
    baml.fs.remove_dir_all("absent");
    baml.io.println(text);
    if(exists && text.char_count()==5 && n==6 && size==6 && absolute>0) { 7 } else { 9 }
}"#,
            None,
        ),
        (
            "fs_error",
            r#"function main()->string { baml.fs.read("missing") }"#,
            None,
        ),
        (
            "fs_utf8",
            r#"function main()->string { baml.fs.read("bad-utf8") }"#,
            None,
        ),
    ] {
        if only
            .as_ref()
            .is_some_and(|names| !names.split(',').any(|n| n == name))
        {
            continue;
        }
        exercised.insert(name);
        let dir = build.join(name);
        let source_dir = dir.join("source");
        std::fs::create_dir_all(source_dir.join("baml_src")).unwrap();
        std::fs::write(source_dir.join("baml.toml"),"[package]\nname=\"rt-host-test\"\n[pack.env_var_names]\nBAML_TELEMETRY=\"BAML_TELEMETRY\"\n").unwrap();
        std::fs::write(source_dir.join("baml_src/main.baml"), source).unwrap();
        let bytecode = dir.join("bytecode");
        let rt = dir.join("rt");
        pack_result(
            &build.join("vm-build"),
            workspace,
            &source_dir,
            &bytecode,
            ["main"],
            false,
        )
        .unwrap();
        pack_result(
            &build.join("rt-build"),
            workspace,
            &source_dir,
            &rt,
            ["main"],
            true,
        )
        .unwrap();
        let levels: &[&str] = if matches!(
            name,
            "math"
                | "logs"
                | "hook"
                | "float_capture"
                | "parse_error"
                | "overflow_error"
                | "invalid_math"
                | "fs"
                | "fs_error"
                | "fs_utf8"
        ) {
            &["off", "low", "medium", "high"]
        } else {
            &["off"]
        };
        for level in levels {
            for stress in ["0", "1"] {
                let run = |path: &Path, is_rt: bool| {
                    let home = tempfile::tempdir_in(&build).unwrap();
                    if name == "fs_utf8" {
                        std::fs::write(home.path().join("bad-utf8"), [0xff]).unwrap();
                    }
                    let mut cmd = Command::new(path);
                    cmd.current_dir(home.path());
                    if !is_rt {
                        cmd.arg("main");
                    }
                    if let Some(json) = json {
                        cmd.args(["--json-args", json]);
                    }
                    cmd.env("BAML_TELEMETRY", level)
                        .env("BAML_RT_HOST_VALUE", "env-ready")
                        .env_remove("BAML_RT_HOST_MISSING")
                        .env("BAML_GC_STRESS", stress)
                        .env("BAML_COMPILED_BODIES", "0")
                        .env("BAML_HOME", home.path())
                        .env("NO_COLOR", "1");
                    let output = run_probe(&mut cmd, Duration::from_secs(120)).unwrap();
                    let normalized = home.path().join("normalized.jsonl");
                    let read = run_probe(
                        Command::new(&reader).arg(home.path()).arg(&normalized),
                        Duration::from_secs(360),
                    )
                    .unwrap();
                    assert!(
                        read.status.success(),
                        "{}",
                        String::from_utf8_lossy(&read.stderr)
                    );
                    let trace = std::fs::read_to_string(normalized).unwrap();
                    if *level != "off" {
                        assert!(!trace.trim().is_empty(), "{name}/{level}: no telemetry");
                    }
                    (
                        output,
                        known_telemetry_divergences::without_gc(&trace, "rt"),
                    )
                };
                let (expected, expected_trace) = run(&bytecode, false);
                let (actual, actual_trace) = run(&rt, true);
                let success = !matches!(
                    name,
                    "parse_error" | "overflow_error" | "invalid_math" | "fs_error" | "fs_utf8"
                );
                assert_eq!(
                    expected.status.success(),
                    success,
                    "{name}: {}",
                    String::from_utf8_lossy(&expected.stderr)
                );
                assert_eq!(
                    actual.status.code(),
                    expected.status.code(),
                    "{name}/rt: {}",
                    String::from_utf8_lossy(&actual.stderr)
                );
                assert_eq!(
                    String::from_utf8_lossy(&actual.stdout),
                    String::from_utf8_lossy(&expected.stdout),
                    "{name}/stress={stress}"
                );
                assert_eq!(
                    actual.stderr, expected.stderr,
                    "{name}/stderr/stress={stress}"
                );
                assert_eq!(
                    actual_trace, expected_trace,
                    "{name}/{level}/stress={stress}: telemetry mismatch"
                );
            }
        }
    }
    if let Some(names) = only {
        for name in names.split(',') {
            assert!(exercised.contains(name), "unknown host case: {name}");
        }
    }
    assert!(!exercised.is_empty(), "no host cases selected");
}
