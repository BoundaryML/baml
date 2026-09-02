//! Native handlers for `baml.csv` (BEP-060).
//!
//! Architecture: the streaming reader is the only parser. All tokenization,
//! typed decoding, and encoding happens here, synchronously, against an
//! in-memory byte buffer held in the handle's `RustData` state. File IO is
//! *not* done here: the BAML-level method bodies in `ns_csv/csv.baml` pump
//! chunks from a `baml.fs.File` into `_feed` / `_feed_eof` whenever a native
//! poll returns the `_NeedData` marker. That keeps every heap `Value` (the
//! `File` handle, the `on_skip` closure) in regular GC-traced instance
//! fields — the `RustData` state holds plain Rust data only.
//!
//! Error values are `baml.csv.Error` instances built from plain-Rust
//! [`ErrInfo`] records; skip diagnostics are retained as `ErrInfo` and
//! materialized on demand by `skipped()`.

// Pedantic-lint posture for this module:
// - `struct_excessive_bools`: the option structs mirror `ReaderOptions` /
//   `WriterOptions`, whose independent boolean knobs are fixed by BEP-060.
// - `cast_possible_wrap`: every `usize as i64` here is a cell/record/byte
//   count bounded far below 2^62 (BAML int range checks would fail first).
// - `result_large_err`: error values are cold-path BAML exception payloads;
//   boxing them would only add noise on the hot Ok path.
// - `used_underscore_items`: the codegen names `$rust_type` accessors and
//   internal stdlib methods with a leading underscore (`_handle`, `_poll`);
//   calling them is the entire point of this module.
#![allow(
    clippy::struct_excessive_bools,
    clippy::cast_possible_wrap,
    clippy::result_large_err,
    clippy::used_underscore_items
)]

use std::{
    collections::{HashMap, HashSet},
    fmt::Write as _,
    sync::{Arc, Mutex, MutexGuard},
};

use bex_heap::TlabHolder;
use bex_vm_types::{
    ValueKind,
    types::{Instance, Object, Value},
};
use indexmap::IndexMap;
use time::{
    Date, OffsetDateTime, PrimitiveDateTime,
    format_description::well_known::{Iso8601, Rfc2822, Rfc3339},
};

use super::{
    BamlClassCsvReader, BamlClassCsvRecord, BamlClassCsvWriter, BamlNamespaceCsv, PackageBamlImpl,
    copy, view,
};
use crate::{
    BexVm,
    errors::{VmInternalError, VmRustFnError},
};

const CSV_ERROR_KIND_FQN: &str = "baml.csv.ErrorKind";
const ITER_DONE_FQN: &str = "baml.iter.Done";
use baml_type::typetag::TypeTag;

const INSTANT_FQN: &str = "baml.time.Instant";
const PLAINDATE_FQN: &str = "baml.time.PlainDate";
const PLAINDATETIME_FQN: &str = "baml.time.PlainDateTime";

/// Identify one of the stdlib time classes by head.
///
/// A compiled declaration's tag is content-addressed from its fully-qualified
/// name, so this compares two integers and never renders a name at runtime —
/// and it cannot be spoofed by a runtime declaration that happens to print the
/// same, since those draw counter tags from a disjoint range.
fn is_class(head: bex_vm_types::TypeHead, fq_name: &str) -> bool {
    head.tag() == TypeTag::of_head(fq_name)
}

// =============================================================================
// Error plumbing
// =============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
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
    fn variant_name(self) -> &'static str {
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

/// Plain-Rust mirror of `baml.csv.Error`, safe to retain inside `RustData`
/// state (no heap `Value`s). Materialized via [`error_value`].
#[derive(Clone, Debug)]
struct ErrInfo {
    kind: Kind,
    message: String,
    line: Option<i64>,
    record: Option<i64>,
    field: Option<i64>,
    column: Option<String>,
    expected: Option<i64>,
    found: Option<i64>,
}

