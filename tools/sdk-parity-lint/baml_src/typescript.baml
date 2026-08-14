function compact_condition(condition: string) -> string {
    condition.replace_all(" ", "").replace_all("\t", "").replace_all("\r", "").replace_all("\n", "")
}

function runtimes_for_condition(
    condition: string,
    source_file: string,
    source_line: int,
) -> string[] {
    let compact = compact_condition(condition);
    if (compact == "isTestRuntime(\"node\")" || compact == "isTestRuntime('node')") {
        ["node"]
    } else if (compact == "isTestRuntime(\"web\")" || compact == "isTestRuntime('web')") {
        ["web"]
    } else if (compact == "isTestRuntime(\"workers\")" || compact == "isTestRuntime('workers')") {
        ["workers"]
    } else if (compact == "isWebRuntime") {
        ["web", "workers"]
    } else {
        throw baml.errors.ParseError {
            message: `unknown TypeScript runtime condition at ${source_file}:${source_line}: ${condition}`,
        };
    }
}

function intersect_runtimes(left: string[], right: string[]) -> string[] {
    let result: string[] = [];
    for (let runtime in ["node", "web", "workers"]) {
        if (array_contains(left, runtime) && array_contains(right, runtime)) {
            result.push(runtime);
        }
    }
    result
}

function apply_typescript_runtime_gates(tests: DiscoveredTest[], scans: SourceScan[]) -> void {
    let suite_gates: AstMatch[] = [];
    for (let scan in scans) {
        if (scan.config.language == "typescript") {
            for (let ast_match in scan.matches) {
                if (ast_match.ruleId == "typescript-suite-runtime-gate") {
                    let condition = ast_single(ast_match, "COND").text;
                    let _ = runtimes_for_condition(condition, ast_match.file, ast_match.range.start.line + 1);
                    suite_gates.push(ast_match);
                }
            }
        }
    }
    for (let test_case in tests) {
        if (test_case.language == "typescript") {
            let runtimes = ["node", "web", "workers"];
            for (let gate in suite_gates) {
                if (
                    gate.file == test_case.source_file
                        && gate.range.byteOffset.start <= test_case.match_start
                        && gate.range.byteOffset.end >= test_case.match_end
                ) {
                    runtimes
                        = intersect_runtimes(
                            runtimes,
                            runtimes_for_condition(ast_single(gate, "COND").text, gate.file, gate.range.start.line + 1),
                        );
                }
            }
            if let direct_condition: string = test_case.runtime_condition {
                runtimes
                    = intersect_runtimes(
                        runtimes,
                        runtimes_for_condition(direct_condition, test_case.source_file, test_case.source_line),
                    );
            }
            if (runtimes.length() == 0) {
                throw baml.errors.InvalidArgument {
                    message: `TypeScript runtime gates have an empty intersection for ${test_case.sdk_test_id} at ${test_case.source_file}:${test_case.source_line}`,
                };
            }
            test_case.runtimes = runtimes;
        }
    }
}
