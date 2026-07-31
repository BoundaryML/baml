// The independent memory curator: reads a completed turn and decides which
// durable facts to save or delete. Runs in its own single-step Task after the
// coding turn finishes.

class Edit {
    op: "save" | "delete",
    name: string,
    keywords: string[],
    body: string,
}

function Curator(turn: string, index: string) -> Edit[] {
    provider: openai_provider(max_output_tokens = 1024)
    prompt: `
        You maintain the long-term memory of a coding agent. You are not the coding agent.
        Read only the completed turn below and decide what, if anything, is durable enough
        to affect future sessions.

        Save only confirmed preferences, project conventions and invariants, commands known
        to work, important locations, and decisions with their reasons.

        Never save transient task state, progress, code that can be reread, secrets, personal
        data, or guesses. Use delete when an existing memory is obsolete. Correct a memory by
        saving under the same name. Empty output is normally correct. Return at most five edits.

        Existing memory index:
        ----------------------------------------------------------------------
        ${index}
        ----------------------------------------------------------------------

        Just-completed turn:
        ----------------------------------------------------------------------
        ${turn}
        ----------------------------------------------------------------------

        ${ctx.output_format}
    `
}

// One curation pass can partially succeed: a filesystem error on edit
// three must not silently discard edits one and two, nor the report.
class EditReport {
    applied: string[],
    rejected: string[],
}

function apply_edits(edits: Edit[], store: Store) -> EditReport {
    //# Apply only validated, bounded curator decisions — at most five per turn
    let applied: string[] = [];
    let rejected: string[] = [];
    for (let edit in edits.slice(0, edits.length().min(5))) {
        match (edit.op) {
            "save" => {
                if (edit.name.trim().length() > 0 && edit.body.trim().length() > 0) {
                    let saved = store.remember(edit.name, edit.keywords, edit.body) catch_all (error) {
                        _ => {
                            rejected.push(`+${_slugify(edit.name)}: ${error}`);
                            continue;
                        },
                    };
                    applied.push(`+${saved.name}`);
                } else {
                    rejected.push(`+${_slugify(edit.name)}: empty name or body`);
                };
            },
            "delete" => {
                let removed = store.forget(edit.name) catch_all (error) {
                    _ => {
                        rejected.push(`-${_slugify(edit.name)}: ${error}`);
                        continue;
                    },
                };
                if (removed) {
                    applied.push(`-${_slugify(edit.name)}`);
                };
            },
        };
    }
    EditReport { applied: applied, rejected: rejected }
}

// Never lets a curation failure break the interactive session: any error
// simply means no memory edits this turn.
function curate_turn(turn: string, store: Store, backend: Backend) -> EditReport {
    if (turn.trim().length() == 0) {
        return EditReport { applied: [], rejected: [] };
    };
    let index = store.index() catch_all (error) {
        _ => "(memory unavailable)",
    };
    let task = match (backend) {
        Backend.OpenAI => Curator@task(turn, index),
        Backend.Google => {
            //# Apply the Google override to the independent curator Task too
            Curator@task(turn, index).with_provider(_provider_for(backend, max_output_tokens = 1024))
        },
    };
    let outcome = task
        .run(
            runner = ai
                .run
                .Agent<Edit[]>
                .new(
                    max_steps = 1,
                    max_tool_calls_per_step = 0,
                ),
        ) catch_all (error) {
        _ => {
            return EditReport { applied: [], rejected: [`curator run failed: ${error}`] };
        },
    };
    let edits: Edit[] = match (outcome) {
        let done: ai.Done<Edit[]> => done.value,
        _ => [],
    };
    apply_edits(edits, store)
}
