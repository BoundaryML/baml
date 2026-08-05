// Matching: fold the two pooled sides of each mapping row into MemberRows and
// assemble the full MatrixReport.

function status_rank(status: string) -> string {
    match (status) {
        "matched" => "0",
        "aligned" => "1",
        "baml_only" => "2",
        _ => "3",
    }
}

function build_container_report(
    row: ContainerPair,
    baml_pool: PoolAccumulator,
    ts_pool: PooledMember[],
) -> ContainerReport {
    let by_key: map<string, MemberRow> = {};
    let key_order: string[] = [];

    for (let member in baml_pool.members) {
        if let existing: MemberRow = by_key.get(member.key) {
            existing.baml_alternates.push(member.display);
        } else {
            let _ = by_key.set(member.key, MemberRow {
                key: member.key,
                status: "baml_only",
                baml: member,
                ts: null,
                baml_alternates: [],
                ts_alternates: [],
                alignment: null,
            });
            key_order.push(member.key);
        }
    }
    for (let member in ts_pool) {
        if let existing: MemberRow = by_key.get(member.key) {
            if let _already: PooledMember = existing.ts {
                existing.ts_alternates.push(member.display);
            } else {
                existing.ts = member;
                if (existing.status == "baml_only") {
                    existing.status = "matched";
                }
            }
        } else {
            let _ = by_key.set(member.key, MemberRow {
                key: member.key,
                status: "ts_only",
                baml: null,
                ts: member,
                baml_alternates: [],
                ts_alternates: [],
                alignment: null,
            });
            key_order.push(member.key);
        }
    }

    let rows: MemberRow[] = [];
    for (let key in key_order) {
        if let member_row: MemberRow = by_key.get(key) {
            rows.push(member_row);
        }
    }
    rows.sort_by_key(
        (r) -> {
            `${status_rank(r.status)}:${r.key}`
        },
    );

    let matched = 0;
    let aligned = 0;
    let baml_only = 0;
    let ts_only = 0;
    for (let member_row in rows) {
        match (member_row.status) {
            "matched" => { matched += 1; },
            "aligned" => { aligned += 1; },
            "baml_only" => { baml_only += 1; },
            _ => { ts_only += 1; },
        }
    }

    ContainerReport {
        key: row.key,
        note: row.note,
        baml_origins: row.baml_types.concat(row.baml_namespaces),
        ts_origins: row.ts_containers,
        matched: matched,
        aligned: aligned,
        baml_only: baml_only,
        ts_only: ts_only,
        synthetic_hidden: baml_pool.synthetic_hidden,
        rows: rows,
    }
}

function build_matrix_report(
    export_doc: PackageExportDoc,
    ts_doc: TsSurfaceDoc,
    baml_surface_sha256: string,
) -> MatrixReport {
    let rows = container_mapping();
    let uncompared_baml = uncompared_baml_surfaces();
    let uncompared_ts = uncompared_ts_surfaces();
    let gaps = mapping_gaps(export_doc, ts_doc, rows, uncompared_baml, uncompared_ts);
    if (gaps.length() > 0) {
        throw baml.errors.InvalidArgument {
            message: `the container mapping is out of date:\n  ${gaps.join("\n  ")}`,
        };
    }

    let index = index_export(export_doc);
    let containers: ContainerReport[] = [];
    let matched = 0;
    let aligned = 0;
    let baml_only = 0;
    let ts_only = 0;
    let synthetic_hidden = 0;
    for (let row in rows) {
        let report = build_container_report(
            row,
            pool_baml_side(index, row),
            pool_ts_side(ts_doc, row),
        );
        matched += report.matched;
        aligned += report.aligned;
        baml_only += report.baml_only;
        ts_only += report.ts_only;
        synthetic_hidden += report.synthetic_hidden;
        containers.push(report);
    }

    MatrixReport {
        format_version: 1,
        provenance: MatrixProvenance {
            baml_surface_sha256: baml_surface_sha256,
            export_format_version: export_doc.format_version,
            typescript_version: ts_doc.typescript_version,
            types_node_version: ts_doc.types_node_version,
        },
        uncompared_baml: uncompared_baml,
        uncompared_ts: uncompared_ts,
        containers: containers,
        totals: MatrixTotals {
            containers: containers.length(),
            matched: matched,
            aligned: aligned,
            baml_only: baml_only,
            ts_only: ts_only,
            synthetic_hidden: synthetic_hidden,
        },
    }
}
