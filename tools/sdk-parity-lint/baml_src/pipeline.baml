function validate_repo_root(repo_root: string) -> string {
    let normalized = trim_trailing_slashes(repo_root);
    if (!baml.fs.exists(`${normalized}/baml_language/sdk_tests/crates`)) {
        throw baml.errors.InvalidArgument {
            message: `${normalized} is not a BAML repository with SDK tests`,
        };
    }
    normalized
}

function build_inventory_for(
    sdk_env: string,
    configs: SourceConfig[],
    tests: DiscoveredTest[],
) -> SdkInventory {
    if (!array_contains(sdk_environments(), sdk_env)) {
        throw baml.errors.InvalidArgument {
            message: `unknown SDK environment ${sdk_env}`,
        };
    }
    let source_directory = sdk_source_directory(sdk_env);
    let config = source_config_for(configs, source_directory);
    let inventory_tests: InventoryTest[] = [];
    let ids: map<string, string> = {};
    for (let test_case in tests) {
        let applies = test_case.source_directory == source_directory;
        if let runtime: string = sdk_runtime(sdk_env) {
            applies = applies && array_contains(test_case.runtimes, runtime);
        }
        if (applies) {
            if let previous: string = ids.get(test_case.sdk_test_id) {
                throw baml.errors.InvalidArgument {
                    message: `duplicate ${test_case.sdk_test_id} in ${sdk_env}: ${previous} and ${test_case.source_file}:${test_case.source_line}`,
                };
            }
            let _ = ids.set(test_case.sdk_test_id, `${test_case.source_file}:${test_case.source_line}`);
            inventory_tests.push(inventory_test(test_case));
        }
    }
    inventory_tests
        .sort_by_key(
            (test_case) -> {
                test_case.sdk_test_id
            },
        );
    SdkInventory { sdk_env: sdk_env, source_root: config.source_root, tests: inventory_tests }
}

function render_inventory_output(sdk_env: string, repo_root: string = ".") -> string {
    let normalized_repo_root = validate_repo_root(repo_root);
    let ast_grep_version = verify_ast_grep(normalized_repo_root);
    let configs = source_configs(normalized_repo_root);
    let source_directory = sdk_source_directory(sdk_env);
    let scan = scan_source(normalized_repo_root, source_config_for(configs, source_directory));
    let tests = discover_tests([scan]);
    apply_typescript_runtime_gates(tests, [scan]);
    render_inventory(build_inventory_for(sdk_env, configs, tests), ast_grep_version)
}

function render_parity_output(repo_root: string = ".") -> RenderedParity {
    let normalized_repo_root = validate_repo_root(repo_root);
    let ast_grep_version = verify_ast_grep(normalized_repo_root);
    let sdk_envs = sdk_environments();
    let configs = source_configs(normalized_repo_root);
    let scans = scan_all_sources(normalized_repo_root, configs);
    let tests = discover_tests(scans);
    apply_typescript_runtime_gates(tests, scans);
    let annotations = build_annotation_configs(normalized_repo_root, scans, tests);
    let inventories = build_inventories(sdk_envs, configs, tests);
    let report = build_parity_report_with_tests(sdk_envs, inventories, annotations, tests);
    RenderedParity {
        ast_grep_version: ast_grep_version,
        markdown: render_parity_report(report, ast_grep_version),
        distinct_test_ids: report.rows.length(),
        required_gap_count: report.required_gap_count,
    }
}

function resolve_repo_file(repo_root: string, path: string) -> string {
    if (path.starts_with("/")) {
        path
    } else {
        `${repo_root}/${trim_surrounding_slashes(path)}`
    }
}

function parent_directory(path: string) -> string {
    let last_separator = -1;
    let index = 0;
    while (index < path.length()) {
        if (path.char_at(index) == "/") {
            last_separator = index;
        }
        index += 1;
    }
    if (last_separator <= 0) {
        "."
    } else {
        path.substring(0, last_separator)
    }
}

function run_ratchet(
    repo_root: string = ".",
    baseline_file: string = "",
    output_file: string = "",
) -> RatchetResult {
    let normalized_repo_root = validate_repo_root(repo_root);
    let resolved_baseline = if (baseline_file.length() == 0) {
        `${normalized_repo_root}/baml_language/sdk_tests/crates/parity_analysis.md`
    } else {
        resolve_repo_file(normalized_repo_root, baseline_file)
    };
    let resolved_output = if (output_file.length() == 0) {
        `${parent_directory(resolved_baseline)}/parity_analysis.new.md`
    } else {
        resolve_repo_file(normalized_repo_root, output_file)
    };
    let sdk_envs = sdk_environments();
    let baseline = parse_baseline(baml.fs.read(resolved_baseline), resolved_baseline, sdk_envs);
    let _ = verify_ast_grep(normalized_repo_root);
    let configs = source_configs(normalized_repo_root);
    let scans = scan_all_sources(normalized_repo_root, configs);
    let tests = discover_tests(scans);
    apply_typescript_runtime_gates(tests, scans);
    let annotations = build_annotation_configs(normalized_repo_root, scans, tests);
    let inventories = build_inventories(sdk_envs, configs, tests);
    let report = build_parity_report_with_tests(sdk_envs, inventories, annotations, tests);
    let comparison = compare_to_baseline(baseline, report);
    RatchetResult {
        markdown: render_ratchet_report(report, comparison),
        baseline_file: resolved_baseline,
        output_file: resolved_output,
        total_ids: report.rows.length(),
        comparison: comparison,
    }
}

function render_parity_json_output(repo_root: string = ".") -> string {
    baml.json.serialize(render_parity_output(repo_root = repo_root))
}

function main(repo_root: string = ".") -> RenderedParity {
    render_parity_output(repo_root = repo_root)
}
