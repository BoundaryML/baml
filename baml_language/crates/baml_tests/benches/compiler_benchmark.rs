//! BAML Compiler Benchmarks
//!
//! Run with: cargo bench --bench compiler_benchmark

use baml_compiler2_emit::{CompileOptions, OptLevel, generate_project_bytecode};
use baml_db::*;
use baml_project::ProjectDatabase;
use divan::{Bencher, black_box};

fn main() {
    // Run registered benchmarks
    divan::main();
}

/// Force full compilation of a database to bytecode (used in benchmarks).
fn force_compile(db: &ProjectDatabase) {
    let opts = CompileOptions {
        emit_test_cases: false,
    };
    let _ = generate_project_bytecode(db, &opts);
}

// Additional manual benchmarks
const BAML_EXT: &str = ".baml";

#[divan::bench]
fn bench_empty_project(bencher: Bencher) {
    bencher.bench(|| {
        let mut db = ProjectDatabase::new();
        let _ = db.set_project_root(std::path::Path::new("."));
        let _ = black_box(force_compile(&db));
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
        let mut db = ProjectDatabase::new();
        let _root = db.set_project_root(std::path::Path::new("."));
        let filename = format!("test{}", BAML_EXT);
        db.add_file(&filename, content);
        let _ = black_box(force_compile(&db));
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
            let mut db = ProjectDatabase::new();
            let _root = db.set_project_root(std::path::Path::new("."));
            let filename = format!("types{}", BAML_EXT);

            // Initial compilation to warm up Salsa
            db.add_file(&filename, initial);
            let _ = force_compile(&db);

            (db, _root, filename)
        })
        .bench_values(|(mut db, _root, filename)| {
            // Measure only the incremental update
            db.add_file(&filename, updated);
            let _ = black_box(force_compile(&db));
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
            let mut db = ProjectDatabase::new();
            let _root = db.set_project_root(std::path::Path::new("."));
            let filename = format!("app{}", BAML_EXT);

            // Initial compilation to warm up Salsa
            db.add_file(&filename, initial);
            let _ = force_compile(&db);

            (db, _root, filename)
        })
        .bench_values(|(mut db, _root, filename)| {
            // Measure only the incremental update
            db.add_file(&filename, updated);
            let _ = black_box(force_compile(&db));
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
            let mut db = ProjectDatabase::new();
            let _root = db.set_project_root(std::path::Path::new("."));

            // Add first file and compile
            db.add_file("user.baml", existing_file);
            let _ = force_compile(&db);

            (db, _root)
        })
        .bench_values(|(mut db, _root)| {
            // Measure adding a new file to existing project
            db.add_file("post.baml", new_file);
            let _ = black_box(force_compile(&db));
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
            let mut db = ProjectDatabase::new();
            let _root = db.set_project_root(std::path::Path::new("."));

            db.add_file("app.baml", content);
            let _ = force_compile(&db);

            (db, _root)
        })
        .bench_values(|(db, _root)| {
            // Measure cost of re-checking when nothing changed
            // Salsa should return memoized results immediately
            let _ = black_box(force_compile(&db));
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
        let mut db = ProjectDatabase::new();
        let filename = format!("test{}", BAML_EXT);
        let file = db.add_file(&filename, content);
        let _ = black_box(baml_compiler_parser::syntax_tree(&db, file));
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
        let mut db = ProjectDatabase::new();
        let filename = format!("test{}", BAML_EXT);
        let file = db.add_file(&filename, content);
        let _ = black_box(baml_compiler_lexer::lex_file(&db, file));
    });
}

// Include generated benchmarks from build script
include!(concat!(env!("OUT_DIR"), "/generated_benchmarks.rs"));
