function parse_annotation(ast_match: AstMatch, sdk_test_id: string) -> AnnotationConfig {
    let marker = "SDK_PARITY_LINT(skip):";
    if let marker_index: int = ast_match.text.index_of(marker) {
        let reason = baml
            .String
            .from_utf8(
                ast_match
                    .text
                    .substring(marker_index + marker.length(), ast_match.text.length())
                    .trim()
                    .to_utf8(),
            );
        if (reason.length() == 0) {
            throw baml.errors.ParseError {
                message: `SDK_PARITY_LINT(skip) at ${ast_match.file}:${ast_match.range.start.line + 1} requires a reason`,
            };
        }
        return AnnotationConfig {
            sdk_test_id: baml.String.from_utf8(sdk_test_id.to_utf8()),
            reason: reason,
            source_file: baml.String.from_utf8(ast_match.file.to_utf8()),
            source_line: ast_match.range.start.line + 1,
        };
    }
    throw baml.errors.ParseError {
        message: `annotation rule returned a comment without SDK_PARITY_LINT(skip) at ${ast_match.file}:${ast_match.range.start.line + 1}`,
    };
}

function line_is_annotation_layout(language: string, line: string) -> bool {
    let trimmed = line.trim();
    if (trimmed.length() == 0) {
        true
    } else if (language == "python" || language == "java") {
        trimmed.starts_with("@")
    } else if (language == "rust" || language == "csharp") {
        trimmed.starts_with("#[") && trimmed.ends_with("]")
    } else {
        false
    }
}

function validate_annotation_adjacency(
    repo_root: string,
    ast_match: AstMatch,
    test_case: DiscoveredTest,
) -> void {
    let source_lines = baml.fs.read(`${repo_root}/${ast_match.file}`).lines();
    let comment_line = source_lines[ast_match.range.start.line];
    let before_comment = comment_line.substring(0, ast_match.range.start.column);
    if (before_comment.trim().length() > 0) {
        throw baml.errors.ParseError {
            message: `SDK_PARITY_LINT(skip) must be a standalone comment at ${ast_match.file}:${ast_match.range.start.line + 1}`,
        };
    }
    let next_index = ast_match.range.end.line + 1;
    let declaration_index = test_case.match_start_line - 1;
    if (declaration_index < next_index || declaration_index - next_index > 20) {
        throw baml.errors.ParseError {
            message: `orphaned SDK_PARITY_LINT(skip) at ${ast_match.file}:${ast_match.range.start.line + 1}`,
        };
    }
    while (next_index < declaration_index) {
        if (!line_is_annotation_layout(test_case.language, source_lines[next_index])) {
            throw baml.errors.ParseError {
                message: `SDK_PARITY_LINT(skip) at ${ast_match.file}:${ast_match.range.start.line + 1} is not immediately before ${test_case.sdk_test_id}`,
            };
        }
        next_index += 1;
    }
}

function annotation_target(
    repo_root: string,
    ast_match: AstMatch,
    tests: DiscoveredTest[],
) -> DiscoveredTest {
    let best_index = -1;
    let index = 0;
    while (index < tests.length()) {
        let test_case = tests[index];
        if (
            test_case.source_file == ast_match.file
                && test_case.source_line > ast_match.range.end.line + 1
                && (
                    best_index < 0
                        || test_case.source_line < tests[best_index].source_line
                        || (
                            test_case.source_line == tests[best_index].source_line
                                && test_case.match_start < tests[best_index].match_start
                        )
                )
        ) {
            best_index = index;
        }
        index += 1;
    }
    if (best_index < 0) {
        throw baml.errors.ParseError {
            message: `orphaned SDK_PARITY_LINT(skip) at ${ast_match.file}:${ast_match.range.start.line + 1}`,
        };
    }
    let target = tests[best_index];
    validate_annotation_adjacency(repo_root, ast_match, target);
    target
}

function build_annotation_configs(
    repo_root: string,
    scans: SourceScan[],
    tests: DiscoveredTest[],
) -> AnnotationConfig[] {
    let annotations: AnnotationConfig[] = [];
    let annotation_index: map<string, int> = {};
    let source_targets: map<string, bool> = {};
    for (let scan in scans) {
        for (let ast_match in scan.matches) {
            if (ast_match.ruleId == "sdk-parity-lint-skip") {
                let target = annotation_target(repo_root, ast_match, tests);
                let source_key = `${target.source_file}:${target.match_start}`;
                if (source_targets.has(source_key)) {
                    throw baml.errors.ParseError {
                        message: `multiple SDK_PARITY_LINT(skip) comments target ${target.sdk_test_id} at ${target.source_file}:${target.source_line}`,
                    };
                }
                let _ = source_targets.set(source_key, true);
                let annotation = parse_annotation(ast_match, target.sdk_test_id);
                if let existing_index: int = annotation_index.get(annotation.sdk_test_id) {
                    let existing = annotations[existing_index];
                    if (existing.reason != annotation.reason) {
                        throw baml.errors.ParseError {
                            message: `conflicting SDK_PARITY_LINT(skip) annotations for ${annotation.sdk_test_id} at ${existing.source_file}:${existing.source_line} and ${annotation.source_file}:${annotation.source_line}`,
                        };
                    }
                } else {
                    let _ = annotation_index.set(annotation.sdk_test_id, annotations.length());
                    annotations.push(annotation);
                }
            }
        }
    }
    annotations
        .sort_by_key(
            (annotation) -> {
                annotation.sdk_test_id
            },
        );
    annotations
}
