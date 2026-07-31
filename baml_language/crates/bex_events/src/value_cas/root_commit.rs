use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::{self, Write},
    path::Path,
};

use super::{Cid, DagChunk, ValueDag, manifest::CidManifestWriter, pack::PackWriter};

/// One group-commit unit. `value_records` are already framed `.bamlvalue`
/// records; this layer intentionally does not reinterpret the protobuf.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RootCommitBatch {
    pub dags: Vec<ValueDag>,
    pub value_records: Vec<Vec<u8>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RootCommitOutcome {
    pub roots_committed: usize,
    pub chunks_considered: usize,
    pub chunks_appended: usize,
    pub value_record_bytes: usize,
}

/// Owns the three files whose ordering prevents durable capture roots from
/// ever naming non-durable chunks.
#[derive(Debug)]
pub struct RootCommitter {
    pack: PackWriter,
    manifest: CidManifestWriter,
    value_file: File,
}

impl RootCommitter {
    pub fn new(
        pack: PackWriter,
        manifest: CidManifestWriter,
        value_file_path: impl AsRef<Path>,
    ) -> io::Result<Self> {
        let value_file = OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(value_file_path)?;
        Ok(Self {
            pack,
            manifest,
            value_file,
        })
    }

    /// Commit in the only safe order:
    ///
    /// 1. append all chunks;
    /// 2. D1-sync the pack;
    /// 3. append and D1-sync `manifest.bamlcids`;
    /// 4. append and D1-sync capture-root/audit records.
    ///
    /// Failures can leave unreachable chunks or conservative manifest pins,
    /// but cannot leave a durable `.bamlvalue` root dangling.
    pub fn commit(&mut self, batch: RootCommitBatch) -> io::Result<RootCommitOutcome> {
        let mut chunks = BTreeMap::<Cid, DagChunk>::new();
        let mut roots = Vec::with_capacity(batch.dags.len());
        for dag in batch.dags {
            if dag.node_codec_version != super::NODE_CODEC_VERSION {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "root commit received an unsupported node codec version",
                ));
            }
            roots.push(dag.root);
            for chunk in dag.chunks {
                chunks.entry(chunk.cid).or_insert(chunk);
            }
        }

        let chunks_considered = chunks.len();
        let mut chunks_appended = 0_usize;
        for chunk in chunks.values() {
            let outcome = self.pack.append_chunk(chunk)?;
            chunks_appended = chunks_appended.saturating_add(usize::from(outcome.appended));
        }
        if roots.iter().any(|root| !self.pack.contains(*root)) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "root commit DAG omitted its root chunk",
            ));
        }
        self.pack.sync_data()?;

        self.manifest.append_all(roots.iter().copied())?;
        self.manifest.sync_data()?;

        let mut value_record_bytes = 0_usize;
        for record in batch.value_records {
            self.value_file.write_all(&record)?;
            value_record_bytes = value_record_bytes.saturating_add(record.len());
        }
        self.value_file.flush()?;
        self.value_file.sync_data()?;

        Ok(RootCommitOutcome {
            roots_committed: roots.len(),
            chunks_considered,
            chunks_appended,
            value_record_bytes,
        })
    }

    /// Audit-only records name no chunks and therefore need only the root-file
    /// durability barrier.
    pub fn append_audit_record(&mut self, framed_record: &[u8]) -> io::Result<()> {
        self.value_file.write_all(framed_record)?;
        self.value_file.flush()?;
        self.value_file.sync_data()
    }

    pub fn into_parts(self) -> (PackWriter, CidManifestWriter, File) {
        (self.pack, self.manifest, self.value_file)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::{
        ids::BoundaryId,
        value_cas::{
            CanonicalValue, CidManifestReader, CidManifestWriter, PackIndex, PackWriter,
            RootCommitBatch, RootCommitter, encode_value_dag,
        },
    };

    fn temp_root() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "baml-root-commit-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn committed_root_has_durable_pack_and_prior_manifest_entry() {
        let root = temp_root();
        let boundary_dir = root.join("history/boundary");
        fs::create_dir_all(&boundary_dir).unwrap();
        let manifest_path = boundary_dir.join("manifest.bamlcids");
        let value_path = boundary_dir.join("value-1.bamlvalue");
        fs::write(&value_path, b"existing-header").unwrap();
        let pack = PackWriter::create(root.join("store"), [5; 16], 1, 10).unwrap();
        let manifest =
            CidManifestWriter::create(&manifest_path, BoundaryId::from_bytes([6; 16])).unwrap();
        let mut committer = RootCommitter::new(pack, manifest, &value_path).unwrap();
        let dag = encode_value_dag(&CanonicalValue::String("hello".to_string())).unwrap();
        let root_cid = dag.root;
        let outcome = committer
            .commit(RootCommitBatch {
                dags: vec![dag],
                value_records: vec![b"framed-root-record".to_vec()],
            })
            .unwrap();
        assert_eq!(outcome.roots_committed, 1);
        let (pack, manifest, _) = committer.into_parts();
        let pack_paths = pack.seal().unwrap();
        manifest.seal().unwrap();

        let roots = CidManifestReader::read(&manifest_path).unwrap();
        assert_eq!(roots.manifest.cids, vec![root_cid]);
        let index = PackIndex::read(pack_paths.index, pack_paths.pack).unwrap();
        assert!(index.find(root_cid).is_some());
        assert!(
            fs::read(value_path)
                .unwrap()
                .ends_with(b"framed-root-record")
        );
        fs::remove_dir_all(root).unwrap();
    }
}
