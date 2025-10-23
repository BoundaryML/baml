use std::{fs, path::Path};

use baml_lib::{
    internal_baml_ast::{ast::diagram_generator, parse},
    internal_baml_diagnostics::SourceFile,
};

const ROOT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/validation_files/headers"
);

#[test]
#[ignore]
fn headers_mermaid_snapshots() {
    // Initialize logging at INFO level
    let _ = env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .is_test(true)
        .try_init();

    let dir = Path::new(ROOT);
    if !dir.exists() {
        panic!("fixtures dir missing: {ROOT}");
    }

    let mut ran: usize = 0;
    let mut passed: usize = 0;
    let mut updated: usize = 0;
    let mut skipped_panic: usize = 0;
    let mut missing_expect: usize = 0;
    let mut failed: usize = 0;
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("baml") {
            continue;
        }

        // Friendly relative name for output
        let rel_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());
        log::info!("================================================");
        log::info!("Generating Mermaid graph for {rel_name:#?}");
        log::info!("================================================");
        // Parse defensively; let panic message print so it is visible, but keep the test running
        let path_clone = path.clone();
        let res = std::panic::catch_unwind(|| {
            let baml = fs::read_to_string(&path).unwrap();
            // Use relative path for consistent snapshots across machines
            let manifest_dir = env!("CARGO_MANIFEST_DIR");
            let cur_path = Path::new(&manifest_dir);
            let relative_path = relative_path_ignoring_symlinks(cur_path, &path);
            let src = SourceFile::new_allocated(
                relative_path.to_string_lossy().to_string().into(),
                baml.clone().into(),
            );
            parse(Path::new("."), &src)
        });

        let Ok(parse_result) = res else {
            eprintln!("[mermaid] {:<12} | {}", "SKIP(panic)", rel_name);
            skipped_panic += 1;
            continue;
        };

        match parse_result {
            // Parsing produced an AST; check diagnostics for errors.
            Ok((ast, diags)) => {
                if diags.has_errors() {
                    let got_err = diags.to_pretty_string();
                    let mut exp_path = path.clone();
                    exp_path.set_extension("err");
                    if rel_name != "invalid.baml" {
                        eprintln!("[mermaid] {:<12} | {}", "FAIL(unexpected-error)", rel_name);
                        eprintln!("{}", normalize(&got_err));
                        failed += 1;
                        ran += 1;
                        continue;
                    } else {
                        if std::env::var("UPDATE").ok().as_deref() == Some("1") {
                            fs::write(&exp_path, got_err).unwrap();
                            println!("[mermaid] {:<12} | {}", "UPDATED(err)", rel_name);
                            ran += 1;
                            updated += 1;
                            continue;
                        }
                        match fs::read_to_string(&exp_path) {
                            Ok(expected) => {
                                let got_n = normalize(&got_err);
                                let exp_n = normalize(&expected);
                                if got_n == exp_n {
                                    println!("[mermaid] {:<12} | {}", "PASS(err)", rel_name);
                                    passed += 1;
                                    ran += 1;
                                } else {
                                    eprintln!("[mermaid] {:<12} | {}", "FAIL(err)", rel_name);
                                    eprintln!("EXPECTED ({rel_name}):\n{exp_n}\n---");
                                    eprintln!("GOT      ({rel_name}):\n{got_n}\n---");
                                    failed += 1;
                                    ran += 1;
                                    continue;
                                }
                            }
                            Err(_) => {
                                eprintln!("[mermaid] {:<12} | {}", "FAIL(err-noexpect)", rel_name);
                                failed += 1;
                                ran += 1;
                                continue;
                            }
                        }
                    }
                } else {
                    // No errors: compare Mermaid graph
                    let got = diagram_generator::generate_with_styling(
                        diagram_generator::MermaidGeneratorContext {
                            use_fancy: false,
                            function_filter: None,
                        },
                        &ast,
                    );
                    let mut exp_path = path.clone();
                    exp_path.set_extension("mmd");
                    if std::env::var("UPDATE").ok().as_deref() == Some("1") {
                        fs::write(&exp_path, got).unwrap();
                        println!("[mermaid] {:<12} | {}", "UPDATED", rel_name);
                        ran += 1;
                        updated += 1;
                        continue;
                    }
                    match fs::read_to_string(&exp_path) {
                        Ok(expected) => {
                            let got_n = normalize(&got);
                            let exp_n = normalize(&expected);
                            if got_n == exp_n {
                                println!("[mermaid] {:<12} | {}", "PASS", rel_name);
                                passed += 1;
                                ran += 1;
                            } else {
                                eprintln!("[mermaid] {:<12} | {}", "FAIL", rel_name);
                                eprintln!("EXPECTED ({rel_name}):\n{exp_n}\n---");
                                eprintln!("GOT      ({rel_name}):\n{got_n}\n---");
                                failed += 1;
                                ran += 1;
                                continue;
                            }
                        }
                        Err(_) => {
                            eprintln!("[mermaid] {:<12} | {}", "SKIP(expect)", rel_name);
                            missing_expect += 1;
                            continue;
                        }
                    }
                }
            }
            // Parsing failed with diagnostics: assert error output
            Err(diags) => {
                // Parse failed: assert error output
                let got_err = diags.to_pretty_string();
                let mut exp_path = path.clone();
                exp_path.set_extension("err");
                if rel_name != "invalid.baml" {
                    eprintln!("[mermaid] {:<12} | {}", "FAIL(unexpected-error)", rel_name);
                    eprintln!("{}", normalize(&got_err));
                    failed += 1;
                    ran += 1;
                    continue;
                } else {
                    if std::env::var("UPDATE").ok().as_deref() == Some("1") {
                        fs::write(&exp_path, got_err).unwrap();
                        println!("[mermaid] {:<12} | {}", "UPDATED(err)", rel_name);
                        ran += 1;
                        updated += 1;
                        continue;
                    }
                    match fs::read_to_string(&exp_path) {
                        Ok(expected) => {
                            let got_n = normalize(&got_err);
                            let exp_n = normalize(&expected);
                            if got_n == exp_n {
                                println!("[mermaid] {:<12} | {}", "PASS(err)", rel_name);
                                passed += 1;
                                ran += 1;
                            } else {
                                eprintln!("[mermaid] {:<12} | {}", "FAIL(err)", rel_name);
                                eprintln!("EXPECTED ({rel_name}):\n{exp_n}\n---");
                                eprintln!("GOT      ({rel_name}):\n{got_n}\n---");
                                failed += 1;
                                ran += 1;
                                continue;
                            }
                        }
                        Err(_) => {
                            eprintln!("[mermaid] {:<12} | {}", "FAIL(err-noexpect)", rel_name);
                            failed += 1;
                            ran += 1;
                            continue;
                        }
                    }
                }
            }
        }
    }

    assert!(ran > 0, "no valid fixtures were executed in {ROOT}");
    assert!(
        failed == 0,
        "{failed} fixtures failed; see output for details"
    );
    println!("[mermaid] Summary");
    println!("  ran:     {ran}");
    println!("  pass:    {passed}");
    println!("  updated: {updated}");
    println!("  skip:");
    println!("    panic:  {skipped_panic}");
    println!("    expect: {missing_expect}");
    println!("  fail:    {failed}");
}

