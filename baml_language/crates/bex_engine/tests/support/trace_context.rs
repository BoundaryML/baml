use std::collections::BTreeMap;

use btel_reader::context::{ContextReference, reference};
use btel_recorder::proto;
use btel_snapshot::{DecodedObject, DecodedRoot, DecodedSnapshot, DecodedValue};

pub(super) const SOURCE: &str =
    include_str!("../../../baml_tests/baml_src/ns_trace_context/recording.baml");

fn metadata(snapshot: &DecodedSnapshot) -> (Option<&str>, BTreeMap<&str, &DecodedValue>) {
    let DecodedRoot::Value(DecodedValue::Object(root)) = snapshot.root else {
        panic!("context root must be a map");
    };
    let DecodedObject::Map { entries, .. } = snapshot.object(root) else {
        panic!("context root must be a map");
    };
    let fields: BTreeMap<_, _> = entries
        .iter()
        .map(|(key, value)| (key.as_ref(), value))
        .collect();
    let identity = match fields["distinct_id"] {
        DecodedValue::String(identity) => Some(identity.as_ref()),
        DecodedValue::Null => None,
        _ => panic!("context identity must be a string or null"),
    };
    let DecodedValue::Object(metadata) = fields["metadata"] else {
        panic!("context metadata must be a map");
    };
    let DecodedObject::Map { entries, .. } = snapshot.object(*metadata) else {
        panic!("context metadata must be a map");
    };
    (
        identity,
        entries
            .iter()
            .map(|(key, value)| (key.as_ref(), value))
            .collect(),
    )
}

/// Inspect uploaded/persisted bytes, not the producer's in-memory context.
pub(super) fn assert_leaf_contexts(
    files: &[proto::RecordingFile],
    snapshots: &BTreeMap<[u8; 16], DecodedSnapshot>,
    minimum: usize,
) {
    use proto::span_event::Event;
    let mut entries = BTreeMap::new();
    let mut completions = BTreeMap::new();
    for section in files
        .iter()
        .flat_map(|file| file.spans.iter().flat_map(|spans| &spans.sections))
    {
        let ContextReference::Snapshot(id) = reference(section) else {
            continue;
        };
        let snapshot = &snapshots[id.as_bytes()];
        assert!(!snapshot.limited);
        let (identity, values) = metadata(snapshot);
        let Some(DecodedValue::String(phase)) = values.get("phase").copied() else {
            continue;
        };
        assert_eq!(identity, Some("user-42"));
        assert_eq!(values["root"], &DecodedValue::String("kept".into()));
        assert_eq!(
            values["text"],
            &DecodedValue::String("quote=\" backslash=\\ newline=\n tab=\t".into())
        );
        for event in &section.events {
            match &event.event {
                Some(Event::FunctionAnnouncement(entry)) if phase.as_ref() == "entry" => {
                    assert_eq!(values.len(), 4);
                    assert_eq!(values["remove"], &DecodedValue::String("yes".into()));
                    entries.insert(entry.id, id);
                }
                Some(Event::FunctionCompletion(done)) if phase.as_ref() == "entry" => {
                    assert_eq!(values.len(), 4);
                    assert_eq!(values["remove"], &DecodedValue::String("yes".into()));
                    completions.insert(done.id, id);
                }
                _ => {}
            }
        }
    }
    assert!(
        entries.len() >= minimum,
        "missing entry contexts: {}",
        entries.len()
    );
    assert_eq!(
        entries, completions,
        "each invocation must retain the same scoped context at completion"
    );
}
