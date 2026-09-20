/**
 * Event sources for the reducer: one live `EventSource` connection per site of
 * the registry, or the playback of a fixture.
 */

import type { Dispatch } from "react";
import { httpApi, type Api } from "./api";
import { clock } from "./clock";
import { TRIP_BAML, TRIP_BAML_FILE } from "./fixtures/trip.baml";
import type { Fixture } from "./fixtures";
import {
  DEFAULT_SITES,
  parseSseEvent,
  REGISTRY_SITE,
  sitesFromInfo,
  type Site,
  type SiteEntry,
  type SiteInfo,
  type SseEvent,
} from "./protocol";
import type { Action } from "./state";

export interface Session {
  api: Api;
  stop(): void;
}

// ---------------------------------------------------------------------------
// Live
// ---------------------------------------------------------------------------

/** Delay before the app opens a new `EventSource` after the browser gave up on one. */
const REOPEN_DELAY_MS = 2000;
/** The delay doubles with every failed attempt in a row, up to this value. */
const REOPEN_DELAY_MAX_MS = 8000;
/** How long the app waits for the site registry before it uses the default one. */
const REGISTRY_TIMEOUT_MS = 2500;

export function reopenDelay(failures: number): number {
  return Math.min(REOPEN_DELAY_MS * 2 ** Math.max(failures - 1, 0), REOPEN_DELAY_MAX_MS);
}

/**
 * The site registry from `GET /<REGISTRY_SITE>/api/info`, or the default
 * registry when the request fails or takes too long (section 8.4).
 */
export async function loadSiteRegistry(api: Pick<Api, "info">, timeoutMs = REGISTRY_TIMEOUT_MS): Promise<{ sites: readonly SiteEntry[]; info: SiteInfo | null }> {
  const timeout = new Promise<null>((resolve) => setTimeout(() => resolve(null), timeoutMs));
  const info = await Promise.race([api.info(REGISTRY_SITE).catch(() => null), timeout]);
  return { sites: sitesFromInfo(info) ?? DEFAULT_SITES, info };
}

export function startLiveSession(dispatch: Dispatch<Action>): Session {
  clock.reset();
  let stopped = false;
  const cleanups: (() => void)[] = [];
  const connected = new Set<Site>();

  /** Applies a registry and opens a connection to every site that has none yet. */
  const applySites = (sites: readonly SiteEntry[]): void => {
    if (stopped) return;
    dispatch({ type: "sites", sites });
    for (const site of sites) {
      if (connected.has(site.name)) continue;
      connected.add(site.name);
      connect(site.name, 0);
    }
  };

  const connect = (site: Site, failures: number): void => {
    if (stopped) return;
    const source = new EventSource(`/${site}/api/events`);
    let timer: ReturnType<typeof setTimeout> | null = null;
    let failed = failures;
    source.onopen = () => {
      failed = 0;
      dispatch({ type: "connection", site, status: "open" });
      httpApi
        .info(site)
        .then((info) => {
          dispatch({ type: "info", site, info });
          // The registry site can come up after the app. Its registry replaces the default one.
          const sites = site === REGISTRY_SITE ? sitesFromInfo(info) : null;
          if (sites !== null) applySites(sites);
        })
        .catch(() => undefined);
    };
    source.onerror = () => {
      dispatch({ type: "connection", site, status: "disconnected" });
      // EventSource reconnects by itself after a dropped connection. It gives
      // up for good after an HTTP error status, which is what the dev proxy
      // answers while a site server is down. In that case, open a new one.
      // One site that is down does not affect the connections of the others.
      if (source.readyState === EventSource.CLOSED && timer === null) {
        failed += 1;
        timer = setTimeout(() => {
          source.close();
          connect(site, failed);
        }, reopenDelay(failed));
      }
    };
    source.onmessage = (message: MessageEvent<string>) => {
      let raw: unknown;
      try {
        raw = JSON.parse(message.data);
      } catch {
        return;
      }
      const event = parseSseEvent(raw, site);
      // `null` is a message of an unknown type. The contract requires the app to ignore it.
      if (event !== null) dispatch({ type: "sse", site, event });
    };
    cleanups.push(() => {
      if (timer !== null) clearTimeout(timer);
      source.close();
    });
  };

  void loadSiteRegistry(httpApi).then(({ sites }) => applySites(sites));
  return {
    api: httpApi,
    stop: () => {
      stopped = true;
      cleanups.forEach((cleanup) => cleanup());
    },
  };
}

