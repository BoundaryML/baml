use std::{collections::HashMap, sync::Arc, time::Duration};

use bex_engine::{BexEngine, BexExternalValue as External, EngineError, TelemetryRecording};
use bex_vm_types::Program;
use btel_recorder::{CompletionFlags, RecordingConfig, proto};
use btel_snapshot::{
    Builder, Limits, Snapshot, SnapshotObject, SnapshotPool, SnapshotValue as Value,
};
use sys_native::SysOpsExt;

fn capture_id(id: &proto::SnapshotId) -> btel_snapshot::SnapshotId {
    let mut digest = [0; 16];
    digest[..8].copy_from_slice(&id.low.to_le_bytes());
    digest[8..].copy_from_slice(&id.high.to_le_bytes());
    btel_snapshot::SnapshotId::from_bytes(digest)
}

struct Recording {
    directory: tempfile::TempDir,
    spans: Vec<(u64, proto::FunctionAnnouncement, proto::FunctionCompletion)>,
    threads: HashMap<u64, proto::ThreadDefinition>,
}

impl Recording {
    async fn run(
        program: &Program,
        function: &str,
        args: Vec<External>,
    ) -> (Result<External, EngineError>, Self) {
        Self::run_with_cancellation(program, function, args, false).await
    }

    async fn run_with_cancellation(
        program: &Program,
        function: &str,
        args: Vec<External>,
        cancel_after_log: bool,
    ) -> (Result<External, EngineError>, Self) {
        let directory = tempfile::tempdir().unwrap();
        let recording =
            TelemetryRecording::local_files_in(directory.path(), RecordingConfig::default());
        let recording_id = recording.id();
        let engine = Arc::new(
            BexEngine::new_with_telemetry_recording(
                program.clone(),
                Arc::new(sys_native::SysOps::native()),
                vec![],
                None,
                btel_clock::ClockMode::Monotonic,
                recording,
            )
            .unwrap(),
        );
        let result = if cancel_after_log {
            let cancel = bex_engine::CancellationToken::new();
            let logger = bex_engine::logger::TraceLogger::bounded(1);
            let context = bex_engine::FunctionCallContextBuilder::new(sys_types::CallId::next())
                .with_cancel_token(cancel.clone())
                .with_logger(logger.clone())
                .build();
            let execution = Box::pin(engine.call_function(function, args, context, true));
            let cancellation = async {
                while logger.stats().published == 0 {
                    tokio::task::yield_now().await;
                }
                cancel.cancel();
            };
            tokio::time::timeout(Duration::from_secs(10), async {
                let (result, ()) = tokio::join!(execution, cancellation);
                result
            })
            .await
            .expect("callee must enter before cancellation")
        } else {
            engine
                .call_function(function, args, super::context(), true)
                .await
        };
        tokio::time::timeout(Duration::from_secs(10), engine.shutdown())
            .await
            .unwrap();
        let mut entries = HashMap::new();
        let mut completed = Vec::new();
        let mut threads = HashMap::new();
        if let Some(path) = engine.telemetry_recording_directory() {
            assert_eq!(engine.telemetry_result(), Some(Ok(())));
            let files = btel_file::read_directory(path).unwrap().files;
            engine.shutdown().await;
            assert_eq!(
                btel_file::read_directory(path).unwrap().files.len(),
                files.len()
            );
            for file in files {
                assert_eq!(file.header.unwrap().recording_id, recording_id.as_bytes());
                for thread in file.definitions.unwrap().threads {
                    threads.insert(thread.thread_id, thread);
                }
                for section in file.spans.unwrap().sections {
                    for event in section.events {
                        match event.event.unwrap() {
                            proto::span_event::Event::FunctionAnnouncement(entry) => {
                                assert!(
                                    entries
                                        .insert(entry.id, (section.thread_id, entry))
                                        .is_none()
                                );
                            }
                            proto::span_event::Event::FunctionCompletion(done) => {
                                completed.push((section.thread_id, done));
                            }
                            _ => {}
                        }
                    }
                }
            }
        } else {
            assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
        }
        let spans = completed
            .into_iter()
            .map(|(thread, done)| {
                let (entry_thread, entry) = entries.remove(&done.id).expect("span announcement");
                assert_ne!(done.id, 0);
                assert_eq!(thread, entry_thread);
                assert_eq!(entry.parent_id, done.parent_id);
                assert_eq!(entry.entered_at_ticks, done.entered_at_ticks);
                assert_eq!(u64::from(entry.call_path_id), done.node >> 1);
                let flags = CompletionFlags::from_wire(done.completion_flags, false).unwrap();
                if entry.inputs_cas_id.is_some() {
                    assert!(flags.requires_announcement());
                }
                (thread, entry, done)
            })
            .collect();
        assert!(
            entries.is_empty(),
            "every announced invocation must complete"
        );
        drop(engine);
        let recording = Self {
            directory,
            spans,
            threads,
        };
        for (_, entry, done) in &recording.spans {
            for id in entry.inputs_cas_id.iter().chain(done.value_cas_id.iter()) {
                recording.blob(id);
            }
        }
        (result, recording)
    }

