/// The most function rows the table shows.
const MAX_ROWS = 50;

/// The largest radius the radar draws, as a percentage of the base. Larger
/// values are drawn at the edge; the axis label still gives the real figure.
const RADAR_CAP = 300n;

/// The smallest top the radar's scale gets, so the base's 100% ring always
/// has room outside it.
const RADAR_MIN_TOP = 150n;

/// Where the comment points and what it calls things.
class Context {
    server_url: string,
    repo: string,
    head_sha: string,
    base_label: string,
    /// Project directory relative to the repository root: empty, or ending in `/`.
    project_prefix: string,
    /// The `project` input, which keys the comment marker.
    project: string,

    /// Takes `server_url` with or without trailing slashes, and `project_prefix`
    /// as `git rev-parse --show-prefix` prints it, which may be `.` or use
    /// backslashes on Windows.
    function new(
        server_url: string,
        repo: string,
        head_sha: string,
        base_label: string,
        project_prefix: string,
        project: string,
    ) -> Self {
        Context {
            // Trailing slashes, as in "https://github.com/".
            server_url: server_url.trim_end().replace(baml.regex.new("/+$"), ""),
            repo: repo,
            head_sha: head_sha,
            base_label: base_label,
            project_prefix: Context.normalize_prefix(project_prefix),
            project: project,
        }
    }

    function normalize_prefix(prefix: string) -> string {
        let trimmed = prefix.replace_all("\\", "/")
            .split("/")
            .filter((part) -> {
                part && part != "."
            })
            .join("/");
        if (trimmed) {
            `${trimmed}/`
        } else {
            ""
        }
    }

    function short_sha(self) -> string {
        self.head_sha.slice(0, 7)
    }

    function link(self, report: FunctionReport) -> string {
        let path = `${self.project_prefix}${report.file}`;
        let url = `${self.server_url}/${self.repo}/blob/${self.head_sha}/${path}#L${report.start_line}-L${report.end_line}`;
        `[\`${report.name}\`](${url})`
    }

    function marker(self) -> string {
        `<!-- baml-action:codequality:${self.project} -->`
    }

    function footer(self) -> string {
        `<sub>Compared \`${self.short_sha()}\` with \`${self.base_label}\` using \`baml codequality\`. This comment updates on every push.</sub>`
    }
}

/// What a metric rewards, so that a bigger value on the radar reads as better.
function health_name(metric: Metric) -> string {
    match (metric) {
        Metric.Cognitive => "Readable",
        Metric.Cyclomatic => "Few branches",
        Metric.NPath => "Few paths",
        Metric.Nesting => "Flat",
        Metric.Exits => "Few exits",
    }
}

/// One spoke of the radar: the base's average for a metric divided by the
/// head's, as a percentage, so above 100 means the head is simpler. Null when
/// the head's average is zero and the base's is not, which no percentage
/// expresses.
class RadarAxis {
    metric: Metric,
    health: bigint?,

    function of(c: Comparison, metric: Metric) -> Self {
        let zero = Ratio.new(0n, 1n);
        let before = c.before.average(metric) ?? zero;
        let after = c.after.average(metric) ?? zero;
        let health = match (after.numerator) {
            0n => {
                if (before.numerator) {
                    null
                } else {
                    100n
                }
            },
            _ => {
                Ratio
                    .new(before.numerator * after.denominator, before.denominator * after.numerator)
                    .percent()
            },
        };
        RadarAxis { metric: metric, health: health }
    }

    function drawn(self) -> bigint {
        (self.health ?? RADAR_CAP).min(RADAR_CAP)
    }

    function label(self) -> string {
        match (self.health) {
            null => `${health_name(self.metric)} >${RADAR_CAP}%`,
            let h: bigint => `${health_name(self.metric)} ${count(h)}%`,
        }
    }
}

/// A Mermaid radar of code health per function, with the base at 100% on
/// every axis so metrics of very different sizes share one chart. Null when
/// the base has no functions or no average moved.
function radar(c: Comparison, ctx: Context) -> string? {
    if (!c.before.functions) {
        return null;
    }
    let axes = ALL_METRICS.map((m) -> {
        RadarAxis.of(c, m)
    });
    if (
        axes.every((a) -> {
            a.health == 100n
        })
    ) {
        return null;
    }
    let largest = axes.reduce(
        (top, a) -> {
            top.max(a.drawn())
        },
        RADAR_MIN_TOP,
    );
    // Round the scale up to a multiple of 50.
    let top = (largest + 49n) / 50n * 50n;
    let spokes = axes
        .map((a) -> {
            `${a.metric.to_string().to_lower_case()}["${a.label()}"]`
        })
        .join(", ");
    let base_curve = ALL_METRICS
        .map((m) -> {
            "100"
        })
        .join(", ");
    let head_curve = axes
        .map((a) -> {
            a.drawn().to_string()
        })
        .join(", ");
    let base_label = ctx.base_label.replace_all("\"", "'");
    // Mermaid's default radar is 600px square, taller than the text around it.
    // Brand colors (BoundaryML/brand design.md): Pasture 300 for the base,
    // Clover 200 for the head, both legible on light and dark. Mid-grey
    // hairlines keep the grid faint in either theme.
    ````
        Code health per function vs \`${ctx.base_label}\` (bigger is better, \`${ctx.base_label}\` = 100%):

        ```mermaid
        ---
        config:
          radar:
            width: 280
            height: 280
            marginTop: 30
            marginBottom: 30
            marginLeft: 70
            marginRight: 70
            graticule: polygon
            curveTension: 0
          themeVariables:
            cScale0: "#82B571"
            cScale1: "#CAAAF2"
            radar:
              graticuleOpacity: 0
              graticuleColor: "#828280"
              axisColor: "#828280"
              graticuleStrokeWidth: 0.5
              axisStrokeWidth: 0.5
              curveOpacity: 0.3
        ---
        radar-beta
          axis ${spokes}
          curve before["${base_label}"]{${base_curve}}
          curve after["${ctx.short_sha()}"]{${head_curve}}
          max ${top}
          min 0
          ticks 3
        ```
    ````
}

/// One row per function whose change is significant, worse first. Null when
/// none is.
function function_table(c: Comparison, ctx: Context) -> string? {
    let rows = c.worse.concat(c.better)
        .filter((change) -> {
            change.significant()
        })
        .map((change) -> {
            `| ${ctx.link(change.report)} | ${change} |`
        });
    if (!rows) {
        return null;
    }
    let table = ["| Function | What changed |", "| --- | --- |"].concat(rows.slice(0, MAX_ROWS)).join("\n");
    if (rows.length() > MAX_ROWS) {
        return `${table}\n\n...and ${rows.length() - MAX_ROWS} more.`;
    }
    table
}

/// The comment body: blocks separated by blank lines.
function render(outcome: Outcome, ctx: Context) -> string {
    let blocks: (string | null)[] = match (outcome) {
        Unmeasured => {
            [
                `${ctx.marker()}\nCould not measure \`${ctx.base_label}\`, so there is nothing to compare against.`,
            ]
        },
        let c: Comparison => {
            [
                `${ctx.marker()}\n${c.headline()}`,
                radar(c, ctx),
                function_table(c, ctx),
                EXPLANATIONS,
            ]
        },
    };
    let body = blocks.concat([ctx.footer()])
        .filter_map((block) -> {
            block
        })
        .join("\n\n");
    `${body}\n`
}
