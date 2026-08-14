class SourceConfig {
    language: string,
    source_directory: string,
    source_root: string,
    rule_file: string,
    file_glob: string,
    suffix: string,
    argv: string[],
    declaration_rule_ids: string[],
    raw_name_field: string?,
    prefix: string,
}

class AstPosition {
    line: int,
    column: int,
}

class AstByteOffset {
    start: int,
    end: int,
}

class AstRange {
    byteOffset: AstByteOffset,
    start: AstPosition,
    end: AstPosition,
}

class AstMetaValue {
    text: string,
    range: AstRange,
}

class AstMetaVariables {
    single: map<string, AstMetaValue>,
}

class AstMatch {
    text: string,
    file: string,
    range: AstRange,
    metaVariables: AstMetaVariables?,
    ruleId: string,
}

class SourceScan {
    config: SourceConfig,
    matches: AstMatch[],
}

class DiscoveredTest {
    language: string,
    source_directory: string,
    sdk_test_id: string,
    raw_name: string,
    source_file: string,
    source_line: int,
    source_column: int,
    match_start: int,
    match_end: int,
    match_start_line: int,
    rule_id: string,
    runtime_condition: string?,
    runtimes: string[],
}

class AnnotationConfig {
    sdk_test_id: string,
    reason: string,
    source_file: string,
    source_line: int,
}

class InventoryTest {
    sdk_test_id: string,
    raw_name: string,
    source_file: string,
    source_line: int,
    source_column: int,
    rule_id: string,
}

class SdkInventory {
    sdk_env: string,
    source_root: string,
    tests: InventoryTest[],
}

class ParityCell {
    sdk_env: string,
    present: bool,
    required: bool,
}

class ParityRow {
    sdk_test_id: string,
    cells: ParityCell[],
    required_envs: string[],
    reason: string,
    has_required_gap: bool,
}

class ParityReport {
    sdk_envs: string[],
    rows: ParityRow[],
    required_gap_count: int,
}

class RenderedParity {
    ast_grep_version: string,
    markdown: string,
    distinct_test_ids: int,
    required_gap_count: int,
}

class ParityPair {
    sdk_test_id: string,
    sdk_env: string,
}

class ParityComparison {
    regressions: ParityPair[],
    resolved: ParityPair[],
    weakened_requirements: ParityPair[],
    baseline_missing_count: int,
    current_missing_count: int,
    baseline_present_count: int,
    current_present_count: int,
    outcome: string,
}

class RatchetResult {
    markdown: string,
    baseline_file: string,
    output_file: string,
    total_ids: int,
    comparison: ParityComparison,
}
