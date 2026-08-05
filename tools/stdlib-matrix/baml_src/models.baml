// Typed models for the two input documents — the BAML export IR from
// `baml describe <pkg> --export` and the TypeScript surface from
// extractors/ts-surface.mjs — plus the matrix's own report shapes.
//
// The input models are deliberately partial: decoding ignores JSON fields the
// models do not declare, so each class lists only what the matrix consumes.
// Field names match the JSON keys exactly, with one exception: the export
// format uses a `throws` key inside signature records, and `throws` cannot be
// declared as a BAML class field. `ExportSignature` reconciles that with a
// `baml.FromJson` override; see the note there and in ingest.baml.

/// A type reference in the export IR: canonical display text plus an optional
/// head-constructor SymbolId. `unresolved: true` marks sentinel-bearing types
/// (`unknown` / `!error`).
class TyRef {
    display: string,
    head: string?,
    unresolved: bool?,
}

class ExportGeneric {
    name: string,
    bounds: string[],
}

class ExportParam {
    name: string,
    ty: TyRef,
    /// A default-valued parameter, which BAML makes named-only.
    optional: bool?,
}

/// The export format spells this record's throws facet `throws`, which cannot
/// be a BAML field name, so the field is `throws_spec` and the JSON shape is
/// reconciled by a `baml.FromJson` override rather than by rewriting the input.
/// The other fields are decoded exactly as the structural default would.
class ExportSignature {
    generics: ExportGeneric[]?,
    params: ExportParam[],
    returns: TyRef?,
    throws_spec: TyRef?,

    implements baml.FromJson {
        function from_json(j: baml.json.json) -> Self throws baml.json.JsonDecodeError {
            ExportSignature {
                generics: baml.json.to<ExportGeneric[]?>(baml.json.field(j, "generics")),
                params: baml.json.to<ExportParam[]>(baml.json.field(j, "params")),
                returns: baml.json.to<TyRef?>(baml.json.field(j, "returns")),
                throws_spec: baml.json.to<TyRef?>(baml.json.field(j, "throws")),
            }
        }
    }
}

class ExportField {
    name: string,
    docstring: string?,
    ty: TyRef?,
}

class ExportMethod {
    name: string,
    docstring: string?,
    signature: ExportSignature?,
}

/// One top-level item in the package export. `kind` discriminates
/// (class / enum / function / interface / type_alias / …); the kind-specific
/// fields are all optional so one model covers every kind.
class ExportItem {
    id: string,
    kind: string,
    name: string,
    docstring: string?,
    fields: ExportField[]?,
    methods: ExportMethod[]?,
    impls: string[]?,
    signature: ExportSignature?,
    required_methods: ExportMethod[]?,
    default_methods: ExportMethod[]?,
}

class ExportImplMethod {
    name: string,
    docstring: string?,
    signature: ExportSignature?,
    from_default: bool?,
}

/// A top-level impl record. `interface` is a declarable field name, but the
/// parser rejects it in struct-literal position, so this class can only be
/// built by JSON decoding — which is how it always arrives (tests build
/// fixtures with `baml.json.from_string` for the same reason).
class ExportImpl {
    id: string,
    interface: string,
    for_ty: TyRef,
    methods: ExportImplMethod[],
}

class PackageExportDoc {
    format_version: int,
    package: string,
    items: ExportItem[],
    impls: ExportImpl[],
}

// ── TypeScript surface (extractors/ts-surface.mjs output) ──────────────────

class TsMember {
    name: string,
    static: bool,
    signatures: string[],
    doc: string?,
    since: string,
}

class TsContainer {
    name: string,
    kind: string,
    members: TsMember[],
}

class TsSurfaceDoc {
    format_version: int,
    typescript_version: string,
    types_node_version: string?,
    containers: TsContainer[],
}

// ── matrix domain ───────────────────────────────────────────────────────────

/// One member pooled from either side. `key` is the deterministic match key
/// (see match_key in surface.baml); `display` is container-qualified for
/// readability. `via` names the interface when an attached impl contributed
/// the member. `panics`/`errors` carry the BAML throws split (TS declares
/// neither — that asymmetry is one of the matrix's main outputs).
class PooledMember {
    key: string,
    display: string,
    origin: string,
    static: bool,
    signature: string?,
    doc: string?,
    via: string?,
    /// The effective error contract, absent when the call cannot fail.
    /// (Named `errors`, not `throws`: `throws` is a keyword and cannot be a
    /// field name.)
    errors: string?,
}

