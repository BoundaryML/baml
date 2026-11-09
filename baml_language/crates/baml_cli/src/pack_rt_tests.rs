//! Per-leaf RT admission. VM discovery is pack-time only; packed tests contain no VM.
#![allow(unsafe_code, reason = "heap inspection holds an active GC permit")]
use anyhow::{Result, anyhow};
use baml_exec::test_report::{self, TestSelection};
use bex_engine::{
    BexEngine, BexExternalValue, CallId, CancellationToken, FunctionCallContextBuilder,
};
use bex_heap::HeapPermit;
use bex_vm_types::{HeapPtr, Object, Program, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    sync::Arc,
};

pub(super) fn compile(
    db: &baml_db::ProjectDatabase,
    root: baml_db::SourceRoot,
    program: &Program,
    engine: Arc<BexEngine>,
    selection: TestSelection,
) -> Result<String> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let discovered = rt.block_on(async {
        let registry = engine
            .collect_tests(CallId::next(), CancellationToken::new())
            .await?;
        if matches!(registry, BexExternalValue::Null) {
            return Err(anyhow!("no tests selected"));
        }
        let names = test_report::list_filtered(
            &engine,
            &registry,
            &selection,
            FunctionCallContextBuilder::new(CallId::next()).build(),
        )
        .await?;
        let BexExternalValue::Handle(handle) = &registry else {
            return Err(anyhow!("test registry is not a handle"));
        };
        let permit = engine
            .heap_permit_manager()
            .new_permit(())
            .await
            .acquire()
            .await;
        let ptr = engine
            .resolve_handle(permit.proof(), handle)
            .ok_or_else(|| anyhow!("stale test registry"))?;
        let mut leaves = BTreeMap::new();
        let mut unexpanded = BTreeSet::new();
        let package = &program.packages[program.root as usize];
        let functions: std::collections::HashMap<_, _> = program
            .objects
            .iter()
            .enumerate()
            .filter_map(|(i, object)| match object {
                Object::Function(f) if package.objects.contains(&(i as u32)) => {
                    Some((f.identity.clone(), i))
                }
                _ => None,
            })
            .collect();
        // SAFETY: the active permit prevents moving GC for the entire traversal.
        unsafe {
            collect(
                &engine,
                ptr,
                false,
                &functions,
                &mut leaves,
                &mut unexpanded,
            )?;
        }
        Ok::<_, anyhow::Error>((names, leaves, unexpanded))
    });
    rt.block_on(engine.shutdown());
    let (names, mut leaves, unexpanded) = discovered?;
    if baml_env::os_var("BAML_RT_CORPUS_ADMISSION_CACHE").is_some() {
        let selected: BTreeSet<_> = names.iter().collect();
        for (name, kind) in leaves
            .keys()
            .filter(|name| !selected.contains(name))
            .map(|name| (name, "test"))
            .chain(unexpanded.iter().map(|name| (name, "subtree")))
        {
            eprintln!(
                "BAML_CORPUS_PROFILE_EXCLUDED\t{}",
                serde_json::json!({"name":name,"kind":kind})
            );
        }
    }
    let mut source = String::new();
    let mut rows = Vec::new();
    let mut dispatch = String::new();
    let mut entries: BTreeMap<usize, Result<usize, String>> = BTreeMap::new();
    let admission_cache = baml_env::os_var("BAML_RT_CORPUS_ADMISSION_CACHE")
        .map(|path| {
            let (_, digest) =
                baml_artifact::encode_with_digest(baml_artifact::ArtifactKind::Program, program)
                    .map_err(|e| anyhow!("test admission fingerprint: {e}"))?;
            let key: String = digest.iter().map(|b| format!("{b:02x}")).collect();
            let path = std::path::PathBuf::from(path).join(key);
            std::fs::create_dir_all(&path)?;
            Ok::<_, anyhow::Error>(path)
        })
        .transpose()?;
    let compiler = baml_db::rust_backend::RtTestCompiler::new(
        db,
        root,
        program,
        crate::bytecode_cache::CLI_OPT_LEVEL,
    );
    for name in &names {
        let admission = leaves
            .remove(name)
            .unwrap_or_else(|| Err("unsupported in baml_rt: testset expansion failed".into()))
            .and_then(|entry| {
                if let Some(result) = entries.get(&entry) {
                    return result.clone();
                }
                let cache_file = admission_cache.as_ref().map(|p| p.join(format!("{entry}.json")));
                let admitted: Result<String, String> = if let Some(path) = &cache_file
                    && path.exists()
                {
                    let data = std::fs::read(path).expect("read admission cache");
                    serde_json::from_slice(&data).expect("decode admission cache")
                } else {
                    if let Some(path) = &cache_file {
                        eprintln!("BAML_CORPUS_ADMISSION\t{}", serde_json::json!({"phase":"start","name":name,"entry":entry,"cache_file":path}));
                    }
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        compiler.compile(entry)
                    }))
                    .unwrap_or_else(|panic| {
                        let message = panic
                            .downcast_ref::<String>()
                            .map(String::as_str)
                            .or_else(|| panic.downcast_ref::<&str>().copied())
                            .unwrap_or("non-string panic");
                        Err(format!("RT compiler panic: {message}"))
                    });
                    if let Some(path) = &cache_file {
                        let data = serde_json::to_vec(&result).expect("encode admission cache");
                        std::fs::write(path, data).expect("write admission cache");
                        eprintln!("BAML_CORPUS_ADMISSION\t{}", serde_json::json!({"phase":"done","name":name,"entry":entry}));
                    }
                    result
                };
                let result = admitted.map(|body| {
                    writeln!(source, "mod test_{entry} {{\n{body}\n}}").unwrap();
                    entry
                });
                entries.insert(entry, result.clone());
                result
            });
        match admission {
            Ok(entry) => {
                writeln!(dispatch, "{name:?} => test_{entry}::run({name:?}),")?;
                rows.push(serde_json::json!({"name":name,"entry":entry,"admission":null}));
            }
            Err(error) if error.starts_with("unsupported in baml_rt:") => {
                rows.push(serde_json::json!({"name":name,"admission":error}));
                let result =
                    serde_json::json!({"name":name,"outcome":"unsupported","message":error})
                        .to_string();
                writeln!(
                    dispatch,
                    "{name:?} => {{println!(\"BAML_CORPUS_RESULT\\t{{}}\", {result:?});std::process::ExitCode::from(3)}},"
                )?;
            }
            Err(error) => {
                rows.push(serde_json::json!({"name":name,"admission":error}));
                let result =
                    serde_json::json!({"name":name,"outcome":"crash","message":error}).to_string();
                writeln!(
                    dispatch,
                    "{name:?} => {{println!(\"BAML_CORPUS_RESULT\\t{{}}\", {result:?});std::process::ExitCode::from(1)}},"
                )?;
            }
        }
    }
    let listing = serde_json::to_string(&rows)?;
    writeln!(
        source,
        r#"
fn main() -> std::process::ExitCode {{
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args == ["--list"] {{
        println!("{{}}", {listing:?});
        return std::process::ExitCode::SUCCESS;
    }}
    if args.len() != 2 || args[0] != "--exact-test" {{
        let mut includes = Vec::new();
        let mut excludes = Vec::new();
        let mut tokens = args.iter();
        while let Some(token) = tokens.next() {{
            match token.as_str() {{
                "-i" | "--include" => if let Some(value) = tokens.next() {{ includes.push(value.as_str()); }} else {{ return std::process::ExitCode::from(1); }},
                "-x" | "--exclude" => if let Some(value) = tokens.next() {{ excludes.push(value.as_str()); }} else {{ return std::process::ExitCode::from(1); }},
                "-h" | "--help" => {{ println!("[SELECTOR] [-i SELECTOR] [-x SELECTOR] [--list]"); return std::process::ExitCode::SUCCESS; }},
                value if value.starts_with('-') => {{ eprintln!("unknown option: {{value}}"); return std::process::ExitCode::from(1); }},
                value => includes.push(value),
            }}
        }}
        let selected: Vec<_> = {names:?}.into_iter().filter(|name|
            (includes.is_empty() || includes.iter().any(|p| glob_match(name, p))) &&
            !excludes.iter().any(|p| glob_match(name, p))
        ).collect();
        if selected.is_empty() {{ return std::process::ExitCode::from(5); }}
        let mut status = std::process::ExitCode::SUCCESS;
        for name in selected {{
            let child = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact-test", name]).status().unwrap();
            if !child.success() {{ status = std::process::ExitCode::from(2); }}
        }}
        return status;
    }}
    match args[1].as_str() {{
        {dispatch}
        _ => {{ eprintln!("select one exact test ID, or --list"); std::process::ExitCode::from(5) }}
    }}
}}
fn glob_match(subject: &str, pattern: &str) -> bool {{
    let parts: Vec<_> = pattern.split('*').collect();
    if parts.len() == 1 {{ return subject.contains(pattern); }}
    let first = parts[0];
    let last = parts[parts.len()-1];
    if !subject.starts_with(first) || !subject.ends_with(last) || first.len()+last.len()>subject.len() {{ return false; }}
    let mut pos = first.len();
    let limit = subject.len()-last.len();
    for part in &parts[1..parts.len()-1] {{
        let Some(i) = subject[pos..limit].find(part) else {{ return false; }};
        pos += i + part.len();
    }}
    true
}}
"#
    )?;
    Ok(source)
}

