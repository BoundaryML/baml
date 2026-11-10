//! Classification of every Rust-backed builtin: its census group and the
//! adapter form the generator emits for it on the shared object format.
//!
//! [`ROWS`](crate::triage_table::ROWS) holds one row per native declaration,
//! sorted by path. [`check_coverage`] fails when a declaration has no row or
//! a row names no declaration, so a new `$rust_function` or
//! `$rust_io_function` cannot be added without classifying it.

use crate::{NativeBuiltin, triage_table::ROWS};

/// The census bucket (`v2-design-review/stdlib-surface.md`, a to e).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Group {
    /// (a) Scalars and strings.
    Scalar,
    /// (b) Collections, bytes and bigint.
    Collection,
    /// (c) Classes, enums, unions and dynamic data.
    ClassData,
    /// (d) VM, call, task, reflection and trace services.
    VmInternal,
    /// (e) I/O and host boundary: every sysop plus `sys.argv`.
    HostIo,
}

impl Group {
    pub const ALL: [Group; 5] = [
        Group::Scalar,
        Group::Collection,
        Group::ClassData,
        Group::VmInternal,
        Group::HostIo,
    ];

    /// The census letter.
    pub const fn census_letter(self) -> char {
        match self {
            Group::Scalar => 'a',
            Group::Collection => 'b',
            Group::ClassData => 'c',
            Group::VmInternal => 'd',
            Group::HostIo => 'e',
        }
    }
}

/// What the generator emits for a builtin on the shared object format.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AdapterForm {
    /// One adapter that reads the receiver's descriptor and branches on its
    /// slot kind. Per-kind bodies exist but stay private.
    Generic,
    /// The generic adapter plus one public adapter per slot kind, for
    /// compiled code that knows the kind statically.
    Specialized,
    /// Keeps today's `Value` glue: it touches no object slots (scalars,
    /// strings, sysops with owned marshalling).
    ValueGlue,
    /// Touches object layout but has no adapter yet.
    Pending(PendingReason),
}

impl AdapterForm {
    /// Whether the generator emits an adapter for this row.
    pub const fn has_adapter(self) -> bool {
        match self {
            AdapterForm::Generic | AdapterForm::Specialized => true,
            AdapterForm::ValueGlue | AdapterForm::Pending(_) => false,
        }
    }
}

/// Why a builtin that touches object layout has no adapter yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PendingReason {
    /// Yields to a BAML callable per element; needs the PR C continuation
    /// ABI over slot-typed receivers.
    Callback,
    /// Fixed-signature container or string operation outside the v1 set;
    /// same generic shape as the generated ones.
    OutsideV1Set,
    /// `uint8array` payload; byte objects have no slot kind in the design yet.
    Bytes,
    /// Heap bigint payload; its object layout belongs to PR A.
    Bigint,
    /// Reads or builds class instances; needs per-class slot offsets from
    /// the descriptor table.
    ClassFields,
    /// Walks arbitrary values by runtime type (JSON, SAP, structural
    /// string/equality/hash, CSV decode); needs descriptor type ids and
    /// override continuations.
    DynamicWalker,
    /// Operands are declared erased (`unknown` or a bare generic), so the
    /// signature does not say which slot kind to branch on.
    ErasedOperands,
    /// Futures, tasks, trace, testing, cleanup and error context: frames and
    /// scheduler state, not slot layout.
    VmService,
    /// Reflection over type and function objects.
    Reflection,
}

impl PendingReason {
    pub const ALL: [PendingReason; 9] = [
        PendingReason::Callback,
        PendingReason::OutsideV1Set,
        PendingReason::Bytes,
        PendingReason::Bigint,
        PendingReason::ClassFields,
        PendingReason::DynamicWalker,
        PendingReason::ErasedOperands,
        PendingReason::VmService,
        PendingReason::Reflection,
    ];

