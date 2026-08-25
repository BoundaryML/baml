//! Build script that generates tests from the projects/ directory.
//! Each folder becomes a test module with comprehensive compiler phase tests.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use walkdir::WalkDir;

/// Creates an `include_str!(r"path")` expression.
/// Since paths may contain backslashes on Windows, we need to be careful.
/// Using a regular string literal with the path works fine.
fn make_include_str(path: &str) -> TokenStream {
    // Create a string literal - quote! will handle the escaping
    let lit = syn::LitStr::new(path, proc_macro2::Span::call_site());
    quote! {
        include_str!(#lit)
    }
}

#[path = "build_stdlib_prefix_config.rs"]
mod stdlib_prefix_config;

/// Compile the stdlib once and embed it, so the compile helpers in
/// `src/stdlib_prefix.rs` can splice it in instead of re-deriving it per test.
/// See that module for why this cannot be an in-process cache.
fn generate_stdlib_prefix() {
    use baml_project::stdlib_prefix::{OptLevel, build_stdlib_prefix};

    println!("cargo:rerun-if-changed=build_stdlib_prefix_config.rs");

    let prefixes = stdlib_prefix_config::OPT_LEVELS
        .into_iter()
        .map(|raw| {
            let opt = match raw {
                0 => OptLevel::Zero,
                1 => OptLevel::One,
                2 => OptLevel::Two,
                other => panic!("OPT_LEVELS lists {other}, which is not an OptLevel"),
            };
            build_stdlib_prefix(opt)
        })
        .collect();
    let bytes = baml_project::stdlib_prefix::encode_artifact(
        &stdlib_prefix_config::artifact_key(),
        prefixes,
    );

    let out_dir = env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR for build scripts");
    fs::write(PathBuf::from(out_dir).join("stdlib_prefix.borsh"), bytes)
        .expect("write stdlib prefix artifact");
}

fn main() {
    // Watch the projects directory for changes
    println!("cargo:rerun-if-changed=projects");

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();

    generate_stdlib_prefix();

    // Generate tests
    generate_tests(&manifest_dir);

    // Generate CodSpeed benches from the tools/speedtest workloads.
    generate_speedtest_benches(&manifest_dir);
}

// ============================================================================
// Speedtest workload benches
// ============================================================================
//
// The `tools/speedtest` harness drives a corpus of cross-language micro-
// benchmarks (BAML vs Python vs JS) defined as `.md` workloads. Rather than
// hand-maintaining a parallel set of `#[divan::bench]` functions in
// runtime_benchmark.rs, we lift every workload's *BAML* source straight out of
// that corpus and emit one bench per workload.
//
// Expansion (some workloads use a Python `## eval-setup` block + `$$` templating)
// is delegated to `tools/speedtest/export_baml.py`, which reuses the exact same
// `speedtest.loader` logic the harness itself uses — so there is a single source
// of truth for parsing. We shell out to `python3` once and read back JSON.
//
// The generated functions are named `vm_speedtest_<slug>` so they are picked up
// by the same `vm_` CodSpeed filter as the hand-written VM benches, and are
// `include!`d into runtime_benchmark.rs after `bench_vm_main` is in scope.
//
// This step degrades gracefully: if `python3` or the workloads are unavailable
// (e.g. a minimal build environment), it emits an empty file and a warning
// rather than failing the build of the whole crate.

/// Fully-qualified name of the blocking sleep builtin. Workloads whose BAML
/// calls this are excluded from the generated walltime benches.
const SLEEP_FQN: &str = "baml.sys.sleep";

struct Workload {
    name: String,
    baml: String,
}

fn generate_speedtest_benches(manifest_dir: &str) {
    // crates/baml_tests -> repo root -> tools/speedtest
    let speedtest_dir = Path::new(manifest_dir)
        .join("..")
        .join("..")
        .join("tools")
        .join("speedtest");
    let workloads_dir = speedtest_dir.join("workloads");
    let export_script = speedtest_dir.join("export_baml.py");

    // Re-run whenever the corpus or the expansion logic changes.
    println!("cargo:rerun-if-changed={}", workloads_dir.display());
    println!("cargo:rerun-if-changed={}", export_script.display());
    println!(
        "cargo:rerun-if-changed={}",
        speedtest_dir.join("src/speedtest/loader.py").display()
    );

    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("speedtest_benches.rs");

    let all_workloads = match load_speedtest_workloads(&export_script, &workloads_dir) {
        Ok(w) => w,
        Err(e) => {
            println!("cargo:warning=speedtest benches disabled: {e}");
            // Emit empty (but valid) files so the include!s in both bench
            // targets compile.
            fs::write(
                &dest_path,
                "// speedtest workloads unavailable at build time; no benches generated.\n",
            )
            .unwrap();
            fs::write(
                Path::new(&out_dir).join("speedtest_profiling_sources.rs"),
                "// speedtest workloads unavailable at build time.\n\
                 pub const PROF_SRC_COMPUTE_PURE_CALL_1M: &str = \"\";\n\
                 pub const PROF_SRC_COMPUTE_ARRAY_BUILD_SUM_100K: &str = \"\";\n\
                 pub const PROF_SRC_COMPUTE_FIB32_RECURSIVE: &str = \"\";\n\
                 pub const PROF_SRC_CONCURRENCY_SPAWN_AWAIT_X10K: &str = \"\";\n\
                 pub const PROF_SRC_STRING_CONCAT_LOOP_10K: &str = \"\";\n",
            )
            .unwrap();
            return;
        }
    };

    // Exclude workloads that call the blocking sleep builtin: as a walltime
    // benchmark their sample time is dominated by sleeping rather than VM work,
    // which only adds noise and CI time. Matched by fully-qualified name so a
    // workload that merely mentions "sleep" elsewhere is unaffected.
    let (workloads, skipped): (Vec<&Workload>, Vec<&Workload>) = all_workloads
        .iter()
        .partition(|w| !w.baml.contains(SLEEP_FQN));
    if !skipped.is_empty() {
        let names: Vec<&str> = skipped.iter().map(|w| w.name.as_str()).collect();
        println!(
            "cargo:warning=speedtest: skipping {} sleep-based workload(s) (calls `{SLEEP_FQN}`): {}",
            skipped.len(),
            names.join(", ")
        );
    }

    let mut used = std::collections::BTreeSet::new();
    let benches: TokenStream = workloads
        .iter()
        .map(|w| {
            // `name` already carries the `category::` prefix (e.g.
            // "classes::method call 100k"), so slugify it directly.
            let mut slug = slugify(&w.name);
            // Guard against the (unlikely) collision after slugification.
            while !used.insert(slug.clone()) {
                slug.push('_');
            }
            let fn_ident = format_ident!("vm_speedtest_{slug}");
            let source = &w.baml;
            let display = &w.name;
            quote! {
                #[doc = #display]
                #[divan::bench]
                fn #fn_ident(bencher: divan::Bencher) {
                    bench_vm_main(bencher, #source);
                }
            }
        })
        .collect();

    let header = "\
// Auto-generated speedtest workload benches by build.rs.
// Source of truth: tools/speedtest/workloads/*.md (expanded via export_baml.py).
// Do not edit this file directly.
";

    write_formatted_code(&dest_path, benches, header);

    // Also emit the small fixed subset consumed by the `profiling_overhead`
    // bench target: same single source of truth, but only the workloads chosen
    // to characterize tracing cost (per-call ring pairs, allocation-heavy
    // loops, the known ring-overflow reproducer, and a string baseline).
    let subset = [
        "compute::pure call 1m",
        "compute::array build sum 100k",
        "compute::fib32 recursive",
        "concurrency::spawn await x10k",
        "string::concat loop 10k",
    ];
    let prof_consts: TokenStream = subset
        .iter()
        .map(|name| {
            let slug = slugify(name);
            let ident = format_ident!("PROF_SRC_{}", slug.to_uppercase());
            let source = all_workloads
                .iter()
                .find(|w| w.name == *name)
                .map(|w| w.baml.as_str())
                .unwrap_or("");
            quote! {
                pub const #ident: &str = #source;
            }
        })
        .collect();
    let prof_path = Path::new(&out_dir).join("speedtest_profiling_sources.rs");
    write_formatted_code(
        &prof_path,
        prof_consts,
        "// Auto-generated profiling-overhead workload sources by build.rs.\n\
         // Source of truth: tools/speedtest/workloads/*.md (expanded via export_baml.py).\n\
         // An empty const means the corpus was unavailable at build time; the\n\
         // corresponding bench skips itself.\n",
    );
}

/// Run `export_baml.py` and parse its JSON output into the workload list.
fn load_speedtest_workloads(
    export_script: &Path,
    workloads_dir: &Path,
) -> Result<Vec<Workload>, String> {
    if !export_script.is_file() {
        return Err(format!(
            "export script not found at {}",
            export_script.display()
        ));
    }
    if !workloads_dir.is_dir() {
        return Err(format!(
            "workloads dir not found at {}",
            workloads_dir.display()
        ));
    }

    let python = env::var("PYTHON3").unwrap_or_else(|_| "python3".to_string());
    let output = std::process::Command::new(&python)
        .arg(export_script)
        .arg(workloads_dir)
        .output()
        .map_err(|e| format!("failed to run `{python}`: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "`{python} export_baml.py` exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    // The exporter (via speedtest.loader) warns to stderr when a workload `.md`
    // fails to parse and is dropped. Surface those so a malformed workload can't
    // silently disappear from the generated suite behind a still-green build.
    let stderr = String::from_utf8_lossy(&output.stderr);
    for line in stderr.lines().map(str::trim).filter(|l| !l.is_empty()) {
        println!("cargo:warning=speedtest export: {line}");
    }

    let value: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("failed to parse export_baml.py JSON: {e}"))?;
    let arr = value
        .as_array()
        .ok_or_else(|| "export_baml.py did not emit a JSON array".to_string())?;

    let mut workloads = Vec::with_capacity(arr.len());
    for item in arr {
        let field = |key: &str| {
            item.get(key)
                .and_then(|v| v.as_str())
                .map(str::to_owned)
                .ok_or_else(|| format!("workload missing string field `{key}`"))
        };
        workloads.push(Workload {
            name: field("name")?,
            baml: field("baml")?,
        });
    }
    Ok(workloads)
}

/// Turn an arbitrary workload name into a valid, lowercase Rust identifier
/// fragment (e.g. "string::split long literal 1k" -> "string_split_long_literal_1k").
fn slugify(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut prev_underscore = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.extend(ch.to_lowercase());
            prev_underscore = false;
        } else if !prev_underscore {
            out.push('_');
            prev_underscore = true;
        }
    }
    out.trim_matches('_').to_string()
}

