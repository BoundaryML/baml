//! BAML Compiler Benchmarks
//!
//! Run with: cargo bench --bench compiler_benchmark

use baml_db::*;
use codspeed_bencher_compat::{Bencher, benchmark_group, benchmark_main};

// Additional manual benchmarks
const BAML_EXT: &str = ".baml";

fn bench_empty_project(b: &mut Bencher) {
    b.iter(|| {
        let mut db = RootDatabase::new();
        let root = db.set_project_root(std::path::PathBuf::from("."));
        let _ = codspeed_bencher_compat::black_box(baml_hir::project_items(&db, root));
    });
}

fn bench_single_simple_file(b: &mut Bencher) {
    let content = r###"
class User {
    id: string
    name: string
}

function GetUser(id: string) -> User {
    client GPT4
    prompt #"Get user {{id}}"#
}

client GPT4 {
    provider: "openai"
    model: "gpt-4"
}
"###;

    b.iter(|| {
        let mut db = RootDatabase::new();
        let root = db.set_project_root(std::path::PathBuf::from("."));
        let filename = format!("test{}", BAML_EXT);
        db.add_file(&filename, content);
        let _ = codspeed_bencher_compat::black_box(baml_hir::project_items(&db, root));
    });
}

fn bench_incremental_simple_change(b: &mut Bencher) {
    let initial = r###"
class User {
    id: string
    name: string
}
"###;

    let updated = r###"
class User {
    id: string
    name: string
    email: string
}
"###;

    b.iter(|| {
        let mut db = RootDatabase::new();
        let root = db.set_project_root(std::path::PathBuf::from("."));
        let filename = format!("types{}", BAML_EXT);

        // Initial compilation
        db.add_file(&filename, initial);
        let _ = baml_hir::project_items(&db, root);

        // Simulate incremental update by adding the same file again
        // In Salsa, this should trigger incremental recompilation
        db.add_file(&filename, updated);
        let _ = codspeed_bencher_compat::black_box(baml_hir::project_items(&db, root));
    });
}

fn bench_parse_only_simple(b: &mut Bencher) {
    let content = r###"
class User {
    id: string
    name: string
    email: string
    posts: Post[]
}

class Post {
    id: string
    title: string
    content: string
    author: User
}

function GetUser(id: string) -> User {
    client GPT4
    prompt #"Get user {{id}}"#
}

client GPT4 {
    provider: "openai"
    model: "gpt-4"
}
"###;

    b.iter(|| {
        let mut db = RootDatabase::new();
        let filename = format!("test{}", BAML_EXT);
        let file = db.add_file(&filename, content);
        let _ = codspeed_bencher_compat::black_box(baml_parser::syntax_tree(&db, file));
    });
}

fn bench_lexer_only_simple(b: &mut Bencher) {
    let content = r###"
class User {
    id: string
    name: string
    email: string
    posts: Post[]
}

class Post {
    id: string
    title: string
    content: string
    author: User
}

function GetUser(id: string) -> User {
    client GPT4
    prompt #"Get user {{id}}"#
}

client GPT4 {
    provider: "openai"
    model: "gpt-4"
}
"###;

    b.iter(|| {
        let mut db = RootDatabase::new();
        let filename = format!("test{}", BAML_EXT);
        let file = db.add_file(&filename, content);
        let _ = codspeed_bencher_compat::black_box(baml_lexer::lex_file(&db, file));
    });
}

// Include generated benchmarks from build script
include!(concat!(env!("OUT_DIR"), "/generated_benchmarks.rs"));

// Combine all benchmarks into groups
benchmark_group!(
    manual_benches,
    bench_empty_project,
    bench_single_simple_file,
    bench_incremental_simple_change,
    bench_parse_only_simple,
    bench_lexer_only_simple
);

// The generated benchmarks are expected to define their own benchmark_group! macro call
// So we just need to reference them in benchmark_main
benchmark_main!(
    manual_benches,
    generated_incremental_benches,
    generated_scale_benches
);
