function split_markdown_row(line: string, source: string) -> string[] {
    if (!line.starts_with("|") || !line.ends_with("|")) {
        throw baml.errors.ParseError {
            message: `invalid parity table row in ${source}: ${line}`,
        };
    }
    let cells: string[] = [];
    let current = "";
    let escaped = false;
    let index = 1;
    while (index < line.length() - 1) {
        let char = line.char_at(index);
        if (escaped) {
            current += char;
            escaped = false;
        } else if (char == "\\") {
            escaped = true;
        } else if (char == "|") {
            cells.push(current.trim());
            current = "";
        } else {
            current += char;
        }
        index += 1;
    }
    if (escaped) {
        throw baml.errors.ParseError {
            message: `dangling escape in parity table row in ${source}: ${line}`,
        };
    }
    cells.push(current.trim());
    cells
}

function required_environments_from_cell(
    cell: string,
    sdk_envs: string[],
    sdk_test_id: string,
    source: string,
) -> string[] {
    if (cell == "all") {
        return sdk_envs;
    }
    let requested: map<string, bool> = {};
    for (let value in cell.split(",")) {
        let sdk_env = value.trim();
        if (sdk_env.length() == 0) {
            throw baml.errors.ParseError {
                message: `empty required environment set for ${sdk_test_id} in ${source}`,
            };
        }
        if (!array_contains(sdk_envs, sdk_env)) {
            throw baml.errors.ParseError {
                message: `unknown required environment ${sdk_env} for ${sdk_test_id} in ${source}`,
            };
        }
        let _ = requested.set(sdk_env, true);
    }
    let ordered: string[] = [];
    for (let sdk_env in sdk_envs) {
        if (requested.has(sdk_env)) {
            ordered.push(sdk_env);
        }
    }
    if (ordered.length() == 0) {
        throw baml.errors.ParseError {
            message: `empty required environment set for ${sdk_test_id} in ${source}`,
        };
    }
    ordered
}

function parse_baseline(contents: string, source: string, sdk_envs: string[]) -> ParityReport {
    let lines = contents.lines();
    let header_index = -1;
    let index = 0;
    while (index < lines.length()) {
        if (lines[index].starts_with("| Test case |")) {
            header_index = index;
            break;
        }
        index += 1;
    }
    if (header_index < 0) {
        throw baml.errors.ParseError {
            message: `baseline has no parity table: ${source}`,
        };
    }
    let expected_header: string[] = ["Test case"];
    for (let sdk_env in sdk_envs) {
        expected_header.push(sdk_env);
    }
    expected_header.push("Required in");
    expected_header.push("Reason");
    let header = split_markdown_row(lines[header_index], source);
    if (header != expected_header) {
        throw baml.errors.ParseError {
            message: `baseline environment columns do not match the configured matrix: ${source}`,
        };
    }
    if (header_index + 1 >= lines.length()) {
        throw baml.errors.ParseError {
            message: `baseline parity table has no separator row: ${source}`,
        };
    }
    let separator = split_markdown_row(lines[header_index + 1], source);
    if (separator.length() != expected_header.length()) {
        throw baml.errors.ParseError {
            message: `invalid baseline parity table separator: ${source}`,
        };
    }
    for (let cell in separator) {
        if (cell != "---") {
            throw baml.errors.ParseError {
                message: `invalid baseline parity table separator: ${source}`,
            };
        }
    }
    let rows: ParityRow[] = [];
    let seen: map<string, bool> = {};
    let required_gap_count = 0;
    index = header_index + 2;
    while (index < lines.length() && lines[index].starts_with("|")) {
        let cells = split_markdown_row(lines[index], source);
        if (cells.length() != expected_header.length()) {
            throw baml.errors.ParseError {
                message: `invalid baseline parity table width in ${source}: ${lines[index]}`,
            };
        }
        let sdk_test_id = cells[0];
        if let _separator: int = sdk_test_id.index_of("/") {
        } else {
            throw baml.errors.ParseError {
                message: `invalid test ID in baseline ${source}: ${sdk_test_id}`,
            };
        }
        if (seen.has(sdk_test_id)) {
            throw baml.errors.ParseError {
                message: `duplicate test ID in baseline ${source}: ${sdk_test_id}`,
            };
        }
        let _ = seen.set(sdk_test_id, true);
        let required_envs = required_environments_from_cell(cells[cells.length() - 2], sdk_envs, sdk_test_id, source);
        let parity_cells: ParityCell[] = [];
        let has_required_gap = false;
        let env_index = 0;
        while (env_index < sdk_envs.length()) {
            let marker = cells[env_index + 1];
            if (marker != "y" && marker != "-") {
                throw baml.errors.ParseError {
                    message: `invalid coverage cell for ${sdk_test_id} in baseline ${source}`,
                };
            }
            let present = marker == "y";
            let required = array_contains(required_envs, sdk_envs[env_index]);
            if (required && !present) {
                has_required_gap = true;
                required_gap_count += 1;
            }
            parity_cells
                .push(
                    ParityCell {
                        sdk_env: sdk_envs[env_index],
                        present: present,
                        required: required,
                    },
                );
            env_index += 1;
        }
        rows.push(
            ParityRow {
                sdk_test_id: sdk_test_id,
                cells: parity_cells,
                required_envs: required_envs,
                reason: cells[cells.length() - 1],
                has_required_gap: has_required_gap,
            },
        );
        index += 1;
    }
    if (rows.length() == 0) {
        throw baml.errors.ParseError {
            message: `baseline parity table has no test rows: ${source}`,
        };
    }
    ParityReport { sdk_envs: sdk_envs, rows: rows, required_gap_count: required_gap_count }
}