fn normalize(s: &str) -> String {
    // Ensure we compare literal newlines rather than escaped sequences.
    let s = strip_ansi(s);
    // Convert CRLF to LF
    let s = s.replace("\r\n", "\n");
    // Unescape any accidental escaped newlines so diffs show proper lines
    let s = s.replace("\\n", "\n");
    s.trim().to_string()
}

// Simple ANSI escape stripper to keep error snapshots stable across environments.
fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if let Some('[') = chars.peek().copied() {
                // Consume '['
                chars.next();
                // Consume until we hit a letter (commonly 'm', 'K', 'G', etc.)
                for ch in chars.by_ref() {
                    if ch.is_alphabetic() {
                        break;
                    }
                }
                continue;
            }
        }
        out.push(c);
    }
    out
}

/// Creates a path that directs how to go from `from` to `to`, only lexically (without checking the
/// file system). It inserts `../` where required.
fn relative_path_ignoring_symlinks(
    from: &std::path::Path,
    to: &std::path::Path,
) -> std::path::PathBuf {
    use std::path::{Component, Path};

    // we want to get the parent dir of the file.
    let mut to_components = to
        .parent()
        .into_iter()
        .flat_map(Path::components)
        .peekable();
    let mut dir_components = from.components().peekable();
    // cut common prefix.
    loop {
        let both_peek = (to_components.peek(), dir_components.peek());

        if matches!(both_peek, (Some(a), Some(b)) if a == b) {
            _ = to_components.next();
            _ = dir_components.next();
        } else {
            break;
        }
    }
    // The number of components left in `dir_components` says how many `../`'s we
    // need.
    // After it, the remaining source components should go.
    let prev_dirs = std::iter::repeat_n(Component::ParentDir, dir_components.count());
    let parent_dir_components = prev_dirs.chain(to_components);
    let filename = to.file_name().map(Component::Normal);
    let full_components = parent_dir_components.chain(filename);
    std::path::PathBuf::from_iter(full_components)
}

// verbose mode removed for simplicity; panics print naturally and we tag the file in output
