//! Cold function-definition metadata for one recording. The engine supplies
//! an owned table before execution; lookups never touch the VM heap.
use std::sync::Arc;

use btel_types::{FunctionId, FunctionMetadata, FunctionMetadataTable};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::proto::{self, function_definition::Resolution};

#[derive(Default)]
pub(crate) struct FunctionDefinitions {
    table: Option<Arc<FunctionMetadataTable>>,
    host: FxHashMap<FunctionId, FunctionMetadata>,
    // Functions whose metadata this recording already published.
    published: FxHashSet<FunctionId>,
}

impl FunctionDefinitions {
    pub(crate) fn set_table(&mut self, table: Arc<FunctionMetadataTable>) {
        self.table = Some(table);
    }

    pub(crate) fn register_host(&mut self, metadata: &FunctionMetadata) {
        self.host
            .entry(metadata.function_id)
            .or_insert_with(|| metadata.clone());
    }

    /// The one-based line of `pc` in `function`'s own source file, as the
    /// function's line table places it: what a reader resolves from the
    /// published definition (`btel_reader::source_map`). `None` without a
    /// table, for a PC the table does not cover, and for code inlined from
    /// another file, whose line is not a line of this function's file.
    pub(crate) fn line(&self, function: FunctionId, pc: u32) -> Option<u32> {
        let metadata = self
            .host
            .get(&function)
            .or_else(|| self.table.as_ref().and_then(|table| table.get(function)))?;
        let map = metadata.source_map.as_ref()?;
        if pc == u32::MAX || pc >= map.code_bytes {
            return None;
        }
        let covering = map.entries.partition_point(|entry| entry.pc <= pc);
        let entry = map.entries.get(covering.checked_sub(1)?)?;
        let own_file = metadata.source_span.as_ref()?.file_id;
        (entry.file_id == own_file).then_some(entry.line)
    }

    /// The definition to publish for a newly referenced function, if any.
    /// Known metadata is published once per recording: later files resolve
    /// the reference through the earlier definition. Unknown functions keep
    /// the per-reference `MetadataUnavailable` observation.
    pub(crate) fn resolve(&mut self, function: FunctionId) -> Option<Resolution> {
        if self.published.contains(&function) {
            return None;
        }
        match self
            .host
            .get(&function)
            .or_else(|| self.table.as_ref().and_then(|table| table.get(function)))
        {
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
        model_invoke: m.model_invoke,
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

#[cfg(test)]
mod tests {
    use btel_types::{SourceMap, SourceMapEntry, SourceSpan};

    use super::*;

    const OWN_FILE: u32 = 3;
    const OTHER_FILE: u32 = 4;

    fn entry(pc: u32, file_id: u32, line: u32) -> SourceMapEntry {
        SourceMapEntry {
            pc,
            file_id,
            start: line * 10,
            end: line * 10 + 5,
            line,
        }
    }

    fn metadata(
        function_id: FunctionId,
        source_span: Option<SourceSpan>,
        source_map: Option<SourceMap>,
    ) -> FunctionMetadata {
        FunctionMetadata {
            function_id,
            fqn: "user.Logger".into(),
            display_name: "Logger".into(),
            source_file: Some("main.baml".into()),
            source_span,
            kind: btel_types::RuntimeFunctionKind::Bytecode,
            origin: btel_types::RuntimeFunctionOrigin::UserDefined,
            owner_type: None,
            parent_function: None,
            lambda_path: None,
            definition_key: None,
            package_name: Some("user".into()),
            namespace: Vec::new(),
            argument_layout: None,
            source_map,
            model_invoke: false,
        }
    }

    fn own_span() -> SourceSpan {
        SourceSpan {
            file_id: OWN_FILE,
            start: 0,
            end: 500,
        }
    }

    /// Code of 30 bytes: nothing maps PCs 0..4, lines 10 and 11 of the
    /// function's own file follow, and PCs 20..30 are code from another file.
    fn map() -> SourceMap {
        SourceMap {
            code_bytes: 30,
            entries: vec![
                entry(4, OWN_FILE, 10),
                entry(12, OWN_FILE, 11),
                entry(20, OTHER_FILE, 99),
            ],
        }
    }

    #[test]
    fn line_is_the_covering_entry_of_the_functions_own_file() {
        let allocator = btel_types::FunctionIdAllocator::default();
        let ids: Vec<FunctionId> = (0..5).map(|_| allocator.allocate().unwrap()).collect();
        let [mapped, unmapped, no_span, host, unknown] = ids[..] else {
            unreachable!()
        };
        let mut definitions = FunctionDefinitions::default();
        // Without a table, no function has a line.
        assert_eq!(definitions.line(mapped, 4), None);
        definitions.set_table(Arc::new(FunctionMetadataTable {
            functions: vec![
                metadata(mapped, Some(own_span()), Some(map())),
                metadata(unmapped, Some(own_span()), None),
                metadata(no_span, None, Some(map())),
            ],
        }));
        definitions.register_host(&metadata(host, Some(own_span()), Some(map())));

        for (pc, line) in [
            // Before the first entry: no instruction there is mapped.
            (0, None),
            (3, None),
            // An entry applies from its PC until the next entry's.
            (4, Some(10)),
            (11, Some(10)),
            (12, Some(11)),
            (19, Some(11)),
            // Code inlined from another file has no line in this one.
            (20, None),
            (29, None),
            // At or past the end of the code, and the saturated sentinel.
            (30, None),
            (31, None),
            (u32::MAX - 1, None),
            (u32::MAX, None),
        ] {
            assert_eq!(definitions.line(mapped, pc), line, "pc {pc}");
            // A host-registered definition resolves like a table one.
            assert_eq!(definitions.line(host, pc), line, "host pc {pc}");
            assert_eq!(definitions.line(unmapped, pc), None, "no map, pc {pc}");
            // Without a definition span there is no own file to be a line of.
            assert_eq!(definitions.line(no_span, pc), None, "no span, pc {pc}");
            assert_eq!(definitions.line(unknown, pc), None, "unknown, pc {pc}");
        }

        // An empty table covers no PC.
        let empty = allocator.allocate().unwrap();
        definitions.register_host(&metadata(
            empty,
            Some(own_span()),
            Some(SourceMap {
                code_bytes: 30,
                entries: Vec::new(),
            }),
        ));
        assert_eq!(definitions.line(empty, 0), None);
        assert_eq!(definitions.line(empty, 29), None);
    }
}