    fn blob(&self, id: &proto::SnapshotId) -> Vec<u8> {
        let id = capture_id(id);
        let path = btel_file::cas_path(&self.directory.path().join("cas"), id);
        let bytes = std::fs::read(path).unwrap();
        assert_eq!(&bytes[..8], &btel_snapshot::BLOB_MAGIC);
        assert_eq!(&bytes[8..12], &btel_snapshot::BLOB_VERSION.to_le_bytes());
        assert_eq!(&bytes[12..28], id.as_bytes());
        bytes
    }

    #[track_caller]
    fn assert_capture(&self, actual: Option<&proto::SnapshotId>, expected: Option<&Snapshot>) {
        match (actual, expected) {
            (None, None) => {}
            (Some(actual), Some(expected)) => {
                assert_eq!(capture_id(actual), expected.id());
                let mut expected_bytes = Vec::new();
                expected.write_blob(&mut expected_bytes).unwrap();
                assert_eq!(self.blob(actual), expected_bytes);
            }
            _ => panic!(
                "capture presence differs: actual={}, expected={}",
                actual.is_some(),
                expected.is_some()
            ),
        }
    }
}

fn snapshot(args: bool, values: &[External]) -> Snapshot {
    let pool = SnapshotPool::new(1, Limits::default());
    let mut builder = pool.try_acquire().unwrap();
    let roots: Vec<_> = values
        .iter()
        .map(|value| scalar(&mut builder, value))
        .collect();
    finish(builder, args, &roots)
}

fn finish(mut builder: Builder, args: bool, roots: &[Value]) -> Snapshot {
    if args {
        let start = builder.value_start();
        for root in roots {
            builder.push_value(*root);
        }
        let slots = builder.value_range(start);
        builder.finish_args(roots.len(), slots)
    } else {
        assert_eq!(roots.len(), 1);
        builder.finish_value(roots[0])
    }
}

fn scalar(builder: &mut Builder, value: &External) -> Value {
    match value {
        External::Null => Value::Null,
        External::Bool(value) => Value::Bool(*value),
        External::Int(value) => Value::Int(*value),
        External::Float(value) => Value::Float(*value),
        External::String(value) => Value::String(builder.string(value).unwrap()),
        External::Bigint(value) => Value::Bigint(builder.bigint(&Arc::new(value.clone())).unwrap()),
        External::Uint8Array(value) => {
            let object = builder.reserve_object().unwrap();
            let data = builder.copy_bytes(value);
            builder.set_object(
                object,
                SnapshotObject::Bytes {
                    data,
                    original_len: value.len(),
                },
            );
            Value::Object(object)
        }
        External::RustData(_) => {
            let object = builder.reserve_object().unwrap();
            builder.set_object(object, SnapshotObject::NonSnapshotableValue {});
            Value::Object(object)
        }
        _ => panic!("expected a scalar fixture value"),
    }
}

