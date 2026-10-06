function normalize_matches(
    result: Match | Match[] | bool,
    changes: FileChange[],
    reference: string,
) -> Match[] throws never {
    match (result) {
        let applies: bool => {
            let results: Match[] = [];
            for (let change in changes) {
                results.push(
                    Match {
                        path: change.path,
                        verdict: if (applies) {
                            Verdict.Applies
                        } else {
                            Verdict.DoesNotApply
                        },
                        reason: "Condition " + reference + " returned " + applies.to_string(),
                        evidence: [],
                    },
                );
            }
            results
        },
        let detailed: Match => [detailed],
        let detailed: Match[] => detailed,
    }
}

function evaluate(
    rule: Rule,
    changes: FileChange[],
    pr: PullRequest,
    package: reflect.Package,
) -> Match[] {
    if (rule.condition == null) {
        let results: Match[] = [];
        for (let change in changes) {
            results.push(
                Match {
                    path: change.path,
                    verdict: Verdict.Applies,
                    reason: "Matches ownership path " + rule.pattern,
                    evidence: [],
                },
            );
        }
        return results;
    }
    let name = "root." + (rule.condition ?? throw "Missing condition");
    let signature = package.functions().get(name) ?? throw "Unknown condition: " + name;
    let parameter = condition_input(signature);
    if (parameter.type == reflect.Type.of<FileChange>()) {
        let condition = package.get_function<FileCondition>(name) ?? throw "Missing file condition";
        let results: Match[] = [];
        for (let change in changes) {
            // Extensions get their own data, so mutation cannot affect another job.
            let input = baml.json.from_string<FileChange>(baml.json.to_string(change));
            let normalized = normalize_matches(condition(input), [change], rule.condition ?? "");
            for (let result in normalized) {
                if (result.path != change.path) {
                    throw "File condition returned a different file";
                }
                results.push(result);
            }
        }
        results
    } else if (parameter.type == reflect.Type.of<FileChange[]>()) {
        let condition = package.get_function<FilesCondition>(name) ?? throw "Missing multi-file condition";
        normalize_matches(
            condition(baml.json.from_string<FileChange[]>(baml.json.to_string(changes))),
            changes,
            rule.condition ?? "",
        )
    } else if (parameter.type == reflect.Type.of<PullRequest>()) {
        let condition = package.get_function<PrCondition>(name) ?? throw "Missing PR condition";
        normalize_matches(
            condition(baml.json.from_string<PullRequest>(baml.json.to_string(pr))),
            changes,
            rule.condition ?? "",
        )
    } else {
        throw "Unsupported condition input type";
    }
}

// Every condition invocation shares this native limit. There are no nested
// runner limits that multiply the cap; tasks never mutate the final report.
function start_job(
    job: Job,
    pr: PullRequest,
    package: reflect.Package,
    limit: baml.spawn.Limit,
) -> baml.future.Future<JobResult, never> {
    spawn with limit {
        let outcome = evaluate(job.rule, job.changes, pr, package) catch_all (error) {
            _ => {
                return JobResult { matches: [], error: error.to_string() };
            }
        };
        JobResult { matches: outcome, error: null }
    }
}

function evaluate_policy(
    pr: PullRequest,
    owners: string,
    sources: map<string, string>,
    concurrency: int,
) -> Report {
    if (concurrency < 1) {
        throw "Concurrency must be a positive integer";
    }
    let report = Report {
        context: null,
        requirements: [],
        evaluations: [],
        errors: [],
        unowned_files: [],
        evaluation_complete: false,
    };
    report.context = pr.context;
    let rules = parse_rules(owners);
    let package = reflect.Package.compile(sources, packages = { "codeturtle": reflect.Package.current() });
    let owned: string[] = [];
    let jobs: Job[] = [];
    let batches: RuleBatch[] = [];
    for (let rule in rules) {
        let matching: FileChange[] = [];
        for (let change in pr.changes) {
            if (
                matches_path(rule.pattern, change.path)
                    || (
                        change.old_path != null
                            && matches_path(rule.pattern, change.old_path ?? "")
                    )
            ) {
                matching.push(change);
            }
        }
        if (matching.length() == 0) {
            continue;
        }
        let allowed: string[] = [];
        for (let change in matching) {
            allowed.push(change.path);
        }
        let filewise = false;
        if (rule.condition != null) {
            let signature = package.functions().get("root." + (rule.condition ?? ""));
            if (signature != null) {
                let parameter = condition_input(signature) catch_all (_) {
                    _ => null
                };
                if (parameter != null) {
                    filewise = parameter.type == reflect.Type.of<FileChange>();
                }
            }
        }
        let indices: int[] = [];
        if (filewise) {
            for (let change in matching) {
                indices.push(jobs.length());
                jobs.push(Job { rule: rule, changes: [change] });
            }
        } else {
            indices.push(jobs.length());
            jobs.push(Job { rule: rule, changes: matching });
        }
        batches.push(RuleBatch { rule: rule, allowed: allowed, indices: indices });
    }
    let limit = baml.spawn.Limit.new(concurrency);
    let futures: baml.future.Future<JobResult, never>[] = [];
    for (let job in jobs) {
        futures.push(start_job(job, pr, package, limit));
    }
    let completed: JobResult[] = [];
    for (let future in futures) {
        completed.push(await future);
    }
    // Gather in ownership-rule and file order, regardless of completion order.
    for (let batch in batches) {
        let rule = batch.rule;
        let results: Match[] = [];
        let error: string? = null;
        for (let index in batch.indices) {
            let result = completed[index];
            if (result.error != null) {
                if (error == null) {
                    error = result.error;
                }
            } else {
                for (let match_result in result.matches) {
                    results.push(match_result);
                }
            }
        }
        if (error != null) {
            report.errors.push(
                EvaluationError {
                    rule_line: rule.line,
                    message: "Condition evaluation failed: "
                        + (rule.condition ?? "unconditional")
                        + ": "
                        + (error ?? ""),
                },
            );
            continue;
        }
        let valid = true;
        for (let result in results) {
            if (!batch.allowed.includes(result.path)) {
                valid = false;
            }
        }
        if (!valid) {
            report.errors.push(
                EvaluationError {
                    rule_line: rule.line,
                    message: "Condition returned a file outside the ownership rule",
                },
            );
            continue;
        }
        for (let result in results) {
            let evaluation = Evaluation { rule_line: rule.line, condition: rule.condition, result: result };
            report.evaluations.push(evaluation);
            if (result.verdict != Verdict.DoesNotApply) {
                owned.push(result.path);
                report.requirements.push(
                    Requirement {
                        path: result.path,
                        owners: rule.owners,
                        approval: rule.approval,
                        evaluation: evaluation,
                    },
                );
            }
        }
    }
    for (let change in pr.changes) {
        if (!owned.includes(change.path)) {
            report.unowned_files.push(change.path);
        }
    }
    // This describes evaluation coverage, never human approval.
    report.evaluation_complete = report.errors.length() == 0 && report.unowned_files.length() == 0;
    report
}

function check_sources(
    pr: PullRequest,
    owners: string,
    sources: map<string, string>,
    concurrency: int = 8,
) -> Report {
    evaluate_policy(pr, owners, sources, concurrency) catch_all (error) {
        _ => {
            Report {
                context: pr.context,
                requirements: [],
                evaluations: [],
                unowned_files: [],
                evaluation_complete: false,
                errors: [EvaluationError { rule_line: null, message: error.to_string() }],
            }
        }
    }
}
