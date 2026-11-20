use std::collections::BTreeMap;

use btel_core::{
    cas::{DecodedBlob, DecodedObject, DecodedRoot, DecodedValue},
    reader::context::{ContextReference, reference},
};
use btel_recorder::proto;

pub(super) const SOURCE: &str =
    include_str!("../../../baml_tests/baml_src/ns_trace_context/recording.baml");

/// Compile through the shared manifest/claim path so identity hashes also cover
/// the version, as they do for real CLI and generated SDK programs.
pub(super) fn program_with_version(source: &str, version: &str) -> bex_vm_types::Program {
    let mut db = baml_test_support::setup_test_db(source);
    let root = db.workspace_root().unwrap();
    let manifest =
        baml_db::manifest::parse(&format!("[package]\nversion = {version:?}\n")).unwrap();
    db.set_claimed_identity(root, baml_db::manifest::claimed_identity(&manifest));
    db.get_bytecode(root).unwrap()
}

/// The entries of a context map, which must have been captured whole.
fn whole_map(snapshot: &DecodedBlob, id: btel_core::cas::NodeId) -> BTreeMap<&str, &DecodedValue> {
    let DecodedObject::Map {
        entries,
        original_len,
        ..
    } = snapshot.object(id)
    else {
        panic!("context values must be maps");
    };
    assert_eq!(
        entries.len() as u64,
        *original_len,
        "context captured whole"
    );
    assert!(
        !entries
            .iter()
            .any(|(_, value)| matches!(value, DecodedValue::Truncated(_))),
        "context captured whole"
    );
    entries
        .iter()
        .map(|(key, value)| {
            let DecodedValue::String(key) = key else {
                panic!("context keys must be strings");
            };
            (key.as_ref(), value)
        })
        .collect()
}

/// Decode an entire context snapshot into its identity and scalar metadata,
/// rejecting truncated maps so propagation assertions cannot silently pass.
pub(super) fn metadata(snapshot: &DecodedBlob) -> (Option<&str>, BTreeMap<&str, &DecodedValue>) {
    let DecodedRoot::Value(DecodedValue::Object(root)) = snapshot.root else {
        panic!("context root must be a map");
    };
    let fields = whole_map(snapshot, root);
    let identity = match fields["distinct_id"] {
        DecodedValue::String(identity) => Some(identity.as_ref()),
        DecodedValue::Null => None,
        _ => panic!("context identity must be a string or null"),
    };
    let DecodedValue::Object(metadata) = fields["metadata"] else {
        panic!("context metadata must be a map");
    };
    (identity, whole_map(snapshot, *metadata))
}

/// Inspect uploaded/persisted bytes, not the producer's in-memory context.
pub(super) fn assert_leaf_contexts(
    files: &[proto::RecordingFile],
    snapshots: &BTreeMap<[u8; 16], DecodedBlob>,
    minimum: usize,
    app_version: Option<&str>,
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
        let (identity, values) = metadata(snapshot);
        let Some(DecodedValue::String(phase)) = values.get("phase").copied() else {
            continue;
        };
        assert_eq!(identity, Some("user-42"));
        assert_eq!(
            values["baml.ctx.baml_version"],
            &DecodedValue::String(baml_version::CANONICAL_VERSION.into())
        );
        assert_eq!(
            values.get("baml.ctx.app_version").copied(),
            app_version
                .map(|version| DecodedValue::String(version.into()))
                .as_ref()
        );
        assert_eq!(values["root"], &DecodedValue::String("kept".into()));
        assert_eq!(
            values["text"],
            &DecodedValue::String("quote=\" backslash=\\ newline=\n tab=\t".into())
        );
        for event in &section.events {
            match &event.event {
                Some(Event::FunctionAnnouncement(entry)) if phase.as_ref() == "entry" => {
                    assert_eq!(values.len(), 5 + usize::from(app_version.is_some()));
                    assert_eq!(values["remove"], &DecodedValue::String("yes".into()));
                    entries.insert(entry.id, id);
                }
                Some(Event::FunctionCompletion(done)) if phase.as_ref() == "entry" => {
                    assert_eq!(values.len(), 5 + usize::from(app_version.is_some()));
                    assert_eq!(values["remove"], &DecodedValue::String("yes".into()));
                    completions.insert(done.id, id);
                }
                _ => {}
            }
        }
    }
    // A recording for an upload writes no start record: its completions are
    // the invocations; a local one's starts carry the same contexts.
    assert!(
        completions.len() >= minimum,
        "missing completion contexts: {}",
        completions.len()
    );
    if !entries.is_empty() {
        assert_eq!(
            entries, completions,
            "each invocation must retain the same scoped context at completion"
        );
    }
}
