/**
 * Loads telemetry for the Telemetry tab.
 *
 * Reads come from the profile store through the run store client, so they
 * are request/response rather than subscriptions: the store publishes on an
 * interval and a bound query sees a frozen prefix of it. The list therefore
 * refreshes when the tab opens, when the project changes, and when a run
 * finishes; a run in flight is not streamed in.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { RunCommandError, type RunStoreClient } from '../run-store-client';
import type {
  ExecutionTelemetry,
  TelemetryExecution,
} from '../worker-protocol';
import {
  buildEvidence,
  type Evidence,
  type ExecutionRow,
  toExecutionRow,
} from './evidence';

export interface TelemetryState {
  executions: ExecutionRow[];
  evidence: Evidence | null;
  selectedId: string | null;
  select: (executionId: string | null) => void;
  refresh: () => void;
  loading: boolean;
  storeMissing: boolean;
  error: string | null;
}

export interface UseTelemetryOptions {
  client: RunStoreClient;
  /** Project root; reads are scoped to its store. Null disables loading. */
  project: string | null;
  /** Only fetch while the tab is showing. */
  active: boolean;
  /** Functions the project reports as model calls. */
  llmFunctions: ReadonlySet<string>;
  /**
   * Bumped when a run completes, so the list picks up the execution the
   * profiler has just sealed.
   */
  revision?: unknown;
}

/**
 * How often to re-read while something is running.
 *
 * An execution started outside the playground -- from the CLI, or a test
 * run -- produces no run patches here, so nothing would ever prompt a
 * refetch and the view would claim "running" indefinitely after it had
 * finished. Polling stops as soon as nothing is running.
 */
const RUNNING_POLL_MS = 2000;

/**
 * An execution with no recorded end that started longer ago than this is
 * more likely one whose writer stopped than one still in flight. Nothing
 * says which, so it is re-read rarely rather than never: a long run still
 * shows its end, and a dead one does not cost a query every two seconds.
 */
const RECENT_INCOMPLETE_MS = 10 * 60_000;
const STALE_POLL_MS = 30_000;

/** How often to re-read the list, or null when nothing can change. */
export function pollInterval(
  rows: readonly Pick<ExecutionRow, 'status' | 'startedMs'>[],
  now: number,
): number | null {
  let incomplete = false;
  for (const row of rows) {
    if (row.status === 'running') return RUNNING_POLL_MS;
    if (row.status !== 'incomplete') continue;
    if (row.startedMs != null && now - row.startedMs < RECENT_INCOMPLETE_MS) {
      return RUNNING_POLL_MS;
    }
    incomplete = true;
  }
  return incomplete ? STALE_POLL_MS : null;
}

/**
 * The server bounds telemetry work per session, and says so with this code
 * when a request arrives while its share is already running. The usual
 * cause is this hook's own polling outrunning a slow query, so a refresh
 * that is turned away is skipped rather than shown as a failure: the data
 * already on screen is still the best answer, and the next poll retries.
 */
const BUSY_CODE = 'telemetryBusy';

function isBusy(cause: unknown): boolean {
  return cause instanceof RunCommandError && cause.code === BUSY_CODE;
}

export function useTelemetry({
  client,
  project,
  active,
  llmFunctions,
  revision,
}: UseTelemetryOptions): TelemetryState {
  const [executions, setExecutions] = useState<TelemetryExecution[]>([]);
  const [telemetry, setTelemetry] = useState<ExecutionTelemetry | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [storeMissing, setStoreMissing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [reloadToken, setReloadToken] = useState(0);

  // A late response for a project or execution the user has moved on from
  // must not overwrite what is on screen.
  const listRequest = useRef(0);
  const detailRequest = useRef(0);
  // Read inside the fetch effects to decide whether a spinner is warranted,
  // without making them re-run when the data changes.
  const executionsRef = useRef(executions);
  executionsRef.current = executions;
  const telemetryRef = useRef(telemetry);
  telemetryRef.current = telemetry;

  // `revision` and `reloadToken` below are refetch triggers, not values the
  // effect reads: a completed run or an explicit refresh has to re-query the
  // store even though nothing else changed.
  // biome-ignore lint/correctness/useExhaustiveDependencies: refetch triggers
  useEffect(() => {
    if (!active || !project) return;
    const request = ++listRequest.current;
    // Loading means "nothing to show yet", not "a request is in flight".
    // Polling a running execution refetches every couple of seconds, and
    // flagging each of those would flash a spinner over a populated list.
    setLoading((current) => current || executionsRef.current.length === 0);
    setError(null);
    client
      .listExecutions(project)
      .then((result) => {
        if (request !== listRequest.current) return;
        setExecutions(result.executions);
        setStoreMissing(result.storeMissing);
      })
      .catch((cause: unknown) => {
        if (request !== listRequest.current || isBusy(cause)) return;
        setExecutions([]);
        setError(cause instanceof Error ? cause.message : String(cause));
      })
      .finally(() => {
        if (request === listRequest.current) setLoading(false);
      });
  }, [active, client, project, reloadToken, revision]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: refetch trigger
  useEffect(() => {
    if (!active || !project || !selectedId) {
      setTelemetry(null);
      return;
    }
    const request = ++detailRequest.current;
    setLoading((current) => current || telemetryRef.current == null);
    setError(null);
    client
      .openExecution(project, selectedId)
      .then((result) => {
        if (request !== detailRequest.current) return;
        setTelemetry(result);
      })
      .catch((cause: unknown) => {
        if (request !== detailRequest.current || isBusy(cause)) return;
        setTelemetry(null);
        setError(cause instanceof Error ? cause.message : String(cause));
      })
      .finally(() => {
        if (request === detailRequest.current) setLoading(false);
      });
  }, [active, client, project, selectedId, reloadToken]);

  // Selecting into a different project would open an execution that store
  // does not have.
  // biome-ignore lint/correctness/useExhaustiveDependencies: reset on project
  useEffect(() => {
    setSelectedId(null);
  }, [project]);

  const rows = useMemo(() => executions.map(toExecutionRow), [executions]);
  // An incomplete execution may still be recording, and only a re-read can
  // show its end arriving. Every re-read recomputes this with a fresh clock.
  const poll = useMemo(() => pollInterval(rows, Date.now()), [rows]);

  useEffect(() => {
    if (!active || !project || poll == null) return;
    const timer = setInterval(() => setReloadToken((token) => token + 1), poll);
    return () => clearInterval(timer);
  }, [active, project, poll]);
  const evidence = useMemo(
    () => (telemetry ? buildEvidence(telemetry, { llmFunctions }) : null),
    [llmFunctions, telemetry],
  );

  const select = useCallback((executionId: string | null) => {
    setSelectedId(executionId);
  }, []);
  const refresh = useCallback(() => {
    setReloadToken((token) => token + 1);
  }, []);

  return {
    error,
    evidence,
    executions: rows,
    loading,
    refresh,
    select,
    selectedId,
    storeMissing,
  };
}
