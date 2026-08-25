//! Compile a project dir (or just the stdlib) and print compiler2
//! diagnostics, instead of `bex_project`'s `build.rs` panic.
#![allow(clippy::print_stdout)]

use std::path::Path;

use baml_project::ProjectDatabase;

fn add_dir(db: &mut ProjectDatabase, root: &Path, dir: &Path) {
    for entry in std::fs::read_dir(dir).expect("read_dir") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            add_dir(db, root, &path);
        } else if path.extension().and_then(|e| e.to_str()) == Some("baml") {
            let rel = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let content = std::fs::read_to_string(&path).expect("read");
            db.add_file(&rel, &content);
        }
    }
}

fn main() {
    let mut db = ProjectDatabase::new();
    db.set_project_root(Path::new("."));
    if let Some(root) = std::env::args().nth(1) {
        let root = Path::new(&root).join("baml_src");
        let r = root.clone();
        add_dir(&mut db, &r, &root);
    } else {
        db.add_file("main.baml", "function main() -> int { 1 }");
    }
    let diags = baml_project::collect_compiler2_diagnostics(&db);
    for d in diags.iter().take(60) {
        println!("{d:?}");
    }
    println!("total diagnostics: {}", diags.len());
    println!("running stdlib bytecode generation...");
    let program =
        baml_compiler2_emit::generate_stdlib_program(&db, baml_compiler2_emit::OptLevel::default());
    match program {
        Ok(_) => println!("stdlib bytecode: OK"),
        Err(e) => println!("stdlib bytecode error: {e:?}"),
    }
}