fn generate_tests(manifest_dir: &str) {
    let projects_dir = Path::new(&manifest_dir).join("projects");

    // Discover all projects
    let projects = discover_projects(&projects_dir);

    // Write to the source directory so that file!() returns a stable path
    // (OUT_DIR contains a hash that changes between builds, causing noisy
    // diffs in insta snapshot metadata).
    let dest_path = Path::new(&manifest_dir).join("src/generated_tests.rs");

    // Group projects by tier
    let mut tier_groups: std::collections::BTreeMap<&'static str, Vec<TokenStream>> =
        std::collections::BTreeMap::new();

    for project in &projects {
        let module = generate_project_tests(project, manifest_dir);
        tier_groups
            .entry(project.tier.dir_name())
            .or_default()
            .push(module);
    }

    // Emit nested modules: mod broken_syntax { mod project1 { ... } mod project2 { ... } }
    let test_modules: TokenStream = tier_groups
        .into_iter()
        .map(|(tier_name, modules)| {
            let tier_ident = format_ident!("{}", tier_name);
            quote! {
                #[cfg(test)]
                mod #tier_ident {
                    #(#modules)*
                }
            }
        })
        .collect();

    let header = "\
// Auto-generated tests from projects/ by build.rs
// Do not edit this file directly.
";

    write_formatted_code(&dest_path, test_modules, header);
}

