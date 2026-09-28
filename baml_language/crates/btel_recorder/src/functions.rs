//! Cold function-definition metadata for one recording. The engine supplies
//! an owned table before execution; lookups never touch the VM heap.
use std::sync::Arc;

use btel_types::{FunctionId, FunctionMetadata, FunctionMetadataTable};
use rustc_hash::FxHashSet;

use crate::proto::{self, function_definition::Resolution};

#[derive(Default)]
pub(crate) struct FunctionDefinitions {
    table: Option<Arc<FunctionMetadataTable>>,
    // Functions whose metadata this recording already published.
    published: FxHashSet<FunctionId>,
}

impl FunctionDefinitions {
    pub(crate) fn set_table(&mut self, table: Arc<FunctionMetadataTable>) {
        self.table = Some(table);
    }

    /// The definition to publish for a newly referenced function, if any.
    /// Known metadata is published once per recording: later files resolve
    /// the reference through the earlier definition. Unknown functions keep
    /// the per-reference `MetadataUnavailable` observation.
    pub(crate) fn resolve(&mut self, function: FunctionId) -> Option<Resolution> {
        if self.published.contains(&function) {
            return None;
        }
        match self.table.as_ref().and_then(|table| table.get(function)) {
            Some(metadata) => {
                self.published.insert(function);
                Some(Resolution::Metadata(convert(metadata)))
            }
            None => Some(Resolution::Unavailable(proto::MetadataUnavailable {})),
        }
    }
}

fn convert(m: &FunctionMetadata) -> proto::FunctionMetadata {
    use btel_types::{RuntimeFunctionKind as Kind, RuntimeFunctionOrigin as Origin};
    let (kind, sys_op_name) = match &m.kind {
        Kind::Bytecode => (proto::FunctionKind::Bytecode, None),
        Kind::SysOp(name) => (proto::FunctionKind::SysOp, Some(name.clone())),
        Kind::Native => (proto::FunctionKind::Native, None),
        Kind::NativeUnresolved => (proto::FunctionKind::NativeUnresolved, None),
    };
    proto::FunctionMetadata {
        fqn: m.fqn.clone(),
        display_name: m.display_name.clone(),
        source_file: m.source_file.clone(),
        source_span: m.source_span.as_ref().map(|span| proto::SourceSpan {
            file_id: span.file_id,
            start: span.start,
            end: span.end,
        }),
        kind: kind as i32,
        sys_op_name,
        origin: match m.origin {
            Origin::UserDefined => proto::FunctionOrigin::UserDefined,
            Origin::Companion => proto::FunctionOrigin::Companion,
            Origin::Internal => proto::FunctionOrigin::Internal,
            Origin::Builtin => proto::FunctionOrigin::Builtin,
            Origin::AutoDerive => proto::FunctionOrigin::AutoDerive,
        } as i32,
        owner_type_key: m.owner_type.as_ref().map(|key| key.0.clone()),
        parent_function_key: m.parent_function.as_ref().map(|key| key.0.clone()),
        lambda_path: m.lambda_path.clone(),
        definition_key: m.definition_key.as_ref().map(|key| key.0.clone()),
        package_name: m.package_name.clone(),
        namespace: m.namespace.clone(),
        argument_layout: m
            .argument_layout
            .as_ref()
            .map(|layout| proto::ArgumentLayout {
                slots: layout
                    .slots
                    .iter()
                    .map(|slot| proto::ArgumentSlot {
                        name: slot.name.clone(),
                        receiver: slot.receiver,
                    })
                    .collect(),
            }),
        source_map: m.source_map.as_ref().map(|map| source_map(m, map)),
    }
}

/// Packed parallel arrays; the file array is omitted when every entry is in
/// the function's own definition file.
fn source_map(m: &FunctionMetadata, map: &btel_types::SourceMap) -> proto::SourceMap {
    let own_file = m.source_span.as_ref().map(|span| span.file_id);
    let same_file = own_file.is_some_and(|file| map.entries.iter().all(|e| e.file_id == file));
    proto::SourceMap {
        coordinate: proto::PcCoordinate::CompactByteOffset as i32,
        code_bytes: map.code_bytes,
        pc: map.entries.iter().map(|e| e.pc).collect(),
        file_id: if same_file {
            Vec::new()
        } else {
            map.entries.iter().map(|e| e.file_id).collect()
        },
        start: map.entries.iter().map(|e| e.start).collect(),
        end: map.entries.iter().map(|e| e.end).collect(),
        line: map.entries.iter().map(|e| e.line).collect(),
    }
}
