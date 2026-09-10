#![cfg(not(target_arch = "wasm32"))]

use std::sync::{Arc, atomic::AtomicBool};

use baml_db::testing::compile_source;
use bex_vm::{BexVm, VmExecState};
use bex_vm_types::{Object, Value};

const MAX_EXEC_CALLS: usize = 64;

#[test]
fn log_payload_preserves_null_and_named_events() {
    let program = compile_source(
        r#"
function main() -> int {
    log.info(11)
    log.debug(22, event_name = null)
    log.warn(event_name = "checkout", data = 33)
    log.error(data = 44, event_name = "$baml_log")
    7
}
"#,
    );
    let entry_index = program.function_index("user.main").expect("main");
    let mut vm =
        BexVm::from_program(program, Arc::new(AtomicBool::new(false))).expect("from_program");
    let entry_ptr = vm.heap.compile_time_ptr(entry_index);
    vm.set_entry_point(entry_ptr, &[]);
    let expected = [
        ("info", 11, None),
        ("debug", 22, None),
        ("warn", 33, Some("checkout")),
        ("error", 44, Some("$baml_log")),
    ];
    let mut seen = 0;
    for _ in 0..MAX_EXEC_CALLS {
        match vm.exec().expect("exec") {
            VmExecState::Event {
                event_name, data, ..
            } => {
                assert_eq!(event_name, "$baml_log");
                let Object::Map(map) = vm.get_object(data.as_object_ptr().expect("payload map"))
                else {
                    panic!("log payload must be a map");
                };
                let fields = map.to_index_map();
                assert_eq!(fields.len(), 3);
                let (level, body, name) = expected[seen];
                for (key, value) in fields {
                    match key.to_string().as_str() {
                        "data" => assert_eq!(value.as_int(), Some(body)),
                        "level" => {
                            let Object::String(actual) =
                                vm.get_object(value.as_object_ptr().expect("level string"))
                            else {
                                panic!("level must be a string");
                            };
                            assert_eq!(actual.as_str(), level);
                        }
                        "event_name" => match name {
                            None => assert_eq!(value, Value::NULL),
                            Some(name) => {
                                let Object::String(actual) = vm
                                    .get_object(value.as_object_ptr().expect("event name string"))
                                else {
                                    panic!("event name must be a string");
                                };
                                assert_eq!(actual.as_str(), name);
                            }
                        },
                        other => panic!("unexpected log field: {other}"),
                    }
                }
                seen += 1;
                vm.stack.push(Value::NULL);
            }
            VmExecState::EarlyYield => {}
            VmExecState::Complete(value) => {
                assert_eq!(seen, expected.len());
                assert_eq!(value.as_int(), Some(7));
                return;
            }
            other => panic!("unexpected VM state: {other:?}"),
        }
    }
    panic!("vm did not finish log payload test");
}

#[test]
fn log_event_source_uses_unknown_column_and_real_offsets() {
    let source = "function main() -> int {\n    log.info(\"offset-probe\");\n    7\n}\n";
    let call_start = u32::try_from(source.find("log.info").expect("source contains log call"))
        .expect("test source offset fits in u32");

    let program = compile_source(source);
    let entry_index = program
        .function_index("user.main")
        .expect("user.main function emitted");
    let mut vm =
        BexVm::from_program(program, Arc::new(AtomicBool::new(false))).expect("from_program");
    let entry_ptr = vm.heap.compile_time_ptr(entry_index);
    vm.set_entry_point(entry_ptr, &[]);

    for _ in 0..MAX_EXEC_CALLS {
        match vm.exec().expect("exec") {
            VmExecState::Event {
                event_name,
                source_location,
                ..
            } => {
                assert_eq!(event_name, "$baml_log");
                let source_location =
                    source_location.expect("log event should carry source location");

                assert_ne!(source_location.file_id, u32::MAX);
                assert_eq!(source_location.line, 2);
                assert_eq!(source_location.column, 0);
                assert_ne!(
                    source_location.column, source_location.start_offset,
                    "column must be the unknown-column sentinel, not the byte offset"
                );
                assert!(source_location.start_offset < source_location.end_offset);
                assert!(
                    source_location.start_offset <= call_start
                        && call_start < source_location.end_offset,
                    "source span {}..{} should cover log.info at {call_start}",
                    source_location.start_offset,
                    source_location.end_offset,
                );
                return;
            }
            VmExecState::EarlyYield => {}
            other => panic!("unexpected VM state before log event: {other:?}"),
        }
    }

    panic!("vm did not emit log event within {MAX_EXEC_CALLS} exec() calls");
}
