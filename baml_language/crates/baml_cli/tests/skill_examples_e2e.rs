//! The bundled agent skills (`skills/*/SKILL.md`) teach by example, so every
//! example must hold on this toolchain:
//!
//! every ```` ```baml ```` block compiles with no errors or warnings, is
//! formatted, and its `test` blocks pass.
//!
//! Each block is a standalone program, like a rustdoc doctest: it is placed
//! in a namespace of its own (`ns_line_<n>/`), so blocks never see each
//! other's declarations. The file is padded so that its line numbers are the
//! block's line numbers in `SKILL.md`, which makes every diagnostic and test
//! failure point straight at the skill.

mod common;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Command,
};

use baml_db::{
    ProjectDatabase, ProjectDiagnostics,
    baml_compiler_diagnostics::{Diagnostic, RenderConfig, render::render_diagnostics},
    testing::TestDbExt,
};
use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag, TagEnd};

/// One `skills/<name>/SKILL.md`.
struct Skill {
    /// Repo-relative path, as diagnostics display it.
    path: PathBuf,
    examples: Vec<Example>,
}

/// A ```` ```baml ```` block.
struct Example {
    /// 1-based `SKILL.md` line of the block's first code line.
    line: usize,
    code: String,
}

impl Example {
    /// Workspace-relative path; the directory gives the block its namespace.
    fn rel_path(&self) -> PathBuf {
        PathBuf::from(format!("ns_line_{}/example.baml", self.line))
    }

    /// The code, preceded by blank lines so its line numbers match `SKILL.md`.
    fn padded(&self) -> String {
        "\n".repeat(self.line - 1) + &self.code
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("crates/baml_cli sits three levels under the repo root")
        .to_path_buf()
}

fn skills() -> Vec<Skill> {
    let root = repo_root();
    let mut paths: Vec<PathBuf> = std::fs::read_dir(root.join("skills"))
        .expect("read skills/")
        .map(|entry| entry.expect("skills/ entry").path().join("SKILL.md"))
        .filter(|path| path.is_file())
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "no skills/*/SKILL.md found");
    paths
        .into_iter()
        .map(|path| {
            let source = std::fs::read_to_string(&path).expect("read SKILL.md");
            let rel = path.strip_prefix(&root).expect("skill under repo root");
            parse_skill(rel.to_path_buf(), &source)
        })
        .collect()
}

fn line_of(source: &str, offset: usize) -> usize {
    source[..offset].matches('\n').count() + 1
}

fn parse_skill(path: PathBuf, source: &str) -> Skill {
    let mut examples = Vec::new();
    // The open ```baml block: its first line and code, once its text arrives.
    let mut open: Option<Option<Example>> = None;
    for (event, range) in Parser::new(source).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) => {
                let mut words = info.split_whitespace();
                if words.next() != Some("baml") {
                    continue;
                }
                assert!(
                    words.next().is_none(),
                    "{}:{}: ```baml blocks take no attributes; every example must compile and pass",
                    path.display(),
                    line_of(source, range.start),
                );
                open = Some(None);
            }
            Event::Text(text) => {
                if let Some(example) = &mut open {
                    example
                        .get_or_insert_with(|| Example {
                            line: line_of(source, range.start),
                            code: String::new(),
                        })
                        .code
                        .push_str(&text);
                }
            }
            Event::End(TagEnd::CodeBlock) => examples.extend(open.take().flatten()),
            _ => {}
        }
    }
    Skill { path, examples }
}

/// Writes `skill`'s examples as a BAML project.
fn materialize(skill: &Skill) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("baml.toml"),
        "[package]\nname = \"skill_examples\"\n",
    )
    .expect("write baml.toml");
    for example in &skill.examples {
        let path = dir.path().join("baml_src").join(example.rel_path());
        std::fs::create_dir_all(path.parent().expect("example dir")).expect("create example dir");
        std::fs::write(path, example.padded()).expect("write example");
    }
    dir
}

fn report(failures: &[String]) {
    assert!(failures.is_empty(), "\n{}", failures.join("\n\n"));
}

#[test]
fn examples_compile_without_diagnostics() {
    let mut failures = Vec::new();
    for skill in skills() {
        let mut db = ProjectDatabase::new();
        db.workspace(Path::new("."));
        let mut example_files = HashMap::new();
        for example in &skill.examples {
            let file = db.file(example.rel_path(), &example.padded());
            example_files.insert(file.file_id(&db), file);
        }

        let diagnostics: Vec<Diagnostic> = ProjectDiagnostics::collect(&db)
            .into_vec()
            .into_iter()
            .filter(|diagnostic| {
                diagnostic.annotations.iter().any(|annotation| {
                    annotation.is_primary && example_files.contains_key(&annotation.span.file_id)
                })
            })
            .collect();
        if diagnostics.is_empty() {
            continue;
        }

        let mut sources = HashMap::new();
        let mut paths = HashMap::new();
        for file in baml_db::baml_compiler2_hir::compiler2_all_files(&db) {
            let file_id = file.file_id(&db);
            sources.insert(file_id, file.text(&db).clone());
            let path = if example_files.contains_key(&file_id) {
                skill.path.clone()
            } else {
                file.path(&db)
            };
            paths.insert(file_id, path);
        }
        failures.push(render_diagnostics(
            &diagnostics,
            &sources,
            &paths,
            &RenderConfig::test(),
        ));
    }
    report(&failures);
}

#[test]
fn examples_are_formatted() {
    let mut failures = Vec::new();
    for skill in skills() {
        for example in &skill.examples {
            let formatted = baml_fmt::format(&example.code, &baml_fmt::FormatOptions::default())
                .unwrap_or_else(|error| {
                    panic!(
                        "{}:{}: format failed: {error:?}",
                        skill.path.display(),
                        example.line
                    )
                });
            if formatted != example.code {
                failures.push(format!(
                    "{}:{}: example is not formatted; `baml fmt` gives:\n{formatted}",
                    skill.path.display(),
                    example.line
                ));
            }
        }
    }
    report(&failures);
}

#[test]
fn example_tests_pass() {
    let mut failures = Vec::new();
    for skill in skills() {
        let project = materialize(&skill);
        let output = Command::new(common::baml_cli())
            .arg("test")
            .arg("--project")
            .arg(project.path())
            .args(["--agent-skill-check", "off", "--color", "never"])
            .env("BAML_CLI_ALLOW_DIRECT", "1")
            .env("BAML_HOME", common::shared_baml_home())
            .env("BAML_TELEMETRY", "off")
            .output()
            .expect("spawn baml-cli test");
        if !output.status.success() {
            failures.push(format!(
                "{}: `baml test` over its examples failed (paths are ns_line_<n>/example.baml, \
                 line numbers are SKILL.md's):\n{}{}",
                skill.path.display(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            ));
        }
    }
    report(&failures);
}