fn write_formatted_code(path: &Path, code: TokenStream, header: &str) {
    let code_string = code.to_string();
    let syntax_tree = syn::parse_file(&code_string).expect("Failed to parse generated code");
    let formatted = prettyplease::unparse(&syntax_tree);

    // Prepend the header (doc comments that prettyplease would strip)
    let output = format!("{header}\n{formatted}");
    fs::write(path, output).unwrap();
}

// Test-related structures and functions
//
// Projects that compile cleanly are not a tier here: they live in
// `baml_src/ns_fixtures/` and are covered by the single-compile corpus
// snapshot pass in `src/corpus.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Tier {
    BrokenSyntax,
    DiagnosticErrors,
}

impl Tier {
    fn dir_name(&self) -> &'static str {
        match self {
            Tier::BrokenSyntax => "broken_syntax",
            Tier::DiagnosticErrors => "diagnostic_errors",
        }
    }

    const ALL: &[Tier] = &[Tier::BrokenSyntax, Tier::DiagnosticErrors];
}

struct TestProject {
    name: String,
    #[allow(dead_code)]
    path: PathBuf,
    files: Vec<BamlFile>,
    tier: Tier,
}

struct BamlFile {
    name: String,
    relative_path: PathBuf,
    full_path: PathBuf,
}

