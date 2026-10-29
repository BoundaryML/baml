//! The state of a `baml.csv.Record`: one parsed record, read-only once parsed.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use crate::Meter;

/// A CSV header row, shared by every record a reader produces after it.
#[derive(Debug)]
pub struct CsvHeader {
    pub names: Vec<String>,
    /// First index per name.
    pub index: HashMap<String, usize>,
    /// Names appearing more than once.
    pub dup: HashSet<String>,
}

impl CsvHeader {
    pub fn new(names: Vec<String>) -> Self {
        let mut index = HashMap::new();
        let mut dup = HashSet::new();
        for (i, n) in names.iter().enumerate() {
            if index.contains_key(n) {
                dup.insert(n.clone());
            } else {
                index.insert(n.clone(), i);
            }
        }
        CsvHeader { names, index, dup }
    }

    /// Report the names and indexes.
    pub fn measure(&self, meter: &mut Meter) {
        meter.bytes(self.names.capacity() * size_of::<String>());
        meter.bytes(self.index.capacity() * size_of::<(String, usize)>());
        meter.bytes(self.dup.capacity() * size_of::<String>());
        for name in self
            .names
            .iter()
            .chain(self.index.keys())
            .chain(self.dup.iter())
        {
            meter.bytes(name.capacity());
        }
    }
}

/// One cell of a record, with whether its text was quoted.
#[derive(Debug)]
pub struct CsvCell {
    pub text: String,
    pub quoted: bool,
}

/// One parsed record and where it was read.
#[derive(Debug)]
pub struct CsvRecord {
    pub cells: Vec<CsvCell>,
    pub header: Option<Arc<CsvHeader>>,
    pub null_values: Arc<[String]>,
    pub byte: i64,
    pub line: i64,
    pub record: i64,
}

impl CsvRecord {
    /// Report the cells, and the header and null spellings shared with the
    /// reader that produced the record.
    pub fn measure(&self, meter: &mut Meter) {
        meter.bytes(self.cells.capacity() * size_of::<CsvCell>());
        for cell in &self.cells {
            meter.bytes(cell.text.capacity());
        }
        if let Some(header) = &self.header {
            meter.shared(header, |meter| header.measure(meter));
        }
        meter.shared(&self.null_values, |meter| {
            for value in self.null_values.iter() {
                meter.bytes(value.capacity());
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_counts_text_and_deduplicates_shared_header_and_null_values() {
        let header = Arc::new(CsvHeader::new(vec!["h".repeat(10_000)]));
        let null_values: Arc<[String]> = Arc::from(vec!["n".repeat(20_000)]);
        let record = CsvRecord {
            cells: vec![CsvCell {
                text: "c".repeat(30_000),
                quoted: false,
            }],
            header: Some(header),
            null_values,
            byte: 0,
            line: 1,
            record: 1,
        };
        let mut meter = Meter::census();
        record.measure(&mut meter);
        let first = meter.total();
        assert!(first >= 70_000);
        record.measure(&mut meter);
        assert_eq!(
            meter.total() - first,
            record.cells.capacity() * size_of::<CsvCell>() + 30_000
        );
    }
}
