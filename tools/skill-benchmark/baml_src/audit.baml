function truncate_text(value: string, limit: int) -> string {
    if (value.length() <= limit) {
        value
    } else {
        `${value.slice(0, limit)}\n[truncated]`
    }
}

function audit_succeeded(metadata: AuditMetadata) -> bool {
    metadata.status == AuditStatus.exited && metadata.success == true
}

function load_audit_metadata(audit_root: string) -> AuditMetadata[] {
    let records: AuditMetadata[] = [];
    if (!baml.fs.exists(audit_root)) {
        return records;
    }

    for (let entry in baml.fs.read_dir(audit_root)) {
        let metadata_path = `${audit_root}/${entry.name}/metadata.json`;
        if (entry.is_dir && baml.fs.exists(metadata_path)) {
            records.push(baml.json.from_string<AuditMetadata>(baml.fs.read(metadata_path)));
        }
    }
    records.sort_by_key((record) -> {
        record.started_at_unix_ms
    })
}

function later_success(
    metadata: AuditMetadata,
    all_metadata: AuditMetadata[],
    same_command: bool,
) -> bool {
    all_metadata.some((candidate) -> {
        candidate.started_at_unix_ms > metadata.started_at_unix_ms
            && candidate.cwd == metadata.cwd
            && audit_succeeded(candidate)
            && if (same_command) {
                candidate.argv == metadata.argv
            } else {
                candidate.subcommand == metadata.subcommand
            }
    })
}

function read_audit_output(path: string) -> string {
    if (baml.fs.exists(path)) {
        truncate_text(baml.fs.read(path), 6000)
    } else {
        ""
    }
}

function failed_invocations(metadata: AuditMetadata[]) -> FailedInvocation[] {
    metadata
        .filter((invocation) -> {
            !audit_succeeded(invocation)
        })
        .map((invocation) -> {
            FailedInvocation {
                invocation_id: invocation.invocation_id,
                command: invocation.argv,
                subcommand: invocation.subcommand,
                status: invocation.status,
                exit_code: invocation.exit_code,
                stdout: read_audit_output(invocation.stdout_path),
                stderr: read_audit_output(invocation.stderr_path),
                later_success_same_command: later_success(invocation, metadata, true),
                later_success_same_subcommand: later_success(invocation, metadata, false),
            }
        })
}

function describe_calls(attempt_id: string, metadata: AuditMetadata[]) -> DescribeCall[] {
    metadata
        .filter((invocation) -> {
            invocation.subcommand == "describe"
        })
        .map((invocation) -> {
            DescribeCall {
                command: invocation.argv,
                succeeded: audit_succeeded(invocation),
                evidence: EvidenceRef { attempt_id: attempt_id, invocation_id: invocation.invocation_id },
            }
        })
}

function count_subcommand(metadata: AuditMetadata[], subcommand: string) -> int {
    metadata
        .filter((invocation) -> {
            invocation.subcommand == subcommand
        })
        .length()
}

function count_failed_subcommand(metadata: AuditMetadata[], subcommand: string) -> int {
    metadata
        .filter((invocation) -> {
            invocation.subcommand == subcommand && !audit_succeeded(invocation)
        })
        .length()
}

function claude_events(path: string) -> json[] {
    let events: json[] = [];
    if (!baml.fs.exists(path)) {
        return events;
    }
    for (let line in baml.fs.read(path).lines()) {
        if (line.trim()) {
            let parsed = baml.json.parse(line) catch_all (e) {
                _ => null,
            };
            if (parsed != null) {
                events.push(parsed);
            }
        }
    }
    events
}

function claude_tool_calls(events: json[]) -> int {
    let count = 0;
    for (let event in events) {
        for (let block in baml.json.path_or<json[]>(event, ".message.content", [])) {
            if (baml.json.path_or<string>(block, ".type", "") == "tool_use") {
                count += 1;
            }
        }
    }
    count
}

function claude_turns(events: json[]) -> int? {
    let turns: int? = null;
    for (let event in events) {
        if (baml.json.path_or<string>(event, ".type", "") == "result") {
            turns = baml.json.path_or<int?>(event, ".num_turns", null);
        }
    }
    turns
}

function skill_was_loaded(events: json[]) -> bool {
    events.some((event) -> {
        baml.json.path_or<json[]>(event, ".message.content", []).some((block) -> {
            baml.json.path_or<string>(block, ".type", "") == "tool_use"
                && baml.json.path_or<string>(block, ".name", "") == "Skill"
                && baml.json.path_or<string>(block, ".input.skill", "") == "baml-core"
        })
    })
}