fn discover_projects(projects_dir: &Path) -> Vec<TestProject> {
    let mut projects = Vec::new();

    if !projects_dir.exists() {
        return projects;
    }

    for tier in Tier::ALL {
        let tier_dir = projects_dir.join(tier.dir_name());
        if !tier_dir.exists() {
            continue;
        }

        for entry in fs::read_dir(&tier_dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();

            if !path.is_dir() {
                continue;
            }

            let name = path.file_name().unwrap().to_str().unwrap().to_string();

            // parser_stress remains excluded (it would only appear if
            // someone moved it into a tier directory by mistake)
            if name == "parser_stress" {
                continue;
            }

            let files = discover_baml_files(&path);

            if !files.is_empty() {
                projects.push(TestProject {
                    name,
                    path,
                    files,
                    tier: *tier,
                });
            }
        }
    }

    // Sort by tier first, then by name within each tier
    projects.sort_by(|a, b| a.tier.cmp(&b.tier).then(a.name.cmp(&b.name)));
    projects
}

fn discover_baml_files(dir: &Path) -> Vec<BamlFile> {
    let mut files = Vec::new();

    for entry in WalkDir::new(dir) {
        let entry = entry.unwrap();
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) == Some("baml") {
            let relative_path = path.strip_prefix(dir).unwrap().to_path_buf();

            // Create safe test name from path
            let name = relative_path
                .to_str()
                .unwrap()
                .replace(['/', '\\'], "_")
                .replace(".baml", "");

            files.push(BamlFile {
                name,
                relative_path,
                full_path: path.to_path_buf(),
            });
        }
    }

    files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    files
}

