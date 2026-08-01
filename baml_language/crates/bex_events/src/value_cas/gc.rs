use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs, io,
    path::{Path, PathBuf},
    str::FromStr as _,
};

use super::{
    Cid, DagChunk, GcGuard, PackWriter, RetentionLog, Tombstone,
    manifest::CidManifestReader,
    pack::{PackIndex, PackIndexEntry, read_pack_chunk, scan_pack},
    referenced_cids,
};
use crate::value::{ValueFileRecord, read_bamlvalue_from_bytes};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProjectGcOptions {
    pub now_ms: u64,
    pub grace_ms: u64,
    pub dry_run: bool,
    pub origin_euid: [u8; 16],
    pub first_repack_seq: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectGcOutcome {
    pub skipped_live_writers: bool,
    pub roots_marked: usize,
    pub packs_examined: usize,
    pub packs_deleted: usize,
    pub packs_compacted: usize,
    pub chunks_rewritten: usize,
    pub reclaimable_stored_bytes: u64,
    pub planned: Vec<GcPackPlan>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MarkSet {
    cids: BTreeSet<Cid>,
}

impl MarkSet {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, cid: Cid) -> bool {
        self.cids.insert(cid)
    }

    pub fn extend(&mut self, cids: impl IntoIterator<Item = Cid>) {
        self.cids.extend(cids);
    }

    #[must_use]
    pub fn contains(&self, cid: &Cid) -> bool {
        self.cids.contains(cid)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.cids.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cids.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Cid> {
        self.cids.iter()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RootMarkReport {
    pub roots: MarkSet,
    pub manifests_read: usize,
    pub truncated_manifests: Vec<PathBuf>,
    pub derived_unsealed_roots: usize,
    pub upload_pins: usize,
}

/// Mark project roots from boundary manifests, flight manifests, upload pins,
/// and a caller-supplied scanner for unsealed boundary `.bamlvalue` records.
///
/// The derived-root callback is what lets this storage crate remain decoupled
/// from the evolving capture-root protobuf while still enforcing the rule that
/// persisted unsealed roots are never sweepable.
pub fn collect_project_roots(
    project_baml_dir: impl AsRef<Path>,
    mut derive_unsealed: impl FnMut(&Path) -> io::Result<Vec<Cid>>,
) -> io::Result<RootMarkReport> {
    let project_baml_dir = project_baml_dir.as_ref();
    let mut report = RootMarkReport::default();
    for path in files_named(&project_baml_dir.join("history"), "manifest.bamlcids")? {
        let outcome = CidManifestReader::read(&path)?;
        report.manifests_read = report.manifests_read.saturating_add(1);
        report.roots.extend(outcome.manifest.cids);
        if outcome.truncated {
            report.truncated_manifests.push(path.clone());
        }
        if !outcome.manifest.sealed {
            let boundary_dir = path
                .parent()
                .ok_or_else(|| io::Error::other("manifest path has no boundary directory"))?;
            let derived = derive_unsealed(boundary_dir)?;
            report.derived_unsealed_roots =
                report.derived_unsealed_roots.saturating_add(derived.len());
            report.roots.extend(derived);
        }
    }
    for path in files_with_extension(&project_baml_dir.join("sessions"), "bamlcids")? {
        let outcome = CidManifestReader::read(&path)?;
        report.manifests_read = report.manifests_read.saturating_add(1);
        report.roots.extend(outcome.manifest.cids);
        if outcome.truncated {
            report.truncated_manifests.push(path);
        }
    }

    let uploads_pin = project_baml_dir.join("uploads.pin");
    match fs::read_to_string(&uploads_pin) {
        Ok(contents) => {
            for (line_index, line) in contents.lines().enumerate() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                let cid = Cid::from_str(line).map_err(|error| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!(
                            "invalid CID in {} line {}: {error}",
                            uploads_pin.display(),
                            line_index + 1
                        ),
                    )
                })?;
                report.roots.insert(cid);
                report.upload_pins = report.upload_pins.saturating_add(1);
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    Ok(report)
}

/// Derive CAS roots from persisted `.bamlvalue` records in an unsealed
/// boundary. GC callers can pass this directly to [`collect_project_roots`]
/// or [`execute_project_gc`].
pub fn derive_unsealed_bamlvalue_roots(boundary_dir: &Path) -> io::Result<Vec<Cid>> {
    let mut roots = BTreeSet::new();
    for path in files_with_extension(boundary_dir, "bamlvalue")? {
        let contents = read_bamlvalue_from_bytes(&fs::read(path)?)?;
        for record in contents.records {
            let dag_ref = match record {
                ValueFileRecord::CapturedValue(record) => record.dag_ref,
                ValueFileRecord::LogEvent(record) => record.dag_ref,
                ValueFileRecord::CaptureLoss(_)
                | ValueFileRecord::Audit(_)
                | ValueFileRecord::RunStarted(_)
                | ValueFileRecord::RunCompleted(_) => None,
            };
            if let Some(dag_ref) = dag_ref {
                roots.insert(Cid::from_bytes(dag_ref.root_cid));
            }
        }
    }
    Ok(roots.into_iter().collect())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackInventory {
    pub pack_path: PathBuf,
    pub index_path: PathBuf,
    pub created_ms: u64,
    pub entries: Vec<PackIndexEntry>,
}

pub fn build_pack_inventory(packs_dir: impl AsRef<Path>) -> io::Result<Vec<PackInventory>> {
    let packs_dir = packs_dir.as_ref();
    let mut inventory = Vec::new();
    match fs::read_dir(packs_dir) {
        Ok(entries) => {
            for entry in entries {
                let index_path = entry?.path();
                if !index_path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with(".bamlpack.idx"))
                {
                    continue;
                }
                let mut pack_path = index_path.clone();
                pack_path.set_extension("");
                let scan = scan_pack(&pack_path)?;
                if scan.torn_tail {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("indexed pack has a torn tail: {}", pack_path.display()),
                    ));
                }
                let index = PackIndex::read(&index_path, &pack_path)?;
                inventory.push(PackInventory {
                    pack_path,
                    index_path,
                    created_ms: scan.created_ms,
                    entries: index.entries,
                });
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    }
    inventory.sort_by(|left, right| {
        left.created_ms
            .cmp(&right.created_ms)
            .then_with(|| left.pack_path.cmp(&right.pack_path))
    });
    Ok(inventory)
}

fn files_named(root: &Path, name: &str) -> io::Result<Vec<PathBuf>> {
    walk_files(root, |path| {
        path.file_name().and_then(|file_name| file_name.to_str()) == Some(name)
    })
}

fn files_with_extension(root: &Path, extension: &str) -> io::Result<Vec<PathBuf>> {
    walk_files(root, |path| {
        path.extension().and_then(|value| value.to_str()) == Some(extension)
    })
}

fn walk_files(root: &Path, predicate: impl Fn(&Path) -> bool) -> io::Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        match fs::read_dir(&directory) {
            Ok(entries) => {
                for entry in entries {
                    let entry = entry?;
                    let file_type = entry.file_type()?;
                    if file_type.is_symlink() {
                        continue;
                    }
                    if file_type.is_dir() {
                        pending.push(entry.path());
                    } else if file_type.is_file() && predicate(&entry.path()) {
                        paths.push(entry.path());
                    }
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    paths.sort();
    Ok(paths)
}

/// Expand root marks through canonical child CID references.
///
/// Missing root/child CIDs are fatal: proceeding would make a readable root's
/// already-broken closure look sweepable.
pub fn expand_mark_closure(
    inventory: &[PackInventory],
    roots: impl IntoIterator<Item = Cid>,
) -> io::Result<MarkSet> {
    let mut locations = BTreeMap::<Cid, (&Path, PackIndexEntry)>::new();
    // Newest wins, matching the reader search order.
    for pack in inventory {
        for entry in &pack.entries {
            locations.insert(entry.cid, (&pack.pack_path, *entry));
        }
    }
    let mut marks = MarkSet::new();
    let mut pending = VecDeque::new();
    for root in roots {
        if marks.insert(root) {
            pending.push_back(root);
        }
    }
    while let Some(cid) = pending.pop_front() {
        let (pack_path, entry) = locations.get(&cid).copied().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("marked value CID {cid} is absent from all sealed packs"),
            )
        })?;
        let bytes = read_pack_chunk(pack_path, entry)?;
        for child in referenced_cids(&bytes)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
        {
            if marks.insert(child) {
                pending.push_back(child);
            }
        }
    }
    Ok(marks)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GcPackDisposition {
    Keep,
    Delete,
    /// Rewrite these entries into a fresh pack, then unlink the old pack as a
    /// whole. Chunk-level deletion in-place is intentionally impossible.
    Compact {
        retained: Vec<PackIndexEntry>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GcPackPlan {
    pub pack_path: PathBuf,
    pub index_path: PathBuf,
    pub disposition: GcPackDisposition,
    pub reclaimable_stored_bytes: u64,
}

/// Classify sealed packs under a mark set and grace window.
///
/// This is side-effect free. A caller must hold [`super::GcGuard`] while
/// applying plans and must tombstone every compacted/deleted old pack.
#[must_use]
pub fn plan_sweep(
    inventory: &[PackInventory],
    marks: &MarkSet,
    now_ms: u64,
    grace_ms: u64,
) -> Vec<GcPackPlan> {
    let grace_cutoff = now_ms.saturating_sub(grace_ms);
    inventory
        .iter()
        .map(|pack| {
            let protected_by_grace = pack.created_ms >= grace_cutoff;
            let retained = pack
                .entries
                .iter()
                .copied()
                .filter(|entry| protected_by_grace || marks.contains(&entry.cid))
                .collect::<Vec<_>>();
            let reclaimable_stored_bytes = pack
                .entries
                .iter()
                .filter(|entry| {
                    retained
                        .binary_search_by_key(&entry.cid, |retained| retained.cid)
                        .is_err()
                })
                .map(|entry| u64::from(entry.stored_len))
                .sum();
            let disposition = if retained.len() == pack.entries.len() {
                GcPackDisposition::Keep
            } else if retained.is_empty() {
                GcPackDisposition::Delete
            } else {
                GcPackDisposition::Compact { retained }
            };
            GcPackPlan {
                pack_path: pack.pack_path.clone(),
                index_path: pack.index_path.clone(),
                disposition,
                reclaimable_stored_bytes,
            }
        })
        .collect()
}

/// Execute one coarse-lock GC pass.
///
/// The exclusive guard is acquired before root collection and held until all
/// replacement packs are sealed, old packs are unlinked, and tombstones are
/// flushed. `WouldBlock` from a live writer is reported as a clean skip.
pub fn execute_project_gc(
    project_baml_dir: impl AsRef<Path>,
    options: ProjectGcOptions,
    derive_unsealed: impl FnMut(&Path) -> io::Result<Vec<Cid>>,
) -> io::Result<ProjectGcOutcome> {
    let project_baml_dir = project_baml_dir.as_ref();
    let store_dir = project_baml_dir.join("store");
    let _guard = match GcGuard::try_acquire(&store_dir) {
        Ok(guard) => guard,
        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
            return Ok(ProjectGcOutcome {
                skipped_live_writers: true,
                ..ProjectGcOutcome::default()
            });
        }
        Err(error) => return Err(error),
    };

    let roots = collect_project_roots(project_baml_dir, derive_unsealed)?;
    let inventory = build_pack_inventory(store_dir.join("packs"))?;
    let marks = expand_mark_closure(&inventory, roots.roots.iter().copied())?;
    let planned = plan_sweep(&inventory, &marks, options.now_ms, options.grace_ms);
    let mut outcome = ProjectGcOutcome {
        roots_marked: marks.len(),
        packs_examined: inventory.len(),
        reclaimable_stored_bytes: planned
            .iter()
            .map(|plan| plan.reclaimable_stored_bytes)
            .sum(),
        planned: planned.clone(),
        ..ProjectGcOutcome::default()
    };
    if options.dry_run {
        return Ok(outcome);
    }

    let mut retention_log = RetentionLog::open(project_baml_dir)?;
    let mut next_pack_seq = options.first_repack_seq;
    for plan in &planned {
        match &plan.disposition {
            GcPackDisposition::Keep => {}
            GcPackDisposition::Delete => {
                let bytes = fs::metadata(&plan.pack_path).ok().map(|meta| meta.len());
                unlink_pack(&plan.pack_path, &plan.index_path)?;
                outcome.packs_deleted = outcome.packs_deleted.saturating_add(1);
                retention_log.append(&Tombstone {
                    deleted_at_ms: options.now_ms,
                    kind: "value_pack".to_string(),
                    path: plan.pack_path.clone(),
                    reason: "gc_fully_dead".to_string(),
                    bytes,
                })?;
            }
            GcPackDisposition::Compact { retained } => {
                let store_dir = plan
                    .pack_path
                    .parent()
                    .and_then(Path::parent)
                    .ok_or_else(|| io::Error::other("value pack is outside store/packs"))?;
                let mut writer = loop {
                    match PackWriter::create_for_gc(
                        store_dir,
                        options.origin_euid,
                        next_pack_seq,
                        options.now_ms,
                        &plan.pack_path,
                    ) {
                        Ok(writer) => break writer,
                        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                            next_pack_seq = next_pack_seq.checked_add(1).ok_or_else(|| {
                                io::Error::other("exhausted GC repack sequence space")
                            })?;
                        }
                        Err(error) => return Err(error),
                    }
                };
                next_pack_seq = next_pack_seq.saturating_add(1);
                for entry in retained {
                    let canonical_bytes = read_pack_chunk(&plan.pack_path, *entry)?;
                    let append = writer.append_chunk(&DagChunk {
                        cid: entry.cid,
                        canonical_bytes,
                        logical_len: u64::from(entry.logical_len),
                    })?;
                    outcome.chunks_rewritten = outcome
                        .chunks_rewritten
                        .saturating_add(usize::from(append.appended));
                }
                writer.seal()?;
                let bytes = fs::metadata(&plan.pack_path).ok().map(|meta| meta.len());
                unlink_pack(&plan.pack_path, &plan.index_path)?;
                outcome.packs_compacted = outcome.packs_compacted.saturating_add(1);
                retention_log.append(&Tombstone {
                    deleted_at_ms: options.now_ms,
                    kind: "value_pack".to_string(),
                    path: plan.pack_path.clone(),
                    reason: "gc_compacted".to_string(),
                    bytes,
                })?;
            }
        }
    }
    retention_log.sync_data()?;
    Ok(outcome)
}

fn unlink_pack(pack_path: &Path, index_path: &Path) -> io::Result<()> {
    remove_if_exists(index_path)?;
    let lease = pack_path.with_extension("lease");
    remove_if_exists(&lease)?;
    remove_if_exists(pack_path)?;
    if let Some(parent) = pack_path.parent() {
        fs::File::open(parent)?.sync_all()?;
    }
    Ok(())
}

fn remove_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::ids::BoundaryId;
    use crate::value_cas::{
        CanonicalValue, CidManifestWriter, GcPackDisposition, PackWriter, ProjectGcOptions,
        build_pack_inventory, collect_project_roots, encode_value_dag, execute_project_gc,
        expand_mark_closure, plan_sweep,
    };

    fn temp_store() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "baml-gc-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn mark_expands_inline_and_addressed_descendants() {
        let store = temp_store();
        let dag = encode_value_dag(&CanonicalValue::List(vec![
            CanonicalValue::String("large".repeat(1000)),
            CanonicalValue::Int(1),
        ]))
        .unwrap();
        let mut writer = PackWriter::create(&store, [1; 16], 1, 100).unwrap();
        for chunk in &dag.chunks {
            writer.append_chunk(chunk).unwrap();
        }
        writer.seal().unwrap();
        let inventory = build_pack_inventory(store.join("packs")).unwrap();
        let marks = expand_mark_closure(&inventory, [dag.root]).unwrap();
        assert_eq!(marks.len(), dag.chunks.len());
        fs::remove_dir_all(store).unwrap();
    }

    #[test]
    fn sweep_keeps_grace_and_compacts_partially_live_pack() {
        let store = temp_store();
        let live = encode_value_dag(&CanonicalValue::String("live".repeat(1000))).unwrap();
        let dead = encode_value_dag(&CanonicalValue::String("dead".repeat(1000))).unwrap();
        let mut writer = PackWriter::create(&store, [2; 16], 1, 100).unwrap();
        for chunk in live.chunks.iter().chain(&dead.chunks) {
            writer.append_chunk(chunk).unwrap();
        }
        writer.seal().unwrap();
        let inventory = build_pack_inventory(store.join("packs")).unwrap();
        let marks = expand_mark_closure(&inventory, [live.root]).unwrap();
        let plan = plan_sweep(&inventory, &marks, 1_000, 100);
        assert!(matches!(
            plan[0].disposition,
            GcPackDisposition::Compact { .. }
        ));
        assert!(plan[0].reclaimable_stored_bytes > 0);

        let grace_plan = plan_sweep(&inventory, &marks, 150, 100);
        assert!(matches!(grace_plan[0].disposition, GcPackDisposition::Keep));
        fs::remove_dir_all(store).unwrap();
    }

    #[test]
    fn project_roots_union_manifests_uploads_and_unsealed_derivation() {
        let project = temp_store();
        let boundary = project.join("history/run-1");
        fs::create_dir_all(&boundary).unwrap();
        let durable = encode_value_dag(&CanonicalValue::Int(1)).unwrap().root;
        let derived = encode_value_dag(&CanonicalValue::Int(2)).unwrap().root;
        let upload = encode_value_dag(&CanonicalValue::Int(3)).unwrap().root;
        let mut manifest = CidManifestWriter::create(
            boundary.join("manifest.bamlcids"),
            BoundaryId::from_bytes([1; 16]),
        )
        .unwrap();
        manifest.append(durable).unwrap();
        manifest.sync_data().unwrap();
        drop(manifest);
        fs::write(
            project.join("uploads.pin"),
            format!("# pending\n{upload}\n"),
        )
        .unwrap();
        let report = collect_project_roots(&project, |boundary_dir| {
            assert_eq!(boundary_dir, boundary);
            Ok(vec![derived])
        })
        .unwrap();
        assert!(report.roots.contains(&durable));
        assert!(report.roots.contains(&derived));
        assert!(report.roots.contains(&upload));
        assert_eq!(report.derived_unsealed_roots, 1);
        assert_eq!(report.upload_pins, 1);
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn executable_gc_skips_live_writer_and_then_deletes_dead_pack() {
        let project = temp_store();
        let store = project.join("store");
        let dag = encode_value_dag(&CanonicalValue::String("dead".repeat(1000))).unwrap();
        let mut writer = PackWriter::create(&store, [7; 16], 1, 100).unwrap();
        for chunk in &dag.chunks {
            writer.append_chunk(chunk).unwrap();
        }
        let skipped = execute_project_gc(
            &project,
            ProjectGcOptions {
                now_ms: 1_000,
                grace_ms: 0,
                dry_run: false,
                origin_euid: [8; 16],
                first_repack_seq: 100,
            },
            |_| Ok(Vec::new()),
        )
        .unwrap();
        assert!(skipped.skipped_live_writers);

        let old_paths = writer.seal().unwrap();
        let collected = execute_project_gc(
            &project,
            ProjectGcOptions {
                now_ms: 1_000,
                grace_ms: 0,
                dry_run: false,
                origin_euid: [8; 16],
                first_repack_seq: 100,
            },
            |_| Ok(Vec::new()),
        )
        .unwrap();
        assert!(!collected.skipped_live_writers);
        assert_eq!(collected.packs_deleted, 1);
        assert!(!old_paths.pack.exists());
        assert!(!old_paths.index.exists());
        assert!(
            fs::read_to_string(project.join("retention.log"))
                .unwrap()
                .contains("gc_fully_dead")
        );
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn executable_gc_seals_replacement_before_unlinking_partial_pack() {
        let project = temp_store();
        let store = project.join("store");
        let live = encode_value_dag(&CanonicalValue::String("live".repeat(1000))).unwrap();
        let dead = encode_value_dag(&CanonicalValue::String("dead".repeat(1000))).unwrap();
        let mut writer = PackWriter::create(&store, [9; 16], 1, 100).unwrap();
        for chunk in live.chunks.iter().chain(&dead.chunks) {
            writer.append_chunk(chunk).unwrap();
        }
        let old_paths = writer.seal().unwrap();
        let boundary = project.join("history/run");
        fs::create_dir_all(&boundary).unwrap();
        let mut manifest = CidManifestWriter::create(
            boundary.join("manifest.bamlcids"),
            BoundaryId::from_bytes([10; 16]),
        )
        .unwrap();
        manifest.append(live.root).unwrap();
        manifest.seal().unwrap();

        let collected = execute_project_gc(
            &project,
            ProjectGcOptions {
                now_ms: 1_000,
                grace_ms: 0,
                dry_run: false,
                origin_euid: [11; 16],
                first_repack_seq: 100,
            },
            |_| Ok(Vec::new()),
        )
        .unwrap();
        assert_eq!(collected.packs_compacted, 1);
        assert!(!old_paths.pack.exists());
        let inventory = build_pack_inventory(store.join("packs")).unwrap();
        let marks = expand_mark_closure(&inventory, [live.root]).unwrap();
        assert_eq!(marks.len(), live.chunks.len());
        assert!(inventory.iter().all(|pack| {
            pack.entries
                .iter()
                .all(|entry| !dead.chunks.iter().any(|chunk| chunk.cid == entry.cid))
        }));
        fs::remove_dir_all(project).unwrap();
    }
}