/// One member row of a container comparison. `status` is "matched",
/// "baml_only", or "ts_only". Additional same-key members on either side are
/// recorded as alternates rather than separate rows.
class MemberRow {
    key: string,
    status: string,
    baml: PooledMember?,
    ts: PooledMember?,
    baml_alternates: string[],
    ts_alternates: string[],
    /// Present only on "aligned" rows: why a model judged these equivalent.
    alignment: AlignmentInfo?,
}

/// A model's judgment about one pair, carried so a reader can weigh it. An
/// aligned row is a claim, not a fact — unlike a deterministic match, which
/// follows from the names.
class AlignmentInfo {
    confidence: string,
    reason: string,
}

/// An explicit name equivalence for the deterministic pass: the BAML member's
/// key is canonicalized onto the TS member's key before matching.
class NamePair {
    baml_name: string,
    ts_name: string,
}

/// One compared container pair — a row of the matrix scope. `baml_types` are
/// qualified top-level type names whose members (own + attached impls) enter
/// the pool; `baml_namespaces` pull every member-bearing item in a namespace.
/// `ts_containers` name containers in the TS surface document. A side may be
/// empty: the row then documents a one-sided gap.
class ContainerPair {
    key: string,
    baml_types: string[],
    baml_namespaces: string[],
    ts_containers: string[],
    synonyms: NamePair[],
    note: string?,
}

class UncomparedEntry {
    name: string,
    reason: string,
}

class ContainerReport {
    key: string,
    note: string?,
    baml_origins: string[],
    ts_origins: string[],
    matched: int,
    /// Pairs a model judged equivalent, counted apart from `matched` so the
    /// deterministic figure stays comparable across runs.
    aligned: int,
    baml_only: int,
    ts_only: int,
    synthetic_hidden: int,
    rows: MemberRow[],
}

class MatrixTotals {
    containers: int,
    matched: int,
    aligned: int,
    baml_only: int,
    ts_only: int,
    synthetic_hidden: int,
}

/// Identifies the inputs a report was built from — the ratchet key.
///
/// BAML's own surface is fingerprinted by hashing the export document, not by
/// a version or a commit. We are the team building BAML, so we run this
/// against intermediate states — arbitrary commits, and working trees with
/// uncommitted work — where the release version has not moved even though the
/// stdlib has; and the git hash has the opposite problem, moving constantly
/// for commits that never touch the stdlib. Hashing the export changes exactly
/// when the surface changes, in every one of those states.
///
/// The other languages are consumed as published releases, so their surface
/// tracks the version and a version string is the sharper, cheaper key there.
///
/// The hash is computed by the `run` wrapper: BAML has no hashing API.
class MatrixProvenance {
    baml_surface_sha256: string,
    export_format_version: int,
    typescript_version: string,
    types_node_version: string?,
}

class MatrixReport {
    format_version: int,
    provenance: MatrixProvenance,
    uncompared_baml: UncomparedEntry[],
    uncompared_ts: UncomparedEntry[],
    containers: ContainerReport[],
    totals: MatrixTotals,
}

// ── ratchet ─────────────────────────────────────────────────────────────────

/// Per-container counts, carried in deltas so a reader sees both sides.
class ContainerCounts {
    key: string,
    matched: int,
    baml_only: int,
    ts_only: int,
}

/// How one container's counts moved between the baseline report and this
/// run. `status` is "added", "removed", or "changed" — unchanged containers
/// are not recorded.
class ContainerDelta {
    key: string,
    status: string,
    baseline: ContainerCounts?,
    current: ContainerCounts?,
}

/// The verdict of comparing a fresh run against a baseline report.
///
/// `up_to_date` is keyed on provenance, not on the document body: the body is
/// expected to drift once LLM alignment lands, while the inputs are exactly
/// what determines whether the report still describes reality. `body_differs`
/// is advisory — with identical inputs it means the tool itself changed
/// between the two reports, not something CI should fail on.
class BaselineComparison {
    up_to_date: bool,
    baseline_present: bool,
    body_differs: bool,
    input_changes: string[],
    deltas: ContainerDelta[],
    regressions: string[],
    improvements: string[],
    neutral: string[],
}

class CheckResult {
    markdown: string,
    output_file: string,
    json_file: string,
    comparison: BaselineComparison,
    report: MatrixReport,
}

/// `report` is the canonical artifact — the wrapper writes it to `json_file`,
/// and `markdown` is a rendered view of exactly that structure.
/// `build_matrix_aligned`'s result: the same artifacts plus what the model
/// contributed and what was thrown out.
class AlignedMatrixResult {
    markdown: string,
    output_file: string,
    json_file: string,
    report: MatrixReport,
    applied: int,
    rejected: string[],
}

class MatrixResult {
    markdown: string,
    output_file: string,
    json_file: string,
    report: MatrixReport,
}