function parity_pair_key(sdk_test_id: string, sdk_env: string) -> string {
    `${sdk_test_id}\t${sdk_env}`
}

function missing_pair_map(report: ParityReport) -> map<string, ParityPair> {
    let pairs: map<string, ParityPair> = {};
    for (let row in report.rows) {
        for (let cell in row.cells) {
            if (cell.required && !cell.present) {
                let pair = ParityPair { sdk_test_id: row.sdk_test_id, sdk_env: cell.sdk_env };
                let _ = pairs.set(parity_pair_key(pair.sdk_test_id, pair.sdk_env), pair);
            }
        }
    }
    pairs
}

function present_declaration_count(report: ParityReport) -> int {
    let count = 0;
    for (let row in report.rows) {
        for (let cell in row.cells) {
            if (cell.present) {
                count += 1;
            }
        }
    }
    count
}

function sorted_pairs(pairs: ParityPair[]) -> ParityPair[] {
    pairs
        .sort_by_key(
            (pair) -> {
                parity_pair_key(pair.sdk_test_id, pair.sdk_env)
            },
        );
    pairs
}

function compare_to_baseline(baseline: ParityReport, current: ParityReport) -> ParityComparison {
    let baseline_missing = missing_pair_map(baseline);
    let current_missing = missing_pair_map(current);
    let regressions: ParityPair[] = [];
    for (let key in current_missing.keys()) {
        if (!baseline_missing.has(key)) {
            if let pair: ParityPair = current_missing.get(key) {
                regressions.push(pair);
            }
        }
    }
    let resolved: ParityPair[] = [];
    for (let key in baseline_missing.keys()) {
        if (!current_missing.has(key)) {
            if let pair: ParityPair = baseline_missing.get(key) {
                resolved.push(pair);
            }
        }
    }
    let current_by_id: map<string, ParityRow> = {};
    for (let row in current.rows) {
        let _ = current_by_id.set(row.sdk_test_id, row);
    }
    let weakened: ParityPair[] = [];
    for (let baseline_row in baseline.rows) {
        if (baseline_row.reason.length() > 0) {
            if let current_row: ParityRow = current_by_id.get(baseline_row.sdk_test_id) {
                for (let sdk_env in baseline_row.required_envs) {
                    if (!array_contains(current_row.required_envs, sdk_env)) {
                        weakened
                            .push(
                                ParityPair {
                                    sdk_test_id: baseline_row.sdk_test_id,
                                    sdk_env: sdk_env,
                                },
                            );
                    }
                }
            }
        }
    }
    regressions = sorted_pairs(regressions);
    resolved = sorted_pairs(resolved);
    weakened = sorted_pairs(weakened);
    let baseline_present_count = present_declaration_count(baseline);
    let current_present_count = present_declaration_count(current);
    let outcome = if (
        current.required_gap_count > baseline.required_gap_count
            || current_present_count < baseline_present_count
            || weakened.length() > 0
    ) {
        "worse"
    } else if (
        current.required_gap_count < baseline.required_gap_count
            || current_present_count > baseline_present_count
    ) {
        "better"
    } else {
        "unchanged"
    };
    ParityComparison {
        regressions: regressions,
        resolved: resolved,
        weakened_requirements: weakened,
        baseline_missing_count: baseline.required_gap_count,
        current_missing_count: current.required_gap_count,
        baseline_present_count: baseline_present_count,
        current_present_count: current_present_count,
        outcome: outcome,
    }
}