fn generate_project_tests(project: &TestProject, manifest_dir: &str) -> TokenStream {
    let module_name = format_ident!("{}", project.name.replace("-", "_"));
    let snapshot_path = format!(
        "{}/snapshots/{}/{}",
        manifest_dir.replace('\\', "/"),
        project.tier.dir_name(),
        project.name
    );

    // All tiers get diagnostics (with tier-specific invariant assertions)
    let diagnostics_test = generate_diagnostics_test(project, project.tier);

    // Tier-specific phases
    let (hir_test, formatter_tests) = match project.tier {
        Tier::BrokenSyntax => {
            // Tier 1: diagnostics only - no higher phases
            (quote! {}, quote! {})
        }
        Tier::DiagnosticErrors => {
            // Tier 2: HIR, formatter — no MIR, no codegen
            let hir = generate_hir_test(project);
            let fmt: TokenStream = project.files.iter().map(generate_formatter_test).collect();
            (hir, fmt)
        }
    };

    // parser_-prefixed tests: only for parser_ projects regardless of tier
    let parser_specific_tests = if project.name.starts_with("parser_") {
        let incremental_tests: TokenStream = project
            .files
            .iter()
            .map(generate_incremental_parsing_test)
            .collect();

        let node_reuse_tests: TokenStream =
            project.files.iter().map(generate_node_reuse_test).collect();

        let tree_lossless_test = generate_tree_lossless_test(project);

        quote! {
            #incremental_tests
            #node_reuse_tests
            #tree_lossless_test
        }
    } else {
        quote! {}
    };

    quote! {
        mod #module_name {
            use baml_db::*;
            use baml_project::ProjectDatabase;
            use std::collections::HashMap;
            use insta::{assert_snapshot, with_settings};
            use std::fmt::Write;
            #[allow(unused_imports)]
            use crate::utils::*;
            const SNAPSHOT_PATH: &str = #snapshot_path;

            #hir_test
            #diagnostics_test
            #formatter_tests
            #parser_specific_tests
        }
    }
}

fn generate_hir_test(project: &TestProject) -> TokenStream {
    let file_loaders: TokenStream = project
        .files
        .iter()
        .map(|baml_file| {
            let full_path = baml_file.full_path.display().to_string();
            let relative_path = baml_file.relative_path.display().to_string();
            let include_content = make_include_str(&full_path);

            quote! {
                {
                    let content = #include_content;
                    let content = content.replace("\r\n", "\n");
                    let sf = db.add_file(
                        #relative_path,
                        &content,
                    );
                    source_files.push(sf);
                }
            }
        })
        .collect();

    quote! {
        #[test]
        fn test_03_ppir() {
            use crate::compiler2_tir::support::render_ppir;

            let mut db = ProjectDatabase::new();
            let _root = db.set_project_root(std::path::Path::new("."));
            // The stdlib is identical across every one of these projects, so its
            // interface is served from the build-time slice instead of being
            // re-derived per project. The stdlib *sources* stay in the database,
            // so spans and body-walking checks (E0153, E0163) are unaffected.
            db.set_seeded_stdlib_interface(
                crate::stdlib_prefix::prefix(crate::stdlib_prefix::OptLevel::One)
                    .interfaces
                    .clone(),
            );
            let mut source_files = Vec::new();

            #file_loaders

            let mut output = String::new();
            writeln!(output, "=== PPIR ===").unwrap();

            for source_file in &source_files {
                output.push_str(&render_ppir(&db, *source_file));
            }

            with_settings!({snapshot_path => SNAPSHOT_PATH, omit_expression => true}, {
                assert_snapshot!("03_ppir", output);
            });
        }
    }
}