// ---------------------------------------------------------------------------
// Fixture playback
// ---------------------------------------------------------------------------

function fixtureInfo(site: Site): SiteInfo {
  const city = [{ name: "city", type: "string" }];
  return {
    site,
    sites: [...DEFAULT_SITES],
    remote_pool: ["cloud", "cloud2"],
    program_dir: "../program",
    functions: [
      { name: "durable_plan_trip", params: city, durable: true, remote: false },
      { name: "durable_plan_trip_parallel", params: city, durable: true, remote: false },
      { name: "plan_trip", params: city, durable: false, remote: false },
      { name: "remote_fetch_weather", params: city, durable: false, remote: true },
    ],
  };
}

/**
 * Plays a fixture into the reducer. `speed` is the playback rate relative to
 * the recorded time. `Infinity` dispatches every message at once.
 */
export function startFixtureSession(fixture: Fixture, dispatch: Dispatch<Action>, speed: number): Session {
  const events = fixture.events;
  const first = events[0]?.ts ?? 0;
  const last = events[events.length - 1]?.ts ?? first;
  const played: SseEvent[] = [];
  let cursor = 0;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let stopped = false;

  const wallStart = performance.now();
  const virtualNow = (): number =>
    Number.isFinite(speed) ? Math.min(first + (performance.now() - wallStart) * speed, last) : last;
  clock.set(virtualNow);

  dispatch({ type: "sites", sites: DEFAULT_SITES });
  for (const { name: site } of DEFAULT_SITES) {
    dispatch({ type: "connection", site, status: "fixture" });
    dispatch({ type: "info", site, info: fixtureInfo(site) });
  }

  const step = (): void => {
    if (stopped) return;
    const now = virtualNow();
    while (cursor < events.length) {
      const event = events[cursor] as SseEvent;
      if (event.ts > now) break;
      played.push(event);
      dispatch({ type: "sse", site: event.site, event });
      cursor += 1;
    }
    const next = events[cursor];
    if (next) timer = setTimeout(step, Math.max(4, (next.ts - now) / speed));
  };
  step();

  const unavailable = (): Promise<never> =>
    Promise.reject(new Error("fixture mode: commands are not sent to a site server"));

  const api: Api = {
    mode: "fixture",
    info: (site) => Promise.resolve(fixtureInfo(site)),
    runEvents: (site, run) =>
      Promise.resolve(
        played.filter((event) => {
          if (event.site !== site) return false;
          if (event.type === "init") return false;
          return event.type === "run" ? event.run.id === run : event.run === run;
        }),
      ),
    source: (_site, file) =>
      file === TRIP_BAML_FILE
        ? Promise.resolve({ file, text: TRIP_BAML })
        : Promise.reject(new Error(`fixture mode: no source for ${file}`)),
    snapshotState: (site, run, n) => {
      // A run imported by migration serves the state dumps of its origin.
      const elsewhere = Object.keys(fixture.states).find((key) => key.endsWith(`/${run}/${n}`));
      const state = fixture.states[`${site}/${run}/${n}`] ?? (elsewhere === undefined ? undefined : fixture.states[elsewhere]);
      return state ? Promise.resolve(state) : Promise.reject(new Error(`fixture mode: no state for snapshot ${n}`));
    },
    startRun: unavailable,
    command: unavailable,
  };

  return {
    api,
    stop: () => {
      stopped = true;
      if (timer !== null) clearTimeout(timer);
      clock.reset();
    },
  };
}
