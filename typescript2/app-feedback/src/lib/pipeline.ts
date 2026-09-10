import type { Issue } from './types';

// The pipeline an issue moves through, in order. The first three are the
// triage stages (create_issue / organize_issue / gauge_issue); the rest are
// handle_issue's passes, read from its outcome.json.
export const STAGES = [
  'triaged',
  'organized',
  'gauged',
  'design',
  'fix',
  'gate',
  'pr',
] as const;

export type Stage = (typeof STAGES)[number];

export type StageState = 'done' | 'running' | 'failed' | 'skipped' | 'todo';

export interface StageInfo {
  stage: Stage;
  state: StageState;
  /** One line shown in the row's tooltip / detail timeline. */
  detail: string;
}

export const STAGE_LABELS: Record<Stage, string> = {
  design: 'Design pass',
  fix: 'Fix pass',
  gate: 'Gate',
  gauged: 'Gauged',
  organized: 'Organized',
  pr: 'PR',
  triaged: 'Triaged',
};

export function stageInfo(issue: Issue): StageInfo[] {
  const out: StageInfo[] = [];
  out.push({
    detail: `${issue.repros.length} repro${issue.repros.length === 1 ? '' : 's'} from ${issue.feedback_ids.length} report${issue.feedback_ids.length === 1 ? '' : 's'}`,
    stage: 'triaged',
    state: 'done',
  });
  out.push(
    issue.shepherd
      ? {
          detail: `shepherd: ${issue.shepherd}`,
          stage: 'organized',
          state: 'done',
        }
      : { detail: 'no shepherd yet', stage: 'organized', state: 'todo' },
  );
  out.push(
    issue.difficulty
      ? { detail: issue.difficulty, stage: 'gauged', state: 'done' }
      : { detail: 'not gauged', stage: 'gauged', state: 'todo' },
  );

  const o = issue.outcome;
  const terminal =
    issue.status.state === 'rejected' || issue.status.state === 'deferred';
  const todo = (stage: Stage, detail: string): StageInfo => ({
    detail,
    stage,
    state: terminal ? 'skipped' : 'todo',
  });

  if (!o) {
    out.push(todo('design', 'not started'));
    out.push(todo('fix', 'not started'));
    out.push(todo('gate', 'not run'));
    out.push(todo('pr', 'none'));
    return out;
  }

  const mins = Math.round(o.seconds / 60);
  const runInfo = `${o.turns} turns, ${mins} min`;

  if (o.running) {
    const order: Stage[] = ['design', 'fix', 'gate', 'pr'];
    for (const s of order) {
      if (s === o.running)
        out.push({
          detail: `running (${runInfo})`,
          stage: s,
          state: 'running',
        });
      else if (order.indexOf(s) < order.indexOf(o.running))
        out.push({ detail: 'done', stage: s, state: 'done' });
      else out.push({ detail: 'pending', stage: s, state: 'todo' });
    }
    return out;
  }

  switch (o.kind) {
    case 'agent_stopped': {
      const inFix = o.branch !== null && o.design_doc !== null;
      out.push(
        inFix
          ? { detail: 'plan written', stage: 'design', state: 'done' }
          : { detail: o.reason ?? 'stopped', stage: 'design', state: 'failed' },
      );
      out.push(
        inFix
          ? { detail: o.reason ?? 'stopped', stage: 'fix', state: 'failed' }
          : { detail: 'not reached', stage: 'fix', state: 'skipped' },
      );
      out.push({ detail: 'not run', stage: 'gate', state: 'skipped' });
      out.push({ detail: 'none', stage: 'pr', state: 'skipped' });
      return out;
    }
    case 'hard':
      out.push({
        detail: `design doc written (${runInfo})`,
        stage: 'design',
        state: 'done',
      });
      out.push({
        detail: 'hard: handed to the shepherd',
        stage: 'fix',
        state: 'skipped',
      });
      out.push({ detail: 'not run', stage: 'gate', state: 'skipped' });
      out.push({ detail: 'none', stage: 'pr', state: 'skipped' });
      return out;
    case 'gate_failed': {
      const failed = o.gate?.steps.find((s) => !s.ok);
      out.push({ detail: 'plan written', stage: 'design', state: 'done' });
      out.push({ detail: runInfo, stage: 'fix', state: 'done' });
      out.push({
        detail: failed ? `${failed.name} failed` : 'failed',
        stage: 'gate',
        state: 'failed',
      });
      out.push({
        detail: 'branch kept for a human',
        stage: 'pr',
        state: 'skipped',
      });
      return out;
    }
    case 'fixed':
      out.push({ detail: 'plan written', stage: 'design', state: 'done' });
      out.push({ detail: runInfo, stage: 'fix', state: 'done' });
      out.push({
        detail: `${o.gate?.steps.length ?? 0} steps green`,
        stage: 'gate',
        state: 'done',
      });
      out.push(
        o.pr
          ? {
              detail: o.pr.replace('https://github.com/', ''),
              stage: 'pr',
              state: 'done',
            }
          : { detail: 'dry run: not pushed', stage: 'pr', state: 'todo' },
      );
      return out;
  }
}

/** 0..1, how far the issue is through the pipeline. */
export function progress(issue: Issue): number {
  const infos = stageInfo(issue);
  const done = infos.filter((s) => s.state === 'done').length;
  return done / infos.length;
}

export function statusLabel(issue: Issue): string {
  const s = issue.status;
  switch (s.state) {
    case 'open':
      return 'Open';
    case 'awaiting_approval':
      return 'Awaiting approval';
    case 'approved':
      return 'Approved';
    case 'in_progress':
      return s.pr ? 'PR open' : 'In progress';
    case 'merged':
      return 'Merged';
    case 'shipped':
      return `Shipped ${s.version}`;
    case 'deferred':
      return 'Deferred';
    case 'rejected':
      return 'Rejected';
  }
}

export function formatSeconds(s: number): string {
  if (s < 60) return `${s}s`;
  const m = Math.round(s / 60);
  if (m < 60) return `${m} min`;
  return `${Math.floor(m / 60)}h ${m % 60}m`;
}

export function relativeTime(iso: string, now: number): string {
  const diff = now - new Date(iso).getTime();
  if (diff < 0) return 'in the future';
  const minutes = Math.floor(diff / 60000);
  const hours = Math.floor(diff / 3600000);
  const days = Math.floor(diff / 86400000);
  if (minutes < 1) return 'just now';
  if (minutes < 60) return `${minutes}m ago`;
  if (hours < 24) return `${hours}h ago`;
  if (days < 30) return `${days}d ago`;
  return new Date(iso).toLocaleDateString();
}