fn flag(value: Option<bool>) -> External {
    value.map_or(External::Null, External::Bool)
}

fn text(value: &str) -> External {
    External::String(value.into())
}

async fn capture_flags(program: &Program) {
    for inputs in [None, Some(false), Some(true)] {
        for output in [None, Some(false), Some(true)] {
            for error in [None, Some(false), Some(true)] {
                for reserved in [false, true] {
                    for fail in [false, true] {
                        let value = text(&format!(
                            "{inputs:?}/{output:?}/{error:?}/{reserved}/{fail}"
                        ));
                        let (result, recording) = Recording::run(
                            program,
                            "matrix_case",
                            vec![
                                value.clone(),
                                flag(inputs),
                                flag(output),
                                flag(error),
                                External::Bool(reserved),
                                External::Bool(fail),
                            ],
                        )
                        .await;
                        assert_eq!(result.is_err(), fail);
                        assert_eq!(recording.spans.len(), 1);
                        let (_, entry, done) = &recording.spans[0];
                        let args = snapshot(true, std::slice::from_ref(&value));
                        let returned = snapshot(false, std::slice::from_ref(&value));
                        recording.assert_capture(
                            entry.inputs_cas_id.as_ref(),
                            (inputs == Some(true)).then_some(&args),
                        );
                        recording.assert_capture(
                            done.value_cas_id.as_ref(),
                            (if fail { error } else { output } == Some(true)).then_some(&returned),
                        );
                        assert_eq!(
                            CompletionFlags::from_wire(done.completion_flags, false)
                                .unwrap()
                                .outcome(),
                            if fail {
                                btel_types::InvocationOutcome::Errored
                            } else {
                                btel_types::InvocationOutcome::Ok
                            },
                        );
                    }
                }
            }
        }
    }
}

async fn scalar_values(program: &Program) {
    let values = [
        External::Null,
        External::Bool(false),
        External::Bool(true),
        External::Int(bex_vm_types::Value::INT_MIN),
        External::Int(0),
        External::Int(bex_vm_types::Value::INT_MAX),
        External::Float(0.0),
        External::Float(-0.0),
        External::Float(1.5),
        External::Float(f64::INFINITY),
        External::Float(f64::NEG_INFINITY),
        External::Float(f64::from_bits(0x7ff8_0000_0000_1234)),
        text(""),
        text("quote=\" slash=\\ newline=\n tab=\t"),
        text("unicode=\u{1f600}\u{4e2d}\u{e9}"),
        External::String("long-value".repeat(8192).into()),
        External::Bigint("123456789012345678901234567890".parse().unwrap()),
        External::Bigint("-123456789012345678901234567890".parse().unwrap()),
        External::Bigint(0.into()),
        External::Uint8Array(vec![]),
        External::Uint8Array(vec![0, 127, 255]),
    ];
    for value in values {
        for fail in [false, true] {
            let (result, recording) = Recording::run(
                program,
                "matrix_case",
                vec![
                    value.clone(),
                    External::Bool(true),
                    External::Bool(true),
                    External::Bool(true),
                    External::Bool(false),
                    External::Bool(fail),
                ],
            )
            .await;
            assert_eq!(result.is_err(), fail, "value={value:?}, result={result:?}");
            assert_eq!(recording.spans.len(), 1);
            let (_, entry, done) = &recording.spans[0];
            recording.assert_capture(
                entry.inputs_cas_id.as_ref(),
                Some(&snapshot(true, std::slice::from_ref(&value))),
            );
            recording.assert_capture(
                done.value_cas_id.as_ref(),
                Some(&snapshot(false, std::slice::from_ref(&value))),
            );
        }
    }
}