fn generate_diagnostics_test(project: &TestProject, tier: Tier) -> TokenStream {
    let file_loaders: TokenStream = project
        .files
        .iter()
        .map(|baml_file| {
            let full_path = baml_file.full_path.display().to_string();
            let relative_path = baml_file.relative_path.display().to_string();
            let include_content = make_include_str(&full_path);

            quote! {
                {
                    let content = #include_content;
                    let content = content.replace("\r\n", "\n");
                    let source_file = db.add_file(
                        #relative_path,
                        &content,
                    );
                    source_files.push(source_file);
                }
            }
        })
        .collect();

    let project_name = &project.name;
    let tier_name = tier.dir_name();

    let tier_assertion = match tier {
        Tier::BrokenSyntax => quote! {
            // Tier 1 invariant: at least one file must have parse errors
            let has_parse_errors = diagnostics.iter().any(|d| {
                d.phase == DiagnosticPhase::Parse
                    && d.severity == baml_compiler_diagnostics::Severity::Error
            });
            let error_count = diagnostics.iter().filter(|d| d.severity == baml_compiler_diagnostics::Severity::Error).count();
            let warning_count = diagnostics.iter().filter(|d| d.severity == baml_compiler_diagnostics::Severity::Warning).count();
            assert!(
                has_parse_errors,
                "Tier invariant failed for project '{}' in '{}/'\n\
                 \n\
                 Expected: at least one parse-phase error (broken_syntax/ projects test invalid syntax)\n\
                 Got:      {} error(s), {} warning(s), 0 parse errors\n\
                 \n\
                 This usually means a parser change fixed a syntax error this project was testing.\n\
                 The snapshot above shows the actual diagnostics.\n\
                 \n\
                 To fix:\n\
                 1. If intentional, update the .baml files to test a different syntax error\n\
                 2. If the project now has only semantic errors, move it to diagnostic_errors/\n\
                 3. If the project now compiles cleanly, move it to baml_src/ns_fixtures/",
                #project_name,
                #tier_name,
                error_count,
                warning_count,
            );
        },
        Tier::DiagnosticErrors => quote! {
            // Tier 2 invariant: must have error diagnostics but no parse errors
            let parse_error_count = diagnostics.iter().filter(|d| {
                d.phase == DiagnosticPhase::Parse
                    && d.severity == baml_compiler_diagnostics::Severity::Error
            }).count();
            let error_count = diagnostics.iter().filter(|d| d.severity == baml_compiler_diagnostics::Severity::Error).count();
            let warning_count = diagnostics.iter().filter(|d| d.severity == baml_compiler_diagnostics::Severity::Warning).count();
            assert!(
                parse_error_count == 0,
                "Tier invariant failed for project '{}' in '{}/'\n\
                 \n\
                 Expected: error diagnostics but no parse errors (diagnostic_errors/ projects have valid syntax with semantic errors)\n\
                 Got:      {} parse error(s) out of {} total error(s), {} warning(s)\n\
                 \n\
                 This usually means a code change introduced a syntax error in this project.\n\
                 The snapshot above shows the actual diagnostics.\n\
                 \n\
                 To fix:\n\
                 1. If a code change broke parsing, fix the parser regression\n\
                 2. If the .baml files were edited to have intentionally broken syntax, move it to broken_syntax/",
                #project_name,
                #tier_name,
                parse_error_count,
                error_count,
                warning_count,
            );
            assert!(
                error_count > 0,
                "Tier invariant failed for project '{}' in '{}/'\n\
                 \n\
                 Expected: at least one error diagnostic (diagnostic_errors/ projects test semantic errors)\n\
                 Got:      0 errors, {} warning(s)\n\
                 \n\
                 This usually means a compiler change resolved the errors this project was testing.\n\
                 The snapshot above shows the actual diagnostics.\n\
                 \n\
                 To fix:\n\
                 1. If intentional, update the .baml files to test a different semantic error\n\
                 2. If the project now compiles cleanly, move it to baml_src/ns_fixtures/",
                #project_name,
                #tier_name,
                warning_count,
            );
        },
    };

    quote! {
        #[test]
        fn test_05_diagnostics() {
            use baml_compiler_diagnostics::{DiagnosticPhase, RenderConfig, render_diagnostic};
            use baml_compiler2_hir::compiler2_all_files;
            use crate::stdlib_prefix::check_user_files;
            use std::path::PathBuf;

            let mut db = ProjectDatabase::new();
            let _root = db.set_project_root(std::path::Path::new("."));
            // The stdlib is identical across every one of these projects, so its
            // interface is served from the build-time slice instead of being
            // re-derived per project. The stdlib *sources* stay in the database,
            // so spans and body-walking checks (E0153, E0163) are unaffected.
            db.set_seeded_stdlib_interface(
                crate::stdlib_prefix::prefix(crate::stdlib_prefix::OptLevel::One)
                    .interfaces
                    .clone(),
            );
            let mut source_files = Vec::new();

            #file_loaders

            let all_files = compiler2_all_files(&db);
            // Only this project's files are checked: the stdlib contributes no
            // diagnostics of its own (the corpus pass in `src/corpus.rs` holds it
            // to that), and re-checking its ~50 files once per project is the
            // dominant cost of this suite.
            let diagnostics = check_user_files(&db);

            let mut sources: HashMap<baml_db::FileId, String> = HashMap::new();
            let mut file_paths: HashMap<baml_db::FileId, PathBuf> = HashMap::new();
            for source_file in &all_files {
                let file_id = source_file.file_id(&db);
                sources.insert(file_id, source_file.text(&db).to_string());
                file_paths.insert(file_id, source_file.path(&db));
            }

            // Diagnostic spans must tightly cover the offending construct, never
            // the leading/trailing whitespace around it (see
            // `assert_diagnostic_spans_exclude_trivia`).
            assert_diagnostic_spans_exclude_trivia(#project_name, &diagnostics, &sources);

            let config = RenderConfig::test();

            let mut output = String::new();
            writeln!(output, "=== COMPILER2 DIAGNOSTICS ===").unwrap();
            if diagnostics.is_empty() {
                writeln!(output, "No errors found.").unwrap();
            } else {
                for diag in &diagnostics {
                    let phase_name = match diag.phase {
                        DiagnosticPhase::Parse => "parse",
                        DiagnosticPhase::Hir => "hir",
                        DiagnosticPhase::Validation => "validation",
                        DiagnosticPhase::Type => "type",
                    };
                    let rendered = render_diagnostic(diag, &sources, &file_paths, &config);
                    writeln!(output, "  [{}] {}", phase_name, rendered).unwrap();
                }
            }

            with_settings!({snapshot_path => SNAPSHOT_PATH, omit_expression => true}, {
                assert_snapshot!("05_diagnostics", output);
            });

            #tier_assertion
        }
    }
}

// Parser-specific test generation functions
fn generate_incremental_parsing_test(baml_file: &BamlFile) -> TokenStream {
    let test_name = format_ident!("test_07_incremental_{}", baml_file.name);
    let full_path = baml_file.full_path.display().to_string();
    let relative_path = baml_file.relative_path.display().to_string();
    let include_content = make_include_str(&full_path);

    quote! {
        #[test]
        fn #test_name() {
            let content = #include_content;
            let content = content.replace("\r\n", "\n");

            // Test single character edits maintain correctness
            let mut db = ProjectDatabase::new();
            let source_file = db.add_file(#relative_path, &content);
            let original_tree = baml_compiler_parser::syntax_tree(&db, source_file);

            // Test adding a character
            let modified = insert_char(&content, content.len() / 2, 'x');
            let modified_file = db.add_file("modified.baml", &modified);
            let modified_tree = baml_compiler_parser::syntax_tree(&db, modified_file);

            // Verify the trees are valid
            assert_no_panics(&original_tree);
            assert_no_panics(&modified_tree);
        }
    }
}

fn generate_node_reuse_test(baml_file: &BamlFile) -> TokenStream {
    let test_name = format_ident!("test_08_node_reuse_{}", baml_file.name);
    let full_path = baml_file.full_path.display().to_string();
    let relative_path = baml_file.relative_path.display().to_string();
    let include_content = make_include_str(&full_path);

    quote! {
        #[test]
        fn #test_name() {
            let content = #include_content;
            let content = content.replace("\r\n", "\n");

            // Measure node reuse for single character edit
            let mut db = ProjectDatabase::new();
            let source_file = db.add_file(#relative_path, &content);
            let original_tree = baml_compiler_parser::syntax_tree(&db, source_file);

            // Make a small edit
            let modified = insert_char(&content, content.len() / 2, 'a');
            let modified_file = db.add_file("modified.baml", &modified);
            let modified_tree = baml_compiler_parser::syntax_tree(&db, modified_file);

            // Measure reuse (this is a simplified check)
            // In a real implementation, you'd check actual node reuse
            assert_no_panics(&original_tree);
            assert_no_panics(&modified_tree);
        }
    }
}

fn generate_tree_lossless_test(project: &TestProject) -> TokenStream {
    let file_checks: TokenStream = project
        .files
        .iter()
        .map(|baml_file| {
            let full_path = baml_file.full_path.display().to_string();
            let relative_path = baml_file.relative_path.display().to_string();
            let include_content = make_include_str(&full_path);

            quote! {
                {
                    let content = #include_content;
                    let content = content.replace("\r\n", "\n");
                    let mut db = ProjectDatabase::new();
                    let source_file = db.add_file(#relative_path, &content);
                    let tree = baml_compiler_parser::syntax_tree(&db, source_file);
                    assert_tree_is_lossless(&tree, &content);
                }
            }
        })
        .collect();

    quote! {
        #[test]
        fn test_09_tree_lossless() {
            // Verify parse trees can reconstruct original source
            #file_checks
        }
    }
}

fn generate_formatter_test(baml_file: &BamlFile) -> TokenStream {
    let test_name = format_ident!("test_10_formatter_{}", baml_file.name);

    let snapshot_name = format!("10_formatter__{}", baml_file.name);
    let full_path = baml_file.full_path.display().to_string();
    let relative_path = baml_file.relative_path.display().to_string();

    quote! {
        #[test]
        fn #test_name() {
            // Read at runtime rather than include_str!: an embedded copy goes
            // stale when a restored CI target/ cache skips re-embedding a
            // changed corpus file, making the formatter output disagree with a
            // freshly-updated snapshot on CI only.
            let content = std::fs::read_to_string(#full_path)
                .unwrap_or_else(|e| panic!("failed to read {}: {e}", #full_path));
            // Normalize line endings for cross-platform compatibility
            let content = content.replace("\r\n", "\n");
            let options = baml_fmt::FormatOptions::default();

            let first = match baml_fmt::format(&content, &options) {
                Ok(formatted) => formatted,
                Err(e) => {
                    let output = match e {
                        baml_fmt::FormatterError::ParseErrors(e) => {
                            format!("=== PARSER ERROR ===\n{:?}", e)
                        }
                        baml_fmt::FormatterError::StrongAstError(e) => {
                            let e = e.print_with_file_context(#relative_path, &content);
                            format!("=== STRONG AST ERROR ===\n{}", e)
                        }
                    };
                    panic!(
                        "Formatter rejected compiler-test input that reaches the formatter tier ({}):\n{}",
                        #relative_path,
                        output
                    );
                }
            };

            with_settings!({snapshot_path => SNAPSHOT_PATH, omit_expression => true}, {
                assert_snapshot!(#snapshot_name, first);
            });

            // Format a second time – the output must be identical (idempotency).
            let second = match baml_fmt::format(&first, &options) {
                Ok(formatted) => formatted,
                Err(e) => {
                    panic!(
                        "Formatter succeeded on the original input ({}) but failed on its own output:\n{}",
                        #relative_path, e
                    );
                }
            };

            if first != second {
                std::fs::write(format!("{}/{}.new", SNAPSHOT_PATH, #relative_path), second.as_bytes()).unwrap();
                panic!(
                    "Formatter is not idempotent for {}.\n\
                    Second pass output written to {}/{}.new",
                    #relative_path,
                    SNAPSHOT_PATH,
                    #relative_path
                );
            }
        }
    }
}
