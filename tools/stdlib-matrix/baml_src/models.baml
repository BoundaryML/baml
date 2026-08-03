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
}

/// The declared/effective throws facet of a signature. Decoded from the
/// `throws_spec` key that ingest synthesizes (the on-disk key is `throws`).
class ExportThrows {
    declared: TyRef?,
    effective: TyRef?,
    panics: TyRef[],
    errors: TyRef[],
}

/// The export format spells this record's throws facet `throws`, which cannot
/// be a BAML field name, so the field is `throws_spec` and the JSON shape is
/// reconciled by a `baml.FromJson` override rather than by rewriting the input.
/// The other fields are decoded exactly as the structural default would.
class ExportSignature {
    generics: ExportGeneric[]?,
    params: ExportParam[],
    returns: TyRef?,
    throws_spec: ExportThrows?,

    implements baml.FromJson {
        function from_json(j: baml.json.json) -> Self throws baml.json.JsonDecodeError {
            ExportSignature {
                generics: baml.json.to<ExportGeneric[]?>(baml.json.field(j, "generics")),
                params: baml.json.to<ExportParam[]>(baml.json.field(j, "params")),
                returns: baml.json.to<TyRef?>(baml.json.field(j, "returns")),
                throws_spec: baml.json.to<ExportThrows?>(baml.json.field(j, "throws")),
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
    panics: string[],
    errors: string[],
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
    baml_only: int,
    ts_only: int,
    synthetic_hidden: int,
    rows: MemberRow[],
}

class MatrixTotals {
    containers: int,
    matched: int,
    baml_only: int,
    ts_only: int,
    synthetic_hidden: int,
}

class MatrixReport {
    format_version: int,
    export_format_version: int,
    typescript_version: string,
    types_node_version: string?,
    uncompared_baml: UncomparedEntry[],
    uncompared_ts: UncomparedEntry[],
    containers: ContainerReport[],
    totals: MatrixTotals,
}

class MatrixResult {
    markdown: string,
    output_file: string,
    report: MatrixReport,
}