// All returned information is owned; no VM pointer survives the active permit.
unsafe fn collect(
    engine: &BexEngine,
    registry: HeapPtr,
    inherited_runner: bool,
    functions: &std::collections::HashMap<bex_vm_types::types::FunctionIdentity, usize>,
    out: &mut BTreeMap<String, Result<usize, String>>,
    unexpanded: &mut BTreeSet<String>,
) -> Result<()> {
    unsafe {
        let heap = engine.heap();
        let field = |ptr: HeapPtr, name: &str| -> Result<Value> {
            let Object::Instance(instance) = heap.get_object(ptr) else {
                return Err(anyhow!("registry instance expected"));
            };
            let Object::Class(class) = heap.get_object(instance.class) else {
                return Err(anyhow!("registry class expected"));
            };
            let i = class
                .fields
                .iter()
                .position(|f| f.name.as_str() == name)
                .ok_or_else(|| anyhow!("missing registry field {name}"))?;
            Ok(instance.load_field(i))
        };
        let pointer = |v: Value| {
            v.as_object_ptr()
                .ok_or_else(|| anyhow!("registry object expected"))
        };
        let text = |v: Value| -> Result<String> {
            let Object::String(s) = heap.get_object(pointer(v)?) else {
                return Err(anyhow!("registry string expected"));
            };
            Ok(s.to_string())
        };
        let collector = pointer(field(registry, "collector")?)?;
        let Object::Array(tests) = heap.get_object(pointer(field(collector, "tests")?)?) else {
            return Err(anyhow!("registry tests expected"));
        };
        for test in tests.to_vec() {
            let test = pointer(test)?;
            let name = text(field(test, "name")?)?;
            let runner = inherited_runner || !field(test, "runner")?.is_null();
            let entry = if runner {
                Err("unsupported in baml_rt: custom test runner".into())
            } else {
                let body = heap.get_object(pointer(field(test, "body")?)?);
                let function = match body {
                    Object::Closure(c)
                        if c.captures.is_empty() && c.captured_type_args.is_empty() =>
                    {
                        Ok(c.function)
                    }
                    Object::Function(_) => Ok(pointer(field(test, "body")?)?),
                    _ => Err("unsupported in baml_rt: captured test closure".to_string()),
                };
                function.and_then(|ptr| {
                    let Object::Function(f) = heap.get_object(ptr) else {
                        return Err("unsupported in baml_rt: test callable".into());
                    };
                    functions
                        .get(&f.identity)
                        .copied()
                        .ok_or_else(|| "test function identity missing from linked program".into())
                })
            };
            out.insert(name, entry);
        }
        let Object::Array(testsets) = heap.get_object(pointer(field(collector, "testsets")?)?)
        else {
            return Err(anyhow!("registry testsets expected"));
        };
        let mut runners = BTreeMap::new();
        for ts in testsets.to_vec() {
            let ts = pointer(ts)?;
            runners.insert(text(field(ts, "name")?)?, !field(ts, "runner")?.is_null());
        }
        let Object::Map(expansions) = heap.get_object(pointer(field(registry, "expansions")?)?)
        else {
            return Err(anyhow!("registry expansions expected"));
        };
        let mut expanded = BTreeSet::new();
        for (name, child) in expansions.snapshot_entries() {
            let name = text(name)?;
            expanded.insert(name.clone());
            collect(
                engine,
                pointer(child)?,
                inherited_runner || runners.get(&name).copied().unwrap_or(false),
                functions,
                out,
                unexpanded,
            )?;
        }
        unexpanded.extend(
            runners
                .keys()
                .filter(|name| !expanded.contains(*name))
                .cloned(),
        );
        Ok(())
    }
}
