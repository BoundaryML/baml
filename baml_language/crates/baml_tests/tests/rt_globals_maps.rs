//! Execute emitted Rust as well as the VM: admission-only tests cannot catch a
//! generated type mismatch or a missed map/global GC edge.
use baml_compiler2_emit::OptLevel;
use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};
use sys_native::SysOpsExt;

#[path = "../rust_diff/telemetry.rs"]
mod telemetry;
#[allow(dead_code)]
#[path = "../rust_diff/known_telemetry_divergences.rs"]
mod telemetry_exceptions;

fn runtime_library(deps: &Path, name: &str) -> PathBuf {
    let mut candidates: Vec<_> = std::fs::read_dir(deps)
        .unwrap()
        .map(|p| p.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(&format!("lib{name}-"))
                && p.extension().is_some_and(|e| e == "rlib")
        })
        .collect();
    candidates.sort_by_key(|p| std::fs::metadata(p).unwrap().modified().unwrap());
    candidates
        .pop()
        .unwrap_or_else(|| panic!("build {name} before this ignored test"))
}
#[tokio::test(flavor = "multi_thread")]
#[ignore = "subprocess oracle for emitted-code telemetry comparison"]
async fn vm_recording_worker() {
    let Some(path) = std::env::var_os("BAML_RT_ORACLE_PROGRAM") else {
        return;
    };
    let program = baml_artifact::decode(
        baml_artifact::ArtifactKind::Program,
        &std::fs::read(path).unwrap(),
    )
    .unwrap();
    let result = match BexEngine::new_with_config(
        program,
        Arc::new(sys_native::SysOps::native()),
        Vec::new(),
        bex_engine::EngineConfig {
            recording: Some(
                bex_engine::TelemetryRecording::user_files(Default::default()).with_host("pack"),
            ),
            ..Default::default()
        },
    ) {
        Err(error) => Err(error.to_string()),
        Ok(engine) => {
            let engine = Arc::new(engine);
            let result = engine
                .call_function(
                    "main",
                    Vec::new(),
                    FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
                    true,
                )
                .await;
            engine.record_process_exit(if result.is_ok() {
                bex_engine::ProcessStatus::Success
            } else if engine.root_panicked() {
                bex_engine::ProcessStatus::Panicked
            } else {
                bex_engine::ProcessStatus::Error
            });
            engine.shutdown().await;
            match result {
                Err(error) => Err(error.to_string()),
                Ok(BexExternalValue::Int(value)) => Ok(value),
                Ok(other) => panic!("integer oracle result: {other:?}"),
            }
        }
    };
    if let Some(path) = std::env::var_os("BAML_RT_ORACLE_RESULT") {
        std::fs::write(path, serde_json::to_vec(&result).unwrap()).unwrap();
    }
}
#[tokio::test(flavor = "multi_thread")]
#[ignore = "executes rustc; build baml_rt first"]
async fn globals_and_maps_match_vm_with_gc_stress() {
    let sources = [
        r#"function main()->int { let m: map<string,string[]> = {"k":["a","bc"]}; let values=m.values(); values[0][1].char_count() }"#,
        r#"function main()->int { let m: map<string,int> = {"a":1,"b":2}; baml.json.stringify(m).char_count() }"#,
        r#"function main()->int { let m: map<string,map<string,int>> = {"inner":{"x":7}}; m["inner"]["x"] }"#,
        r#"function main()->int { let m: map<float,int> = {1.25:7,2.5:9}; m[1.25]+m.length() }"#,
        r#"function main()->int { let m: map<bool,int> = {true:7,false:9}; m[true]+m.length() }"#,
        "const SECOND=FIRST+1; const FIRST=41; function main()->int { SECOND }",
        r#"const M: map<string,int> = {"a":1,"b":2}; function main()->int { M["a"]=3; M.get_or_insert("c",4); M.delete("b"); let n=0; for(let v in M.values()){n=n*10+v;} n+M.length() }"#,
        r#"class Box { m: map<int,string> } function main()->int { let b=Box{m:{1:"a",2:"b"}}; b.m[1]="c"; b.m.delete(2); b.m.length() }"#,
        r#"function inspect(m:map<string,int?>)->int { if(m.has("null")){m.get("some") ?? 0}else{99} } function main()->int { let m: map<string,int?> = {"null":null,"some":7}; inspect(m) }"#,
        r#"function main()->int { let m=baml.json.decode<map<string,int>>(`{"z":"discarded","a":2,"z":3}`); let n=0; for(let v in m.values()){n=n*10+v;} n }"#,
        "const BAD=1/zero(); function zero()->int {0} function main()->int {0}",
        r#"interface Source { function value(self)->int throws never; }
class ReadsLate { implements Source { function value(self)->int throws never { LATE } } }
function read(source:Source)->int { source.value() }
const GATE=read(ReadsLate {});
const LATE=99;
function main()->int { GATE }"#,
        r#"function main()->int { let m: map<string,int> = {"a":1}; m["missing"] }"#,
        r#"function main()->int { let m=baml.json.decode<map<string,int>>(`{"a":false}`); m.length() }"#,
        r#"function main()->int { let m=baml.json.decode<map<string,int>>(`{"a":false,}`); m.length() }"#,
        r#"function main()->int { let m: map<int|string|bool|float|null,int> = {1:2,"1":3,true:5,1.0:7,null:11}; m[1]+m["1"]+m[true]+m[1.0]+m[null]+m.length() }"#,
        r#"function main()->int { let m=baml.json.decode<map<string,map<string,int>>>(`{"outer":{"z":1,"a":2,"z":3}}`); m["outer"]["z"]+m["outer"].length() }"#,
        r#"function main()->int { let m=baml.json.decode<map<string,int?>>(`{"a":null,"b":7}`); if(m.has("a")){m["b"] ?? 0}else{99} }"#,
        r#"function main()->int { let m=baml.json.decode<map<string,map<string,int>>>(`{"outer":{"bad":false}}`); m.length() }"#,
        r#"function main()->int { let m=baml.json.decode<map<string,string?>>(`{"a":null,"b":"hello"}`); (m["b"] ?? "bad").char_count()+baml.json.stringify(m).char_count() }"#,
        r#"function main()->int { let m=baml.json.decode<map<string,float>>(`{"a":1.25,"b":2}`); m.length() }"#,
        r#"function main()->int { let m=baml.json.decode<map<string,int>>(`{"a\u001f":false}`); m.length() }"#,
        r#"function main()->int { let m:map<string,int> = {"a":1,"b":2}; let alias=m; let values=m.values(); m.set("a",3); m.delete("b"); m.get_or_insert("a",9); m.clear(); alias.set("c",4); values[0]*100+values[1]*10+alias["c"]+m.length() }"#,
        r#"class Box { x: string } function inspect(m:map<string,Box|int>)->int {m.length()} function main()->int { let m:map<string,Box|int> = {"a":Box{x:"kept"},"b":3}; inspect(m) }"#,
        r#"function main()->int { let m:map<string,map<string,string?>> = {"nested":{"a":null,"empty":"", "quoted":"a,\"[]{}:b"},"empty":{}}; let text=baml.json.stringify(m,naming=baml.json.Naming.Aliased,indent=2,omit_nulls=true); let n=0; for(let c in text.to_code_points()){n=(n*31+c)%1000000007;} n }"#,
        r#"function main()->int { let m:map<string,int> = {"a":1,"b":2}; let text=baml.json.stringify(m,indent="\t"); let n=0; for(let c in text.to_code_points()){n=(n*31+c)%1000000007;} n }"#,
        r#"function render(m:map<string,int>,indent:int|string)->string {baml.json.stringify(m,indent=indent)} function main()->int {let n=0;for(let c in render({"a":1},"\t").to_code_points()){n=(n*31+c)%1000000007;} n}"#,
        r#"function main()->int { let m:map<string,float> = {"a":1.25,"b":-0.0}; let n=0;for(let c in baml.json.stringify(m,indent=2).to_code_points()){n=(n*31+c)%1000000007;} n }"#,
        r#"function main()->int {baml.json.stringify({"a":1},indent=-1).char_count()}"#,
        r#"function main()->int {baml.json.stringify({"a":1},indent="bad").char_count()}"#,
        r#"function main()->int {let m:map<int,int> = {1:2};baml.json.stringify(m).char_count()}"#,
        r#"function main()->int {let m=baml.json.decode<map<string,int?>>(`{"a":null,"b":7}`);let n=0;for(let c in baml.json.stringify(m).to_code_points()){n=(n*31+c)%1000000007;}n}"#,
        r#"function main()->int {let m:map<string,int|string|bool|float|null> = {"a":1,"b":"hello","c":true,"d":1.25,"e":null};let n=0;for(let c in baml.json.stringify(m).to_code_points()){n=(n*31+c)%1000000007;}n}"#,
        r#"function main()->int {let m:map<string,int[]> = {"a":[1,2,3],"b":[]};let n=0;for(let c in baml.json.stringify(m,indent=2).to_code_points()){n=(n*31+c)%1000000007;}n}"#,
        r#"function main()->int {let m:map<string,string[][]> = {"a":[["one","two"],[]],"b":[]};let n=0;for(let c in baml.json.stringify(m,indent=2).to_code_points()){n=(n*31+c)%1000000007;}n}"#,
    ];
    let selected = std::env::var("BAML_RT_GLOBALS_MAPS_CASES")
        .ok()
        .map(|value| {
            let cases: Vec<usize> = value
                .split(',')
                .map(|part| part.parse().expect("case index"))
                .collect();
            assert!(
                !cases.is_empty() && cases.iter().all(|&index| index < sources.len()),
                "known, nonempty case selection"
            );
            cases
        });
    let deps = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let rt = runtime_library(&deps, "baml_rt");
    let gc = runtime_library(&deps, "baml_rt_gc");
    let temp = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    // Cargo can replace the test executable while another test invocation is
    // running. Keep this oracle's executable and protocol stable for the run.
    let worker = temp.path().join("oracle-worker");
    let executable = if cfg!(target_os = "linux") {
        PathBuf::from("/proc/self/exe")
    } else {
        std::env::current_exe().unwrap()
    };
    std::fs::copy(executable, &worker).unwrap();
    for (i, source) in sources.iter().enumerate() {
        if selected.as_ref().is_some_and(|cases| !cases.contains(&i)) {
            continue;
        }
        eprintln!("globals/maps case {i}");
        let db = baml_test_support::setup_multi_file_db(&[("globals_maps.baml", *source)]);
        let program = baml_db::CheckedWorld::collect(&db, db.workspace_root().unwrap())
            .unwrap()
            .compile(OptLevel::Two)
            .unwrap();
        let entry = program.rendered_callables()["user.main"].object.raw();
        let mut emitted = baml_db::rust_backend::compile_rt(
            &db,
            db.workspace_root().unwrap(),
            &program,
            OptLevel::Two,
            entry,
        )
        .unwrap_or_else(|e| panic!("case {i}: {e}"));
        let compare_telemetry = std::env::var_os("BAML_RT_COMPARE_TELEMETRY").is_some();
        if compare_telemetry {
            // The in-process VM oracle returns the integer to its caller; it
            // does not call the CLI's ToString formatter after the invocation.
            let marker = ",Some(";
            let start = emitted.rfind(marker).expect("driver format site");
            let end = emitted[start..].find(')').unwrap() + start + 1;
            emitted.replace_range(start..end, ",None");
        }
        let program_path = temp.path().join(format!("case{i}.bexa"));
        std::fs::write(
            &program_path,
            baml_artifact::encode(baml_artifact::ArtifactKind::Program, &program).unwrap(),
        )
        .unwrap();
        let src = temp.path().join(format!("case{i}.rs"));
        let bin = temp.path().join(format!("case{i}"));
        std::fs::write(&src, emitted).unwrap();
        let build = Command::new("rustc")
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .args(["--edition=2024", "-C", "opt-level=2"])
            .arg(&src)
            .arg("--extern")
            .arg(format!("baml_rt={}", rt.display()))
            .arg("--extern")
            .arg(format!("baml_rt_gc={}", gc.display()))
            .arg("-L")
            .arg(format!("dependency={}", deps.display()))
            .arg("-o")
            .arg(&bin)
            .output()
            .unwrap();
        assert!(
            build.status.success(),
            "case {i}: {}",
            String::from_utf8_lossy(&build.stderr)
        );
        let oracle_home = tempfile::tempdir_in(temp.path()).unwrap();
        let result_path = oracle_home.path().join("result.json");
        let oracle = Command::new(&worker)
            .args(["--ignored", "--exact", "vm_recording_worker"])
            .env("BAML_RT_ORACLE_PROGRAM", &program_path)
            .env("BAML_RT_ORACLE_RESULT", &result_path)
            .env("BAML_HOME", oracle_home.path())
            .env("BAML_TELEMETRY", "off")
            .output()
            .unwrap();
        assert!(
            oracle.status.success(),
            "VM worker: {}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        let expected: Result<i64, String> =
            serde_json::from_slice(&std::fs::read(result_path).unwrap()).unwrap();
        for level in if compare_telemetry {
            &["low", "medium", "high", "default"][..]
        } else {
            &["off"][..]
        } {
            for stress in ["0", "1"] {
                let home = tempfile::tempdir_in(temp.path()).unwrap();
                let mut command = Command::new(&bin);
                command
                    .env("BAML_HOME", home.path())
                    .env("BAML_TELEMETRY", level)
                    .env("BAML_GC_STRESS", stress);
                if *level == "default" {
                    command.env_remove("BAML_TELEMETRY");
                }
                let output = command.output().unwrap();
                if compare_telemetry {
                    let oracle_home = tempfile::tempdir_in(temp.path()).unwrap();
                    let mut oracle = Command::new(&worker);
                    oracle
                        .args(["--ignored", "--exact", "vm_recording_worker"])
                        .env("BAML_RT_ORACLE_PROGRAM", &program_path)
                        .env("BAML_HOME", oracle_home.path())
                        .env("BAML_TELEMETRY", level)
                        .env("BAML_GC_STRESS", "0");
                    if *level == "default" {
                        oracle.env_remove("BAML_TELEMETRY");
                    }
                    let oracle = oracle.output().unwrap();
                    assert!(
                        oracle.status.success(),
                        "VM worker: {}",
                        String::from_utf8_lossy(&oracle.stderr)
                    );
                    let actual =
                        telemetry_exceptions::without_gc(&telemetry::read(home.path()), "rt");
                    let reference = telemetry_exceptions::without_gc(
                        &telemetry::read(oracle_home.path()),
                        "rt",
                    );
                    assert!(!reference.is_empty(), "oracle recorded telemetry");
                    // Persist mismatches inside the test scratch directory while
                    // running, so the first differing record remains inspectable.
                    if actual != reference {
                        let target = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
                        std::fs::write(target.join("rt-map-recording.txt"), &actual).unwrap();
                        std::fs::write(target.join("vm-map-recording.txt"), &reference).unwrap();
                    }
                    assert_eq!(actual, reference, "case {i}, {level}, stress {stress}");
                }
                if let Err(expected) = &expected {
                    assert!(
                        !output.status.success(),
                        "case {i}, stress {stress}: execution must fail"
                    );
                    assert_eq!(
                        String::from_utf8_lossy(&output.stderr).trim(),
                        format!("error: {expected}"),
                        "case {i}, stress {stress}"
                    );
                    continue;
                }
                assert!(
                    output.status.success(),
                    "case {i}, stress {stress}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                assert_eq!(
                    String::from_utf8_lossy(&output.stdout).trim(),
                    expected.as_ref().unwrap().to_string(),
                    "case {i}, stress {stress}"
                );
            }
        }
    }
}