    /// One line on what unblocks the adapter.
    pub const fn description(self) -> &'static str {
        match self {
            PendingReason::Callback => {
                "yields to a BAML callable; needs the PR C continuation ABI over slot-typed receivers"
            }
            PendingReason::OutsideV1Set => {
                "outside the v1 set (escape EFFECTS + hot list); same generic shape, add a body and flip the row"
            }
            PendingReason::Bytes => "uint8array payload; byte objects have no slot kind yet (PR A)",
            PendingReason::Bigint => "heap bigint payload; its object layout belongs to PR A",
            PendingReason::ClassFields => {
                "reads or builds class instances; needs per-class slot offsets from the PR A descriptor table"
            }
            PendingReason::DynamicWalker => {
                "walks values by runtime type; needs descriptor type ids (PR A) and override continuations (PR C)"
            }
            PendingReason::ErasedOperands => {
                "operands declared erased (unknown or bare generic); the signature names no slot kind"
            }
            PendingReason::VmService => "frames, tasks or scheduler state, not slot layout; PR C",
            PendingReason::Reflection => {
                "type and function objects; static tables in PR D, interpreter in PR C"
            }
        }
    }
}

/// One classified native declaration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TriageRow {
    /// The declaration's native key, as [`NativeBuiltin::path`].
    pub path: &'static str,
    pub group: Group,
    pub form: AdapterForm,
}

/// Every native declaration across all packages: VM natives and sysops,
/// sorted by path.
pub fn native_inventory() -> Result<Vec<NativeBuiltin>, crate::ExtractNativeBuiltinsError> {
    let packages: std::collections::BTreeSet<_> =
        baml_builtins2::ALL.iter().map(|f| f.package).collect();
    let mut declarations = Vec::new();
    for package in packages {
        let (vm, io, _) = crate::extract_native_builtins_for(package)?;
        declarations.extend(vm);
        declarations.extend(io);
    }
    declarations.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(declarations)
}

/// All rows, sorted by path.
pub fn rows() -> &'static [TriageRow] {
    ROWS
}

/// The row for a native key.
pub fn row_for(path: &str) -> Option<&'static TriageRow> {
    ROWS.binary_search_by(|row| row.path.cmp(path))
        .ok()
        .map(|index| &ROWS[index])
}

/// A mismatch between the table and the declarations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoverageError {
    /// A declaration has no row.
    Unclassified(String),
    /// A row names no declaration.
    Stale(&'static str),
    /// Rows are out of order or repeated at this path.
    Unsorted(&'static str),
}

impl std::fmt::Display for CoverageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CoverageError::Unclassified(path) => {
                write!(f, "native `{path}` has no row in triage_table.rs")
            }
            CoverageError::Stale(path) => {
                write!(f, "triage_table.rs row `{path}` names no declaration")
            }
            CoverageError::Unsorted(path) => {
                write!(
                    f,
                    "triage_table.rs row `{path}` is out of order or repeated"
                )
            }
        }
    }
}

/// Every declaration has exactly one row and every row a declaration.
pub fn check_coverage(declarations: &[NativeBuiltin]) -> Result<(), Vec<CoverageError>> {
    let declared: std::collections::BTreeSet<&str> =
        declarations.iter().map(|d| d.path.as_str()).collect();
    let unsorted = ROWS
        .windows(2)
        .filter(|pair| pair[0].path >= pair[1].path)
        .map(|pair| CoverageError::Unsorted(pair[1].path));
    let unclassified = declared
        .iter()
        .filter(|path| row_for(path).is_none())
        .map(|path| CoverageError::Unclassified((*path).to_string()));
    let stale = ROWS
        .iter()
        .filter(|row| !declared.contains(row.path))
        .map(|row| CoverageError::Stale(row.path));
    let errors: Vec<_> = unsorted.chain(unclassified).chain(stale).collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_native_declaration_has_exactly_one_row() {
        let declarations = native_inventory().unwrap();
        if let Err(errors) = check_coverage(&declarations) {
            let lines: Vec<String> = errors.iter().map(ToString::to_string).collect();
            panic!("{}", lines.join("\n"));
        }
    }

    /// Every builtin the escape analysis models gets an adapter.
    #[test]
    fn every_escape_modelled_builtin_has_an_adapter() {
        let missing: Vec<&str> = baml_compiler2_mir::escape::EFFECTS
            .iter()
            .map(|(path, _)| *path)
            .filter(|path| !row_for(path).is_some_and(|row| row.form.has_adapter()))
            .collect();
        assert!(missing.is_empty(), "no adapter for {missing:?}");
    }

    /// Sysops never touch the interpreter's object slots.
    #[test]
    fn host_io_rows_keep_value_glue() {
        let wrong: Vec<&str> = ROWS
            .iter()
            .filter(|row| row.group == Group::HostIo && row.form != AdapterForm::ValueGlue)
            .map(|row| row.path)
            .collect();
        assert!(wrong.is_empty(), "{wrong:?}");
    }
}
