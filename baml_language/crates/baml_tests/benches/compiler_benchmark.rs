//! BAML Compiler Benchmarks
//!
//! Run with: cargo bench --bench compiler_benchmark

use baml_db::*;
use divan::{Bencher, black_box};

fn main() {
    // Run registered benchmarks
    divan::main();
}

// Additional manual benchmarks
const BAML_EXT: &str = ".baml";

#[divan::bench]
fn bench_empty_project(bencher: Bencher) {
    bencher.bench(|| {
        let mut db = RootDatabase::new();
        let root = db.set_project_root(std::path::PathBuf::from("."));
        let _ = black_box(baml_hir::project_items(&db, root));
    });
}

#[divan::bench]
fn bench_single_simple_file(bencher: Bencher) {
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

    bencher.bench_local(|| {
        let mut db = RootDatabase::new();
        let root = db.set_project_root(std::path::PathBuf::from("."));
        let filename = format!("test{}", BAML_EXT);
        db.add_file(&filename, content);
        let _ = black_box(baml_hir::project_items(&db, root));
    });
}

#[divan::bench]
fn bench_incremental_simple_change(bencher: Bencher) {
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

    bencher.bench_local(|| {
        let mut db = RootDatabase::new();
        let root = db.set_project_root(std::path::PathBuf::from("."));
        let filename = format!("types{}", BAML_EXT);

        // Initial compilation
        db.add_file(&filename, initial);
        let _ = baml_hir::project_items(&db, root);

        // Simulate incremental update by adding the same file again
        // In Salsa, this should trigger incremental recompilation
        db.add_file(&filename, updated);
        let _ = black_box(baml_hir::project_items(&db, root));
    });
}

#[divan::bench]
fn bench_parse_only_simple(bencher: Bencher) {
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

    bencher.bench_local(|| {
        let mut db = RootDatabase::new();
        let filename = format!("test{}", BAML_EXT);
        let file = db.add_file(&filename, content);
        let _ = black_box(baml_parser::syntax_tree(&db, file));
    });
}

#[divan::bench]
fn bench_lexer_only_simple(bencher: Bencher) {
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

    bencher.bench_local(|| {
        let mut db = RootDatabase::new();
        let filename = format!("test{}", BAML_EXT);
        let file = db.add_file(&filename, content);
        let _ = black_box(baml_lexer::lex_file(&db, file));
    });
}

// Include generated benchmarks from build script
include!(concat!(env!("OUT_DIR"), "/generated_benchmarks.rs"));
