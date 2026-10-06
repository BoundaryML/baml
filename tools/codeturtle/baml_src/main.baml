function main(repo: string, changes_path: string, concurrency: int = 8) -> Report {
    run(repo, changes_path, concurrency) catch_all (error) {
        _ => {
            Report {
                context: null,
                requirements: [],
                evaluations: [],
                unowned_files: [],
                evaluation_complete: false,
                errors: [EvaluationError { rule_line: null, message: error.to_string() }],
            }
        }
    }
}