fn graph_snapshot(program: &Program, scenario: &str, args: bool, value: i64) -> Snapshot {
    use baml_type::RealizedTy;
    use bex_vm_types::Object;
    use btel_snapshot::Range;

    let pool = SnapshotPool::new(1, Limits::default());
    let mut b = pool.try_acquire().unwrap();
    let object = b.reserve_object().unwrap();
    let root = match scenario {
        "cycle" | "node" => {
            let class = program
                .objects
                .0
                .iter()
                .find_map(|object| match object {
                    Object::Class(class) if class.name.display_name().ends_with("MatrixNode") => {
                        Some(class)
                    }
                    _ => None,
                })
                .unwrap();
            let declaration = b.reserve_object().unwrap();
            b.set_object(
                declaration,
                SnapshotObject::Declaration {
                    name: class.name.clone(),
                    tag: class.type_tag,
                    is_enum: false,
                },
            );
            let start = b.entry_start();
            b.entry(&"value".into(), Value::Int(value));
            b.entry(
                &"next".into(),
                if scenario == "cycle" {
                    Value::Object(object)
                } else {
                    Value::Null
                },
            );
            let fields = b.entry_range(start);
            b.set_object(
                object,
                SnapshotObject::Instance {
                    type_arguments: Range::empty(),
                    declaration,
                    fields,
                    original_len: 2,
                },
            );
            Value::Object(object)
        }
        "enum" => {
            let enm = program
                .objects
                .0
                .iter()
                .find_map(|object| match object {
                    Object::Enum(enm) if enm.name.display_name().ends_with("MatrixChoice") => {
                        Some(enm)
                    }
                    _ => None,
                })
                .unwrap();
            b.set_object(
                object,
                SnapshotObject::Declaration {
                    name: enm.name.clone(),
                    tag: enm.type_tag,
                    is_enum: true,
                },
            );
            Value::Enum {
                declaration: object,
                variant: 1,
                name: b.string(&"Second".into()).unwrap(),
            }
        }
        "aliases" => {
            let shared = b.reserve_object().unwrap();
            let outer_type = b.push_type(RealizedTy::List(Box::new(RealizedTy::Int)));
            let inner_type = b.push_type(RealizedTy::Int);
            let start = b.value_start();
            b.push_value(Value::Object(shared));
            b.push_value(Value::Object(shared));
            let items = b.value_range(start);
            b.set_object(
                object,
                SnapshotObject::List {
                    element_type: outer_type,
                    items,
                    original_len: 2,
                },
            );
            let start = b.value_start();
            b.push_value(Value::Int(1));
            b.push_value(Value::Int(2));
            let items = b.value_range(start);
            b.set_object(
                shared,
                SnapshotObject::List {
                    element_type: inner_type,
                    items,
                    original_len: 2,
                },
            );
            Value::Object(object)
        }
        "empty_array" => {
            let element_type = b.push_type(RealizedTy::Int);
            b.set_object(
                object,
                SnapshotObject::List {
                    element_type,
                    items: Range::empty(),
                    original_len: 0,
                },
            );
            Value::Object(object)
        }
        "generic" => {
            let class = program
                .objects
                .0
                .iter()
                .find_map(|object| match object {
                    Object::Class(class) if class.name.display_name().ends_with("MatrixBox") => {
                        Some(class)
                    }
                    _ => None,
                })
                .unwrap();
            let declaration = b.reserve_object().unwrap();
            b.set_object(
                declaration,
                SnapshotObject::Declaration {
                    name: class.name.clone(),
                    tag: class.type_tag,
                    is_enum: false,
                },
            );
            let start = b.type_start();
            b.push_type(RealizedTy::Int);
            let type_arguments = b.type_range(start);
            let start = b.entry_start();
            b.entry(&"value".into(), Value::Int(7));
            let fields = b.entry_range(start);
            b.set_object(
                object,
                SnapshotObject::Instance {
                    type_arguments,
                    declaration,
                    fields,
                    original_len: 1,
                },
            );
            Value::Object(object)
        }
        "map" => {
            let key_type = b.push_type(RealizedTy::String);
            let value_type = b.push_type(RealizedTy::Int);
            let start = b.entry_start();
            if value != 0 {
                b.entry(&"first".into(), Value::Int(1));
                b.entry(&"second".into(), Value::Int(2));
            }
            let entries = b.entry_range(start);
            b.set_object(
                object,
                SnapshotObject::Map {
                    key_type,
                    value_type,
                    entries,
                    original_len: if value == 0 { 0 } else { 2 },
                },
            );
            Value::Object(object)
        }
        _ => unreachable!(),
    };
    finish(b, args, &[root])
}

