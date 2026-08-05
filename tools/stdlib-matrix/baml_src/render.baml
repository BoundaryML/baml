// Markdown rendering of the MatrixReport. The report file is the reviewable
// artifact; keep the output deterministic (no timestamps, no environment).

function escape_cell(value: string) -> string {
    value
        .replace_all("\\", "\\\\")
        .replace_all("|", "\\|")
        .replace_all("\r", "")
        .replace_all("\n", "<br>")
}

function optional_cell(value: string?) -> string {
    escape_cell(value ?? "")
}

function member_cell(member: PooledMember?, alternates: string[]) -> string {
    let out = "";
    if let m: PooledMember = member {
        let static_marker = "";
        if (m.static) {
            static_marker = "static ";
        }
        let via_marker = "";
        if let via: string = m.via {
            via_marker = ` (via ${via})`;
        }
        out = escape_cell(`${static_marker}${m.display}${via_marker}`);
        if (alternates.length() > 0) {
            out = `${out}<br>also: ${escape_cell(alternates.join(", "))}`;
        }
    }
    out
}

function throws_cell(member: PooledMember?) -> string {
    let parts: string[] = [];
    if let m: PooledMember = member {
        if (m.errors.length() > 0) {
            parts.push(`errors: ${m.errors.join(", ")}`);
        }
        if (m.panics.length() > 0) {
            parts.push(`panics: ${m.panics.join(", ")}`);
        }
    }
    escape_cell(parts.join("; "))
}

function signature_cell(member: PooledMember?) -> string {
    let out = "";
    if let m: PooledMember = member {
        out = optional_cell(m.signature);
    }
    out
}

function render_status_label(status: string) -> string {
    match (status) {
        "baml_only" => "BAML only",
        "ts_only" => "TS only",
        "aligned" => "aligned",
        _ => status,
    }
}

/// Aligned rows carry the model's confidence and reasoning; deterministic
/// rows carry nothing, which is itself the signal that no judgment was needed.
function alignment_cell(member_row: MemberRow) -> string {
    if let info: AlignmentInfo = member_row.alignment {
        escape_cell(`${info.confidence}: ${info.reason}`)
    } else {
        ""
    }
}

function render_member_row(member_row: MemberRow) -> string {
    [
        `| ${render_status_label(member_row.status)} `,
        `| ${member_cell(member_row.baml, member_row.baml_alternates)} `,
        `| ${member_cell(member_row.ts, member_row.ts_alternates)} `,
        `| ${signature_cell(member_row.baml)} `,
        `| ${signature_cell(member_row.ts)} `,
        `| ${throws_cell(member_row.baml)} `,
        `| ${alignment_cell(member_row)} |`,
    ]
    .join("")
}

function render_container_section(report: ContainerReport) -> string {
    let out: string[] = [];
    out.push(`### ${report.key}`);
    out.push("");
    out.push(`BAML: ${render_origin_list(report.baml_origins)} · TS: ${render_origin_list(report.ts_origins)}`);
    if let note: string = report.note {
        out.push("");
        out.push(`> ${note}`);
    }
    out.push("");
    out.push(`matched ${report.matched} · aligned ${report.aligned} · BAML only ${report.baml_only} · TS only ${report.ts_only}`);
    out.push("");
    out.push("| status | BAML member | TS member | BAML signature | TS signature | BAML throws | alignment |");
    out.push("| --- | --- | --- | --- | --- | --- | --- |");
    for (let member_row in report.rows) {
        out.push(render_member_row(member_row));
    }
    if (report.synthetic_hidden > 0) {
        out.push("");
        out.push(`(${report.synthetic_hidden} compiler-synthesized BAML member(s) hidden)`);
    }
    out.join("\n")
}

function render_origin_list(origins: string[]) -> string {
    if (origins.length() == 0) {
        "—"
    } else {
        origins.map(
            (origin) -> {
                `\`${origin}\``
            },
        )
        .join(", ")
    }
}

function render_uncompared_section(title: string, entries: UncomparedEntry[]) -> string {
    let out: string[] = [];
    out.push(`### ${title}`);
    out.push("");
    for (let entry in entries) {
        out.push(`- \`${entry.name}\` — ${entry.reason}`);
    }
    out.join("\n")
}

/// Stands in for a version the extractor could not determine. Shared with the
/// ratchet so an absent version reads the same when reported as when rendered.
function provenance_absent() -> string {
    "absent"
}

/// Shows the reader which inputs this view was built from. Purely
/// presentational: `run --check` reads provenance from the canonical JSON
/// report, so this block is free to change shape without breaking the ratchet.
function render_inputs_section(provenance: MatrixProvenance) -> string {
    [
        "## Inputs",
        "",
        "Generated from these inputs; `run --check` compares them against a baseline report.",
        "",
        `- BAML stdlib surface: \`${provenance.baml_surface_sha256}\` (sha256 of the export)`,
        `- BAML export format: v${provenance.export_format_version}`,
        `- TypeScript: ${provenance.typescript_version}`,
        `- @types/node: ${provenance.types_node_version ?? provenance_absent()}`,
    ]
    .join("\n")
}

function render_matrix_markdown(report: MatrixReport) -> string {
    let out: string[] = [];
    out.push("# BAML ↔ TypeScript stdlib surface matrix");
    out.push("");
    out.push("Generated by `tools/stdlib-matrix/run` (structure-only pass: deterministic name matching, no LLM alignment yet). Do not edit by hand.");
    out.push("");
    out.push(render_inputs_section(report.provenance));
    out.push("");
    out.push("## Summary");
    out.push("");
    out.push("| container | matched | aligned | BAML only | TS only | note |");
    out.push("| --- | --- | --- | --- | --- | --- |");
    for (let container in report.containers) {
        out.push(`| ${container.key} | ${container.matched} | ${container.aligned} | ${container.baml_only} | ${container.ts_only} | ${optional_cell(container.note)} |`);
    }
    out.push("");
    out.push(`Totals: ${report.totals.matched} matched · ${report.totals.aligned} aligned · ${report.totals.baml_only} BAML only · ${report.totals.ts_only} TS only across ${report.totals.containers} containers (${report.totals.synthetic_hidden} compiler-synthesized BAML members hidden).`);
    out.push("");
    out.push("*matched* is deterministic name agreement; *aligned* is a model's judgment that two differently-named members are the same operation, and carries its confidence and reasoning.");
    out.push("");
    out.push("## Uncompared surfaces");
    out.push("");
    out.push(render_uncompared_section("BAML", report.uncompared_baml));
    out.push("");
    out.push(render_uncompared_section("TypeScript", report.uncompared_ts));
    out.push("");
    out.push("## Containers");
    out.push("");
    for (let container in report.containers) {
        out.push(render_container_section(container));
        out.push("");
    }
    out.join("\n")
}
