use std::path::Path;

use baml_project::ProjectDatabase;

fn add_baml_tree(db: &mut ProjectDatabase, directory: &Path) {
    for entry in std::fs::read_dir(directory).expect("read BAML source directory") {
        let path = entry.expect("read BAML source entry").path();
        if path.is_dir() {
            add_baml_tree(db, &path);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "baml")
        {
            let source = std::fs::read_to_string(&path).expect("read BAML source file");
            db.add_or_update_file(&path, &source);
        }
    }
}

#[test]
fn observe_an_agent_graph_expands_task_and_runner_dispatch() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("baml_src_temp2");
    let mut db = ProjectDatabase::new();
    db.set_project_root(&root);
    add_baml_tree(&mut db, &root);

    let graph = db
        .ast_control_flow_graph("observe_an_agent")
        .expect("build observe_an_agent graph");
    let labels = graph
        .nodes
        .values()
        .map(|node| node.label.as_str())
        .collect::<Vec<_>>();

    assert!(
        labels.contains(&"Dispatch this typed task to the selected execution policy"),
        "Task.run should be visible in the scenario graph"
    );
    assert!(
        labels.contains(&"Run provider steps until the agent stops"),
        "Agent.run should be visible in the scenario graph"
    );
    assert!(
        graph
            .nodes
            .values()
            .any(|node| format!("{:?}", node.node_type) == "Loop"),
        "the Agent.run loop should be visible in the scenario graph"
    );
}