async fn arguments_and_graphs(program: &Program) {
    let pool = SnapshotPool::new(1, Limits::default());
    let omitted = finish(pool.try_acquire().unwrap(), true, &[Value::OmittedArg]);
    for (function, expected_args, expected_result) in [
        ("matrix_omitted", omitted, text("default")),
        (
            "matrix_explicit_null",
            snapshot(true, &[External::Null]),
            External::Null,
        ),
        (
            "matrix_named",
            snapshot(true, &[text("first"), text("second")]),
            text("firstsecond"),
        ),
        ("matrix_no_args", snapshot(true, &[]), External::Int(42)),
        (
            "matrix_caught",
            snapshot(true, &[text("inside")]),
            text("caught"),
        ),
    ] {
        let (result, recording) = Recording::run(program, function, vec![]).await;
        assert_eq!(result.unwrap(), expected_result, "{function}");
        assert_eq!(recording.spans.len(), 1, "{function}");
        let (_, entry, done) = &recording.spans[0];
        recording.assert_capture(entry.inputs_cas_id.as_ref(), Some(&expected_args));
        recording.assert_capture(
            done.value_cas_id.as_ref(),
            Some(&snapshot(false, &[expected_result])),
        );
        assert_eq!(
            CompletionFlags::from_wire(done.completion_flags, false)
                .unwrap()
                .outcome(),
            btel_types::InvocationOutcome::Ok
        );
    }
    for (scenario, function, arguments, before, after) in [
        ("cycle", "matrix_cycle", vec![External::Bool(true)], 1, 2),
        ("node", "matrix_cycle", vec![External::Bool(false)], 1, 2),
        ("enum", "matrix_enum", vec![], 0, 0),
        ("aliases", "matrix_aliases", vec![], 0, 0),
        ("empty_array", "matrix_empty_array", vec![], 0, 0),
        ("generic", "matrix_generic", vec![], 0, 0),
        ("map", "matrix_map", vec![External::Bool(false)], 1, 1),
        ("map", "matrix_map", vec![External::Bool(true)], 0, 0),
    ] {
        let (result, recording) = Recording::run(program, function, arguments).await;
        assert!(result.is_ok(), "{scenario}: {result:?}");
        if matches!(scenario, "cycle" | "node") {
            assert_eq!(result.unwrap(), External::Int(3));
        }
        assert_eq!(recording.spans.len(), 1, "{scenario}");
        let (_, entry, done) = &recording.spans[0];
        recording.assert_capture(
            entry.inputs_cas_id.as_ref(),
            Some(&graph_snapshot(program, scenario, true, before)),
        );
        recording.assert_capture(
            done.value_cas_id.as_ref(),
            Some(&graph_snapshot(program, scenario, false, after)),
        );
    }
}

