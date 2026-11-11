//! Complete production test registry through VM and RT, isolated per admitted leaf.
#[test]
#[ignore = "builds a native pack and runs the complete BAML corpus twice under RT"]
fn rt_corpus() {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("rust_diff/rt_corpus.py");
    let status = std::process::Command::new("uv")
        .args(["run", "--no-project"])
        .arg(script)
        .status()
        .expect("uv executes the corpus gate");
    assert!(
        status.success(),
        "RT corpus gate failed; see the printed evidence directory"
    );
}
