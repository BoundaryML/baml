function inventory_has(inventory: SdkInventory, sdk_test_id: string) -> bool {
    inventory
        .tests
        .some(
            (test_case) -> {
                test_case.sdk_test_id == sdk_test_id
            },
        )
}

function inventory_test(test_case: DiscoveredTest) -> InventoryTest {
    InventoryTest {
        sdk_test_id: test_case.sdk_test_id,
        raw_name: test_case.raw_name,
        source_file: test_case.source_file,
        source_line: test_case.source_line,
        source_column: test_case.source_column,
        rule_id: test_case.rule_id,
    }
}

function build_inventories(
    sdk_envs: string[],
    configs: SourceConfig[],
    tests: DiscoveredTest[],
) -> SdkInventory[] {
    let inventories: SdkInventory[] = [];
    let represented: map<string, bool> = {};
    for (let sdk_env in sdk_envs) {
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
                let _ = represented
                    .set(
                        `${test_case.source_file}:${test_case.match_start}:${test_case.sdk_test_id}`,
                        true,
                    );
            }
        }
        inventory_tests
            .sort_by_key(
                (test_case) -> {
                    test_case.sdk_test_id
                },
            );
        inventories
            .push(
                SdkInventory {
                    sdk_env: sdk_env,
                    source_root: config.source_root,
                    tests: inventory_tests,
                },
            );
    }
    for (let test_case in tests) {
        let key = `${test_case.source_file}:${test_case.match_start}:${test_case.sdk_test_id}`;
        if (!represented.has(key)) {
            throw baml.errors.InvalidArgument {
                message: `discovered test was not assigned to any SDK environment: ${test_case.sdk_test_id} at ${test_case.source_file}:${test_case.source_line}`,
            };
        }
    }
    inventories
}

function annotation_for(annotations: AnnotationConfig[], sdk_test_id: string) -> AnnotationConfig? {
    for (let annotation in annotations) {
        if (annotation.sdk_test_id == sdk_test_id) {
            return annotation;
        }
    }
    null
}

function validate_inventory_order(sdk_envs: string[], inventories: SdkInventory[]) -> void {
    if (inventories.length() != sdk_envs.length()) {
        throw baml.errors.InvalidArgument {
            message: `inventory count ${inventories.length()} does not match environment count ${sdk_envs.length()}`,
        };
    }
    let index = 0;
    while (index < sdk_envs.length()) {
        if (inventories[index].sdk_env != sdk_envs[index]) {
            throw baml.errors.InvalidArgument {
                message: `inventory order mismatch at ${index}: ${inventories[index].sdk_env} != ${sdk_envs[index]}`,
            };
        }
        index += 1;
    }
}

function build_parity_report(
    sdk_envs: string[],
    inventories: SdkInventory[],
    annotations: AnnotationConfig[],
) -> ParityReport {
    build_parity_report_with_tests(sdk_envs, inventories, annotations, [])
}

function typescript_required_environments(
    sdk_test_id: string,
    sdk_envs: string[],
    tests: DiscoveredTest[],
) -> string[] {
    let required: map<string, bool> = {};
    for (let sdk_env in sdk_envs) {
        if (!sdk_env.starts_with("typescript_")) {
            let _ = required.set(sdk_env, true);
        }
    }
    let found_typescript = false;
    for (let test_case in tests) {
        if (test_case.source_directory == "typescript" && test_case.sdk_test_id == sdk_test_id) {
            found_typescript = true;
            for (let runtime in test_case.runtimes) {
                if (runtime == "node") {
                    let _ = required.set("typescript_node", true);
                } else if (runtime == "web") {
                    let _ = required.set("typescript_web_chromium", true);
                } else if (runtime == "workers") {
                    let _ = required.set("typescript_web_cloudflare_workers", true);
                }
            }
        }
    }
    if (!found_typescript) {
        let _ = required.set("typescript_node", true);
        let _ = required.set("typescript_web_chromium", true);
        let _ = required.set("typescript_web_cloudflare_workers", true);
    }
    let ordered: string[] = [];
    for (let sdk_env in sdk_envs) {
        if (required.has(sdk_env)) {
            ordered.push(sdk_env);
        }
    }
    ordered
}

function build_parity_report_with_tests(
    sdk_envs: string[],
    inventories: SdkInventory[],
    annotations: AnnotationConfig[],
    tests: DiscoveredTest[],
) -> ParityReport {
    validate_inventory_order(sdk_envs, inventories);
    let all_ids: map<string, bool> = {};
    for (let inventory in inventories) {
        for (let test_case in inventory.tests) {
            let _ = all_ids.set(test_case.sdk_test_id, true);
        }
    }
    let sorted_ids = all_ids
        .keys()
        .sort_by_key(
            (sdk_test_id) -> {
                sdk_test_id
            },
        );
    let rows: ParityRow[] = [];
    let required_gap_count = 0;
    for (let sdk_test_id in sorted_ids) {
        let annotation = annotation_for(annotations, sdk_test_id);
        let required_envs = if let configured: AnnotationConfig = annotation {
            let present_envs: string[] = [];
            let index = 0;
            while (index < sdk_envs.length()) {
                if (inventory_has(inventories[index], sdk_test_id)) {
                    present_envs.push(sdk_envs[index]);
                }
                index += 1;
            }
            present_envs
        } else {
            typescript_required_environments(sdk_test_id, sdk_envs, tests)
        };
        let reason = if let configured: AnnotationConfig = annotation {
            configured.reason
        } else {
            ""
        };
        let cells: ParityCell[] = [];
        let has_required_gap = false;
        let index = 0;
        while (index < sdk_envs.length()) {
            let present = inventory_has(inventories[index], sdk_test_id);
            let required = array_contains(required_envs, sdk_envs[index]);
            if (required && !present) {
                has_required_gap = true;
                required_gap_count += 1;
            }
            cells
                .push(ParityCell { sdk_env: sdk_envs[index], present: present, required: required });
            index += 1;
        }
        rows.push(
            ParityRow {
                sdk_test_id: sdk_test_id,
                cells: cells,
                required_envs: required_envs,
                reason: reason,
                has_required_gap: has_required_gap,
            },
        );
    }
    ParityReport { sdk_envs: sdk_envs, rows: rows, required_gap_count: required_gap_count }
}