async fn call_structure(program: &Program) {
    for hidden in [false, true] {
        let (result, recording) =
            Recording::run(program, "matrix_nested", vec![External::Bool(hidden)]).await;
        assert_eq!(result.unwrap(), text("child"));
        assert_eq!(recording.spans.len(), if hidden { 1 } else { 2 });
        let child_args = snapshot(true, &[text("child")]);
        let child = recording
            .spans
            .iter()
            .find(|(_, entry, _)| {
                entry
                    .inputs_cas_id
                    .as_ref()
                    .is_some_and(|id| capture_id(id) == child_args.id())
            })
            .unwrap();
        recording.assert_capture(child.1.inputs_cas_id.as_ref(), Some(&child_args));
        recording.assert_capture(
            child.2.value_cas_id.as_ref(),
            Some(&snapshot(false, &[text("child")])),
        );
        if !hidden {
            let parent = recording
                .spans
                .iter()
                .find(|(_, entry, _)| entry.id != child.1.id)
                .unwrap();
            assert_eq!(child.1.parent_id, parent.1.id);
            assert_eq!(parent.0, child.0);
            recording.assert_capture(
                parent.1.inputs_cas_id.as_ref(),
                Some(&snapshot(true, &[text("parent")])),
            );
        }
    }
    let (result, recording) = Recording::run(program, "matrix_spawned", vec![]).await;
    assert_eq!(result.unwrap(), text("spawned"));
    assert_eq!(recording.spans.len(), 1);
    let (thread, entry, done) = &recording.spans[0];
    assert!(recording.threads[thread].parent_id.is_some());
    recording.assert_capture(
        entry.inputs_cas_id.as_ref(),
        Some(&snapshot(true, &[text("spawned")])),
    );
    recording.assert_capture(
        done.value_cas_id.as_ref(),
        Some(&snapshot(false, &[text("spawned")])),
    );

    for deep_copy in [false, true] {
        let (result, recording) =
            Recording::run(program, "matrix_reused", vec![External::Bool(deep_copy)]).await;
        assert_eq!(result.unwrap(), text("later"));
        assert_eq!(recording.spans.len(), 2);
        for value in ["first", "later"] {
            let expected = snapshot(false, &[text(value)]);
            let span = recording
                .spans
                .iter()
                .find(|(_, _, done)| {
                    done.value_cas_id
                        .as_ref()
                        .is_some_and(|id| capture_id(id) == expected.id())
                })
                .unwrap();
            recording.assert_capture(
                span.1.inputs_cas_id.as_ref(),
                Some(&snapshot(true, &[text(value)])),
            );
            recording.assert_capture(span.2.value_cas_id.as_ref(), Some(&expected));
        }
    }
}