impl ErrInfo {
    fn new(kind: Kind, message: impl Into<String>) -> Self {
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

    fn at(mut self, line: i64, record: i64) -> Self {
        self.line = Some(line);
        self.record = Some(record);
        self
    }
}

fn opt_int_value(o: Option<i64>) -> Value {
    o.and_then(Value::try_int).unwrap_or(Value::NULL)
}

fn error_value(vm: &mut BexVm, e: &ErrInfo) -> Result<Value, VmRustFnError> {
    let enm_ptr = vm.lookup_type_by_fqn(CSV_ERROR_KIND_FQN).ok_or_else(|| {
        VmInternalError::MissingNativeFunction {
            name: CSV_ERROR_KIND_FQN.to_string(),
        }
    })?;
    let idx = match vm.get_object(enm_ptr) {
        Object::Enum(en) => en
            .variants
            .iter()
            .position(|v| v.name == e.kind.variant_name()),
        _ => None,
    }
    .ok_or_else(|| VmInternalError::MissingNativeFunction {
        name: format!("{CSV_ERROR_KIND_FQN}.{}", e.kind.variant_name()),
    })?;
    let kind = Value::object(vm.alloc_variant(enm_ptr, idx));
    let message = Value::object(vm.alloc_string(e.message.clone()));
    let column = match &e.column {
        Some(c) => Value::object(vm.alloc_string(c.clone())),
        None => Value::NULL,
    };
    Ok(copy::csv::Error {
        kind,
        message,
        line: opt_int_value(e.line),
        record: opt_int_value(e.record),
        field: opt_int_value(e.field),
        column,
        expected: opt_int_value(e.expected),
        found: opt_int_value(e.found),
    }
    .to_value(vm))
}

fn throw_err(vm: &mut BexVm, e: &ErrInfo) -> VmRustFnError {
    match error_value(vm, e) {
        Ok(v) => VmRustFnError::thrown_fresh(v),
        Err(fatal) => fatal,
    }
}

fn need_data_value(vm: &mut BexVm) -> Value {
    let class_ptr = vm.resolve_class("baml.csv._NeedData");
    Value::object(vm.alloc_instance(class_ptr, vec![]))
}

fn done_value(vm: &mut BexVm) -> Result<Value, VmRustFnError> {
    let class_ptr = vm.lookup_type_by_fqn(ITER_DONE_FQN).ok_or_else(|| {
        VmInternalError::MissingNativeFunction {
            name: ITER_DONE_FQN.to_string(),
        }
    })?;
    Ok(Value::object(vm.alloc_instance(class_ptr, vec![])))
}

// =============================================================================
// Handle-state access
// =============================================================================

/// Clone the `Arc` out of a `$rust_type` field so the caller can lock it while
/// still holding `&mut BexVm` for allocation.
fn state_arc<T: Send + Sync + 'static>(
    vm: &BexVm,
    holder: Value,
    field_idx: usize,
) -> Result<Arc<T>, VmRustFnError> {
    let inst = vm.as_instance(&holder)?;
    let handle = inst.load_field(field_idx);
    let ptr = handle
        .as_object_ptr()
        .ok_or_else(|| VmInternalError::MissingNativeFunction {
            name: "csv handle field is not an object".to_string(),
        })?;
    match vm.get_object(ptr) {
        Object::RustData(arc) => arc.clone().downcast::<T>().map_err(|_| {
            VmRustFnError::InternalError(VmInternalError::MissingNativeFunction {
                name: "csv handle holds an unexpected Rust type".to_string(),
            })
        }),
        _ => Err(VmRustFnError::InternalError(
            VmInternalError::MissingNativeFunction {
                name: "csv handle field is not RustData".to_string(),
            },
        )),
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

// =============================================================================
// Options
// =============================================================================

#[derive(Clone, Copy, PartialEq)]
enum Ragged {
    Strict,
    Pad,
    Truncate,
}

#[derive(Clone)]
struct ReaderOpts {
    delimiter: u8,
    quote: u8,
    quoting: bool,
    escape: Option<u8>,
    has_header: bool,
    headers_override: Option<Vec<String>>,
    comment: Option<u8>,
    trim_headers: bool,
    trim_fields: bool,
    skip_lines: i64,
    skip_blank: bool,
    ragged: Ragged,
    null_values: Arc<[String]>,
    lossy: bool,
    strip_bom: bool,
    skip_on_error: bool,
    max_skipped: usize,
    limit: Option<i64>,
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
enum QuoteStyle {
    Minimal,
    All,
    Never,
}

struct WriterOpts {
    delimiter: u8,
    quote: u8,
    style: QuoteStyle,
    escape: Option<u8>,
    crlf: bool,
    auto_header: bool,
    headers_override: Option<Vec<String>>,
    null_value: String,
    bom: bool,
    sanitize: bool,
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

fn options_err(vm: &mut BexVm, message: impl Into<String>) -> VmRustFnError {
    throw_err(vm, &ErrInfo::new(Kind::Options, message))
}

/// Read an instance field by name (instance fields are positional; the class
/// declaration provides the name → index mapping).
fn field_by_name(vm: &BexVm, inst_val: Value, name: &str) -> Result<Value, VmRustFnError> {
    let inst = vm.as_instance(&inst_val)?;
    let class_ptr = inst.class;
    let idx = match vm.get_object(class_ptr) {
        Object::Class(c) => c.fields.iter().position(|f| f.name == name),
        _ => None,
    }
    .ok_or_else(|| VmInternalError::MissingNativeFunction {
        name: format!("csv options field `{name}` not found"),
    })?;
    Ok(inst.load_field(idx))
}

fn opt_string(vm: &BexVm, v: Value) -> Result<Option<String>, VmRustFnError> {
    if v.is_null() || v.is_omitted() {
        return Ok(None);
    }
    Ok(Some(vm.as_string(&v)?.as_str().to_string()))
}

fn opt_bool(v: Value) -> Option<bool> {
    match v.kind() {
        ValueKind::Bool(b) => Some(b),
        _ => None,
    }
}

fn opt_int(v: Value) -> Option<i64> {
    match v.kind() {
        ValueKind::Int(i) => Some(i),
        _ => None,
    }
}

fn opt_string_list(vm: &BexVm, v: Value) -> Result<Option<Vec<String>>, VmRustFnError> {
    if v.is_null() || v.is_omitted() {
        return Ok(None);
    }
    let arr = vm.as_array(&v)?.to_vec();
    let mut out = Vec::with_capacity(arr.len());
    for item in &arr {
        out.push(vm.as_string(item)?.as_str().to_string());
    }
    Ok(Some(out))
}

fn single_ascii_byte(vm: &mut BexVm, name: &str, s: &str) -> Result<u8, VmRustFnError> {
    let bytes = s.as_bytes();
    if bytes.len() != 1 || !bytes[0].is_ascii() {
        return Err(options_err(
            vm,
            format!("`{name}` must be exactly one ASCII byte, got {s:?}"),
        ));
    }
    let b = bytes[0];
    if b == b'\r' || b == b'\n' {
        return Err(options_err(
            vm,
            format!("`{name}` must not be a CR or LF byte"),
        ));
    }
    Ok(b)
}

fn check_distinct(vm: &mut BexVm, named: &[(&str, Option<u8>)]) -> Result<(), VmRustFnError> {
    for (i, (name_a, a)) in named.iter().enumerate() {
        let Some(a) = a else { continue };
        for (name_b, b) in named.iter().skip(i + 1) {
            if Some(*a) == *b {
                return Err(options_err(
                    vm,
                    format!("`{name_a}` and `{name_b}` must be distinct bytes"),
                ));
            }
        }
    }
    Ok(())
}

fn parse_reader_options(
    vm: &mut BexVm,
    options: Option<&Value>,
) -> Result<ReaderOpts, VmRustFnError> {
    let mut o = ReaderOpts::default();
    let Some(opts_val) = options else {
        return Ok(o);
    };
    let opts_val = *opts_val;

    if let Some(s) = opt_string(vm, field_by_name(vm, opts_val, "delimiter")?)? {
        o.delimiter = single_ascii_byte(vm, "delimiter", &s)?;
    }
    if let Some(s) = opt_string(vm, field_by_name(vm, opts_val, "quote")?)? {
        o.quote = single_ascii_byte(vm, "quote", &s)?;
    }
    if let Some(b) = opt_bool(field_by_name(vm, opts_val, "quoting")?) {
        o.quoting = b;
    }
    if let Some(s) = opt_string(vm, field_by_name(vm, opts_val, "escape")?)? {
        o.escape = Some(single_ascii_byte(vm, "escape", &s)?);
    }
    if let Some(b) = opt_bool(field_by_name(vm, opts_val, "has_header")?) {
        o.has_header = b;
    }
    o.headers_override = opt_string_list(vm, field_by_name(vm, opts_val, "headers")?)?;
    if let Some(s) = opt_string(vm, field_by_name(vm, opts_val, "comment")?)? {
        o.comment = Some(single_ascii_byte(vm, "comment", &s)?);
    }
    if let Some(s) = opt_string(vm, field_by_name(vm, opts_val, "trim")?)? {
        match s.as_str() {
            "none" => {}
            "headers" => o.trim_headers = true,
            "fields" => o.trim_fields = true,
            "all" => {
                o.trim_headers = true;
                o.trim_fields = true;
            }
            other => {
                return Err(options_err(vm, format!("invalid `trim` value {other:?}")));
            }
        }
    }
    if let Some(n) = opt_int(field_by_name(vm, opts_val, "skip_lines")?) {
        if n < 0 {
            return Err(options_err(vm, "`skip_lines` must be non-negative"));
        }
        o.skip_lines = n;
    }
    if let Some(b) = opt_bool(field_by_name(vm, opts_val, "skip_blank_records")?) {
        o.skip_blank = b;
    }
    if let Some(s) = opt_string(vm, field_by_name(vm, opts_val, "ragged")?)? {
        o.ragged = match s.as_str() {
            "strict" => Ragged::Strict,
            "pad" => Ragged::Pad,
            "truncate" => Ragged::Truncate,
            other => {
                return Err(options_err(vm, format!("invalid `ragged` value {other:?}")));
            }
        };
    }
    if let Some(list) = opt_string_list(vm, field_by_name(vm, opts_val, "null_values")?)? {
        o.null_values = Arc::from(list);
    }
    if let Some(s) = opt_string(vm, field_by_name(vm, opts_val, "encoding")?)? {
        o.lossy = match s.as_str() {
            "utf8" => false,
            "utf8-lossy" => true,
            other => {
                return Err(options_err(
                    vm,
                    format!("invalid `encoding` value {other:?}"),
                ));
            }
        };
    }
    if let Some(s) = opt_string(vm, field_by_name(vm, opts_val, "bom")?)? {
        o.strip_bom = match s.as_str() {
            "strip" => true,
            "keep" => false,
            other => {
                return Err(options_err(vm, format!("invalid `bom` value {other:?}")));
            }
        };
    }
    if let Some(s) = opt_string(vm, field_by_name(vm, opts_val, "on_error")?)? {
        o.skip_on_error = match s.as_str() {
            "throw" => false,
            "skip" => true,
            other => {
                return Err(options_err(
                    vm,
                    format!("invalid `on_error` value {other:?}"),
                ));
            }
        };
    }
    if let Some(n) = opt_int(field_by_name(vm, opts_val, "max_skipped")?) {
        if n < 0 {
            return Err(options_err(vm, "`max_skipped` must be non-negative"));
        }
        o.max_skipped = usize::try_from(n).unwrap_or(usize::MAX);
    }
    if let Some(n) = opt_int(field_by_name(vm, opts_val, "limit")?) {
        if n < 0 {
            return Err(options_err(vm, "`limit` must be non-negative"));
        }
        o.limit = Some(n);
    }

    check_distinct(
        vm,
        &[
            ("delimiter", Some(o.delimiter)),
            ("quote", Some(o.quote)),
            ("escape", o.escape),
            ("comment", o.comment),
        ],
    )?;
    Ok(o)
}

fn parse_writer_options(
    vm: &mut BexVm,
    options: Option<&Value>,
) -> Result<WriterOpts, VmRustFnError> {
    let mut o = WriterOpts::default();
    let Some(opts_val) = options else {
        return Ok(o);
    };
    let opts_val = *opts_val;

    if let Some(s) = opt_string(vm, field_by_name(vm, opts_val, "delimiter")?)? {
        o.delimiter = single_ascii_byte(vm, "delimiter", &s)?;
    }
    if let Some(s) = opt_string(vm, field_by_name(vm, opts_val, "quote")?)? {
        o.quote = single_ascii_byte(vm, "quote", &s)?;
    }
    if let Some(s) = opt_string(vm, field_by_name(vm, opts_val, "quote_style")?)? {
        o.style = match s.as_str() {
            "minimal" => QuoteStyle::Minimal,
            "all" => QuoteStyle::All,
            "never" => QuoteStyle::Never,
            other => {
                return Err(options_err(
                    vm,
                    format!("invalid `quote_style` value {other:?}"),
                ));
            }
        };
    }
    if let Some(s) = opt_string(vm, field_by_name(vm, opts_val, "escape")?)? {
        o.escape = Some(single_ascii_byte(vm, "escape", &s)?);
    }
    if let Some(s) = opt_string(vm, field_by_name(vm, opts_val, "terminator")?)? {
        o.crlf = match s.as_str() {
            "lf" => false,
            "crlf" => true,
            other => {
                return Err(options_err(
                    vm,
                    format!("invalid `terminator` value {other:?}"),
                ));
            }
        };
    }
    if let Some(b) = opt_bool(field_by_name(vm, opts_val, "write_header")?) {
        o.auto_header = b;
    }
    o.headers_override = opt_string_list(vm, field_by_name(vm, opts_val, "headers")?)?;
    if let Some(s) = opt_string(vm, field_by_name(vm, opts_val, "null_value")?)? {
        o.null_value = s;
    }
    if let Some(b) = opt_bool(field_by_name(vm, opts_val, "bom")?) {
        o.bom = b;
    }
    if let Some(b) = opt_bool(field_by_name(vm, opts_val, "sanitize_formulas")?) {
        o.sanitize = b;
    }

    check_distinct(
        vm,
        &[
            ("delimiter", Some(o.delimiter)),
            ("quote", Some(o.quote)),
            ("escape", o.escape),
        ],
    )?;
    Ok(o)
}

// =============================================================================
// Reader state and tokenizer
// =============================================================================

struct Header {
    names: Vec<String>,
    /// First index per name.
    index: HashMap<String, usize>,
    /// Names appearing more than once.
    dup: HashSet<String>,
}

impl Header {
    fn new(names: Vec<String>) -> Self {
        let mut index = HashMap::new();
        let mut dup = HashSet::new();
        for (i, n) in names.iter().enumerate() {
            if index.contains_key(n) {
                dup.insert(n.clone());
            } else {
                index.insert(n.clone(), i);
            }
        }
        Header { names, index, dup }
    }
}

struct CellData {
    text: String,
    quoted: bool,
}

/// Snapshot of one parsed record. Plain Rust only — safe in `RustData`.
struct RecordData {
    cells: Vec<CellData>,
    header: Option<Arc<Header>>,
    null_values: Arc<[String]>,
    byte: i64,
    line: i64,
    record: i64,
}

struct ReaderState {
    opts: ReaderOpts,
    buf: Vec<u8>,
    pos: usize,
    eof: bool,
    bom_done: bool,
    lines_to_skip: i64,
    header: Option<Arc<Header>>,
    header_done: bool,
    header_error: Option<ErrInfo>,
    expected_width: Option<usize>,
    /// Position of the next unread byte/record.
    byte: i64,
    line: i64,
    record: i64,
    yielded: i64,
    skipped: Vec<ErrInfo>,
    skipped_count: i64,
    closed: bool,
    finished: bool,
}

impl ReaderState {
    fn new(opts: ReaderOpts, initial: Vec<u8>, eof: bool) -> Self {
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

    fn register_skip(&mut self, info: &ErrInfo) {
        self.skipped_count += 1;
        if self.skipped.len() < self.opts.max_skipped {
            self.skipped.push(info.clone());
        }
    }

    /// Consume `n` bytes (with `nl` line terminators among them) and drop the
    /// consumed prefix once it grows large.
    fn consume(&mut self, n: usize, nl: i64) {
        self.pos += n;
        self.byte += n as i64;
        self.line += nl;
        if self.pos > 64 * 1024 && self.pos * 2 > self.buf.len() {
            self.buf.drain(..self.pos);
            self.pos = 0;
        }
    }
}

struct RawCell {
    bytes: Vec<u8>,
    quoted: bool,
}

enum Scan {
    NeedData,
    Eof,
    /// A non-data line (blank record or comment line).
    Skip {
        consumed: usize,
        lines: i64,
    },
    Record {
        cells: Vec<RawCell>,
        consumed: usize,
        lines: i64,
    },
    Malformed {
        kind: Kind,
        message: String,
        consumed: usize,
        lines: i64,
        exhausted: bool,
    },
}

/// Find the end of a raw (not quote-aware) line starting at `start`.
/// Returns `(consumed_including_terminator, terminator_count)`.
fn find_raw_line_end(buf: &[u8], start: usize, eof: bool) -> Option<(usize, i64)> {
    let mut i = start;
    while i < buf.len() {
        match buf[i] {
            b'\n' => return Some((i + 1 - start, 1)),
            b'\r' => {
                if i + 1 < buf.len() {
                    let len = if buf[i + 1] == b'\n' { 2 } else { 1 };
                    return Some((i + len - start, 1));
                }
                if eof {
                    return Some((i + 1 - start, 1));
                }
                return None; // need to see whether \n follows
            }
            _ => i += 1,
        }
    }
    if eof {
        Some((buf.len() - start, 0))
    } else {
        None
    }
}

fn is_ws(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

fn finish_cell(
    cells: &mut Vec<RawCell>,
    cell: &mut Vec<u8>,
    cell_quoted: &mut bool,
    after_close: &mut bool,
    trim: bool,
) {
    let mut bytes = std::mem::take(cell);
    if trim && !*cell_quoted {
        while bytes.last().copied().is_some_and(is_ws) {
            bytes.pop();
        }
        // One drain, not remove(0) per byte: a multi-MB run of leading
        // whitespace must stay O(n), not O(n^2), inside this native call.
        let lead = bytes.iter().position(|b| !is_ws(*b)).unwrap_or(bytes.len());
        bytes.drain(..lead);
    }
    cells.push(RawCell {
        bytes,
        quoted: *cell_quoted,
    });
    *cell_quoted = false;
    *after_close = false;
}

/// Scan one record starting at `start`, quote-aware. Does not mutate state;
/// the caller consumes the reported byte count.
#[allow(clippy::too_many_lines)]
fn scan_record(buf: &[u8], start: usize, eof: bool, o: &ReaderOpts, trim: bool) -> Scan {
    if start >= buf.len() {
        return if eof { Scan::Eof } else { Scan::NeedData };
    }

    // Comment lines are raw lines: a comment is not valid CSV.
    if let Some(c) = o.comment {
        if buf[start] == c {
            return match find_raw_line_end(buf, start, eof) {
                None => Scan::NeedData,
                Some((consumed, nl)) => Scan::Skip {
                    consumed,
                    lines: nl,
                },
            };
        }
    }

    let mut cells: Vec<RawCell> = Vec::new();
    let mut cell: Vec<u8> = Vec::new();
    let mut cell_quoted = false;
    let mut in_quotes = false;
    let mut after_close = false;
    let mut lines: i64 = 0;
    let mut err: Option<(Kind, String)> = None;
    let mut i = start;
    let span_end;

    loop {
        if i >= buf.len() {
            if !eof {
                return Scan::NeedData;
            }
            if in_quotes {
                let (kind, message) = err.unwrap_or((
                    Kind::Quote,
                    "unterminated quote at end of input".to_string(),
                ));
                return Scan::Malformed {
                    kind,
                    message,
                    consumed: buf.len() - start,
                    lines,
                    exhausted: true,
                };
            }
            span_end = i;
            finish_cell(
                &mut cells,
                &mut cell,
                &mut cell_quoted,
                &mut after_close,
                trim,
            );
            break;
        }
        let b = buf[i];

        if in_quotes {
            if let Some(e) = o.escape {
                if b == e {
                    if i + 1 >= buf.len() {
                        if !eof {
                            return Scan::NeedData;
                        }
                        err.get_or_insert((
                            Kind::Quote,
                            "dangling escape at end of input".to_string(),
                        ));
                        i += 1;
                        continue;
                    }
                    if buf[i + 1] == b'\n' {
                        lines += 1;
                    }
                    cell.push(buf[i + 1]);
                    i += 2;
                    continue;
                }
            }
            if b == o.quote {
                if o.escape.is_none() {
                    if i + 1 >= buf.len() && !eof {
                        return Scan::NeedData; // `""` vs closing quote undecided
                    }
                    if i + 1 < buf.len() && buf[i + 1] == o.quote {
                        cell.push(o.quote);
                        i += 2;
                        continue;
                    }
                }
                in_quotes = false;
                after_close = true;
                i += 1;
                continue;
            }
            if b == b'\n' {
                lines += 1;
            }
            cell.push(b);
            i += 1;
            continue;
        }

        if b == o.delimiter {
            finish_cell(
                &mut cells,
                &mut cell,
                &mut cell_quoted,
                &mut after_close,
                trim,
            );
            i += 1;
            continue;
        }
        if b == b'\r' {
            if i + 1 >= buf.len() && !eof {
                return Scan::NeedData; // need to see whether \n follows
            }
            span_end = i;
            finish_cell(
                &mut cells,
                &mut cell,
                &mut cell_quoted,
                &mut after_close,
                trim,
            );
            lines += 1;
            i += if i + 1 < buf.len() && buf[i + 1] == b'\n' {
                2
            } else {
                1
            };
            break;
        }
        if b == b'\n' {
            span_end = i;
            finish_cell(
                &mut cells,
                &mut cell,
                &mut cell_quoted,
                &mut after_close,
                trim,
            );
            lines += 1;
            i += 1;
            break;
        }
        if after_close {
            if trim && is_ws(b) {
                i += 1;
                continue;
            }
            err.get_or_insert((
                Kind::Quote,
                "unexpected data after closing quote".to_string(),
            ));
            after_close = false;
            cell.push(b);
            i += 1;
            continue;
        }
        if o.quoting && b == o.quote && !cell_quoted {
            if cell.is_empty() || (trim && cell.iter().copied().all(is_ws)) {
                cell.clear();
                in_quotes = true;
                cell_quoted = true;
                i += 1;
                continue;
            }
            err.get_or_insert((Kind::Quote, "stray quote in unquoted field".to_string()));
            cell.push(b);
            i += 1;
            continue;
        }
        cell.push(b);
        i += 1;
    }

    let consumed = i - start;
    if let Some((kind, message)) = err {
        return Scan::Malformed {
            kind,
            message,
            consumed,
            lines,
            exhausted: false,
        };
    }

    // Blank-record detection on the raw span (before ragged / decode).
    let raw = &buf[start..span_end];
    let blank = raw.is_empty() || (trim && raw.iter().copied().all(is_ws));
    if blank && o.skip_blank {
        return Scan::Skip { consumed, lines };
    }

    Scan::Record {
        cells,
        consumed,
        lines,
    }
}

fn cells_to_text(cells: Vec<RawCell>, lossy: bool) -> Result<Vec<CellData>, (usize, String)> {
    let mut out = Vec::with_capacity(cells.len());
    for (i, c) in cells.into_iter().enumerate() {
        let text = if lossy {
            String::from_utf8_lossy(&c.bytes).into_owned()
        } else {
            String::from_utf8(c.bytes)
                .map_err(|e| (i, format!("invalid UTF-8 in field {i}: {e}")))?
        };
        out.push(CellData {
            text,
            quoted: c.quoted,
        });
    }
    Ok(out)
}

enum Polled {
    NeedData,
    Done,
    Skipped(ErrInfo),
    Rec(RecordData),
}

enum HeadersPolled {
    NeedData,
    Ready(Option<Vec<String>>),
}

/// Consume the BOM and `skip_lines` preamble. Returns `false` when more input
/// is required.
fn ensure_preamble(s: &mut ReaderState) -> bool {
    if !s.bom_done {
        if s.opts.strip_bom {
            let avail = s.buf.len() - s.pos;
            if avail < 3 && !s.eof {
                return false;
            }
            if s.buf[s.pos..].starts_with(&[0xEF, 0xBB, 0xBF]) {
                s.consume(3, 0);
            }
        }
        s.bom_done = true;
    }
    while s.lines_to_skip > 0 {
        match find_raw_line_end(&s.buf, s.pos, s.eof) {
            None => return false,
            Some((consumed, nl)) => {
                // A preamble line with no trailing terminator still counts as
                // a consumed line.
                s.consume(consumed, if nl > 0 { nl } else { 1 });
                s.lines_to_skip -= 1;
                if consumed == 0 {
                    // EOF with nothing left.
                    s.lines_to_skip = 0;
                }
            }
        }
    }
    true
}

/// Resolve the header (consuming the header record if necessary).
/// `Ok(None)` means more input is required.
fn ensure_header(s: &mut ReaderState) -> Result<Option<()>, ErrInfo> {
    if let Some(info) = &s.header_error {
        return Err(info.clone());
    }
    if s.header_done {
        return Ok(Some(()));
    }
    if !s.opts.has_header {
        s.header = s
            .opts
            .headers_override
            .clone()
            .map(|names| Arc::new(Header::new(names)));
        if let Some(h) = &s.header {
            s.expected_width = Some(h.names.len());
        }
        s.header_done = true;
        return Ok(Some(()));
    }
    loop {
        match scan_record(&s.buf, s.pos, s.eof, &s.opts.clone(), s.opts.trim_headers) {
            Scan::NeedData => return Ok(None),
            Scan::Eof => {
                // Empty input: no header row exists. The `headers` option
                // still names columns for typed decode.
                s.header = s
                    .opts
                    .headers_override
                    .clone()
                    .map(|names| Arc::new(Header::new(names)));
                s.header_done = true;
                return Ok(Some(()));
            }
            Scan::Skip { consumed, lines } => {
                s.consume(consumed, lines);
            }
            Scan::Malformed {
                kind,
                message,
                consumed,
                lines,
                exhausted: _,
            } => {
                let line = s.line;
                s.consume(consumed, lines);
                // A header parse error exhausts the reader: continuing would
                // install the FIRST DATA ROW as the header, silently
                // swallowing it and mis-mapping every name-based access.
                // Header errors are always thrown; `on_error` governs data
                // iteration only.
                let mut info = ErrInfo::new(kind, format!("in header record: {message}"));
                info.line = Some(line);
                s.finished = true;
                s.header_error = Some(info.clone());
                return Err(info);
            }
            Scan::Record {
                cells,
                consumed,
                lines,
            } => {
                let line = s.line;
                s.consume(consumed, lines);
                let cells = match cells_to_text(cells, s.opts.lossy) {
                    Ok(c) => c,
                    Err((field, msg)) => {
                        let mut info =
                            ErrInfo::new(Kind::Encoding, format!("in header record: {msg}"));
                        info.line = Some(line);
                        info.field = Some(field as i64);
                        s.finished = true;
                        s.header_error = Some(info.clone());
                        return Err(info);
                    }
                };
                s.expected_width = Some(cells.len());
                let names = match &s.opts.headers_override {
                    Some(over) => over.clone(),
                    None => cells.into_iter().map(|c| c.text).collect(),
                };
                s.header = Some(Arc::new(Header::new(names)));
                s.header_done = true;
                return Ok(Some(()));
            }
        }
    }
}

fn poll_record(s: &mut ReaderState) -> Result<Polled, ErrInfo> {
    if s.closed {
        return Err(ErrInfo::new(Kind::Closed, "reader is closed"));
    }
    if s.finished {
        return Ok(Polled::Done);
    }
    if !ensure_preamble(s) {
        return Ok(Polled::NeedData);
    }
    if ensure_header(s)?.is_none() {
        return Ok(Polled::NeedData);
    }
    if let Some(limit) = s.opts.limit {
        if s.yielded >= limit {
            s.finished = true;
            return Ok(Polled::Done);
        }
    }
    loop {
        match scan_record(&s.buf, s.pos, s.eof, &s.opts.clone(), s.opts.trim_fields) {
            Scan::NeedData => return Ok(Polled::NeedData),
            Scan::Eof => {
                s.finished = true;
                return Ok(Polled::Done);
            }
            Scan::Skip { consumed, lines } => {
                s.consume(consumed, lines);
            }
            Scan::Malformed {
                kind,
                message,
                consumed,
                lines,
                exhausted,
            } => {
                let info = ErrInfo::new(kind, message).at(s.line, s.record);
                s.consume(consumed, lines);
                s.record += 1;
                if exhausted {
                    s.finished = true;
                }
                if s.opts.skip_on_error {
                    s.register_skip(&info);
                    return Ok(Polled::Skipped(info));
                }
                return Err(info);
            }
            Scan::Record {
                cells,
                consumed,
                lines,
            } => {
                let start_byte = s.byte;
                let start_line = s.line;
                let record_idx = s.record;
                s.consume(consumed, lines);
                s.record += 1;

                let mut cells = match cells_to_text(cells, s.opts.lossy) {
                    Ok(c) => c,
                    Err((field, msg)) => {
                        let mut info = ErrInfo::new(Kind::Encoding, msg).at(start_line, record_idx);
                        info.field = Some(field as i64);
                        if s.opts.skip_on_error {
                            s.register_skip(&info);
                            return Ok(Polled::Skipped(info));
                        }
                        return Err(info);
                    }
                };

                // Ragged policy against the expected width.
                let expected = *s.expected_width.get_or_insert(cells.len());
                if cells.len() != expected {
                    let fail = match s.opts.ragged {
                        Ragged::Strict => true,
                        Ragged::Pad => {
                            if cells.len() < expected {
                                while cells.len() < expected {
                                    cells.push(CellData {
                                        text: String::new(),
                                        quoted: false,
                                    });
                                }
                                false
                            } else {
                                true
                            }
                        }
                        Ragged::Truncate => {
                            if cells.len() > expected {
                                cells.truncate(expected);
                                false
                            } else {
                                true
                            }
                        }
                    };
                    if fail {
                        let mut info = ErrInfo::new(
                            Kind::FieldCount,
                            format!("record has {} fields, expected {expected}", cells.len()),
                        )
                        .at(start_line, record_idx);
                        info.expected = Some(expected as i64);
                        info.found = Some(cells.len() as i64);
                        if s.opts.skip_on_error {
                            s.register_skip(&info);
                            return Ok(Polled::Skipped(info));
                        }
                        return Err(info);
                    }
                }

                s.yielded += 1;
                return Ok(Polled::Rec(RecordData {
                    cells,
                    header: s.header.clone(),
                    null_values: Arc::clone(&s.opts.null_values),
                    byte: start_byte,
                    line: start_line,
                    record: record_idx,
                }));
            }
        }
    }
}

fn poll_headers(s: &mut ReaderState) -> Result<HeadersPolled, ErrInfo> {
    if s.closed {
        return Err(ErrInfo::new(Kind::Closed, "reader is closed"));
    }
    if let Some(info) = &s.header_error {
        return Err(info.clone());
    }
    if s.finished {
        return Ok(HeadersPolled::Ready(
            s.header.as_ref().map(|h| h.names.clone()),
        ));
    }
    if !ensure_preamble(s) {
        return Ok(HeadersPolled::NeedData);
    }
    match ensure_header(s)? {
        None => Ok(HeadersPolled::NeedData),
        Some(()) => Ok(HeadersPolled::Ready(
            s.header.as_ref().map(|h| h.names.clone()),
        )),
    }
}

// =============================================================================
// Typed cell decoding
// =============================================================================

#[derive(Clone)]
enum Target {
    Str,
    Int,
    Bigint,
    Float,
    Bool,
    // The enum's head: it *is* the declaration pointer, so decoding a cell
    // dereferences it rather than resolving a name — which also means a
    // runtime-declared enum decodes, where a package-index lookup could not
    // find one.
    Enum(bex_vm_types::TypeHead),
    Instant,
    PlainDate,
    PlainDateTime,
}

struct CellTy {
    target: Target,
    nullable: bool,
}

/// A class head's dotted name — for error text only, never as a key.
fn head_key(head: &bex_vm_types::TypeHead) -> String {
    baml_type::HeadDisplay::head_display_name(head)
}

fn classify_cell_ty(ty: &bex_vm_types::RealizedTy) -> Result<CellTy, String> {
    use bex_vm_types::RealizedTy;
    let nullable = ty.is_nullable_union();
    let base = if nullable {
        ty.strip_null()
    } else {
        ty.clone()
    };
    let target = match &base {
        RealizedTy::String { .. } => Target::Str,
        RealizedTy::Int { .. } => Target::Int,
        RealizedTy::Bigint { .. } => Target::Bigint,
        RealizedTy::Float { .. } => Target::Float,
        RealizedTy::Bool { .. } => Target::Bool,
        RealizedTy::Enum(head, _) => Target::Enum(*head),
        RealizedTy::Class(head, _, _) if is_class(*head, INSTANT_FQN) => Target::Instant,
        RealizedTy::Class(head, _, _) if is_class(*head, PLAINDATE_FQN) => Target::PlainDate,
        RealizedTy::Class(head, _, _) if is_class(*head, PLAINDATETIME_FQN) => {
            Target::PlainDateTime
        }
        other => return Err(format!("type `{other}` is not cell-decodable")),
    };
    Ok(CellTy { target, nullable })
}

enum Conv {
    Ok(Value),
    Bad(String),
}

fn is_plain_int_text(t: &str) -> bool {
    let body = t.strip_prefix(['+', '-']).unwrap_or(t);
    !body.is_empty() && body.bytes().all(|b| b.is_ascii_digit())
}

fn convert_cell(vm: &mut BexVm, text: &str, target: &Target) -> Result<Conv, VmRustFnError> {
    Ok(match target {
        Target::Str => Conv::Ok(Value::object(vm.alloc_string(text.to_string()))),
        Target::Int => {
            if !is_plain_int_text(text) {
                return Ok(Conv::Bad(format!("cannot convert {text:?} to int")));
            }
            match text.parse::<i64>().ok().and_then(Value::try_int) {
                Some(v) => Conv::Ok(v),
                None => Conv::Bad(format!("integer {text:?} is outside the BAML int range")),
            }
        }
        Target::Bigint => {
            if !is_plain_int_text(text) {
                return Ok(Conv::Bad(format!("cannot convert {text:?} to bigint")));
            }
            match num_bigint::BigInt::parse_bytes(text.as_bytes(), 10) {
                Some(b) => Conv::Ok(vm.try_alloc_bigint(Arc::new(b))?),
                None => Conv::Bad(format!("cannot convert {text:?} to bigint")),
            }
        }
        Target::Float => {
            let body = text.strip_prefix(['+', '-']).unwrap_or(text);
            if body.is_empty()
                || body.eq_ignore_ascii_case("nan")
                || body.eq_ignore_ascii_case("inf")
                || body.eq_ignore_ascii_case("infinity")
            {
                return Ok(Conv::Bad(format!("cannot convert {text:?} to float")));
            }
            match text.parse::<f64>() {
                Ok(f) if f.is_finite() => Conv::Ok(Value::object(vm.alloc_float(f))),
                _ => Conv::Bad(format!("cannot convert {text:?} to float")),
            }
        }
        Target::Bool => {
            if text.eq_ignore_ascii_case("true") {
                Conv::Ok(Value::TRUE)
            } else if text.eq_ignore_ascii_case("false") {
                Conv::Ok(Value::FALSE)
            } else {
                Conv::Bad(format!("cannot convert {text:?} to bool"))
            }
        }
        Target::Enum(head) => {
            let enm_ptr = head.ptr();
            let idx = match vm.get_object(enm_ptr) {
                Object::Enum(en) => en.variants.iter().position(|v| v.name == text),
                _ => None,
            };
            match idx {
                Some(i) => Conv::Ok(Value::object(vm.alloc_variant(enm_ptr, i))),
                None => Conv::Bad(format!("{text:?} is not a variant of `{}`", head_key(head))),
            }
        }
        Target::Instant => {
            let parsed = OffsetDateTime::parse(text, &Rfc3339)
                .or_else(|_| OffsetDateTime::parse(text, &Iso8601::DEFAULT))
                .or_else(|_| OffsetDateTime::parse(text, &Rfc2822));
            match parsed {
                Ok(dt) => {
                    let instant = copy::time::Instant {
                        _nanoseconds: Arc::new(num_bigint::BigInt::from(dt.unix_timestamp_nanos())),
                    };
                    Conv::Ok(instant.to_value(vm))
                }
                Err(_) => Conv::Bad(format!("cannot convert {text:?} to baml.time.Instant")),
            }
        }
        Target::PlainDate => match Date::parse(text, super::time::DATE_FORMAT) {
            Ok(date) => Conv::Ok(
                copy::time::PlainDate {
                    _days: super::time::days_since_epoch(date),
                }
                .to_value(vm),
            ),
            Err(_) => Conv::Bad(format!("cannot convert {text:?} to baml.time.PlainDate")),
        },
        Target::PlainDateTime => {
            match PrimitiveDateTime::parse(text, super::time::DATETIME_FORMAT) {
                Ok(dt) => {
                    let civil = dt.assume_utc().unix_timestamp_nanos();
                    Conv::Ok(
                        copy::time::PlainDateTime {
                            _nanoseconds: Arc::new(num_bigint::BigInt::from(civil)),
                        }
                        .to_value(vm),
                    )
                }
                Err(_) => Conv::Bad(format!(
                    "cannot convert {text:?} to baml.time.PlainDateTime"
                )),
            }
        }
    })
}

/// Null-cell classification per BEP-060: an empty unquoted cell, or an
/// unquoted cell matching `null_values`.
fn null_cell_state(cell: &CellData, null_values: &[String]) -> NullState {
    if cell.quoted {
        return NullState::Data;
    }
    if cell.text.is_empty() {
        return NullState::Empty;
    }
    if null_values.iter().any(|n| n == &cell.text) {
        return NullState::NullValue;
    }
    NullState::Data
}

enum NullState {
    Data,
    Empty,
    NullValue,
}

enum DecodeFail {
    Info(ErrInfo),
    Fatal(VmRustFnError),
}

impl From<VmRustFnError> for DecodeFail {
    fn from(e: VmRustFnError) -> Self {
        DecodeFail::Fatal(e)
    }
}

fn record_arc(vm: &BexVm, rec: Value) -> Result<Arc<RecordData>, VmRustFnError> {
    state_arc::<RecordData>(vm, rec, 0)
}

/// Decode a whole record into an instance of class `ty`.
fn decode_record_to_instance(
    vm: &mut BexVm,
    rd: &RecordData,
    ty: &bex_vm_types::RealizedTy,
) -> Result<Value, DecodeFail> {
    use bex_vm_types::RealizedTy;
    let RealizedTy::Class(head, type_args, _) = ty else {
        return Err(DecodeFail::Info(ErrInfo::new(
            Kind::Options,
            format!("decode target `{ty}` is not a class; CSV decodes into flat classes"),
        )));
    };
    let key = head_key(head);
    let class_ptr = head.ptr();
    let Object::Class(_) = vm.get_object(class_ptr) else {
        return Err(DecodeFail::Info(ErrInfo::new(
            Kind::Options,
            format!("class `{key}` not found"),
        )));
    };
    let class_fields = match vm.get_object(class_ptr) {
        Object::Class(c) => c.fields.clone(),
        _ => {
            return Err(DecodeFail::Info(ErrInfo::new(
                Kind::Options,
                format!("`{key}` is not a class"),
            )));
        }
    };

    let mut field_values = Vec::with_capacity(class_fields.len());
    for (fi, cf) in class_fields.iter().enumerate() {
        let field_ty = vm.realize_field_ty(&cf.field_template, type_args);
        let cell_ty = classify_cell_ty(&field_ty).map_err(|msg| {
            DecodeFail::Info(ErrInfo::new(
                Kind::Options,
                format!("field `{}` of `{key}`: {msg}", cf.name),
            ))
        })?;

        // Locate the cell: by header name, or positionally without headers.
        let col = match &rd.header {
            Some(h) => {
                if h.dup.contains(&cf.name) {
                    let mut info = ErrInfo::new(
                        Kind::Header,
                        format!("column `{}` is duplicated in the header", cf.name),
                    )
                    .at(rd.line, rd.record);
                    info.column = Some(cf.name.clone());
                    return Err(DecodeFail::Info(info));
                }
                match h.index.get(&cf.name) {
                    Some(&c) => Some(c),
                    None => {
                        if cell_ty.nullable {
                            field_values.push(Value::NULL);
                            continue;
                        }
                        let mut info = ErrInfo::new(
                            Kind::Header,
                            format!(
                                "no column matching non-optional field `{}` of `{key}`",
                                cf.name
                            ),
                        )
                        .at(rd.line, rd.record);
                        info.column = Some(cf.name.clone());
                        return Err(DecodeFail::Info(info));
                    }
                }
            }
            None => Some(fi),
        };

        let cell = col.and_then(|c| rd.cells.get(c));
        let column_name = rd
            .header
            .as_ref()
            .and_then(|h| col.and_then(|c| h.names.get(c)))
            .cloned();

        let fail = |msg: String| {
            let mut info = ErrInfo::new(Kind::Decode, msg).at(rd.line, rd.record);
            info.field = col.map(|c| c as i64);
            info.column.clone_from(&column_name);
            DecodeFail::Info(info)
        };

        let Some(cell) = cell else {
            if cell_ty.nullable {
                field_values.push(Value::NULL);
                continue;
            }
            return Err(fail(format!(
                "missing cell for non-optional field `{}`",
                cf.name
            )));
        };

        match null_cell_state(cell, &rd.null_values) {
            NullState::Empty => {
                if cell_ty.nullable {
                    field_values.push(Value::NULL);
                    continue;
                }
                if matches!(cell_ty.target, Target::Str) {
                    field_values.push(Value::object(vm.alloc_string(String::new())));
                    continue;
                }
                return Err(fail(format!(
                    "empty cell for non-optional field `{}`",
                    cf.name
                )));
            }
            NullState::NullValue => {
                if cell_ty.nullable {
                    field_values.push(Value::NULL);
                    continue;
                }
                return Err(fail(format!(
                    "null cell ({:?}) for non-optional field `{}`",
                    cell.text, cf.name
                )));
            }
            NullState::Data => {}
        }

        match convert_cell(vm, &cell.text, &cell_ty.target)? {
            Conv::Ok(v) => field_values.push(v),
            Conv::Bad(msg) => return Err(fail(msg)),
        }
    }

    Ok(Value::object(vm.tlab.alloc(Object::Instance(
        Instance::new(
            class_ptr,
            type_args.clone().into_boxed_slice(),
            field_values,
        ),
    ))))
}

fn current_type_arg(vm: &mut BexVm, who: &str) -> Result<bex_vm_types::RealizedTy, VmRustFnError> {
    // `.first()` is the method's own first generic only because `Record` is
    // non-generic, so MIR's receiver-class-type-arg prepend (which would push
    // class args ahead of the method's) contributes nothing here. A generic
    // receiver class would shift the index — see `map_result_element_ty`'s
    // back-indexing in `array.rs`.
    vm.current_call_type_args().first().cloned().ok_or_else(|| {
        VmRustFnError::InternalError(VmInternalError::MissingNativeFunction {
            name: format!("{who}: missing type argument"),
        })
    })
}

/// Convert one cell to `T?` per the `get<T>` / `get_at<T>` rules.
fn cell_to_optional(
    vm: &mut BexVm,
    rd: &RecordData,
    col: Option<usize>,
    ty: &bex_vm_types::RealizedTy,
) -> Result<Option<Value>, VmRustFnError> {
    let cell_ty = match classify_cell_ty(ty) {
        Ok(c) => c,
        Err(msg) => {
            let info = ErrInfo::new(Kind::Options, msg);
            return Err(throw_err(vm, &info));
        }
    };
    let Some(cell) = col.and_then(|c| rd.cells.get(c)) else {
        return Ok(None);
    };
    match null_cell_state(cell, &rd.null_values) {
        NullState::Empty => {
            if !cell_ty.nullable && matches!(cell_ty.target, Target::Str) {
                return Ok(Some(Value::object(vm.alloc_string(String::new()))));
            }
            return Ok(None);
        }
        NullState::NullValue => return Ok(None),
        NullState::Data => {}
    }
    match convert_cell(vm, &cell.text, &cell_ty.target)? {
        Conv::Ok(v) => Ok(Some(v)),
        Conv::Bad(msg) => {
            let mut info = ErrInfo::new(Kind::Decode, msg).at(rd.line, rd.record);
            info.field = col.map(|c| c as i64);
            info.column = rd
                .header
                .as_ref()
                .and_then(|h| col.and_then(|c| h.names.get(c)))
                .cloned();
            Err(throw_err(vm, &info))
        }
    }
}

// =============================================================================
// Writer state and encoding
// =============================================================================

struct WriterState {
    opts: WriterOpts,
    header_written: bool,
    records_written: i64,
    bytes_written: i64,
    /// `Some` for buffer writers; file writers stream through `_emit`.
    buffer: Option<String>,
    bom_pending: bool,
    closed: bool,
}

impl WriterState {
    fn new(opts: WriterOpts, buffered: bool) -> Self {
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

const FORMULA_TRIGGERS: [char; 10] = ['=', '+', '-', '@', '\t', '\r', '\n', '＝', '＋', '－'];

fn encode_cell_into(out: &mut String, cell: &str, o: &WriterOpts) -> Result<(), ErrInfo> {
    let mut content = std::borrow::Cow::Borrowed(cell);
    if o.sanitize {
        let triggers_full = ['＠'];
        if content
            .chars()
            .next()
            .is_some_and(|c| FORMULA_TRIGGERS.contains(&c) || triggers_full.contains(&c))
        {
            content = std::borrow::Cow::Owned(format!("'{content}"));
        }
    }
    let needs_quoting = content
        .bytes()
        .any(|b| b == o.delimiter || b == o.quote || b == b'\r' || b == b'\n');
    let quote_it = match o.style {
        QuoteStyle::All => true,
        QuoteStyle::Minimal => needs_quoting,
        QuoteStyle::Never => {
            if needs_quoting {
                return Err(ErrInfo::new(
                    Kind::Encode,
                    format!(
                        "field {content:?} requires quoting, which `quote_style: \"never\"` forbids"
                    ),
                ));
            }
            false
        }
    };
    if !quote_it {
        out.push_str(&content);
        return Ok(());
    }
    let qc = o.quote as char;
    out.push(qc);
    match o.escape {
        None => {
            for ch in content.chars() {
                if ch == qc {
                    out.push(qc);
                }
                out.push(ch);
            }
        }
        Some(e) => {
            let ec = e as char;
            for ch in content.chars() {
                if ch == qc || ch == ec {
                    out.push(ec);
                }
                out.push(ch);
            }
        }
    }
    out.push(qc);
    Ok(())
}

/// Encode one record line. Pure: writer state (BOM, counters, header flag)
/// is committed only by [`commit_batch`], after the whole call's output has
/// encoded successfully — a thrown `Encode` error must leave the writer
/// exactly as it was.
fn encode_line(o: &WriterOpts, cells: &[String]) -> Result<String, ErrInfo> {
    let mut out = String::new();
    for (i, cell) in cells.iter().enumerate() {
        if i > 0 {
            out.push(o.delimiter as char);
        }
        encode_cell_into(&mut out, cell, o)?;
    }
    out.push_str(if o.crlf { "\r\n" } else { "\n" });
    Ok(out)
}

/// Start an output batch: the BOM is prepended speculatively and only
/// marked consumed when the batch commits.
fn begin_batch(s: &WriterState) -> (String, bool) {
    if s.bom_pending {
        ('\u{FEFF}'.to_string(), true)
    } else {
        (String::new(), false)
    }
}

/// Commit a successfully encoded batch: all writer-state mutations happen
/// here, atomically with handing the text to the buffer/file.
fn commit_batch(
    s: &mut WriterState,
    out: String,
    records: i64,
    wrote_header: bool,
    bom_consumed: bool,
) -> String {
    s.bytes_written += out.len() as i64;
    if bom_consumed {
        s.bom_pending = false;
    }
    if wrote_header {
        s.header_written = true;
    }
    s.records_written += records;
    dispatch_output(s, out)
}

/// Append to the buffer (buffer writers) or hand back for the BAML `_emit`
/// body to write to the file.
fn dispatch_output(s: &mut WriterState, out: String) -> String {
    match &mut s.buffer {
        Some(buf) => {
            buf.push_str(&out);
            String::new()
        }
        None => out,
    }
}

enum CellTextErr {
    NonFinite,
    Unsupported(String),
}

fn float_cell_text(f: f64) -> Result<String, CellTextErr> {
    if !f.is_finite() {
        return Err(CellTextErr::NonFinite);
    }
    Ok(format_float(f))
}

/// Shortest round-trip float text (Rust's `{}` is shortest round-trip), with
/// a `.0` suffix for integral values so the cell round-trips as a float.
fn format_float(f: f64) -> String {
    let s = format!("{f}");
    if s.contains(['.', 'e', 'E']) {
        s
    } else {
        format!("{s}.0")
    }
}

fn plaindate_cell_text(inst: &Instance) -> Result<String, CellTextErr> {
    let days = view::time::PlainDate { instance: inst }._days();
    let date = super::time::date_for_days(days, "csv cell")
        .map_err(|_| CellTextErr::Unsupported("PlainDate out of range".to_string()))?;
    let mut out = String::new();
    super::time::format_date_into(&mut out, date);
    Ok(out)
}

fn plaindatetime_cell_text(inst: &Instance) -> Result<String, CellTextErr> {
    let nanos = view::time::PlainDateTime { instance: inst }._nanoseconds();
    let dt = super::time::civil_datetime(&nanos, "csv cell")
        .map_err(|_| CellTextErr::Unsupported("PlainDateTime out of range".to_string()))?;
    let mut out = String::new();
    super::time::format_date_into(&mut out, dt.date());
    out.push('T');
    super::time::format_clock_into(
        &mut out,
        dt.hour(),
        dt.minute(),
        dt.second(),
        dt.nanosecond(),
    );
    Ok(out)
}

fn instant_cell_text(inst: &Instance) -> Result<String, CellTextErr> {
    let nanos = view::time::Instant { instance: inst }._nanoseconds();
    let nanos = i128::try_from(&*nanos)
        .map_err(|_| CellTextErr::Unsupported("Instant out of range".to_string()))?;
    let dt = OffsetDateTime::from_unix_timestamp_nanos(nanos)
        .map_err(|_| CellTextErr::Unsupported("Instant out of range".to_string()))?;
    dt.format(&Rfc3339)
        .map_err(|_| CellTextErr::Unsupported("Instant out of RFC 3339 range".to_string()))
}

/// Canonical cell text for a BAML value (`Value` or a typed-row field).
fn value_cell_text(vm: &BexVm, v: Value, null_value: &str) -> Result<String, CellTextErr> {
    Ok(match v.kind() {
        ValueKind::Null => null_value.to_string(),
        ValueKind::Bool(b) => if b { "true" } else { "false" }.to_string(),
        ValueKind::Int(i) => i.to_string(),
        ValueKind::OmittedArg => {
            return Err(CellTextErr::Unsupported("omitted argument".to_string()));
        }
        ValueKind::Object(ptr) => match vm.get_object(ptr) {
            Object::String(s) => s.as_str().to_string(),
            Object::Float(f) => float_cell_text(*f)?,
            Object::Bigint(b) => b.to_string(),
            Object::Variant(var) => {
                let name = match vm.get_object(var.enm) {
                    Object::Enum(en) => en.variants.get(var.index).map(|v| v.name.clone()),
                    _ => None,
                };
                name.ok_or_else(|| {
                    CellTextErr::Unsupported("unresolvable enum variant".to_string())
                })?
            }
            Object::Instance(inst) => {
                // Match the instance's class against the builtin date/time
                // classes by tag — an integer compare per cell, with no name
                // rendered and no package-index lookup.
                let class_tag = match vm.get_object(inst.class) {
                    Object::Class(class) => class.type_tag,
                    _ => {
                        return Err(CellTextErr::Unsupported(
                            "value is not representable as a CSV cell".to_string(),
                        ));
                    }
                };
                match class_tag {
                    t if t == TypeTag::of_head(INSTANT_FQN) => instant_cell_text(inst)?,
                    t if t == TypeTag::of_head(PLAINDATE_FQN) => plaindate_cell_text(inst)?,
                    t if t == TypeTag::of_head(PLAINDATETIME_FQN) => plaindatetime_cell_text(inst)?,
                    _ => {
                        return Err(CellTextErr::Unsupported(
                            "nested class values are not CSV cells; serialize explicitly (e.g. baml.json.to_string)".to_string(),
                        ));
                    }
                }
            }
            _ => {
                return Err(CellTextErr::Unsupported(
                    "value is not representable as a CSV cell".to_string(),
                ));
            }
        },
    })
}

fn cell_text_err_info(e: CellTextErr, context: &str) -> ErrInfo {
    match e {
        CellTextErr::NonFinite => ErrInfo::new(
            Kind::Encode,
            format!("{context}: non-finite floats (NaN, inf) cannot be written to CSV"),
        ),
        CellTextErr::Unsupported(msg) => ErrInfo::new(Kind::Options, format!("{context}: {msg}")),
    }
}

/// Field names and per-row field values for a typed row instance.
fn row_fields(vm: &BexVm, row: Value) -> Result<(Vec<String>, Vec<Value>), ErrInfo> {
    let inst = vm.as_instance(&row).map_err(|_| {
        ErrInfo::new(
            Kind::Options,
            "write_row target is not a class instance; CSV writes flat classes",
        )
    })?;
    let names = match vm.get_object(inst.class) {
        Object::Class(c) => c.fields.iter().map(|f| f.name.clone()).collect::<Vec<_>>(),
        _ => {
            return Err(ErrInfo::new(
                Kind::Options,
                "write_row target is not a class instance",
            ));
        }
    };
    let values = (0..names.len()).map(|i| inst.load_field(i)).collect();
    Ok((names, values))
}

/// Encode one typed row (plus the auto-header when `header_pending`), into
/// the batch-local `out`. Pure with respect to writer state; the caller
/// commits via [`commit_batch`].
fn encode_typed_row(
    vm: &BexVm,
    o: &WriterOpts,
    header_pending: &mut bool,
    out: &mut String,
    row: Value,
) -> Result<(), ErrInfo> {
    let (names, values) = row_fields(vm, row)?;
    if *header_pending {
        let header_names = o.headers_override.as_deref().unwrap_or(&names);
        out.push_str(&encode_line(o, header_names)?);
        *header_pending = false;
    }
    let mut cells = Vec::with_capacity(values.len());
    for (name, v) in names.iter().zip(&values) {
        let text = value_cell_text(vm, *v, &o.null_value)
            .map_err(|e| cell_text_err_info(e, &format!("field `{name}`")))?;
        cells.push(text);
    }
    out.push_str(&encode_line(o, &cells)?);
    Ok(())
}

// =============================================================================
// Markdown rendering
// =============================================================================

fn md_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '|' => out.push_str("\\|"),
            '\n' | '\r' => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

fn md_value_text(vm: &mut BexVm, v: Value, field_ty: Option<&bex_vm_types::RealizedTy>) -> String {
    // Prompt text is not meant to round-trip: non-finite floats render as-is.
    if let ValueKind::Object(ptr) = v.kind() {
        if let Object::Float(f) = vm.get_object(ptr) {
            return format_float_md(*f);
        }
    }
    match value_cell_text(vm, v, "") {
        Ok(t) => t,
        Err(_) => {
            // Nested values: fall back to JSON when the field type is known.
            if let Some(ty) = field_ty {
                if let Ok(json) = super::json::json_to_string_typed(vm, v, ty) {
                    return json;
                }
            }
            "<unrepresentable>".to_string()
        }
    }
}

fn format_float_md(f: f64) -> String {
    if f.is_nan() {
        "NaN".to_string()
    } else if f.is_infinite() {
        if f > 0.0 { "inf" } else { "-inf" }.to_string()
    } else {
        format_float(f)
    }
}

fn render_markdown(headers: &[String], rows: &[Vec<String>], total_rows: usize) -> String {
    let mut out = String::new();
    out.push('|');
    for h in headers {
        out.push(' ');
        out.push_str(&md_escape(h));
        out.push_str(" |");
    }
    out.push('\n');
    out.push('|');
    for _ in headers {
        out.push_str(" --- |");
    }
    for row in rows {
        out.push('\n');
        out.push('|');
        for cell in row {
            out.push(' ');
            out.push_str(&md_escape(cell));
            out.push_str(" |");
        }
    }
    if total_rows > rows.len() {
        let _ = write!(out, "\n… ({} more rows)", total_rows - rows.len());
    }
    out
}

// =============================================================================
// Trait implementations
// =============================================================================

impl BamlClassCsvReader for PackageBamlImpl {
    fn skipped(vm: &mut BexVm, csvreader: &Value) -> Vec<Value> {
        let Ok(st) = state_arc::<Mutex<ReaderState>>(vm, *csvreader, 0) else {
            return Vec::new();
        };
        let infos: Vec<ErrInfo> = lock(&st).skipped.clone();
        infos
            .iter()
            .filter_map(|info| error_value(vm, info).ok())
            .collect()
    }

    fn skipped_count(vm: &BexVm, csvreader: &view::csv::Reader<'_>) -> i64 {
        lock(csvreader._handle::<Mutex<ReaderState>>(vm)).skipped_count
    }

    fn position(vm: &mut BexVm, csvreader: &Value) -> Value {
        match state_arc::<Mutex<ReaderState>>(vm, *csvreader, 0) {
            Ok(st) => {
                let (byte, line, record) = {
                    let s = lock(&st);
                    (s.byte, s.line, s.record)
                };
                copy::csv::Position { byte, line, record }.to_value(vm)
            }
            Err(_) => copy::csv::Position {
                byte: 0,
                line: 1,
                record: 0,
            }
            .to_value(vm),
        }
    }

    fn _poll(vm: &mut BexVm, csvreader: &Value) -> Result<Value, VmRustFnError> {
        let st = state_arc::<Mutex<ReaderState>>(vm, *csvreader, 0)?;
        let polled = {
            let mut s = lock(&st);
            poll_record(&mut s)
        };
        match polled {
            Ok(Polled::NeedData) => Ok(need_data_value(vm)),
            Ok(Polled::Done) => done_value(vm),
            Ok(Polled::Skipped(info)) => {
                let error = error_value(vm, &info)?;
                Ok(copy::csv::Skip { error }.to_value(vm))
            }
            Ok(Polled::Rec(rd)) => Ok(copy::csv::Record {
                _handle: Arc::new(rd),
            }
            .to_value(vm)),
            Err(info) => Err(throw_err(vm, &info)),
        }
    }

    fn _poll_headers(vm: &mut BexVm, csvreader: &Value) -> Result<Value, VmRustFnError> {
        let st = state_arc::<Mutex<ReaderState>>(vm, *csvreader, 0)?;
        let polled = {
            let mut s = lock(&st);
            poll_headers(&mut s)
        };
        match polled {
            Ok(HeadersPolled::NeedData) => Ok(need_data_value(vm)),
            Ok(HeadersPolled::Ready(names)) => {
                let names_value = match names {
                    None => Value::NULL,
                    Some(ns) => {
                        let items: Vec<Value> = ns
                            .into_iter()
                            .map(|n| Value::object(vm.alloc_string(n)))
                            .collect();
                        // CSV header names are always strings.
                        Value::object(vm.alloc_array(bex_vm_types::RealizedTy::string(), items))
                    }
                };
                Ok(copy::csv::Headers { names: names_value }.to_value(vm))
            }
            Err(info) => Err(throw_err(vm, &info)),
        }
    }

    fn _feed(vm: &BexVm, csvreader: &view::csv::Reader<'_>, chunk: &[u8]) {
        let mut s = lock(csvreader._handle::<Mutex<ReaderState>>(vm));
        s.buf.extend_from_slice(chunk);
    }

    fn _feed_eof(vm: &BexVm, csvreader: &view::csv::Reader<'_>) {
        lock(csvreader._handle::<Mutex<ReaderState>>(vm)).eof = true;
    }

    fn _mark_closed(vm: &BexVm, csvreader: &view::csv::Reader<'_>) {
        lock(csvreader._handle::<Mutex<ReaderState>>(vm)).closed = true;
    }

    fn _mark_exhausted(vm: &BexVm, csvreader: &view::csv::Reader<'_>) {
        lock(csvreader._handle::<Mutex<ReaderState>>(vm)).finished = true;
    }
}

impl BamlClassCsvRecord for PackageBamlImpl {
    fn fields(vm: &mut BexVm, csvrecord: &Value) -> Vec<bex_str::BexStr> {
        match record_arc(vm, *csvrecord) {
            Ok(rd) => rd
                .cells
                .iter()
                .map(|c| bex_str::BexStr::from(c.text.as_str()))
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    fn length(vm: &BexVm, csvrecord: &view::csv::Record<'_>) -> i64 {
        csvrecord._handle::<RecordData>(vm).cells.len() as i64
    }

    fn position(vm: &mut BexVm, csvrecord: &Value) -> Value {
        match record_arc(vm, *csvrecord) {
            Ok(rd) => copy::csv::Position {
                byte: rd.byte,
                line: rd.line,
                record: rd.record,
            }
            .to_value(vm),
            Err(_) => copy::csv::Position {
                byte: 0,
                line: 1,
                record: 0,
            }
            .to_value(vm),
        }
    }

    fn to_map(
        vm: &mut BexVm,
        csvrecord: &Value,
    ) -> Result<IndexMap<bex_str::BexStr, Value>, VmRustFnError> {
        let rd = record_arc(vm, *csvrecord)?;
        let Some(header) = &rd.header else {
            let info = ErrInfo::new(
                Kind::Header,
                "to_map requires headers (has_header = true or the headers option)",
            )
            .at(rd.line, rd.record);
            return Err(throw_err(vm, &info));
        };
        if !header.dup.is_empty() {
            let dup = header.dup.iter().next().cloned().unwrap_or_default();
            let mut info = ErrInfo::new(
                Kind::Header,
                format!("to_map requires unique headers; `{dup}` is duplicated"),
            )
            .at(rd.line, rd.record);
            info.column = Some(dup);
            return Err(throw_err(vm, &info));
        }
        let mut out = IndexMap::with_capacity(header.names.len());
        for (i, name) in header.names.iter().enumerate() {
            let text = rd.cells.get(i).map(|c| c.text.as_str()).unwrap_or("");
            let v = Value::object(vm.alloc_string(text.to_string()));
            out.insert(bex_str::BexStr::from(name.as_str()), v);
        }
        Ok(out)
    }
}

impl BamlClassCsvWriter for PackageBamlImpl {
    fn records_written(vm: &BexVm, csvwriter: &view::csv::Writer<'_>) -> i64 {
        lock(csvwriter._handle::<Mutex<WriterState>>(vm)).records_written
    }

    fn text(vm: &mut BexVm, csvwriter: &Value) -> Result<bex_str::BexStr, VmRustFnError> {
        let st = state_arc::<Mutex<WriterState>>(vm, *csvwriter, 0)?;
        let text = {
            let s = lock(&st);
            s.buffer.clone()
        };
        match text {
            Some(t) => Ok(bex_str::BexStr::from(t.as_str())),
            None => {
                let info = ErrInfo::new(
                    Kind::Options,
                    "text() is only available on buffer writers (baml.csv.buffer)",
                );
                Err(throw_err(vm, &info))
            }
        }
    }

    fn _encode_record(
        vm: &mut BexVm,
        csvwriter: &Value,
        record: &[Value],
    ) -> Result<bex_str::BexStr, VmRustFnError> {
        let st = state_arc::<Mutex<WriterState>>(vm, *csvwriter, 0)?;
        let mut s = lock(&st);
        if s.closed {
            drop(s);
            let info = ErrInfo::new(Kind::Closed, "writer is closed");
            return Err(throw_err(vm, &info));
        }
        let mut cells = Vec::with_capacity(record.len());
        for (i, v) in record.iter().enumerate() {
            match value_cell_text(vm, *v, &s.opts.null_value) {
                Ok(t) => cells.push(t),
                Err(e) => {
                    drop(s);
                    let info = cell_text_err_info(e, &format!("record field {i}"));
                    return Err(throw_err(vm, &info));
                }
            }
        }
        let (mut out, bom_consumed) = begin_batch(&s);
        match encode_line(&s.opts, &cells) {
            Ok(line) => out.push_str(&line),
            Err(info) => {
                drop(s);
                return Err(throw_err(vm, &info));
            }
        }
        let out = commit_batch(&mut s, out, 1, false, bom_consumed);
        Ok(bex_str::BexStr::from(out.as_str()))
    }

    fn _encode_header(
        vm: &mut BexVm,
        csvwriter: &Value,
        names: &[Value],
    ) -> Result<bex_str::BexStr, VmRustFnError> {
        let st = state_arc::<Mutex<WriterState>>(vm, *csvwriter, 0)?;
        let mut s = lock(&st);
        if s.closed {
            drop(s);
            let info = ErrInfo::new(Kind::Closed, "writer is closed");
            return Err(throw_err(vm, &info));
        }
        if s.records_written > 0 {
            drop(s);
            let info = ErrInfo::new(Kind::Header, "cannot write a header after data records");
            return Err(throw_err(vm, &info));
        }
        let mut name_texts = Vec::with_capacity(names.len());
        for v in names {
            name_texts.push(vm.as_string(v)?.as_str().to_string());
        }
        let (mut out, bom_consumed) = begin_batch(&s);
        match encode_line(&s.opts, &name_texts) {
            Ok(line) => out.push_str(&line),
            Err(info) => {
                drop(s);
                return Err(throw_err(vm, &info));
            }
        }
        let out = commit_batch(&mut s, out, 0, true, bom_consumed);
        Ok(bex_str::BexStr::from(out.as_str()))
    }

    fn _bytes_written(vm: &BexVm, csvwriter: &view::csv::Writer<'_>) -> i64 {
        lock(csvwriter._handle::<Mutex<WriterState>>(vm)).bytes_written
    }

    fn _mark_closed(vm: &BexVm, csvwriter: &view::csv::Writer<'_>) {
        lock(csvwriter._handle::<Mutex<WriterState>>(vm)).closed = true;
    }
}

impl BamlNamespaceCsv for PackageBamlImpl {
    fn _reader(
        vm: &mut BexVm,
        source: &Value,
        options: Option<&Value>,
        owns_file: bool,
    ) -> Result<Value, VmRustFnError> {
        let opts = parse_reader_options(vm, options)?;
        let on_skip = match options {
            Some(o) => field_by_name(vm, *o, "on_skip")?,
            None => Value::NULL,
        };

        let (initial, eof, file) = if let Ok(s) = vm.as_string(source) {
            (s.as_str().as_bytes().to_vec(), true, Value::NULL)
        } else if let Ok(bytes) = vm.as_uint8array(source) {
            let initial = bytes.to_vec();
            (initial, true, Value::NULL)
        } else if vm.as_instance(source).is_ok() {
            (Vec::new(), false, *source)
        } else {
            return Err(options_err(
                vm,
                "reader source must be a string, uint8array, or baml.fs.File",
            ));
        };

        let state = ReaderState::new(opts, initial, eof);
        Ok(copy::csv::Reader {
            _handle: Arc::new(Mutex::new(state)),
            _file: file,
            _on_skip: on_skip,
            _owns_file: owns_file && !file.is_null(),
        }
        .to_value(vm))
    }

    fn _writer(
        vm: &mut BexVm,
        file: &Value,
        options: Option<&Value>,
        owns_file: bool,
    ) -> Result<Value, VmRustFnError> {
        let opts = parse_writer_options(vm, options)?;
        Ok(copy::csv::Writer {
            _handle: Arc::new(Mutex::new(WriterState::new(opts, false))),
            _file: *file,
            _owns_file: owns_file,
        }
        .to_value(vm))
    }

    fn _buffer(vm: &mut BexVm, options: Option<&Value>) -> Result<Value, VmRustFnError> {
        let opts = parse_writer_options(vm, options)?;
        Ok(copy::csv::Writer {
            _handle: Arc::new(Mutex::new(WriterState::new(opts, true))),
            _file: Value::NULL,
            _owns_file: false,
        }
        .to_value(vm))
    }

    fn _validate_columns(vm: &mut BexVm, r: &Value) -> Result<(), VmRustFnError> {
        use bex_vm_types::RealizedTy;
        let ty = current_type_arg(vm, "baml.csv.rows")?;
        let RealizedTy::Class(head, type_args, _) = &ty else {
            let info = ErrInfo::new(
                Kind::Options,
                format!("rows target `{ty}` is not a class; CSV decodes into flat classes"),
            );
            return Err(throw_err(vm, &info));
        };
        let key = head_key(head);
        let class_fields = match vm.get_object(head.ptr()) {
            Object::Class(c) => c.fields.clone(),
            _ => {
                let info = ErrInfo::new(Kind::Options, format!("`{key}` is not a class"));
                return Err(throw_err(vm, &info));
            }
        };

        let st = state_arc::<Mutex<ReaderState>>(vm, *r, 0)?;
        let header = lock(&st).header.clone();

        for cf in &class_fields {
            let field_ty = vm.realize_field_ty(&cf.field_template, type_args);
            let cell_ty = match classify_cell_ty(&field_ty) {
                Ok(c) => c,
                Err(msg) => {
                    let info = ErrInfo::new(
                        Kind::Options,
                        format!("field `{}` of `{key}`: {msg}", cf.name),
                    );
                    return Err(throw_err(vm, &info));
                }
            };
            if let Some(h) = &header {
                if h.dup.contains(&cf.name) {
                    let mut info = ErrInfo::new(
                        Kind::Header,
                        format!("column `{}` is duplicated in the header", cf.name),
                    );
                    info.column = Some(cf.name.clone());
                    return Err(throw_err(vm, &info));
                }
                if !cell_ty.nullable && !h.index.contains_key(&cf.name) {
                    let mut info = ErrInfo::new(
                        Kind::Header,
                        format!(
                            "no column matching non-optional field `{}` of `{key}`",
                            cf.name
                        ),
                    );
                    info.column = Some(cf.name.clone());
                    return Err(throw_err(vm, &info));
                }
            }
        }
        Ok(())
    }

    fn _try_decode(vm: &mut BexVm, rec: &Value, r: &Value) -> Result<Value, VmRustFnError> {
        let ty = current_type_arg(vm, "baml.csv._try_decode")?;
        let rd = record_arc(vm, *rec)?;
        match decode_record_to_instance(vm, &rd, &ty) {
            Ok(v) => Ok(v),
            Err(DecodeFail::Fatal(e)) => Err(e),
            Err(DecodeFail::Info(info)) => {
                let skip = info.kind == Kind::Decode && {
                    let st = state_arc::<Mutex<ReaderState>>(vm, *r, 0)?;
                    let mut s = lock(&st);
                    if s.opts.skip_on_error {
                        s.register_skip(&info);
                        true
                    } else {
                        false
                    }
                };
                if skip {
                    let error = error_value(vm, &info)?;
                    Ok(copy::csv::Skip { error }.to_value(vm))
                } else {
                    Err(throw_err(vm, &info))
                }
            }
        }
    }

    fn _decode_record(vm: &mut BexVm, rec: &Value) -> Result<Value, VmRustFnError> {
        let ty = current_type_arg(vm, "baml.csv.decode")?;
        let rd = record_arc(vm, *rec)?;
        match decode_record_to_instance(vm, &rd, &ty) {
            Ok(v) => Ok(v),
            Err(DecodeFail::Fatal(e)) => Err(e),
            Err(DecodeFail::Info(info)) => Err(throw_err(vm, &info)),
        }
    }

    fn _cell_by_name(
        vm: &mut BexVm,
        rec: &Value,
        column: &bex_str::BexStr,
    ) -> Result<Option<Value>, VmRustFnError> {
        let ty = current_type_arg(vm, "baml.csv.get")?;
        let rd = record_arc(vm, *rec)?;
        let Some(header) = &rd.header else {
            let info = ErrInfo::new(
                Kind::Header,
                "get(column) requires headers (has_header = true or the headers option); use get_at for positional access",
            )
            .at(rd.line, rd.record);
            return Err(throw_err(vm, &info));
        };
        let name = column.as_str();
        if header.dup.contains(name) {
            let mut info = ErrInfo::new(
                Kind::Header,
                format!("column `{name}` is duplicated in the header"),
            )
            .at(rd.line, rd.record);
            info.column = Some(name.to_string());
            return Err(throw_err(vm, &info));
        }
        let col = header.index.get(name).copied();
        if col.is_none() {
            return Ok(None);
        }
        cell_to_optional(vm, &rd, col, &ty)
    }

    fn _cell_by_index(
        vm: &mut BexVm,
        rec: &Value,
        index: i64,
    ) -> Result<Option<Value>, VmRustFnError> {
        let ty = current_type_arg(vm, "baml.csv.get_at")?;
        let rd = record_arc(vm, *rec)?;
        let col = usize::try_from(index).ok();
        cell_to_optional(vm, &rd, col, &ty)
    }

    fn _encode_row(
        vm: &mut BexVm,
        w: &Value,
        row: &Value,
    ) -> Result<bex_str::BexStr, VmRustFnError> {
        let st = state_arc::<Mutex<WriterState>>(vm, *w, 0)?;
        let mut s = lock(&st);
        if s.closed {
            drop(s);
            let info = ErrInfo::new(Kind::Closed, "writer is closed");
            return Err(throw_err(vm, &info));
        }
        let (mut out, bom_consumed) = begin_batch(&s);
        let mut header_pending = s.opts.auto_header && !s.header_written;
        let header_was_pending = header_pending;
        match encode_typed_row(vm, &s.opts, &mut header_pending, &mut out, *row) {
            Ok(()) => {}
            Err(info) => {
                drop(s);
                return Err(throw_err(vm, &info));
            }
        }
        let wrote_header = header_was_pending && !header_pending;
        let out = commit_batch(&mut s, out, 1, wrote_header, bom_consumed);
        Ok(bex_str::BexStr::from(out.as_str()))
    }

    fn _encode_rows(
        vm: &mut BexVm,
        w: &Value,
        rows: &[Value],
    ) -> Result<bex_str::BexStr, VmRustFnError> {
        let st = state_arc::<Mutex<WriterState>>(vm, *w, 0)?;
        let mut s = lock(&st);
        if s.closed {
            drop(s);
            let info = ErrInfo::new(Kind::Closed, "writer is closed");
            return Err(throw_err(vm, &info));
        }
        let (mut out, bom_consumed) = begin_batch(&s);
        let mut header_pending = s.opts.auto_header && !s.header_written;
        let header_was_pending = header_pending;
        let mut records: i64 = 0;
        for row in rows {
            match encode_typed_row(vm, &s.opts, &mut header_pending, &mut out, *row) {
                Ok(()) => records += 1,
                Err(info) => {
                    drop(s);
                    return Err(throw_err(vm, &info));
                }
            }
        }
        let wrote_header = header_was_pending && !header_pending;
        let out = commit_batch(&mut s, out, records, wrote_header, bom_consumed);
        Ok(bex_str::BexStr::from(out.as_str()))
    }

    fn _to_markdown(vm: &mut BexVm, rows: &[Value], max_rows: i64) -> bex_str::BexStr {
        use bex_vm_types::RealizedTy;
        let ty = vm.current_call_type_args().first().cloned();
        let max = usize::try_from(max_rows).unwrap_or(0);

        // Header names + field types from T (or the first row's class).
        let class_info = match &ty {
            Some(RealizedTy::Class(head, type_args, _)) => {
                Some(head.ptr()).and_then(|ptr| match vm.get_object(ptr) {
                    Object::Class(c) => Some(
                        c.fields
                            .iter()
                            .map(|f| {
                                (
                                    f.name.clone(),
                                    vm.realize_field_ty(&f.field_template, type_args),
                                )
                            })
                            .collect::<Vec<_>>(),
                    ),
                    _ => None,
                })
            }
            _ => None,
        };

        let (headers, row_cells): (Vec<String>, Vec<Vec<String>>) = match class_info {
            Some(fields) => {
                let headers = fields.iter().map(|(n, _)| n.clone()).collect();
                let cells = rows
                    .iter()
                    .take(max)
                    .map(|row| match vm.as_instance(row) {
                        Ok(inst) => {
                            let values: Vec<Value> =
                                (0..fields.len()).map(|i| inst.load_field(i)).collect();
                            values
                                .iter()
                                .zip(&fields)
                                .map(|(v, (_, fty))| md_value_text(vm, *v, Some(fty)))
                                .collect()
                        }
                        Err(_) => vec!["<unrepresentable>".to_string(); fields.len()],
                    })
                    .collect();
                (headers, cells)
            }
            None => (
                vec!["value".to_string()],
                rows.iter()
                    .take(max)
                    .map(|v| vec![md_value_text(vm, *v, None)])
                    .collect(),
            ),
        };

        bex_str::BexStr::from(render_markdown(&headers, &row_cells, rows.len()).as_str())
    }

    fn _to_markdown_records(
        vm: &mut BexVm,
        records: &[Value],
        headers: Option<&[Value]>,
        max_rows: i64,
    ) -> bex_str::BexStr {
        let max = usize::try_from(max_rows).unwrap_or(0);
        let mut width = 0usize;
        let rows: Vec<Vec<String>> = records
            .iter()
            .take(max)
            .map(|rec| match vm.as_array(rec) {
                Ok(cells) => cells
                    .iter()
                    .map(|c| match vm.as_string(c) {
                        Ok(s) => s.as_str().to_string(),
                        Err(_) => "<unrepresentable>".to_string(),
                    })
                    .collect(),
                Err(_) => Vec::new(),
            })
            .collect();
        for r in &rows {
            width = width.max(r.len());
        }

        let header_names: Vec<String> = match headers {
            Some(hs) => hs
                .iter()
                .map(|h| match vm.as_string(h) {
                    Ok(s) => s.as_str().to_string(),
                    Err(_) => String::new(),
                })
                .collect(),
            None => (1..=width).map(|i| format!("col{i}")).collect(),
        };
        if header_names.is_empty() && rows.is_empty() {
            return bex_str::BexStr::from("");
        }
        bex_str::BexStr::from(render_markdown(&header_names, &rows, records.len()).as_str())
    }
}
