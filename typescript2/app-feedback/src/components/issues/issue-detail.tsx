import Link from "next/link";
import { ArrowLeft, Check, CircleDashed, ExternalLink, Loader2, X } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import type { Issue, Repro } from "@/lib/types";
import { STAGE_LABELS, formatSeconds, stageInfo, type StageState } from "@/lib/pipeline";
import { cn } from "@/lib/utils";
import { DifficultyBadge, StatusBadge, SubsystemBadge } from "./issue-status";
import { PipelineStripLabeled } from "./pipeline-strip";

function StageIcon({ state }: { state: StageState }) {
  const base = "h-5 w-5 rounded-full flex items-center justify-center shrink-0";
  switch (state) {
    case "done":
      return (
        <span className={cn(base, "bg-stage-done text-white")}>
          <Check className="h-3 w-3" />
        </span>
      );
    case "running":
      return (
        <span className={cn(base, "bg-stage-running text-white")}>
          <Loader2 className="h-3 w-3 animate-spin" />
        </span>
      );
    case "failed":
      return (
        <span className={cn(base, "bg-stage-failed text-white")}>
          <X className="h-3 w-3" />
        </span>
      );
    default:
      return (
        <span className={cn(base, "border text-muted-foreground")}>
          <CircleDashed className="h-3 w-3" />
        </span>
      );
  }
}

function expectationLabel(r: Repro): string {
  switch (r.expectation.check) {
    case "should_compile":
      return "should compile";
    case "should_not_compile":
      return r.expectation.diagnostic_contains
        ? `should not compile (${r.expectation.diagnostic_contains})`
        : "should not compile";
    case "should_evaluate_to":
      return `should evaluate to ${JSON.stringify(r.expectation.expected)}`;
    case "requires_inspection":
      return "requires inspection";
  }
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section>
      <h2 className="text-xs uppercase tracking-wide text-muted-foreground mb-2">{title}</h2>
      {children}
    </section>
  );
}

