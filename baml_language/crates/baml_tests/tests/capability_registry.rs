//! LLM capability registry (`_plan/llm-desugar-capabilities-plan.md` §1.2) —
//! collection tests: `//baml:llm_capability` interfaces and
//! `//baml:llm_companion(<suffix>)` drivers unioned across files.

use baml_compiler2_hir::capability_registry::{CapabilityRegistry, capability_registry};
use baml_project::ProjectDatabase;

fn registry_for(files: &[(&str, &str)]) -> CapabilityRegistry {
    let mut db = ProjectDatabase::new();
    let _root = db.set_project_root(std::path::Path::new("."));
    for (path, source) in files {
        db.add_file(path, source);
    }
    capability_registry(&db)
}

/// Entries contributed by user files only — keeps these tests stable as
/// stdlib capabilities get marked.
fn user_entries(reg: &CapabilityRegistry) -> (Vec<String>, Vec<(String, String, usize)>) {
    let caps = reg
        .capabilities
        .iter()
        .filter(|c| c.package.as_str() == "user")
        .map(|c| c.name.as_str().to_string())
        .collect();
    let drivers = reg
        .drivers
        .iter()
        .filter(|d| d.package.as_str() == "user")
        .map(|d| {
            (
                d.suffix.as_str().to_string(),
                d.function.as_str().to_string(),
                d.generic_arity,
            )
        })
        .collect();
    (caps, drivers)
}

#[test]
fn collects_marked_interface_and_driver() {
    let reg = registry_for(&[(
        "main.baml",
        r#"
//baml:llm_capability
interface Moderated {
  function call_moderated(self, policy: string) -> string
}

//baml:llm_companion(moderated)
function drive_moderated<T>(client: string, prompt: string, policy: string) -> string {
  let p = prompt;
  p
}
"#,
    )]);
    let (caps, drivers) = user_entries(&reg);
    assert_eq!(caps, vec!["Moderated".to_string()]);
    assert_eq!(
        drivers,
        vec![("moderated".to_string(), "drive_moderated".to_string(), 1)]
    );
}

#[test]
fn unions_across_files_and_records_arity() {
    let reg = registry_for(&[
        (
            "caps.baml",
            r#"
//baml:llm_capability
interface Streamy {
  function stream_it(self) -> string
}
"#,
        ),
        (
            "drivers.baml",
            r#"
//baml:llm_companion(streamy)
function drive_streamy<TPartial, T>(client: string, prompt: string) -> string {
  let p = prompt;
  p
}
"#,
        ),
    ]);
    let (caps, drivers) = user_entries(&reg);
    assert_eq!(caps, vec!["Streamy".to_string()]);
    assert_eq!(
        drivers,
        vec![("streamy".to_string(), "drive_streamy".to_string(), 2)]
    );
    let d = reg.driver_for_suffix("streamy").expect("suffix registered");
    assert_eq!(d.function.as_str(), "drive_streamy");
    assert_eq!(d.generic_arity, 2);
}

#[test]
fn unmarked_items_contribute_nothing() {
    let reg = registry_for(&[(
        "main.baml",
        r#"
interface Plain {
  function f(self) -> string
}

function ordinary(x: string) -> string {
  x
}
"#,
    )]);
    let (caps, drivers) = user_entries(&reg);
    assert!(caps.is_empty(), "no capability expected, got {caps:?}");
    assert!(drivers.is_empty(), "no driver expected, got {drivers:?}");
    assert!(reg.driver_for_suffix("anything").is_none());
}
