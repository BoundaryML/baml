// Typed models for the two input documents — the BAML export IR from
// `baml describe <pkg> --export` and the TypeScript surface from
// extractors/ts-surface.mjs.
//
// The matrix's own shapes live with the code that builds them: symbols.baml
// and ts_symbols.baml for the two symbol layers, report.baml for the matrix,
// entry.baml for the entry points' results.
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
        function from_json(j: baml.json.json) -> Self throws baml.json.DecodeError {
            ExportSignature {
                generics: baml.json.to<ExportGeneric[]?>(baml.json.field(j, "generics")),
                params: baml.json.to<ExportParam[]>(baml.json.field(j, "params")),
                returns: baml.json.to<TyRef?>(baml.json.field(j, "returns")),
                throws_spec: baml.json.to<TyRef?>(baml.json.field(j, "throws")),
            }
        }
    }
}

/// Every member record carries the id that addresses it. Ids are the export's
/// stable identity — content-derived, unique across the document, and
/// unaffected by declaration order — which is what makes a judgement about a
/// symbol survive into the next revision.
class ExportField {
    id: string,
    name: string,
    docstring: string?,
    ty: TyRef?,
}

class ExportMethod {
    id: string,
    name: string,
    docstring: string?,
    signature: ExportSignature?,
}

class ExportVariant {
    id: string,
    name: string,
    docstring: string?,
}

/// An interface's associated type. `default` is present when the interface
/// gives the type a default an implementor may leave alone.
class ExportAssocType {
    id: string,
    name: string,
    default: TyRef?,
}

/// One top-level item in the package export. `kind` discriminates
/// (class / enum / function / interface / type_alias / …); the kind-specific
/// fields are all optional so one model covers every kind.
class ExportItem {
    id: string,
    kind: string,
    name: string,
    /// Declared type parameters, for generic classes and interfaces.
    generics: ExportGeneric[]?,
    docstring: string?,
    fields: ExportField[]?,
    variants: ExportVariant[]?,
    methods: ExportMethod[]?,
    impls: string[]?,
    signature: ExportSignature?,
    /// A type alias's target, fully resolved.
    resolved: TyRef?,
    assoc_types: ExportAssocType[]?,
    required_methods: ExportMethod[]?,
    default_methods: ExportMethod[]?,
}

/// A method as an impl block lists it. `id` addresses this entry — under the
/// block, since one declaration is re-listed by every implementor that inherits
/// it — while `declared_by` names the declaration itself, when it has an id of
/// its own. Two entries sharing a `declared_by` are the same code reached
/// through different receivers, which is what lets one judgement cover both.
class ExportImplMethod {
    id: string,
    declared_by: string?,
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

/// A type that holds members: a class, a bare interface, or a namespace object.
class TsContainer {
    name: string,
    /// `class`, `interface`, or `namespace`.
    kind: string,
    /// A namespace object — `JSON`, `Math`, `console` — whose members are
    /// reached on the value itself. It has no prototype, so its members are
    /// addressed like statics even though the lib declares them on an
    /// interface.
    singleton: bool?,
    doc: string?,
    since: string,
    members: TsMember[],
}

/// Something reached on a module rather than on a type: a global function, a
/// module's exported function, or a value like `process.argv`.
class TsFunction {
    name: string,
    signatures: string[],
    doc: string?,
    since: string,
}

/// A top-level group: `(globals)`, `(web)`, or one node module.
///
/// Modules group but are not symbols, exactly as BAML namespaces are not. The
/// things a reader asks about — `Date`, `console`, `node:os.hostname` — are the
/// containers and functions inside them.
class TsModule {
    name: string,
    /// `globals`, `web`, or `node`.
    kind: string,
    containers: TsContainer[],
    functions: TsFunction[],
}

class TsSurfaceDoc {
    format_version: int,
    typescript_version: string,
    types_node_version: string?,
    modules: TsModule[],
}

// ── ratchet ─────────────────────────────────────────────────────────────────

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

/// The verdict of comparing a fresh run against a baseline report.
///
/// `up_to_date` is keyed on provenance, not on the report's contents: the
/// contents drift once model judgments land, while the inputs are what
/// determine whether an older report still describes reality.
///
/// `counts_differ` is advisory, and named for exactly what it compares. The
/// baseline is read as a `ReportHeader`, which carries the tallies and nothing
/// else, so two reports with identical counts but different judgements,
/// reasons, or divergences compare equal here. With identical inputs a
/// difference means the tool itself changed between the two reports — not
/// something CI should fail on.
class BaselineComparison {
    up_to_date: bool,
    baseline_present: bool,
    counts_differ: bool,
    input_changes: string[],
    regressions: string[],
    improvements: string[],
    neutral: string[],
}