export function IssueDetail({ issue }: { issue: Issue }) {
  const stages = stageInfo(issue);
  const o = issue.outcome;
  const st = issue.status;

  return (
    <div className="max-w-[1400px] mx-auto px-4 py-6">
      <Link href="/" className="inline-flex items-center gap-1 text-sm text-muted-foreground hover:text-foreground">
        <ArrowLeft className="h-4 w-4" /> All issues
      </Link>

      <div className="mt-3 flex flex-col lg:flex-row lg:items-start justify-between gap-4">
        <div className="min-w-0">
          <div className="font-mono text-xs text-muted-foreground">{issue.id}</div>
          <h1 className="mt-1 text-2xl font-semibold leading-tight">{issue.title}</h1>
          <div className="mt-3 flex flex-wrap items-center gap-2 text-sm text-muted-foreground">
            <StatusBadge issue={issue} />
            <SubsystemBadge subsystem={issue.subsystem} />
            <DifficultyBadge difficulty={issue.difficulty} />
            <span>{issue.shepherd ? `shepherd @${issue.shepherd}` : "unassigned"}</span>
            <span>·</span>
            <span>seen on v{issue.version}</span>
            <span>·</span>
            <span>
              {issue.feedback_ids.length} report{issue.feedback_ids.length === 1 ? "" : "s"}
            </span>
          </div>
        </div>
        <div className="w-full lg:w-[380px] shrink-0 rounded-lg border bg-card p-4">
          <div className="text-xs text-muted-foreground mb-2">Pipeline</div>
          <PipelineStripLabeled stages={stages} />
        </div>
      </div>

      <div className="mt-8 grid grid-cols-1 lg:grid-cols-[minmax(0,1fr)_380px] gap-8">
        <div className="space-y-8 min-w-0">
          <Section title="Description">
            <p className="text-sm leading-relaxed">{issue.description}</p>
          </Section>

          {st.state === "rejected" && (
            <Section title="Rejected">
              <p className="text-sm">{st.reason}</p>
            </Section>
          )}
          {st.state === "deferred" && (
            <Section title="Deferred">
              <p className="text-sm">{st.reason}</p>
              {st.workaround && <p className="mt-1 text-sm text-muted-foreground">Workaround: {st.workaround}</p>}
            </Section>
          )}

          <Section title={`Repros (${issue.repros.length})`}>
            {issue.repros.length === 0 ? (
              <p className="text-sm text-muted-foreground">No repro attached.</p>
            ) : (
              <div className="space-y-3">
                {issue.repros.map((r, i) => (
                  <div key={i} className="rounded-md border overflow-hidden">
                    <div className="flex items-center justify-between gap-2 px-3 py-1.5 bg-muted/60 text-xs">
                      <span className="font-mono">$ {r.command}</span>
                      <Badge variant="outline" className="font-normal">
                        {expectationLabel(r)}
                      </Badge>
                    </div>
                    {Object.entries(r.files).map(([name, content]) => (
                      <div key={name}>
                        <div className="px-3 py-1 text-[11px] font-mono text-muted-foreground border-t">{name}</div>
                        <pre className="px-3 py-2 text-xs font-mono bg-code-bg text-code-fg overflow-x-auto">
                          {content}
                        </pre>
                      </div>
                    ))}
                  </div>
                ))}
              </div>
            )}
          </Section>

          {issue.resolution_plan && (
            <Section title="Resolution plan (triage)">
              <p className="text-sm leading-relaxed">{issue.resolution_plan}</p>
            </Section>
          )}

          {issue.design_doc && (
            <Section title="Design doc (agent)">
              <pre className="whitespace-pre-wrap text-sm font-sans leading-relaxed rounded-md border p-3 bg-muted/30">
                {issue.design_doc}
              </pre>
            </Section>
          )}

          <Section title={`Comments (${issue.comments.length})`}>
            {issue.comments.length === 0 ? (
              <p className="text-sm text-muted-foreground">No comments.</p>
            ) : (
              <div className="space-y-3">
                {issue.comments.map((c, i) => (
                  <div key={i} className="rounded-md border p-3">
                    <div className="text-xs text-muted-foreground">
                      @{c.author} · {new Date(c.at).toLocaleString()}
                    </div>
                    <p className="mt-1 text-sm">{c.body}</p>
                  </div>
                ))}
              </div>
            )}
          </Section>
        </div>

        <aside className="space-y-6">
          <Section title="Timeline">
            <ol className="relative border-l ml-2.5 space-y-4">
              {stages.map((s) => (
                <li key={s.stage} className="ml-5">
                  <span className="absolute -left-2.5">
                    <StageIcon state={s.state} />
                  </span>
                  <div className="text-sm font-medium leading-5">{STAGE_LABELS[s.stage]}</div>
                  <div className="text-xs text-muted-foreground">{s.detail}</div>
                </li>
              ))}
            </ol>
          </Section>

          {o && (
            <Section title="Last run">
              <dl className="text-sm grid grid-cols-[auto_1fr] gap-x-3 gap-y-1">
                <dt className="text-muted-foreground">outcome</dt>
                <dd className="font-mono text-xs self-center">{o.running ? `running (${o.running})` : o.kind}</dd>
                <dt className="text-muted-foreground">turns</dt>
                <dd className="tabular-nums">{o.turns}</dd>
                <dt className="text-muted-foreground">time</dt>
                <dd className="tabular-nums">
                  {formatSeconds(o.seconds)}
                  {o.timed_out && <span className="text-stage-failed"> (killed at budget)</span>}
                </dd>
                {o.reason && (
                  <>
                    <dt className="text-muted-foreground">reason</dt>
                    <dd>{o.reason}</dd>
                  </>
                )}
                {o.branch && (
                  <>
                    <dt className="text-muted-foreground">branch</dt>
                    <dd className="font-mono text-xs break-all self-center">{o.branch}</dd>
                  </>
                )}
                {o.pr && (
                  <>
                    <dt className="text-muted-foreground">PR</dt>
                    <dd>
                      <a
                        href={o.pr.startsWith("https://github.com/") ? o.pr : undefined}
                        target="_blank"
                        rel="noreferrer"
                        className="inline-flex items-center gap-1 underline underline-offset-2"
                      >
                        {o.pr.replace("https://github.com/", "")}
                        <ExternalLink className="h-3 w-3" />
                      </a>
                    </dd>
                  </>
                )}
              </dl>
              {o.gate && (
                <div className="mt-3">
                  <div className="text-xs text-muted-foreground mb-1">
                    gate · crates: {o.gate.changed_crates.join(", ")}
                  </div>
                  <ul className="rounded-md border divide-y text-xs">
                    {o.gate.steps.map((s) => (
                      <li key={s.name} className="flex items-center gap-2 px-2 py-1">
                        <span
                          className={cn("h-1.5 w-1.5 rounded-full shrink-0", s.ok ? "bg-stage-done" : "bg-stage-failed")}
                        />
                        <span className="font-mono truncate">{s.name}</span>
                        <span className="ml-auto tabular-nums text-muted-foreground">
                          {formatSeconds(s.seconds)}
                        </span>
                        {!s.ok && <span className="text-stage-failed">exit {s.exit_code}</span>}
                      </li>
                    ))}
                  </ul>
                </div>
              )}
            </Section>
          )}

          <Section title="Feedback">
            <ul className="text-xs font-mono space-y-1">
              {issue.feedback_ids.map((f) => (
                <li key={f} className="text-muted-foreground">
                  {f}
                </li>
              ))}
            </ul>
          </Section>
        </aside>
      </div>
    </div>
  );
}
