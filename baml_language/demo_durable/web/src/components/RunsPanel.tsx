import { useEffect, useMemo, useState } from "react";
import type { Api } from "../api";
import { clock } from "../clock";
import { formatClockShort, formatCountdown } from "../format";
import { shortHash, type Run, type Site, type SiteEntry } from "../protocol";
import { groupRuns, pauseBlocked, resumeTargets, runKey, validActions, type AppState, type RunAction, type RunKey } from "../state";
import type { SnapshotRef } from "../timeline/Timeline";
import { SiteChip, StatusBadge } from "./StatusBadge";

interface Props {
  /** The site registry. It names the destinations of "Resume on <site>". */
  sites: readonly SiteEntry[];
  runs: readonly Run[];
  events: AppState["events"];
  selected: RunKey | null;
  tree: readonly RunKey[];
  selectedSnapshot: SnapshotRef | null;
  api: Api;
  onSelect(key: RunKey): void;
  onRunResponse(run: Run, requestedOn: Run["site"], selectIt: boolean): void;
  /** The control that the scenario guide points at, or presses when autoplay is on. */
  coach?: CoachTarget | null;
}

/**
 * A control of the selected run that the scenario guide highlights. `mode` is
 * the look of the highlight: `hint` waits for the user, and `press` announces
 * that autoplay acts. The control is pressed once for every new value of
 * `nonce`. `null` presses nothing, which is the case while a recording plays.
 */
export interface CoachTarget {
  run: RunKey;
  action: RunAction;
  /** The destination of `resume_on`. */
  site?: Site;
  mode: "hint" | "press";
  nonce: number | null;
}

/** The current time, refreshed while a run sleeps, for the countdown to its wake time. */
function useSleepClock(active: boolean): number {
  const [now, setNow] = useState(() => clock.now());
  useEffect(() => {
    if (!active) return;
    setNow(clock.now());
    const id = setInterval(() => setNow(clock.now()), 200);
    return () => clearInterval(id);
  }, [active]);
  return now;
}

/** "wakes 17:00:12 · in 8.2 s" for a sleeping run. */
export function WakeLine({ run, now }: { run: Run; now: number }) {
  if (run.status !== "sleeping") return null;
  const wakeAt = typeof run.wake_at === "number" ? run.wake_at : null;
  return (
    <span className="wake-line" data-testid="wake-line" title="The run suspended itself for a sleep. No process exists. A timer of the site server resumes it.">
      {wakeAt === null ? "wake time unknown" : `wakes ${formatClockShort(wakeAt)} · in ${formatCountdown(wakeAt - now)}`}
    </span>
  );
}

