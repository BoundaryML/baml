//! Memory-leak audit repro (temporary, for the 50GB LSP RSS investigation).
//!
//! Simulates a long editing session: repeatedly mutates a file's text (like
//! didChange with FULL sync) and runs the diagnostics pipeline, printing RSS.
//!
//! Run with:
//!   cargo test -p baml_tests --test memory_leak_audit -- --nocapture --ignored

use baml_tests::engine::TestDbExt;

#[cfg(unix)]
fn rss_mb() -> f64 {
    // macOS: ru_maxrss is bytes
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    unsafe {
        if libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) != 0 {
            return 0.0;
        }
        let usage = usage.assume_init();
        #[cfg(target_os = "macos")]
        {
            usage.ru_maxrss as f64 / (1024.0 * 1024.0)
        }
        #[cfg(not(target_os = "macos"))]
        {
            usage.ru_maxrss as f64 / 1024.0
        }
    }
}

#[cfg(not(unix))]
fn rss_mb() -> f64 {
    0.0
}

#[cfg(unix)]
fn phys_footprint_mb() -> f64 {
    // Use current phys footprint via task_info would be better; fall back to ps
    let pid = std::process::id();
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<f64>()
        .unwrap_or(0.0)
        / 1024.0
}

#[cfg(not(unix))]
fn phys_footprint_mb() -> f64 {
    rss_mb()
}

fn project_source(n_funcs: usize, edit_tag: usize) -> String {
    let mut s = String::new();
    for i in 0..n_funcs {
        s.push_str(&format!(
            r#"
function Process{i}(input: string, count: int) -> string {{
  let x = input;
  let y = count + {edit_tag};
  let z = x;
  if (y > 10) {{ z }} else {{ x }}
}}
"#
        ));
    }
    s
}

#[test]
#[ignore = "manual memory audit"]
fn editing_session_memory_growth() {
    let dir = std::env::temp_dir().join(format!("baml_mem_audit_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file_path = dir.join("main.baml");
    std::fs::write(&file_path, project_source(50, 0)).unwrap();

    let mut db = baml_db::ProjectDatabase::new();
    db.workspace(&dir);
    db.file(&file_path, &project_source(50, 0));

    let baseline_diags = baml_db::collect_compiler2_diagnostics(&db);
    println!(
        "baseline: {} diagnostics, rss={:.1}MB",
        baseline_diags.len(),
        phys_footprint_mb()
    );

    let iters: usize = std::env::var("MEM_AUDIT_ITERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(500);

    let full_lsp_path = std::env::var("MEM_AUDIT_FULL").is_ok();

    for i in 1..=iters {
        // Simulate a keystroke: full-text update with a tiny change.
        let text = project_source(50, i);
        db.file(&file_path, &text);
        let diags = baml_db::collect_compiler2_diagnostics(&db);

        // Simulate the rest of an editor's didChange path: bytecode
        // generation on top of the diagnostics sweep.
        if full_lsp_path && diags.is_empty() {
            let _bytecode = db.get_bytecode();
        }

        if i % 50 == 0 {
            println!(
                "iter {i:5}: rss={:.1}MB maxrss={:.1}MB",
                phys_footprint_mb(),
                rss_mb()
            );
        }
    }
}

#[test]
#[ignore = "manual memory audit"]
fn delete_recreate_churn() {
    // Simulates branch switches / codegen rewriting files: each iteration
    // removes the file and re-adds it with slightly different content. The
    // tombstone-revival path in ProjectDatabase must keep RSS flat.
    let dir = std::env::temp_dir().join(format!("baml_mem_audit_rm_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file_path = dir.join("main.baml");
    std::fs::write(&file_path, project_source(50, 0)).unwrap();

    let mut db = baml_db::ProjectDatabase::new();
    db.workspace(&dir);
    db.file(&file_path, &project_source(50, 0));
    let _ = baml_db::collect_compiler2_diagnostics(&db);
    println!("baseline rss={:.1}MB", phys_footprint_mb());

    let iters: usize = std::env::var("MEM_AUDIT_ITERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(500);

    for i in 1..=iters {
        db.remove_file(&file_path);
        let _ = baml_db::collect_compiler2_diagnostics(&db);
        db.file(&file_path, &project_source(50, i));
        let _ = baml_db::collect_compiler2_diagnostics(&db);
        if i % 50 == 0 {
            println!(
                "iter {i:5}: rss={:.1}MB maxrss={:.1}MB",
                phys_footprint_mb(),
                rss_mb()
            );
        }
    }
}

#[test]
#[ignore = "manual memory audit"]
fn editing_session_rename_churn() {
    // Simulates typing a new function name char by char (identity churn:
    // new LocalItemId per keystroke -> new interned FunctionLoc per keystroke).
    let dir = std::env::temp_dir().join(format!("baml_mem_audit_rn_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file_path = dir.join("main.baml");
    let base = project_source(50, 0);
    std::fs::write(&file_path, &base).unwrap();

    let mut db = baml_db::ProjectDatabase::new();
    db.workspace(&dir);
    db.file(&file_path, &base);
    let _ = baml_db::collect_compiler2_diagnostics(&db);
    println!("baseline rss={:.1}MB", phys_footprint_mb());

    let iters: usize = std::env::var("MEM_AUDIT_ITERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(500);

    for i in 1..=iters {
        let name_suffix = format!("N{i}");
        let text = format!(
            "{base}\nfunction Fresh{name_suffix}(a: string) -> string {{\n  let q = a;\n  q\n}}\n"
        );
        db.file(&file_path, &text);
        let _diags = baml_db::collect_compiler2_diagnostics(&db);
        if i % 50 == 0 {
            println!(
                "iter {i:5}: rss={:.1}MB maxrss={:.1}MB",
                phys_footprint_mb(),
                rss_mb()
            );
        }
    }
}
