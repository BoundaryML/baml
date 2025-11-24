//! Test that verifies the AstId-based incrementality system works correctly.

use baml_db::{RootDatabase, baml_hir};
use salsa::Setter;

/// Helper to find function by name in a file
fn find_function<'db>(
    db: &'db RootDatabase,
    file: baml_db::SourceFile,
    name: &str,
) -> baml_hir::FunctionLoc<'db> {
    let items = baml_hir::file_items(db, file);
    let all_items = items.items(db);

    all_items
        .iter()
        .find_map(|item| match item {
            baml_hir::ItemId::Function(f) => {
                let tree = baml_hir::file_item_tree(db, file);
                let func = &tree[f.id(db)];
                if func.name.as_str() == name {
                    Some(*f)
                } else {
                    None
                }
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("{} not found", name))
}

/// Test that editing one function's body doesn't cause other function bodies to be re-lowered.
///
/// This uses Salsa event tracking (like rust-analyzer) to verify that only the
/// changed function's body query re-runs.
#[test]
fn test_function_body_incrementality_with_events() {
    use std::collections::HashSet;
    use std::sync::{Arc as StdArc, Mutex};

    let executed_queries = StdArc::new(Mutex::new(Vec::new()));
    let executed_queries_clone = StdArc::clone(&executed_queries);

    let mut db = RootDatabase::new_with_event_callback(Box::new(move |event| {
        if let salsa::EventKind::WillExecute { database_key } = event.kind {
            executed_queries_clone
                .lock()
                .unwrap()
                .push(format!("{:?}", database_key));
        }
    }));

    // Create initial file with two functions
    let source = concat!(
        "function GetUser() -> string {\n",
        "  client GPT4\n",
        "  prompt \"Get user info\"\n",
        "}\n",
        "\n",
        "function GetProduct() -> string {\n",
        "  client GPT4\n",
        "  prompt \"Get product info\"\n",
        "}\n",
    );
    let file = db.add_file("test.baml", source);

    // Query both bodies initially to populate cache
    {
        let get_user_id = find_function(&db, file, "GetUser");
        let get_product_id = find_function(&db, file, "GetProduct");

        executed_queries.lock().unwrap().clear();

        let _ = baml_db::function_body(&db, get_user_id);
        let _ = baml_db::function_body(&db, get_product_id);
    }

    // Edit ONLY GetProduct's body
    let modified_source = concat!(
        "function GetUser() -> string {\n",
        "  client GPT4\n",
        "  prompt \"Get user info\"\n", // ← Unchanged
        "}\n",
        "\n",
        "function GetProduct() -> string {\n",
        "  client GPT4\n",
        "  prompt \"CHANGED: Get different product info\"\n", // ← Changed
        "}\n",
    );

    executed_queries.lock().unwrap().clear();
    file.set_text(&mut db).to(modified_source.to_string());

    // Query both bodies again
    {
        let get_user_id = find_function(&db, file, "GetUser");
        let get_product_id = find_function(&db, file, "GetProduct");

        executed_queries.lock().unwrap().clear();

        let _ = baml_db::function_body(&db, get_user_id);
        let _ = baml_db::function_body(&db, get_product_id);
    }

    let queries = executed_queries.lock().unwrap();
    let _query_set: HashSet<_> = queries.iter().map(|s| s.as_str()).collect();

    eprintln!("\nQueries executed after edit:");
    for q in queries.iter() {
        eprintln!("  {}", q);
    }

    // Check that function_body_impl was only called for GetProduct, not GetUser
    let user_body_queries: Vec<_> = queries
        .iter()
        .filter(|q| q.contains("function_body_impl") && q.contains("GetUser"))
        .collect();
    let product_body_queries: Vec<_> = queries
        .iter()
        .filter(|q| q.contains("function_body_impl") && q.contains("GetProduct"))
        .collect();

    assert!(
        user_body_queries.is_empty(),
        "GetUser's body should NOT be re-lowered when only GetProduct changed"
    );
    assert!(
        !product_body_queries.is_empty(),
        "GetProduct's body SHOULD be re-lowered when it changed"
    );

    println!("✓ Function body incrementality test passed!");
    println!("  GetUser body was NOT re-lowered (cached)");
    println!("  GetProduct body WAS re-lowered (as expected)");
}

/// Test that FunctionBodyText interning works correctly.
#[test]
fn test_body_text_interning() {
    let mut db = RootDatabase::new();

    // Create file with one function
    let source1 = "function GetUser() -> string {\n  client GPT4\n  prompt \"Get user info\"\n}\n";
    let file = db.add_file("test.baml", source1);

    // Get function and body text
    let text_1_str = {
        let func_id_1 = find_function(&db, file, "GetUser");
        let body_text_1 = baml_db::function_body_text(&db, func_id_1);
        let text = body_text_1.text(&db).clone();
        eprintln!("Body text 1: {:?}", text);
        text
    };

    // Edit file - change something OUTSIDE the function body
    let source2 = "// Added comment\nfunction GetUser() -> string {\n  client GPT4\n  prompt \"Get user info\"\n}\n";
    file.set_text(&mut db).to(source2.to_string());

    // Get function and body text again
    let text_2_str = {
        let func_id_2 = find_function(&db, file, "GetUser");
        let body_text_2 = baml_db::function_body_text(&db, func_id_2);
        let text = body_text_2.text(&db).clone();
        eprintln!("Body text 2: {:?}", text);
        text
    };

    // The body text strings should be THE SAME
    assert_eq!(
        text_1_str, text_2_str,
        "Body text should be same when body unchanged"
    );

    println!("✓ Body text interning works!");
}
