/**
 * Commands and queries against the site servers (contract sections 3.3 and
 * 8.3). The Vite dev server proxies `/<site>/*` to each site.
 */

import {
  parseSseEvent,
  type ApiError,
  type ForkRequest,
  type ResumeRequest,
  type Run,
  type Site,
  type SiteInfo,
  type SourceResponse,
  type SseEvent,
  type StartRunRequest,
  type StateDump,
} from "./protocol";
import type { RunAction } from "./state";

export interface Api {
  readonly mode: "live" | "fixture";
  info(site: Site): Promise<SiteInfo>;
  runEvents(site: Site, run: string): Promise<SseEvent[]>;
  source(site: Site, file: string): Promise<SourceResponse>;
  snapshotState(site: Site, run: string, n: number): Promise<StateDump>;
  startRun(site: Site, request: StartRunRequest): Promise<Run>;
  /**
   * `options.site` is the destination of `resume_on`, and it is required for
   * that action. `options.snapshot` selects the snapshot of `fork`.
   */
  command(site: Site, run: string, action: RunAction, options?: { snapshot?: number; site?: Site }): Promise<Run>;
}

function isApiError(value: unknown): value is ApiError {
  return typeof value === "object" && value !== null && typeof (value as ApiError).error === "string";
}

async function request<T>(site: Site, path: string, body?: unknown): Promise<T> {
  let response: Response;
  try {
    response = await fetch(`/${site}${path}`, {
      method: body === undefined ? "GET" : "POST",
      headers: body === undefined ? undefined : { "content-type": "application/json" },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
  } catch (cause) {
    throw new Error(`${site}: request failed (${cause instanceof Error ? cause.message : String(cause)})`);
  }
  const text = await response.text();
  let json: unknown = null;
  try {
    json = text === "" ? null : JSON.parse(text);
  } catch {
    json = null;
  }
  // Only the HTTP status marks a failure (section 3.3). A successful response
  // can carry an `error` field of its own: a run record of a failed or lost
  // run has `error: string`.
  if (!response.ok) {
    const message = isApiError(json) ? json.error : text.slice(0, 200) || response.statusText;
    throw new Error(`${site}: ${response.status} ${message}`);
  }
  if (json === null) {
    // Every endpoint of section 3.3 answers with a JSON object or array.
    throw new Error(`${site}: ${response.status} response is not JSON (${text.slice(0, 80)})`);
  }
  return json as T;
}

export const httpApi: Api = {
  mode: "live",
  info: (site) => request<SiteInfo>(site, "/api/info"),
  async runEvents(site, run) {
    const raw = await request<unknown>(site, `/api/runs/${encodeURIComponent(run)}/events`);
    if (!Array.isArray(raw)) return [];
    return raw.flatMap((item) => {
      const event = parseSseEvent(item, site);
      return event ? [event] : [];
    });
  },
  source: (site, file) => request<SourceResponse>(site, `/api/source?file=${encodeURIComponent(file)}`),
  snapshotState: (site, run, n) =>
    request<StateDump>(site, `/api/runs/${encodeURIComponent(run)}/snapshots/${n}/state`),
  startRun: (site, body) => request<Run>(site, "/api/runs", body),
  command(site, run, action, options) {
    const base = `/api/runs/${encodeURIComponent(run)}`;
    switch (action) {
      case "pause":
        return request<Run>(site, `${base}/pause`, {});
      case "resume_here":
        return request<Run>(site, `${base}/resume`, {} satisfies ResumeRequest);
      case "resume_on": {
        const target = options?.site;
        if (target === undefined || target === site) {
          return Promise.reject(new Error(`${site}: resume on another site needs a destination site`));
        }
        return request<Run>(site, `${base}/resume`, { site: target } satisfies ResumeRequest);
      }
      case "kill":
        return request<Run>(site, `${base}/kill`, {});
      case "cancel":
        return request<Run>(site, `${base}/cancel`, {});
      case "fork":
        return request<Run>(
          site,
          `${base}/fork`,
          (options?.snapshot === undefined ? {} : { n: options.snapshot }) satisfies ForkRequest,
        );
    }
  },
};
