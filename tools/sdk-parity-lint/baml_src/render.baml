function yaml_string(value: string) -> string {
    baml.json.serialize(value)
}

function render_inventory_test(test_case: InventoryTest) -> string {
    [
        `  - sdk_test_id: ${yaml_string(test_case.sdk_test_id)}`,
        `    raw_name: ${yaml_string(test_case.raw_name)}`,
        `    source_file: ${yaml_string(test_case.source_file)}`,
        `    source_line: ${test_case.source_line}`,
        `    source_column: ${test_case.source_column}`,
        `    rule_id: ${yaml_string(test_case.rule_id)}`,
    ]
    .join("\n")
}

function render_inventory(inventory: SdkInventory, ast_grep_version: string) -> string {
    let rendered_tests = inventory
        .tests
        .map(
            (test_case) -> {
                render_inventory_test(test_case)
            },
        )
        .join("\n");
    [
        `ast_grep_version: ${yaml_string(ast_grep_version)}`,
        `sdk_env: ${yaml_string(inventory.sdk_env)}`,
        `source_root: ${yaml_string(inventory.source_root)}`,
        if (rendered_tests.length() == 0) {
            "tests: []"
        } else {
            `tests:\n${rendered_tests}`
        },
        "",
    ]
    .join("\n")
}

function escape_markdown_cell(value: string) -> string {
    value
        .replace_all("\\", "\\\\")
        .replace_all("|", "\\|")
        .replace_all("\r", "")
        .replace_all("\n", "<br>")
}

function required_label(row: ParityRow, sdk_envs: string[]) -> string {
    if (baml.deep_equals(row.required_envs, sdk_envs)) {
        "all"
    } else {
        row.required_envs.join(", ")
    }
}

function render_parity_row(row: ParityRow, sdk_envs: string[]) -> string {
    let markers = row
        .cells
        .map(
            (cell) -> {
                if (cell.present) {
                    "y"
                } else {
                    "-"
                }
            },
        )
        .join(" | ");
    `| ${escape_markdown_cell(row.sdk_test_id)} | ${markers} | ${escape_markdown_cell(required_label(row, sdk_envs))} | ${escape_markdown_cell(row.reason)} |`
}

function render_parity_report(report: ParityReport, ast_grep_version: string) -> string {
    let header = `| Test case | ${report.sdk_envs.join(" | ")} | Required in | Reason |`;
    let separator = `| --- | ${report.sdk_envs.map((_sdk_env) -> { "---" }).join(" | ")} | --- | --- |`;
    let rows: string[] = [];
    for (let row in report.rows) {
        rows.push(render_parity_row(row, report.sdk_envs));
    }
    [
        "# SDK test parity analysis",
        "",
        `Generated with ${ast_grep_version}. Distinct canonical test IDs: ${report.rows.length()}. IDs with required gaps: ${report.required_gap_count}.`,
        "",
        "A `y` means a matching declaration is present in that SDK environment; `-` means it is absent. Presence does not mean the test passed.",
        "",
        header,
        separator,
        rows.join("\n"),
        "",
    ]
    .join("\n")
}

function uppercase_outcome(outcome: string) -> string {
    if (outcome == "worse") {
        "WORSE"
    } else if (outcome == "better") {
        "BETTER"
    } else {
        "UNCHANGED"
    }
}

function render_pair_list(title: string, pairs: ParityPair[], weakened: bool) -> string[] {
    if (pairs.length() == 0) {
        return [];
    }
    let lines = [`### ${title}`, ""];
    for (let pair in pairs) {
        if (weakened) {
            lines.push(`- \`${pair.sdk_test_id}\` no longer requires \`${pair.sdk_env}\``);
        } else {
            lines.push(`- \`${pair.sdk_test_id}\` in \`${pair.sdk_env}\``);
        }
    }
    lines.push("");
    lines
}

function render_ratchet_report(report: ParityReport, comparison: ParityComparison) -> string {
    let full_parity = 0;
    for (let row in report.rows) {
        if (!row.has_required_gap) {
            full_parity += 1;
        }
    }
    let lines = [
        "# SDK test coverage parity",
        "",
        "This report inventories checked-in test declarations. It does not report whether tests passed.",
        "",
        `Distinct exact test IDs: ${report.rows.length()}. IDs with complete required parity: ${full_parity}. Required gaps: ${report.required_gap_count}.`,
        "",
        `Baseline ratchet: ${uppercase_outcome(comparison.outcome)}. Required gaps: ${comparison.current_missing_count} (baseline: ${comparison.baseline_missing_count}). Present declarations: ${comparison.current_present_count} (baseline: ${comparison.baseline_present_count}). Newly missing required pairs: ${comparison.regressions.length()}. Resolved baseline gaps: ${comparison.resolved.length()}. Weakened requirements: ${comparison.weakened_requirements.length()}.`,
        "",
        `| Test case | ${report.sdk_envs.join(" | ")} | Required in | Reason |`,
        `| --- | ${report.sdk_envs.map((_sdk_env) -> { "---" }).join(" | ")} | --- | --- |`,
    ];
    for (let row in report.rows) {
        lines.push(render_parity_row(row, report.sdk_envs));
    }
    lines.push("");
    lines.push("## Baseline comparison");
    lines.push("");
    for (let line in render_pair_list("Newly missing required pairs", comparison.regressions, false)) {
        lines.push(line);
    }
    for (let line in render_pair_list("Resolved baseline gaps", comparison.resolved, false)) {
        lines.push(line);
    }
    for (
        let line in render_pair_list("Weakened required-environment sets", comparison.weakened_requirements, true)
    ) {
        lines.push(line);
    }
    if (
        comparison.regressions.length() == 0
            && comparison.resolved.length() == 0
            && comparison.weakened_requirements.length() == 0
    ) {
        lines.push("No required coverage pairs changed.");
        lines.push("");
    }
    lines.join("\n")
}
