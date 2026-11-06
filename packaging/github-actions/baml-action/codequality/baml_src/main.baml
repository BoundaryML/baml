/// The comment body, and whether it is worth posting a new comment for.
class Rendered {
    body: string,
    changed: bool,
}

/// Unparseable base text means the base could not be measured, not that the
/// action failed; unparseable head text is an error.
function render_reports(base_text: string?, head_text: string, ctx: Context) -> Rendered {
    let head = Report.parse(head_text);
    let outcome: Outcome = match (base_text) {
        null => Unmeasured {},
        let text: string => {
            Comparison.new(Report.parse(text), head) catch (e) {
                baml.json.ParseError => Unmeasured {},
                baml.json.DecodeError => Unmeasured {},
            }
        },
    };
    Rendered {
        body: render(outcome, ctx),
        changed: match (outcome) {
            Unmeasured => true,
            let c: Comparison => c.changed(),
        },
    }
}

function read_if_exists(path: string?) -> string? {
    if (path && baml.fs.exists(path)) {
        baml.fs.read(path)
    } else {
        null
    }
}

/// Writes the comment body comparing two `baml codequality --output-format json`
/// reports to `out`, and prints `changed` or `unchanged` so the caller can skip
/// posting a comment that would only say nothing happened. A missing or
/// unreadable `base` renders as "could not measure".
function main(
    head: string,
    out: string,
    server_url: string,
    repo: string,
    head_sha: string,
    base_label: string,
    base: string? = null,
    project_prefix: string = "",
    project: string = ".",
) -> void {
    let ctx = Context.new(server_url, repo, head_sha, base_label, project_prefix, project);
    let rendered = render_reports(read_if_exists(base), baml.fs.read(head), ctx);
    baml.fs.write(out, rendered.body);
    baml.io.println(
        if (rendered.changed) {
            "changed"
        } else {
            "unchanged"
        },
    );
}
