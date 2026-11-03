//! The state of a `baml.csv.Reader` and `baml.csv.Writer`: the options a
//! reader or writer was made with, its position, and the errors it has kept.
//! Plain data with no heap values, held by the `@@rust_type` state class of
//! each; the tokenizer and encoder that drive it are the VM's.

use std::sync::Arc;

use crate::{CsvHeader, RetainedFootprint};

/// The kind of a `baml.csv.Error`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Options,
    Quote,
    FieldCount,
    Encoding,
    Header,
    Decode,
    Encode,
    Closed,
}

impl Kind {
    /// The name of the `baml.csv.ErrorKind` variant this kind is.
    pub fn variant_name(self) -> &'static str {
        match self {
            Kind::Options => "Options",
            Kind::Quote => "Quote",
            Kind::FieldCount => "FieldCount",
            Kind::Encoding => "Encoding",
            Kind::Header => "Header",
            Kind::Decode => "Decode",
            Kind::Encode => "Encode",
            Kind::Closed => "Closed",
        }
    }
}

/// Plain-Rust mirror of `baml.csv.Error`, safe to retain in a reader's state
/// (no heap `Value`s). The VM builds the BAML error from it.
#[derive(Clone, Debug)]
pub struct ErrInfo {
    pub kind: Kind,
    pub message: String,
    pub line: Option<i64>,
    pub record: Option<i64>,
    pub field: Option<i64>,
    pub column: Option<String>,
    pub expected: Option<i64>,
    pub found: Option<i64>,
}

impl ErrInfo {
    pub fn new(kind: Kind, message: impl Into<String>) -> Self {
        ErrInfo {
            kind,
            message: message.into(),
            line: None,
            record: None,
            field: None,
            column: None,
            expected: None,
            found: None,
        }
    }

    #[must_use]
    pub fn at(mut self, line: i64, record: i64) -> Self {
        self.line = Some(line);
        self.record = Some(record);
        self
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Ragged {
    Strict,
    Pad,
    Truncate,
}

/// The options a reader parses with, from its `baml.csv.ReaderOptions`.
#[derive(Clone)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is an independent `ReaderOptions` field"
)]
pub struct ReaderOpts {
    pub delimiter: u8,
    pub quote: u8,
    pub quoting: bool,
    pub escape: Option<u8>,
    pub has_header: bool,
    pub headers_override: Option<Vec<String>>,
    pub comment: Option<u8>,
    pub trim_headers: bool,
    pub trim_fields: bool,
    pub skip_lines: i64,
    pub skip_blank: bool,
    pub ragged: Ragged,
    pub null_values: Arc<[String]>,
    pub lossy: bool,
    pub strip_bom: bool,
    pub skip_on_error: bool,
    pub max_skipped: usize,
    pub limit: Option<i64>,
}

impl Default for ReaderOpts {
    fn default() -> Self {
        ReaderOpts {
            delimiter: b',',
            quote: b'"',
            quoting: true,
            escape: None,
            has_header: true,
            headers_override: None,
            comment: None,
            trim_headers: false,
            trim_fields: false,
            skip_lines: 0,
            skip_blank: true,
            ragged: Ragged::Strict,
            null_values: Arc::from(Vec::new()),
            lossy: false,
            strip_bom: true,
            skip_on_error: false,
            max_skipped: 1000,
            limit: None,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum QuoteStyle {
    Minimal,
    All,
    Never,
}

/// The options a writer encodes with, from its `baml.csv.WriterOptions`.
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is an independent `WriterOptions` field"
)]
pub struct WriterOpts {
    pub delimiter: u8,
    pub quote: u8,
    pub style: QuoteStyle,
    pub escape: Option<u8>,
    pub crlf: bool,
    pub auto_header: bool,
    pub headers_override: Option<Vec<String>>,
    pub null_value: String,
    pub bom: bool,
    pub sanitize: bool,
}

impl Default for WriterOpts {
    fn default() -> Self {
        WriterOpts {
            delimiter: b',',
            quote: b'"',
            style: QuoteStyle::Minimal,
            escape: None,
            crlf: false,
            auto_header: true,
            headers_override: None,
            null_value: String::new(),
            bom: false,
            sanitize: false,
        }
    }
}

/// A reader's input, position and kept errors.
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent progress flags of the parse"
)]
pub struct ReaderState {
    pub opts: ReaderOpts,
    pub buf: Vec<u8>,
    pub pos: usize,
    pub eof: bool,
    pub bom_done: bool,
    pub lines_to_skip: i64,
    pub header: Option<Arc<CsvHeader>>,
    pub header_done: bool,
    pub header_error: Option<ErrInfo>,
    pub expected_width: Option<usize>,
    /// Position of the next unread byte/record.
    pub byte: i64,
    pub line: i64,
    pub record: i64,
    pub yielded: i64,
    pub skipped: Vec<ErrInfo>,
    pub skipped_count: i64,
    pub closed: bool,
    pub finished: bool,
}