export function RunsPanel({ sites, runs, events, selected, tree, selectedSnapshot, api, onSelect, onRunResponse, coach }: Props) {
  // Remote children are listed under their parent, in call order.
  const rows = useMemo(() => groupRuns(runs), [runs]);
  const [collapsed, setCollapsed] = useState<ReadonlySet<RunKey>>(new Set());
  const current = runs.find((run) => runKey(run.site, run.id) === selected) ?? null;
  const known = new Set(runs.map((run) => runKey(run.site, run.id)));
  const now = useSleepClock(runs.some((run) => run.status === "sleeping"));
  // A collapsed parent hides the rows below it that are deeper, up to the next row of its own depth.
  const visible = useMemo(() => {
    const result: typeof rows = [];
    let hideBelow: number | null = null;
    for (const row of rows) {
      if (hideBelow !== null && row.depth > hideBelow) continue;
      hideBelow = collapsed.has(row.key) ? row.depth : null;
      result.push(row);
    }
    return result;
  }, [rows, collapsed]);
  const toggle = (key: RunKey): void =>
    setCollapsed((current) => {
      const next = new Set(current);
      if (!next.delete(key)) next.add(key);
      return next;
    });

  return (
    <section className="panel" data-testid="runs-panel">
      <div className="panel-head">
        <span className="panel-title">Runs</span>
        <span className="muted">all sites, newest first, children under their parent</span>
        <span className="spacer" />
        <span className="num">{runs.length}</span>
      </div>
      <div className="panel-body">
        {rows.length === 0 ? (
          <div className="empty">No runs yet. Pick a function and press Start.</div>
        ) : (
          <table className="runs">
            <colgroup>
              <col className="c-status" />
              <col className="c-site" />
              <col />
              <col className="c-d" />
              <col className="c-pid" />
              <col className="c-seg" />
            </colgroup>
            <thead>
              <tr>
                <th>Status</th>
                <th>Site</th>
                <th>Function</th>
                <th title="durable">D</th>
                <th>Pid</th>
                <th>Seg</th>
              </tr>
            </thead>
            <tbody>
              {visible.map(({ key, run, depth, children }) => {
                const parentKey = run.parent ? runKey(run.parent.site, run.parent.run) : null;
                const originKey = run.origin ? runKey(run.origin.site, run.origin.run) : null;
                // A fork is created on the site of its source. A fork that migrated
                // keeps `forked_from`, and its source stays where the fork was made,
                // which can be more than one migration away.
                const sourceKey = run.forked_from ? forkSourceKey(run, known) : null;
                const blocked = pauseBlocked(run, events[key]);
                return (
                  <tr key={key} className="run-row" aria-selected={key === selected} data-in-tree={tree.includes(key)} data-depth={depth}
                    data-testid="run-row" data-run={run.id} data-site={run.site} data-status={run.status} onClick={() => onSelect(key)}>
                    <td><StatusBadge status={run.status} /></td>
                    <td><SiteChip site={run.site} /></td>
                    <td style={depth > 0 ? { paddingLeft: 6 + Math.min(depth, 3) * 12 } : undefined}>
                      <div className="fn" title={run.function}>
                        {children > 0 && (
                          <button className="group-toggle" data-testid="group-toggle" aria-expanded={!collapsed.has(key)}
                            title={`${children} remote ${children === 1 ? "child" : "children"}. Click to ${collapsed.has(key) ? "show" : "hide"} them.`}
                            onClick={(event) => { event.stopPropagation(); toggle(key); }}>
                            {collapsed.has(key) ? "▸" : "▾"} {children}
                          </button>
                        )}
                        {run.function}
                      </div>
                      <div className="rel mono">
                        {run.id}
                        {shortHash(run.program_hash, 8) && (
                          <span className="prog" data-testid="row-program" title={`program ${run.program_hash}`}> · prog {shortHash(run.program_hash, 8)}</span>
                        )}
                      </div>
                      {run.status === "sleeping" && <div className="rel"><WakeLine run={run} now={now} /></div>}
                      {run.parent && (
                        <div className="rel mono" data-testid="child-of" title={`remote child of ${run.parent.site}/${run.parent.run}`}>
                          {/* The column fits about 19 characters, so the relation is a glyph plus the link. */}
                          <span aria-label="child of">↳</span>{" "}
                          <button disabled={parentKey === null || !known.has(parentKey)} title={`child of ${run.parent.site}/${run.parent.run}, call ${run.parent.call_id}`}
                            onClick={(event) => { event.stopPropagation(); if (parentKey) onSelect(parentKey); }}>
                            {run.parent.site}/{run.parent.run}
                          </button>
                        </div>
                      )}
                      {run.forked_from && (
                        <div className="rel mono" data-testid="forked-from" title={`fork of ${run.forked_from.run} at snapshot #${run.forked_from.n}`}>
                          <span aria-label="fork of">⑂</span>{" "}
                          <button disabled={sourceKey === null || !known.has(sourceKey)} title={`fork of ${run.forked_from.run} at snapshot #${run.forked_from.n}`}
                            onClick={(event) => { event.stopPropagation(); if (sourceKey) onSelect(sourceKey); }}>
                            {run.forked_from.run} #{run.forked_from.n}
                          </button>
                        </div>
                      )}
                      {run.origin && (
                        <div className="rel mono">
                          ⇢ from{" "}
                          <button disabled={originKey === null || !known.has(originKey)}
                            onClick={(event) => { event.stopPropagation(); if (originKey) onSelect(originKey); }}>
                            {run.origin.site}
                          </button>
                        </div>
                      )}
                      {blocked && (
                        <div className="rel blocked-reason" data-testid="row-blocked-reason" title={`${blocked.reason}\n${blocked.path.join(" › ")}`}>
                          blocked ×{blocked.attempts}: {blocked.reason}
                        </div>
                      )}
                    </td>
                    <td><span className="badge-d" data-on={run.durable} title={run.durable ? "durable" : "not durable"}>D</span></td>
                    <td className="mono num">{run.pid ?? "—"}</td>
                    <td className="mono num">{run.segment}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </div>
      <Controls sites={sites} run={current} blocked={pauseBlocked(current, selected ? events[selected] : undefined)} api={api} selectedSnapshot={selectedSnapshot}
        onRunResponse={onRunResponse} coach={coach && coach.run === selected ? coach : null} now={now} />
    </section>
  );
}

/** The record of the run that `run` was forked from: on the same site, on the origin site, or on any site. */
function forkSourceKey(run: Run, known: ReadonlySet<RunKey>): RunKey | null {
  if (!run.forked_from) return null;
  const id = run.forked_from.run;
  const preferred = [runKey(run.site, id), ...(run.origin ? [runKey(run.origin.site, id)] : [])];
  const anywhere = [...known].filter((key) => key.endsWith(`/${id}`));
  return [...preferred, ...anywhere].find((key) => known.has(key)) ?? preferred[0] ?? null;
}

interface ControlButton {
  action: RunAction;
  /** The destination of `resume_on`. */
  site?: Site;
  label: string;
  danger?: boolean;
  title: string;
}

/** One "Resume on <site>" button for every site other than the one that holds the run (section 8.4). */
function controlButtons(sites: readonly SiteEntry[], run: Run | null): ControlButton[] {
  return [
    { action: "pause", label: "Pause", title: "Write a snapshot at the next clean point and end the process" },
    { action: "resume_here", label: "Resume here", title: "Start the next segment on this site from the latest snapshot. A sleeping run wakes before its timer" },
    // Without a run the buttons are disabled. They name the targets of a run on the first site,
    // so that the row of buttons keeps its size when a run is selected.
    ...resumeTargets(sites, run ?? { site: sites[0]?.name ?? "" }).map((site): ControlButton => ({
      action: "resume_on",
      site,
      label: `Resume on ${site}`,
      title: `Send the snapshot to ${site} and resume there`,
    })),
    { action: "kill", label: "Kill process", danger: true, title: "End the worker process without a snapshot" },
    { action: "cancel", label: "Cancel", title: "Cancel the run. A run without a process is cancelled on the site server, and its remote children are cancelled too" },
    { action: "fork", label: "Fork", title: "Create a new paused run from a snapshot of this run" },
  ];
}

function Controls({
  sites,
  run,
  blocked,
  api,
  selectedSnapshot,
  onRunResponse,
  coach,
  now,
}: {
  sites: readonly SiteEntry[];
  run: Run | null;
  blocked: ReturnType<typeof pauseBlocked>;
  api: Api;
  selectedSnapshot: SnapshotRef | null;
  onRunResponse: Props["onRunResponse"];
  coach: CoachTarget | null;
  now: number;
}) {
  const [busy, setBusy] = useState<RunAction | null>(null);
  const buttons = controlButtons(sites, run);
  const [error, setError] = useState<string | null>(null);
  const valid = validActions(run);
  const runId = run ? runKey(run.site, run.id) : null;
  const status = run?.status;
  // An error describes one attempt. It is stale once the run is another one or has moved on.
  useEffect(() => setError(null), [runId, status]);

  const forkSnapshot =
    run && selectedSnapshot && selectedSnapshot.run === run.id && selectedSnapshot.site === run.site ? selectedSnapshot.n : undefined;

  const send = async ({ action, site }: ControlButton): Promise<void> => {
    if (!run) return;
    setBusy(action);
    setError(null);
    try {
      const options = action === "fork" ? { snapshot: forkSnapshot } : action === "resume_on" ? { site } : undefined;
      const response = await api.command(run.site, run.id, action, options);
      onRunResponse(response, run.site, action === "fork");
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(null);
    }
  };

  // Autoplay: the scenario guide presses the control that it points at, once per nonce.
  const coached = coach ? buttons.find((button) => button.action === coach.action && (coach.site === undefined || button.site === coach.site)) : undefined;
  const pressNonce = coach?.nonce ?? null;
  const coachedEnabled = coached !== undefined && valid[coached.action] && busy === null;
  const [pressed, setPressed] = useState<number | null>(null);
  useEffect(() => {
    if (pressNonce === null || pressed === pressNonce || !coached || !coachedEnabled) return;
    setPressed(pressNonce);
    void send(coached);
    // `send` and `coached` follow the run and the registry. Only a new nonce presses.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pressNonce, coachedEnabled]);

  return (
    <div className="controls" data-testid="controls">
      <div className="target">
        {run ? (
          <>
            <SiteChip site={run.site} />
            <span className="mono">{run.id}</span>
            <StatusBadge status={run.status} />
            <WakeLine run={run} now={now} />
            {shortHash(run.program_hash) && (
              <span className="mono muted" data-testid="program-hash" title={`program ${run.program_hash}\nThe SHA-256 of the compiled program. A site runs it from its program store without compiling.`}>
                prog {shortHash(run.program_hash)}
              </span>
            )}
            {run.error && <span className="muted clip" title={run.error}>{run.error}</span>}
          </>
        ) : (
          <span className="muted">No run selected</span>
        )}
      </div>
      {blocked && (
        <div className="blocked-reason blocked-line" data-testid="blocked-reason"
          title={`${blocked.reason}\n${blocked.path.join(" › ")}\nThe worker retries at its next yield. The status stays pausing until a snapshot succeeds.`}>
          snapshot blocked ×{blocked.attempts}: {blocked.reason}
        </div>
      )}
      <div className="buttons">
        {buttons.map((button) => (
          <button key={`${button.action}:${button.site ?? ""}`} className={`btn${button.danger ? " danger" : ""}`} title={button.title}
            data-action={button.action} data-target-site={button.site} disabled={!valid[button.action] || busy !== null} onClick={() => void send(button)}
            data-coach={coach && button === coached ? coach.mode : undefined}>
            {button.label}
            {button.action === "fork" && forkSnapshot !== undefined ? ` #${forkSnapshot}` : ""}
          </button>
        ))}
      </div>
      {error && <div className="error-line" role="alert" data-testid="command-error">{error}</div>}
    </div>
  );
}
