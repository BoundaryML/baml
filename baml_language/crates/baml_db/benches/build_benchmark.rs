use baml_db::{RootDatabase, baml_hir};
use codspeed_bencher_compat::{Bencher, benchmark_group, benchmark_main};
use std::hint::black_box;

fn bench_compile_empty(b: &mut Bencher) {
    b.iter(|| {
        let mut db = RootDatabase::new();
        let root = db.set_project_root(std::path::PathBuf::from("."));
        black_box(root);
    });
}

fn bench_compile_single_file(b: &mut Bencher) {
    let content = r#"
        class User {
            name string
            age int
        }

        function GetUser() -> User {
            prompt "Return a user object"
        }
    "#;

    b.iter(|| {
        let mut db = RootDatabase::new();
        let root = db.set_project_root(std::path::PathBuf::from("."));
        db.add_file("test.baml", content);
        // Force compilation by accessing HIR
        let _ = black_box(baml_hir::project_items(&db, root));
    });
}

fn bench_compile_multiple_files(b: &mut Bencher) {
    let file1 = r#"
        class User {
            name string
            age int
            email string
        }
    "#;

    let file2 = r#"
        class Product {
            id string
            name string
            price float
        }
    "#;

    let file3 = r#"
        function GetUser() -> User {
            prompt "Return a user object"
        }

        function GetProduct() -> Product {
            prompt "Return a product object"
        }
    "#;

    b.iter(|| {
        let mut db = RootDatabase::new();
        let root = db.set_project_root(std::path::PathBuf::from("."));
        db.add_file("user.baml", file1);
        db.add_file("product.baml", file2);
        db.add_file("functions.baml", file3);
        // Force full compilation
        let _ = black_box(baml_hir::project_items(&db, root));
    });
}

benchmark_group!(
    baml_db_benches,
    bench_compile_empty,
    bench_compile_single_file,
    bench_compile_multiple_files
);

benchmark_main!(baml_db_benches);
