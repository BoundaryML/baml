//! Recorded function metadata: names and argument-slot layouts published by the
//! real VM → processor → publisher → file path, aligned with captured inputs.
#![cfg(not(target_arch = "wasm32"))]
use std::{collections::HashMap, sync::Arc};

use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder, TelemetryRecording};
use btel_recorder::{
    RecordingConfig,
    proto::{self, function_definition::Resolution},
};
use sys_native::SysOpsExt;

const SOURCE: &str = r#"
class Customer {
    name string
    age int
}
class Greeter {
    prefix string

    function greet(self, customer: Customer) -> string {
        self.prefix + customer.name
    }
}
function Extract(customer: Customer, limit: int = 10) -> int {
    customer.age + limit
}
function Nothing() -> int { 1 }
function main() -> int {
    let c = Customer { name: "Ann", age: 30 };
    let g = Greeter { prefix: "Hi " };
    let greeting = g.greet(c);
    Extract(c) + Nothing()
}
"#;

/// `FunctionArgs` root parameter count from a current-format blob header.
fn captured_slots(root: &std::path::Path, id: proto::SnapshotId) -> u64 {
    let mut bytes = [0; 16];
    bytes[..8].copy_from_slice(&id.low.to_le_bytes());
    bytes[8..].copy_from_slice(&id.high.to_le_bytes());
    let path = btel_file::cas_path(
        &root.join(".baml/btel/cas"),
        btel_snapshot::SnapshotId::from_bytes(bytes),
    );
    let blob = std::fs::read(path).unwrap();
    // magic(8) version(4) id(16) limited(1) objects(4) root tag(1) count(8)
    assert_eq!(blob[33], 1, "inputs use a FunctionArgs root");
    u64::from_le_bytes(blob[34..42].try_into().unwrap())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn recorded_argument_layouts_align_with_captured_input_slots() {
    let root = tempfile::tempdir().unwrap();
    let mut program = baml_db::testing::compile_source(SOURCE);
    for object in &mut program.objects.0 {
        if let bex_vm_types::Object::Function(f) = object
            && matches!(
                f.name.rsplit('.').next(),
                Some("greet" | "Extract" | "Nothing")
            )
        {
            // Exercise real input capture without network/LLM calls.
            f.body_meta = Some(bex_vm_types::FunctionMeta::Llm {
                client: "test".into(),
            });
        }
    }
    let recording = TelemetryRecording::local_files(root.path(), RecordingConfig::default());
    let engine = Arc::new(
        BexEngine::new_with_telemetry_recording(
            program,
            Arc::new(sys_native::SysOps::native()),
            vec![],
            None,
            btel_clock::ClockMode::Monotonic,
            recording,
        )
        .unwrap(),
    );
    let context = FunctionCallContextBuilder::new(sys_types::CallId::next()).build();
    assert_eq!(
        engine
            .call_function("main", vec![], context, true)
            .await
            .unwrap(),
        BexExternalValue::Int(41)
    );
    let directory = engine.telemetry_recording_directory().unwrap().to_owned();
    engine.shutdown().await;
    assert_eq!(engine.telemetry_result(), Some(Ok(())));

    let read = btel_file::read_directory(&directory).unwrap();
    let mut functions = HashMap::new();
    let mut unavailable = 0;
    let mut paths = HashMap::new();
    let mut inputs = Vec::new();
    for file in &read.files {
        assert_eq!(
            file.header.as_ref().unwrap().format_minor,
            btel_settings::encoding::FORMAT_MINOR
        );
        let definitions = file.definitions.as_ref().unwrap();
        for function in &definitions.functions {
            match function.resolution.as_ref().unwrap() {
                Resolution::Metadata(metadata) => {
                    let previous = functions.insert(function.function_id, metadata.clone());
                    assert!(previous.is_none(), "metadata is published once");
                }
                Resolution::Unavailable(_) => unavailable += 1,
            }
        }
        for path in &definitions.call_paths {
            paths.insert(path.call_path_id, path.callee_function_id);
        }
        for section in &file.spans.as_ref().unwrap().sections {
            for event in &section.events {
                if let Some(proto::span_event::Event::FunctionAnnouncement(a)) = &event.event {
                    inputs.push((a.call_path_id, a.inputs_cas_id.unwrap()));
                }
            }
        }
    }
    assert_eq!(unavailable, 0, "every compile-time callee resolves");
    let by_name = |name: &str| {
        functions
            .values()
            .find(|m| m.display_name == name)
            .unwrap_or_else(|| panic!("{name} metadata"))
    };
    assert_eq!(by_name("main").fqn, "user.main");
    let names = |name: &str| {
        by_name(name).argument_layout.as_ref().map(|layout| {
            layout
                .slots
                .iter()
                .map(|slot| (slot.name.clone(), slot.receiver))
                .collect::<Vec<_>>()
        })
    };
    assert_eq!(
        names("Extract"),
        Some(vec![
            (Some("customer".into()), false),
            (Some("limit".into()), false)
        ])
    );
    assert_eq!(names("Nothing"), Some(vec![]), "known empty layout");
    let greet = names("greet").expect("method layout");
    assert_eq!(greet.len(), 2);
    assert_eq!(greet[0], (Some("self".into()), true), "receiver slot first");
    assert_eq!(greet[1], (Some("customer".into()), false));

    // Every captured input's slot count equals its recorded layout length.
    assert_eq!(inputs.len(), 3);
    for (path, id) in inputs {
        let function = &functions[&paths[&path]];
        let layout = function.argument_layout.as_ref().unwrap();
        assert_eq!(
            captured_slots(root.path(), id),
            layout.slots.len() as u64,
            "{}",
            function.fqn
        );
    }
}