fn one_field_instance(
    program: &Program,
    name: &str,
    field: &External,
    args: bool,
    additional_args: &[External],
) -> Snapshot {
    let class = program
        .objects
        .0
        .iter()
        .find_map(|object| match object {
            bex_vm_types::Object::Class(class) if class.name.display_name().ends_with(name) => {
                Some(class)
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(class.fields.len(), 1);
    let pool = SnapshotPool::new(1, Limits::default());
    let mut b = pool.try_acquire().unwrap();
    let object = b.reserve_object().unwrap();
    let declaration = b.reserve_object().unwrap();
    b.set_object(
        declaration,
        SnapshotObject::Declaration {
            name: class.name.clone(),
            tag: class.type_tag,
            is_enum: false,
        },
    );
    let value = scalar(&mut b, field);
    let start = b.entry_start();
    b.entry(&class.fields[0].name.as_str().into(), value);
    let fields = b.entry_range(start);
    b.set_object(
        object,
        SnapshotObject::Instance {
            type_arguments: btel_snapshot::Range::empty(),
            declaration,
            fields,
            original_len: 1,
        },
    );
    let mut roots = vec![Value::Object(object)];
    roots.extend(additional_args.iter().map(|value| scalar(&mut b, value)));
    finish(b, args, &roots)
}

async fn callable_shapes(program: &Program) {
    let (result, recording) = Recording::run(program, "matrix_native_cleanup", vec![]).await;
    assert_eq!(result.unwrap(), text("native rejected"));
    assert_eq!(recording.spans.len(), 1);
    let (_, entry, done) = &recording.spans[0];
    recording.assert_capture(
        entry.inputs_cas_id.as_ref(),
        Some(&snapshot(true, &[text("native rejected")])),
    );
    recording.assert_capture(
        done.value_cas_id.as_ref(),
        Some(&snapshot(false, &[text("native rejected")])),
    );
    for bound in [false, true] {
        let (result, recording) =
            Recording::run(program, "matrix_method", vec![External::Bool(bound)]).await;
        assert_eq!(result.unwrap(), text("method:input"));
        assert_eq!(recording.spans.len(), 1);
        let (_, entry, done) = &recording.spans[0];
        recording.assert_capture(
            entry.inputs_cas_id.as_ref(),
            Some(&one_field_instance(
                program,
                "MatrixMethod",
                &text("method:"),
                true,
                &[text("input")],
            )),
        );
        recording.assert_capture(
            done.value_cas_id.as_ref(),
            Some(&snapshot(false, &[text("method:input")])),
        );
    }
    let (result, recording) = Recording::run(program, "matrix_closure", vec![]).await;
    assert_eq!(result.unwrap(), text("closure:input"));
    assert_eq!(recording.spans.len(), 1);
    let (_, entry, done) = &recording.spans[0];
    recording.assert_capture(
        entry.inputs_cas_id.as_ref(),
        Some(&snapshot(true, &[text("input")])),
    );
    recording.assert_capture(
        done.value_cas_id.as_ref(),
        Some(&snapshot(false, &[text("closure:input")])),
    );
    for present in [false, true] {
        let (result, recording) =
            Recording::run(program, "matrix_optional", vec![External::Bool(present)]).await;
        assert_eq!(
            result.unwrap(),
            if present {
                text("optional")
            } else {
                External::Null
            }
        );
        assert_eq!(recording.spans.len(), usize::from(present));
        for (_, entry, done) in &recording.spans {
            recording.assert_capture(
                entry.inputs_cas_id.as_ref(),
                Some(&snapshot(true, &[text("optional")])),
            );
            recording.assert_capture(
                done.value_cas_id.as_ref(),
                Some(&snapshot(false, &[text("optional")])),
            );
        }
    }
}

async fn exceptional_completion(program: &Program) {
    for (function, input, class, outcome, cancel) in [
        (
            "matrix_panic",
            "captured panic",
            "baml.panics.UserPanic",
            btel_types::InvocationOutcome::Errored,
            false,
        ),
        (
            "matrix_cancel",
            "cancelled input",
            "baml.panics.Cancelled",
            btel_types::InvocationOutcome::Cancelled,
            true,
        ),
    ] {
        for error in [None, Some(false), Some(true)] {
            let (result, recording) =
                Recording::run_with_cancellation(program, function, vec![flag(error)], cancel)
                    .await;
            let Err(EngineError::UnhandledThrow { value, .. }) = result else {
                panic!("expected {class}, got {result:?}");
            };
            let External::Instance {
                class_name, fields, ..
            } = value.as_ref()
            else {
                panic!("expected panic instance");
            };
            assert_eq!(class_name, class);
            assert_eq!(recording.spans.len(), 1);
            let (_, entry, done) = &recording.spans[0];
            recording.assert_capture(
                entry.inputs_cas_id.as_ref(),
                Some(&snapshot(true, &[text(input)])),
            );
            assert_eq!(
                CompletionFlags::from_wire(done.completion_flags, false)
                    .unwrap()
                    .outcome(),
                outcome,
            );
            let expected = one_field_instance(program, class, &fields["message"], false, &[]);
            recording.assert_capture(
                done.value_cas_id.as_ref(),
                (error == Some(true)).then_some(&expected),
            );
        }
    }
}

async fn unions_and_opaque_values(program: &Program) {
    for string in [false, true] {
        let expected = if string {
            text("union")
        } else {
            External::Int(7)
        };
        let (result, recording) =
            Recording::run(program, "matrix_union", vec![External::Bool(string)]).await;
        assert!(result.is_ok());
        assert_eq!(recording.spans.len(), 1);
        let (_, entry, done) = &recording.spans[0];
        recording.assert_capture(
            entry.inputs_cas_id.as_ref(),
            Some(&snapshot(true, std::slice::from_ref(&expected))),
        );
        recording.assert_capture(
            done.value_cas_id.as_ref(),
            Some(&snapshot(false, &[expected])),
        );
    }
    let (result, recording) = Recording::run(program, "matrix_opaque", vec![]).await;
    assert_eq!(result.unwrap(), External::Int(1));
    assert_eq!(recording.spans.len(), 1);
    let (_, entry, done) = &recording.spans[0];
    let placeholder = External::RustData(Arc::new(()));
    recording.assert_capture(
        entry.inputs_cas_id.as_ref(),
        Some(&one_field_instance(
            program,
            "trace.Options",
            &placeholder,
            true,
            &[],
        )),
    );
    recording.assert_capture(
        done.value_cas_id.as_ref(),
        Some(&one_field_instance(
            program,
            "trace.Options",
            &placeholder,
            false,
            &[],
        )),
    );
}

async fn llm_policy(program: &Program) {
    for capture in [None, Some(false), Some(true)] {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "matrix", "object": "chat.completion", "created": 1, "model": "test",
                "choices": [{"index": 0, "message": {"role": "assistant", "content": "captured llm", "refusal": null}, "finish_reason": "stop"}],
                "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
            })))
            .expect(1)
            .mount(&server).await;
        let (result, recording) = Recording::run(
            program,
            "matrix_llm_policy",
            vec![text(&server.uri()), flag(capture)],
        )
        .await;
        assert_eq!(result.unwrap(), text("captured llm"));
        let expected = snapshot(false, &[text("captured llm")]);
        let (_, entry, done) = recording
            .spans
            .iter()
            .find(|(_, _, done)| {
                done.value_cas_id
                    .as_ref()
                    .is_some_and(|id| capture_id(id) == expected.id())
            })
            .expect("LLM policy captures its output even with false options");
        assert!(entry.inputs_cas_id.is_some());
        recording.assert_capture(done.value_cas_id.as_ref(), Some(&expected));
    }
}