impl ReaderState {
    pub fn new(opts: ReaderOpts, initial: Vec<u8>, eof: bool) -> Self {
        let lines_to_skip = opts.skip_lines;
        ReaderState {
            opts,
            buf: initial,
            pos: 0,
            eof,
            bom_done: false,
            lines_to_skip,
            header: None,
            header_done: false,
            header_error: None,
            expected_width: None,
            byte: 0,
            line: 1,
            record: 0,
            yielded: 0,
            skipped: Vec::new(),
            skipped_count: 0,
            closed: false,
            finished: false,
        }
    }

    pub fn register_skip(&mut self, info: &ErrInfo) {
        self.skipped_count += 1;
        if self.skipped.len() < self.opts.max_skipped {
            self.skipped.push(info.clone());
        }
    }

    /// Consume `n` bytes (with `nl` line terminators among them) and drop the
    /// consumed prefix once it grows large.
    pub fn consume(&mut self, n: usize, nl: i64) {
        self.pos += n;
        self.byte += i64::try_from(n)
            .unwrap_or_else(|_| unreachable!("a buffered chunk is far smaller than `i64::MAX`"));
        self.line += nl;
        if self.pos > 64 * 1024 && self.pos * 2 > self.buf.len() {
            self.buf.drain(..self.pos);
            self.pos = 0;
        }
    }
}

impl RetainedFootprint for ReaderState {
    fn retained_bytes(&self) -> usize {
        let mut meter = crate::Meter::charge();
        meter.bytes(self.buf.capacity());
        meter.bytes(self.skipped.capacity() * size_of::<ErrInfo>());
        for error in self.skipped.iter().chain(self.header_error.iter()) {
            meter.bytes(error.message.capacity());
            meter.bytes(error.column.as_ref().map_or(0, String::capacity));
        }
        if let Some(headers) = &self.opts.headers_override {
            meter.bytes(headers.capacity() * size_of::<String>());
            for header in headers {
                meter.bytes(header.capacity());
            }
        }
        if let Some(header) = &self.header {
            meter.shared(header, |meter| header.measure(meter));
        }
        meter.shared(&self.opts.null_values, |meter| {
            for value in self.opts.null_values.iter() {
                meter.bytes(value.capacity());
            }
        });
        meter.total()
    }
}

pub struct WriterState {
    pub opts: WriterOpts,
    pub header_written: bool,
    pub records_written: i64,
    pub bytes_written: i64,
    /// `Some` for buffer writers; file writers stream through `_emit`.
    pub buffer: Option<String>,
    pub bom_pending: bool,
    pub closed: bool,
}

impl WriterState {
    pub fn new(opts: WriterOpts, buffered: bool) -> Self {
        let bom_pending = opts.bom;
        WriterState {
            opts,
            header_written: false,
            records_written: 0,
            bytes_written: 0,
            buffer: buffered.then(String::new),
            bom_pending,
            closed: false,
        }
    }
}

impl RetainedFootprint for WriterState {
    fn retained_bytes(&self) -> usize {
        self.buffer.as_ref().map_or(0, String::capacity)
    }
}
