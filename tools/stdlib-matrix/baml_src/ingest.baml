// Input loading.
//
// Decoding goes through `baml.json.to`, not `from_string`, because the export
// format's signature records carry a `throws` key and `throws` cannot be a
// BAML field name. `baml.json.to` dispatches on the target type and honors
// `baml.FromJson` overrides at every depth, so `ExportSignature` reconciles
// that one key itself (see models.baml) and everything else takes the
// structural default. `@alias("throws")` is not an option: BEP-038
// deliberately excludes `@alias`/`@skip` from the JSON interchange path
// (`from_string`/`to_string` match raw field names only — see the comment in
// bex_vm/src/package_baml/json.rs).
//
// BUG(bex_sap): `baml.sap.parse<T>` is the one decode path that IS
// alias-aware, but it must not be used for machine-generated JSON: it
// SILENTLY LOSES DATA. Feeding it the real 1.6 MB export yields 457 items
// and 115 impls with **all 492 methods dropped to zero**, no error raised,
// which quietly turns this matrix from 127 matched into 17. An *optional*
// class-typed field whose class itself contains class-typed members coerces
// to null even when a valid object is present; the same class parses
// correctly when the field is required, or when the inner class holds only
// scalars. Minimal repro (verified 2026-07-31):
//
//   class Leaf  { display: string }
//   class Outer { inner: Leaf }          // or: { list: Leaf[] }
//   class Wrap  { name: string, f: Outer? }
//   baml.sap.parse<Wrap>("{\"name\":\"n\",\"f\":{\"inner\":{\"display\":\"int\"}}}")
//   // → f == null, silently
//
// SAP would be the wrong tool here regardless: it accepts prose- and
// markdown-fence-wrapped input, where this tool wants a machine-generated
// document to fail closed.

function parse_export_text(text: string) -> PackageExportDoc {
    baml.json.to<PackageExportDoc>(baml.json.parse(text))
}

function load_baml_surface(path: string) -> PackageExportDoc {
    let doc = parse_export_text(baml.fs.read(path));
    if (doc.format_version != 1) {
        throw baml.errors.InvalidArgument {
            message: `unsupported export format_version ${doc.format_version} in ${path} (expected 1)`,
        };
    }
    doc
}

/// What a baseline report is read for: the ratchet compares inputs and totals,
/// never individual symbols, so only this much is decoded.
///
/// Reading less is also the only thing that works. A full `SymbolMatrix` decode
/// fails on `PathStep`, the `string | ImplStep` union: `ImplStep` serializes
/// through a `baml.ToJson` override that spells its field `interface`, but
/// union decoding does not dispatch to the matching `baml.FromJson`, so it
/// looks for the raw field name and reports "no union member matched".
/// BUG(baml): `baml.json.to` should honor a union member's FromJson override.
class ReportHeader {
    format_version: int,
    provenance: MatrixProvenance,
    counts: MatrixCounts,
}

/// Reads a previously generated report's header, for use as a comparison
/// baseline. A file that cannot be decoded is an error, never a silent "no
/// baseline": treating an unreadable report as absent would let `--check` pass
/// against a corrupt or stale-format artifact.
function load_report(path: string) -> ReportHeader {
    let report = baml.json.to<ReportHeader>(baml.json.parse(baml.fs.read(path))) catch (_) {
        baml.json.ParseError => {
            throw baml.errors.InvalidArgument {
            message: `${path} is not valid JSON; regenerate it with tools/stdlib-matrix/run`,
        }
        },
        baml.json.DecodeError => {
            throw baml.errors.InvalidArgument {
            message: `${path} does not match the current report model, probably written by an older tool; regenerate it with tools/stdlib-matrix/run`,
        }
        },
    };
    if (report.format_version != 2) {
        throw baml.errors.InvalidArgument {
            message: `${path} has report format_version ${report.format_version} (expected 2); regenerate it with tools/stdlib-matrix/run`,
        };
    }
    report
}

function load_ts_surface(path: string) -> TsSurfaceDoc {
    let doc = baml.json.from_string<TsSurfaceDoc>(baml.fs.read(path));
    if (doc.format_version != 2) {
        throw baml.errors.InvalidArgument {
            message: `unsupported ts-surface format_version ${doc.format_version} in ${path} (expected 2); regenerate it with tools/stdlib-matrix/run`,
        };
    }
    doc
}
