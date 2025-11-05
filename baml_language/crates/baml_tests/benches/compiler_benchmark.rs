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
fn bench_incremental_add_field(bencher: Bencher) {
    let initial = r###"
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

    let updated = r###"
class User {
    id: string
    name: string
    email: string  // Added field
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

    bencher
        .with_inputs(|| {
            // Setup: Create and warm up the database
            let mut db = RootDatabase::new();
            let root = db.set_project_root(std::path::PathBuf::from("."));
            let filename = format!("types{}", BAML_EXT);

            // Initial compilation to warm up Salsa
            db.add_file(&filename, initial);
            let _ = baml_hir::project_items(&db, root);

            (db, root, filename)
        })
        .bench_values(|(mut db, root, filename)| {
            // Measure only the incremental update
            db.add_file(&filename, updated);
            let _ = black_box(baml_hir::project_items(&db, root));
        });
}

#[divan::bench]
fn bench_incremental_modify_function(bencher: Bencher) {
    let initial = r###"
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

    let updated = r###"
class User {
    id: string
    name: string
}

function GetUser(id: string) -> User {
    client GPT4
    prompt #"Get user {{id}} with additional details"#  // Modified prompt
}

client GPT4 {
    provider: "openai"
    model: "gpt-4"
}
"###;

    bencher
        .with_inputs(|| {
            // Setup: Create and warm up the database
            let mut db = RootDatabase::new();
            let root = db.set_project_root(std::path::PathBuf::from("."));
            let filename = format!("app{}", BAML_EXT);

            // Initial compilation to warm up Salsa
            db.add_file(&filename, initial);
            let _ = baml_hir::project_items(&db, root);

            (db, root, filename)
        })
        .bench_values(|(mut db, root, filename)| {
            // Measure only the incremental update
            db.add_file(&filename, updated);
            let _ = black_box(baml_hir::project_items(&db, root));
        });
}

#[divan::bench]
fn bench_incremental_add_new_file(bencher: Bencher) {
    let existing_file = r###"
class User {
    id: string
    name: string
}
"###;

    let new_file = r###"
class Post {
    id: string
    title: string
    content: string
    author: User
}

function CreatePost(title: string, content: string) -> Post {
    client GPT4
    prompt #"Create a post with title: {{title}} and content: {{content}}"#
}
"###;

    bencher
        .with_inputs(|| {
            // Setup: Create database with initial file
            let mut db = RootDatabase::new();
            let root = db.set_project_root(std::path::PathBuf::from("."));

            // Add first file and compile
            db.add_file("user.baml", existing_file);
            let _ = baml_hir::project_items(&db, root);

            (db, root)
        })
        .bench_values(|(mut db, root)| {
            // Measure adding a new file to existing project
            db.add_file("post.baml", new_file);
            let _ = black_box(baml_hir::project_items(&db, root));
        });
}

#[divan::bench]
fn bench_incremental_no_change(bencher: Bencher) {
    // This benchmarks the overhead of checking when nothing changed
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

    bencher
        .with_inputs(|| {
            // Setup: Create and compile
            let mut db = RootDatabase::new();
            let root = db.set_project_root(std::path::PathBuf::from("."));

            db.add_file("app.baml", content);
            let _ = baml_hir::project_items(&db, root);

            (db, root)
        })
        .bench_values(|(db, root)| {
            // Measure cost of re-checking when nothing changed
            // Salsa should return memoized results immediately
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