// The environment is process-wide, so each mode runs in a separate process.
#[test]
fn trace_contract_end_to_end() {
    const CHILD: &str = "BAML_TRACE_CAPTURE_MATRIX_CHILD";
    let Ok(mode) = std::env::var(CHILD) else {
        for mode in ["off", "low", "medium", "high"] {
            let result = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "trace_matrix::trace_contract_end_to_end",
                    "--nocapture",
                ])
                .env(CHILD, mode)
                .env("BAML_TELEMETRY", mode)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{mode}:\n{}\n{}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            );
        }
        return;
    };
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let program = baml_db::testing::compile_source(include_str!(
                "../../../baml_tests/baml_src/ns_trace_capture/matrix.baml"
            ));
            for selected in ["default", "hidden", "timing", "span"] {
                for reserved in [false, true] {
                    let expected_span = mode != "off"
                        && (reserved
                            || selected == "span"
                            || (selected == "default" && mode == "high"));
                    let (result, recording) = Recording::run(
                        &program,
                        "matrix_mode",
                        vec![
                            text(selected),
                            External::Bool(expected_span),
                            External::Bool(reserved),
                        ],
                    )
                    .await;
                    assert_eq!(result.unwrap(), text(selected));
                    let captured: Vec<_> = recording
                        .spans
                        .iter()
                        .filter(|(_, _, done)| done.value_cas_id.is_some())
                        .collect();
                    assert_eq!(
                        captured.len(),
                        usize::from(expected_span),
                        "{mode}/{selected}"
                    );
                    for (_, entry, done) in captured {
                        recording.assert_capture(
                            entry.inputs_cas_id.as_ref(),
                            Some(&snapshot(
                                true,
                                &[text(selected), External::Bool(expected_span)],
                            )),
                        );
                        recording.assert_capture(
                            done.value_cas_id.as_ref(),
                            Some(&snapshot(false, &[text(selected)])),
                        );
                    }
                }
            }
            if mode == "off" {
                for deep_copy in [false, true] {
                    let (result, recording) =
                        Recording::run(&program, "matrix_reused", vec![External::Bool(deep_copy)])
                            .await;
                    assert_eq!(result.unwrap(), text("later"));
                    assert!(recording.spans.is_empty());
                }
            }
            if mode == "medium" {
                capture_flags(&program).await;
                Box::pin(scalar_values(&program)).await;
                arguments_and_graphs(&program).await;
                call_structure(&program).await;
                callable_shapes(&program).await;
                exceptional_completion(&program).await;
                unions_and_opaque_values(&program).await;
                llm_policy(&program).await;
            }
        });
}
