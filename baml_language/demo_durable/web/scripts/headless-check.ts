// Headless render check. Loads the fixtures in Chromium, fails on any console
// error or page error, and asserts that the timeline drew what the fixture
// contains.
//
//   pnpm dev                      # in another terminal
//   pnpm check:headless           # BASE_URL, OUT_DIR, SCENES, and LIVE=1 are optional
//
// LIVE=1 also drives the central scene against the site servers behind the dev
// server's proxy (start them with ../scripts/dev.sh). It starts runs there.
// WEB_PORT selects the port of the dev server when BASE_URL is not set.
//
// Node runs this file directly by stripping its types, which needs Node 22.18
// or later. `pnpm build` type checks it.
//
// The script uses the Chromium build that `playwright-core` expects in the
// Playwright browser cache. Install it with `pnpm dlx playwright install
// chromium-headless-shell` when it is missing.

import { mkdirSync } from "node:fs";
import { join } from "node:path";
import { chromium, type Browser, type Page } from "playwright-core";

const BASE_URL = process.env.BASE_URL ?? `http://127.0.0.1:${process.env.WEB_PORT ?? 5173}`;
const OUT_DIR = process.env.OUT_DIR ?? null;
if (OUT_DIR) mkdirSync(OUT_DIR, { recursive: true });

const failures: string[] = [];
function check(scenario: string, name: string, actual: unknown, expected: unknown): void {
  const ok = JSON.stringify(actual) === JSON.stringify(expected);
  console.log(`${ok ? "  ok  " : " FAIL "} ${scenario}: ${name} = ${JSON.stringify(actual)}${ok ? "" : `, expected ${JSON.stringify(expected)}`}`);
  if (!ok) failures.push(`${scenario}: ${name}`);
}

type ColorScheme = "light" | "dark";

const DEFAULT_VIEWPORT = { width: 1280, height: 800 };
const SITES = ["local", "cloud", "cloud2"];

async function open(browser: Browser, scenario: string, path: string, colorScheme: ColorScheme = "light", viewport = DEFAULT_VIEWPORT) {
  const context = await browser.newContext({ viewport, colorScheme });
  const page = await context.newPage();
  const problems: string[] = [];
  page.on("console", (message) => {
    if (message.type() === "error") problems.push(`console.error: ${message.text()}`);
  });
  page.on("pageerror", (error) => problems.push(`pageerror: ${error.message}`));
  await page.goto(`${BASE_URL}${path}`);
  return {
    page,
    async finish() {
      if (OUT_DIR) await page.screenshot({ path: join(OUT_DIR, `${scenario}.png`) });
      check(scenario, "console and page errors", problems, []);
      await context.close();
    },
  };
}

const count = (page: Page, selector: string): Promise<number> => page.locator(selector).count();

async function timelineCounts(page: Page) {
  return {
    segments: await count(page, '[data-testid="tl-segment"]'),
    gaps: await count(page, '[data-testid="tl-gap"]'),
    threads: await count(page, '[data-testid="tl-thread"]'),
    waits: await count(page, '[data-testid="tl-wait"]'),
    call: await count(page, '[data-testid="tl-connector"][data-kind="call"]'),
    return: await count(page, '[data-testid="tl-connector"][data-kind="return"]'),
    migration: await count(page, '[data-testid="tl-connector"][data-kind="migration"]'),
    pause_request: await count(page, '[data-testid="tl-marker"][data-kind="pause_request"]'),
    blocked: await count(page, '[data-testid="tl-marker"][data-kind="blocked"]'),
    snapshot: await count(page, '[data-testid="tl-marker"][data-kind="snapshot"]'),
    resume: await count(page, '[data-testid="tl-marker"][data-kind="resume"]'),
    automatic: await count(page, '[data-testid="tl-marker"][data-kind="snapshot"][data-automatic="true"]'),
    fork_marker: await count(page, '[data-testid="tl-marker"][data-kind="fork"]'),
    fork_connector: await count(page, '[data-testid="tl-connector"][data-kind="fork"]'),
    // Contract section 9.6.
    sleeping: await count(page, '[data-testid="tl-gap"][data-kind="sleeping"]'),
    wake: await count(page, '[data-testid="tl-marker"][data-kind="wake"]'),
    thread_link: await count(page, '[data-testid="tl-thread-link"]'),
    cancel_connector: await count(page, '[data-testid="tl-connector"][data-kind="cancel"]'),
    cancel_marker: await count(page, '[data-testid="tl-marker"][data-kind="cancel"]'),
    cancelled_bar: await count(page, '[data-testid="tl-cancelled-hatch"]'),
  };
}

/** The counts of section 9.6 for a fixture that has none of them. */
const NO_PHASE3 = { sleeping: 0, wake: 0, thread_link: 0, cancel_connector: 0, cancel_marker: 0, cancelled_bar: 0 };

interface Expected {
  timeline: Awaited<ReturnType<typeof timelineCounts>>;
  runs: number;
  enabled: string[];
  /** Segment end kinds in document order. */
  ends?: string[];
  /** Number of run rows that link to the run they were forked from. */
  forks?: number;
  /** The sites whose log lane has lines. Default: local and cloud. */
  logSites?: string[];
  /** Connectors as `kind:from>to`, sorted. Checked when present. */
  connectors?: string[];
  /** Number of run rows that name a parent. Default: 1. */
  children?: number;
  /** Number of threads in the state dump of the first snapshot marker. Default: 1. */
  stateThreads?: number;
  /** The scenario whose guide the fixture brings along, or `null`. Default: not checked. */
  scenario?: string | null;
}

/** The fill of the colored stripe at the left edge of every timeline lane, by site. */
async function laneColors(page: Page): Promise<Record<string, string>> {
  const entries = await page.locator('[data-testid^="lane-"] rect.tl-bar').evaluateAll((nodes) =>
    nodes.map((node) => [(node as SVGElement).dataset.site ?? "", getComputedStyle(node).fill] as [string, string]),
  );
  return Object.fromEntries(entries);
}

/** Every timeline lane, log lane, connection chip, and legend entry, in document order. */
async function siteOrder(page: Page) {
  const testIds = (prefix: string): Promise<string[]> =>
    page.locator(`[data-testid^="${prefix}"]`).evaluateAll((nodes, p) => nodes.map((node) => ((node as HTMLElement).dataset.testid ?? "").slice(p.length)), prefix);
  return {
    timeline: await testIds("lane-"),
    logs: await testIds("log-lane-"),
    header: await testIds("conn-"),
    legend: (await page.locator('[data-testid="legend-site"]').allInnerTexts()).map((text) => text.trim()),
  };
}

async function fixtureScenario(browser: Browser, name: string, expected: Expected, colorScheme: ColorScheme) {
  const scenario = `${name}-${colorScheme}`;
  const { page, finish } = await open(browser, scenario, `/?fixture=${name}&speed=instant`, colorScheme);
  await page.waitForSelector('[data-testid="tl-segment"]');
  check(scenario, "timeline", await timelineCounts(page), expected.timeline);
  check(scenario, "run rows", await count(page, '[data-testid="run-row"]'), expected.runs);
  check(scenario, "child rows name their parent", await count(page, '[data-testid="run-row"] [data-testid="child-of"] button'), expected.children ?? 1);
  // Remote children are listed under their parent, one level deeper.
  check(scenario, "child rows are nested under a parent", await count(page, '[data-testid="run-row"][data-depth="1"]'), expected.children ?? 1);
  if (expected.scenario !== undefined) {
    check(scenario, "scenario guide", await page.locator('[data-testid="coach"]').getAttribute("data-scenario").catch(() => null), expected.scenario);
    if (expected.scenario !== null) {
      check(scenario, "the guide is finished when the whole recording was played", await page.locator('[data-testid="coach"]').getAttribute("data-finished"), "true");
      check(scenario, "the closing hint", (await page.locator('[data-testid="coach-hint"]').innerText()).startsWith("Done."), true);
    }
  }
  check(scenario, "fork rows name their source", await count(page, '[data-testid="run-row"] [data-testid="forked-from"]'), expected.forks ?? 0);
  if (expected.ends) {
    const ends = await page.locator('[data-testid="tl-segment"]').evaluateAll((nodes) => nodes.map((node) => (node as HTMLElement).dataset.endKind ?? ""));
    check(scenario, "segment end kinds", [...ends].sort(), [...expected.ends].sort());
  }
  check(scenario, "one lane per site in registry order", await siteOrder(page), { timeline: SITES, logs: SITES, header: SITES, legend: SITES });
  const withLines: string[] = [];
  for (const site of SITES) {
    if ((await count(page, `[data-testid="log-lane-${site}"] .log-line`)) > 0) withLines.push(site);
  }
  check(scenario, "log lanes with lines", withLines, expected.logSites ?? ["local", "cloud"]);
  if (expected.connectors) {
    const drawn = await page.locator('[data-testid="tl-connector"]').evaluateAll((nodes) =>
      nodes.map((node) => {
        const data = (node as SVGElement).dataset;
        return `${data.kind}:${data.fromSite}>${data.toSite}`;
      }),
    );
    check(scenario, "connectors between lanes", [...drawn].sort(), [...expected.connectors].sort());
  }
  // Each site has its own color, and the runs list, the timeline, and the log lanes agree on it.
  const colors = await laneColors(page);
  check(scenario, "three distinct lane colors", new Set(SITES.map((site) => colors[site])).size, 3);
  for (const site of SITES) {
    const logHead = await page.locator(`[data-testid="log-lane-${site}"] .panel-head`).evaluate((node) => getComputedStyle(node).borderTopColor);
    const dot = await page.locator(`[data-testid="conn-${site}"] .site-dot`).evaluate((node) => getComputedStyle(node).backgroundColor);
    check(scenario, `${site}: log lane and header use the lane color`, [logHead, dot], [colors[site], colors[site]]);
  }
  const rowColors = await page.locator('[data-testid="run-row"]').evaluateAll((nodes) =>
    nodes.map((node) => [(node as HTMLElement).dataset.site ?? "", getComputedStyle(node.querySelector(".site-dot") as Element).backgroundColor]),
  );
  check(scenario, "run rows use the lane color of their site", rowColors.every(([site, color]) => color === colors[site ?? ""]), true);
  check(scenario, "source lines with numbers > 40", (await count(page, '[data-testid="source-view"] .src-line .ln')) > 40, true);
  check(scenario, "current source line", await count(page, '[data-testid="source-view"] .src-line[data-mark="current"]'), 1);
  check(scenario, "page does not scroll", await page.evaluate(() => document.documentElement.scrollHeight <= window.innerHeight && document.documentElement.scrollWidth <= window.innerWidth), true);

  // Selecting the snapshot marker shows its PauseStats and loads the state tree.
  await page.locator('[data-testid="tl-marker"][data-kind="snapshot"]').first().click();
  await page.waitForSelector('[data-testid="state-thread"]');
  await page.mouse.move(5, 5);
  check(scenario, "pause stats shown", await count(page, '[data-testid="timeline-detail"][data-kind="snapshot"] [data-testid="pause-stats"]'), 1);
  check(scenario, "state threads", await count(page, '[data-testid="state-thread"]'), expected.stateThreads ?? 1);
  check(scenario, "state locals", (await count(page, '[data-testid="state-frame"] .key')) >= 4, true);
  check(scenario, "heap table", await count(page, '[data-testid="heap-table"]'), 1);
  await page.locator('[data-testid="tl-marker"][data-kind="resume"]').first().hover();
  check(scenario, "resume stats on hover", await count(page, '[data-testid="resume-stats"]'), 1);

  // Commands are disabled by status. The completed run can only be forked.
  const enabled = await page.locator('[data-testid="controls"] button:not([disabled])').evaluateAll((nodes) => nodes.map((node) => node.dataset.action));
  check(scenario, "enabled commands", enabled, expected.enabled);
  await finish();
}

async function playbackScenario(browser: Browser) {
  const scenario = "central-playback";
  const { page, finish } = await open(browser, scenario, "/?fixture=central&speed=3");
  // About 6.3 s of recorded time: the parent is paused and the child still runs.
  await page.waitForTimeout(2100);
  check(scenario, "follow live is on", await page.locator('[data-testid="follow-live"]').getAttribute("aria-pressed"), "true");
  check(scenario, "paused line highlighted", await count(page, '[data-testid="source-view"] .src-line[data-mark="paused"]'), 1);
  check(scenario, "open gap drawn", await count(page, '[data-testid="tl-gap"]'), 1);
  check(scenario, "state tree loads for the paused run", await count(page, '[data-testid="state-thread"]'), 1);
  const enabled = await page.locator('[data-testid="controls"] button:not([disabled])').evaluateAll((nodes) => nodes.map((node) => node.dataset.action));
  // Section 9.3: a paused run has no process to kill, and it can be cancelled on the site server.
  check(scenario, "enabled commands while paused", enabled, ["resume_here", "resume_on", "resume_on", "cancel", "fork"]);
  const labels = await page.locator('[data-testid="controls"] button').allInnerTexts();
  check(scenario, "one resume button per other site", labels.filter((label) => label.startsWith("Resume")), ["Resume here", "Resume on cloud", "Resume on cloud2"]);
  await page.locator('[data-testid="controls"] button[data-action="resume_here"]').click();
  await page.waitForSelector('[data-testid="command-error"]');
  check(scenario, "command error shown inline", (await page.locator('[data-testid="command-error"]').innerText()).includes("fixture mode"), true);
  if (OUT_DIR) await page.screenshot({ path: join(OUT_DIR, "central-paused.png") });
  await page.waitForTimeout(1600);
  check(scenario, "segments after resume", await count(page, '[data-testid="tl-segment"]'), 3);
  await finish();
}

// A pause that is answered with `blocked` events: the status stays `pausing`,
// and the latest reason is shown next to it.
async function blockedScenario(browser: Browser) {
  const scenario = "spawn-blocked";
  const { page, finish } = await open(browser, scenario, "/?fixture=spawn&speed=1");
  // The pause request is at 1.9 s of recorded time, the first blocked answer at 1.906 s.
  await page.waitForSelector('[data-testid="blocked-reason"]', { timeout: 5000 });
  const text = await page.locator('[data-testid="blocked-reason"]').innerText();
  check(scenario, "blocked reason next to the status", text.includes("a pending future cannot be written") && text.includes("×1"), true);
  check(scenario, "status is pausing", (await page.locator('[data-testid="controls"] .target .status').getAttribute("data-status")), "pausing");
  check(scenario, "the run row shows the reason too", await count(page, '[data-testid="row-blocked-reason"]'), 1);
  if (OUT_DIR) await page.screenshot({ path: join(OUT_DIR, "spawn-blocked-pausing.png") });
  // The snapshot succeeds at 3.162 s.
  await page.waitForSelector('[data-testid="controls"] .target .status[data-status="paused"]', { timeout: 5000 });
  check(scenario, "blocked reason is gone once the run is paused", await count(page, '[data-testid="blocked-reason"]'), 0);
  await finish();
}

// "Resume on <site>": one button for every site other than the one that holds
// the paused run. The chain fixture pauses the run on local and then on cloud.
async function resumeTargetsScenario(browser: Browser) {
  const scenario = "chain-resume-targets";
  // Recorded time at speed 1: the run is paused on local from 0.83 s until it moves to cloud at 2.0 s.
  const { page, finish } = await open(browser, scenario, "/?fixture=chain&speed=1");
  const resumeLabels = async (): Promise<string[]> =>
    (await page.locator('[data-testid="controls"] button').allInnerTexts()).filter((label) => label.startsWith("Resume on"));
  await page.waitForSelector('[data-testid="controls"] .target .status[data-status="paused"]', { timeout: 5000 });
  check(scenario, "a run on local can move to cloud or cloud2", await resumeLabels(), ["Resume on cloud", "Resume on cloud2"]);
  check(scenario, "targets are named on the buttons", await page.locator('[data-testid="controls"] button[data-action="resume_on"]').evaluateAll((nodes) => nodes.map((node) => (node as HTMLElement).dataset.targetSite)), ["cloud", "cloud2"]);
  // The record on cloud appears at 2.024 s and is paused at 3.43 s.
  await page.waitForSelector('[data-testid="run-row"][data-site="cloud"]', { timeout: 5000 });
  await page.locator('[data-testid="run-row"][data-site="cloud"]').first().click();
  await page.waitForSelector('[data-testid="controls"] .target .status[data-status="paused"]', { timeout: 5000 });
  check(scenario, "a run on cloud can move to local or cloud2", await resumeLabels(), ["Resume on local", "Resume on cloud2"]);
  const enabled = await page.locator('[data-testid="controls"] button:not([disabled])').evaluateAll((nodes) => nodes.map((node) => (node as HTMLElement).dataset.action));
  check(scenario, "enabled commands of the paused run on cloud", enabled, ["resume_here", "resume_on", "resume_on", "cancel", "fork"]);
  await finish();
}

// Three log lanes at 1440x900: every lane keeps a usable width, no line is cut
// off, and the timeline shows all three lanes without a scroll bar.
async function wideScenario(browser: Browser, name: string, colorScheme: ColorScheme) {
  const scenario = `${name}-1440-${colorScheme}`;
  const { page, finish } = await open(browser, scenario, `/?fixture=${name}&speed=instant`, colorScheme, { width: 1440, height: 900 });
  await page.waitForSelector('[data-testid="tl-segment"]');
  for (const site of SITES) {
    const lane = page.locator(`[data-testid="log-lane-${site}"]`);
    const box = await lane.boundingBox();
    check(scenario, `${site} log lane is at least 220 px wide`, (box?.width ?? 0) >= 220, true);
    const clipped = await lane.locator(".panel-body").evaluate((node) => node.scrollWidth > node.clientWidth + 1);
    check(scenario, `${site} log lane has no horizontal overflow`, clipped, false);
    const title = await lane.locator(".panel-title").evaluate((node) => (node as HTMLElement).scrollWidth <= (node as HTMLElement).clientWidth + 1);
    check(scenario, `${site} log lane title is not cut off`, title, true);
  }
  // In a lane of this width the time, the run, and the stream share the first row of a line.
  const metaRows = await page.locator(".log-line").evaluateAll((nodes) =>
    nodes.filter((node) => {
      // The stream label uses a smaller font, so its box starts a few pixels lower on the same row.
      const tops = [".t", ".run", ".stream"].map((selector) => (node.querySelector(selector) as HTMLElement).getBoundingClientRect().top);
      return Math.max(...tops) - Math.min(...tops) > 6;
    }).length,
  );
  check(scenario, "log lines whose time, run, and stream do not share a row", metaRows, 0);
  // The text has its own rows: at most three for a line that the app wrote, plus the row above it.
  const tallest = await page.locator('.log-line[data-stream="sys"]').evaluateAll((nodes) => Math.max(0, ...nodes.map((node) => node.getBoundingClientRect().height)));
  check(scenario, "no lifecycle line is taller than four rows", tallest <= 64, true);
  const output = await page.locator('.log-line[data-stream="stdout"] .text').evaluateAll((nodes) => nodes.every((node) => node.scrollHeight <= node.clientHeight + 1));
  check(scenario, "program output is never cut", output, true);
  check(scenario, "three lanes and two separators in the logs group", [await count(page, '.logs [data-testid^="log-lane-"]'), await count(page, ".logs .split-sep")], [3, 2]);
  const plotScrolls = await page.locator(".timeline-plot").evaluate((node) => node.scrollHeight > node.clientHeight + 1);
  check(scenario, "the timeline shows its three lanes without scrolling", plotScrolls, false);
  check(scenario, "page does not scroll", await page.evaluate(() => document.documentElement.scrollHeight <= window.innerHeight && document.documentElement.scrollWidth <= window.innerWidth), true);
  await finish();
}

// A layout that was saved for the two log lanes of the two-site app must not
// reach the three lanes, and a resized three-lane layout must survive a reload.
async function layoutPersistenceScenario(browser: Browser) {
  const scenario = "logs-layout-persistence";
  const context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  const page = await context.newPage();
  const problems: string[] = [];
  page.on("console", (message) => {
    if (message.type() === "error") problems.push(`console.error: ${message.text()}`);
  });
  page.on("pageerror", (error) => problems.push(`pageerror: ${error.message}`));
  // The layout that the two-site app saved: two panes named `log-local` and `log-cloud`.
  await page.addInitScript(() => {
    if (window.localStorage.getItem("seeded") !== null) return;
    window.localStorage.setItem("seeded", "1");
    window.localStorage.setItem("react-resizable-panels:durable-demo:logs:log-local:log-cloud", JSON.stringify({ "log-local": 85, "log-cloud": 15 }));
    window.localStorage.setItem("react-resizable-panels:durable-demo:logs", JSON.stringify({ "log-local,log-cloud": { layout: [85, 15] } }));
  });
  await page.goto(`${BASE_URL}/?fixture=pool&speed=instant`);
  await page.waitForSelector('[data-testid="tl-segment"]');
  const widths = async (): Promise<number[]> => {
    const result: number[] = [];
    for (const site of SITES) result.push(Math.round((await page.locator(`[data-testid="log-lane-${site}"]`).boundingBox())?.width ?? 0));
    return result;
  };
  const initial = await widths();
  check(scenario, "an old two-lane layout leaves three equal lanes", Math.max(...initial) - Math.min(...initial) <= 2 && Math.min(...initial) > 0, true);

  // Drag the first separator of the logs group 60 px to the right.
  const separator = page.locator(".logs .split-sep").first();
  const box = await separator.boundingBox();
  if (box === null) throw new Error("the logs group has no separator");
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2 + 60, box.y + box.height / 2, { steps: 6 });
  await page.mouse.up();
  await page.waitForTimeout(300);
  const resized = await widths();
  check(scenario, "dragging a separator resizes the first two lanes", [Math.abs((resized[0] ?? 0) - (initial[0] ?? 0) - 60) <= 3, Math.abs((resized[1] ?? 0) - (initial[1] ?? 0) + 60) <= 3], [true, true]);
  await page.reload();
  await page.waitForSelector('[data-testid="tl-segment"]');
  check(scenario, "the three-lane layout survives a reload", (await widths()).map((width, index) => Math.abs(width - (resized[index] ?? 0)) <= 2), [true, true, true]);
  check(scenario, "the other groups still restore", await count(page, ".split-sep"), 6);
  if (OUT_DIR) await page.screenshot({ path: join(OUT_DIR, `${scenario}.png`) });
  check(scenario, "console and page errors", problems, []);
  await context.close();
}

// The fork fixture: selecting the fork shows the same tree, and the fork marker has a detail view.
async function forkScenario(browser: Browser) {
  const scenario = "fork-selection";
  const { page, finish } = await open(browser, scenario, "/?fixture=fork&speed=instant");
  await page.waitForSelector('[data-testid="tl-marker"][data-kind="fork"]');
  await page.locator('[data-testid="tl-marker"][data-kind="fork"]').click();
  await page.mouse.move(5, 5);
  check(scenario, "fork marker detail", (await page.locator('[data-testid="timeline-detail"][data-kind="fork"]').innerText()).includes("forked from"), true);
  await page.locator('[data-testid="run-row"] [data-testid="forked-from"]').locator("xpath=ancestor::tr").click();
  await page.waitForSelector('[data-testid="controls"] .target .status[data-status="cancelled"]');
  const counts = await timelineCounts(page);
  check(scenario, "the fork's tree contains its source", { segments: counts.segments, fork_marker: counts.fork_marker, fork_connector: counts.fork_connector }, { segments: 4, fork_marker: 1, fork_connector: 1 });
  check(scenario, "log line for the cancel", await page.locator('[data-testid="log-lane-local"] .log-line', { hasText: "run cancelled" }).count(), 1);
  check(scenario, "log line for the lost process", await page.locator('[data-testid="log-lane-local"] .log-line', { hasText: "ended without a terminal event (signal SIGKILL)" }).count(), 1);
  await page.locator('[data-testid="run-row"] [data-testid="forked-from"] button').click();
  await page.waitForSelector('[data-testid="controls"] .target .status[data-status="completed"]');
  check(scenario, "the fork link selects the source", await page.locator('[data-testid="run-row"][aria-selected="true"]').getAttribute("data-run"), "r-k9d2fw");
  await finish();
}

// Zoom buttons, ctrl+wheel zoom, drag pan, and Fit, measured on the first segment bar.
async function interactionScenario(browser: Browser) {
  const scenario = "timeline-interaction";
  const { page, finish } = await open(browser, scenario, "/?fixture=central&speed=instant");
  await page.waitForSelector('[data-testid="tl-segment"]');
  const bar = async () => {
    const box = await page.locator('[data-testid="tl-segment"] rect').first().boundingBox();
    if (box === null) throw new Error("the first segment bar is not visible");
    return { x: Math.round(box.x), width: Math.round(box.width) };
  };
  const followLive = () => page.locator('[data-testid="follow-live"]').getAttribute("aria-pressed");
  const fitted = await bar();
  const plot = await page.locator('[data-testid="timeline"] svg[role="img"]').boundingBox();
  if (plot === null) throw new Error("the timeline plot is not visible");

  await page.locator('[data-testid="timeline"] button[aria-label="Zoom in"]').click();
  const zoomedIn = await bar();
  check(scenario, "zoom in widens the bar", zoomedIn.width > fitted.width * 1.4, true);
  check(scenario, "zoom turns follow live off", await followLive(), "false");

  await page.locator('[data-testid="timeline"] button[aria-label="Zoom out"]').click();
  check(scenario, "zoom out restores the width", Math.abs((await bar()).width - fitted.width) <= 2, true);

  // Ctrl+wheel zooms around the cursor: the point under the cursor keeps its place.
  const cursorX = fitted.x + fitted.width;
  await page.mouse.move(cursorX, plot.y + plot.height - 6);
  await page.keyboard.down("Control");
  await page.mouse.wheel(0, -200);
  await page.keyboard.up("Control");
  await page.waitForTimeout(100);
  const wheeled = await bar();
  check(scenario, "ctrl+wheel widens the bar", wheeled.width > fitted.width * 1.4, true);
  check(scenario, "ctrl+wheel keeps the time under the cursor", Math.abs(wheeled.x + wheeled.width - cursorX) <= 2, true);

  // Drag pans by the distance of the drag.
  await page.mouse.down();
  await page.mouse.move(cursorX - 60, plot.y + plot.height - 6, { steps: 4 });
  await page.mouse.up();
  const panned = await bar();
  check(scenario, "drag pans by the drag distance", Math.abs(panned.x - (wheeled.x - 60)) <= 2, true);
  check(scenario, "drag keeps the zoom", Math.abs(panned.width - wheeled.width) <= 2, true);

  await page.locator('[data-testid="timeline"] button', { hasText: "Fit" }).click();
  check(scenario, "Fit restores the fitted view", await bar(), fitted);

  await page.locator('[data-testid="follow-live"]').click();
  check(scenario, "follow live can be turned on again", await followLive(), "true");
  await finish();
}

// ---------------------------------------------------------------------------
// Contract section 9.6: durable sleep, cancellation, threads, futures, scenarios
// ---------------------------------------------------------------------------

const shot = async (page: Page, name: string): Promise<void> => {
  if (OUT_DIR) await page.screenshot({ path: join(OUT_DIR, `${name}.png`) });
};
const enabledActions = (page: Page): Promise<(string | undefined)[]> =>
  page.locator('[data-testid="controls"] button:not([disabled])').evaluateAll((nodes) => nodes.map((node) => (node as HTMLElement).dataset.action));

// The fan-out while it sleeps: the gap has its own style, names the wake time,
// and counts down. No parent process exists, and the children keep running.
async function sleepingScenario(browser: Browser, colorScheme: ColorScheme) {
  const scenario = `fanout-sleeping-${colorScheme}`;
  const { page, finish } = await open(browser, scenario, "/?fixture=fanout&speed=2", colorScheme, { width: 1440, height: 900 });
  const gap = '[data-testid="tl-gap"][data-kind="sleeping"][data-open="true"]';
  await page.waitForSelector(gap, { timeout: 5000 });
  const label = page.locator(`${gap} [data-testid="tl-sleep-label"]`);
  const first = (await label.textContent()) ?? "";
  check(scenario, "the gap is labeled sleeping until <time> with a countdown", /^sleeping until \d\d:\d\d:\d\d · \d+\.\d s$/.test(first), true);
  const seconds = (text: string): number => Number(/· (\d+\.\d) s$/.exec(text)?.[1] ?? Number.NaN);
  await page.waitForTimeout(700);
  const second = (await label.textContent()) ?? "";
  check(scenario, "the countdown runs", seconds(second) < seconds(first), true);
  // The band reaches the wake time, which is to the right of the now line.
  const geometry = await page.evaluate((selector) => {
    const band = document.querySelector(`${selector} rect.tl-sleep`)?.getBoundingClientRect();
    const now = document.querySelector(".tl-now")?.getBoundingClientRect();
    return band && now ? { bandRight: band.right, now: now.left, bandLeft: band.left } : null;
  }, gap);
  check(scenario, "the sleeping band reaches past now, to the wake time", geometry !== null && geometry.bandLeft < geometry.now && geometry.now < geometry.bandRight, true);
  check(scenario, "no process bar of the parent is open", await count(page, '[data-testid="tl-segment"][data-run="r-fn7q2a"][data-end-kind="open"]'), 0);
  check(scenario, "children run while the parent sleeps", (await count(page, '[data-testid="tl-segment"][data-end-kind="open"]')) >= 1, true);
  check(scenario, "status of the run", await page.locator('[data-testid="controls"] .target .status').getAttribute("data-status"), "sleeping");
  check(scenario, "wake time and countdown next to the status", /^wakes \d\d:\d\d:\d\d · in \d+\.\d s$/.test(await page.locator('[data-testid="controls"] [data-testid="wake-line"]').innerText()), true);
  check(scenario, "wake time in the run's row", await count(page, '[data-testid="run-row"][data-status="sleeping"] [data-testid="wake-line"]'), 1);
  // Section 9.3: a sleeping run can be resumed early and cancelled. It has no process to pause or kill.
  check(scenario, "enabled commands while sleeping", await enabledActions(page), ["resume_here", "resume_on", "resume_on", "cancel", "fork"]);
  check(scenario, "program hash next to the status", /^prog [0-9a-f]{10}$/.test(await page.locator('[data-testid="program-hash"]').innerText()), true);
  // The state tree shows the snapshot of a sleeping run without a click: five threads.
  await page.waitForSelector('[data-testid="state-thread"]');
  check(scenario, "state threads of the sleeping run", await count(page, '[data-testid="state-thread"]'), 5);
  check(scenario, "how the threads are parked", (await page.locator('[data-testid="state-summary"]').innerText()).includes("1 sleep, 4 remote_call"), true);
  check(scenario, "four pending futures", await count(page, '[data-testid="state-future"][data-future="pending"]'), 4);
  // The guide follows the event stream.
  check(scenario, "the guide is on the sleep step", await page.locator('[data-testid="coach"]').getAttribute("data-step"), "sleep");
  const hint = await page.locator('[data-testid="coach-hint"]').innerText();
  check(scenario, "the hint says that no process exists, and counts", hint.includes("no parent process exists while the run sleeps") && /\d of 4 children have finished/.test(hint) && /wakes the run in \d+\.\d s/.test(hint), true);
  // The gap has a detail panel.
  await page.locator(`${gap} .tl-gap-hit`).hover();
  check(scenario, "detail of the sleeping gap", await page.locator('[data-testid="timeline-detail"][data-kind="gap"]').getAttribute("data-gap-kind"), "sleeping");
  check(scenario, "the detail names the wake time", (await page.locator('[data-testid="timeline-detail"]').innerText()).includes("wake at"), true);
  await page.mouse.move(5, 5);
  await shot(page, `fanout-asleep-${colorScheme}`);

  // The timer resumes the run: the countdown ends, the gap closes, and the threads continue.
  await page.waitForSelector('[data-testid="tl-marker"][data-kind="wake"]', { timeout: 8000 });
  await page.waitForSelector('[data-testid="controls"] .target .status[data-status="completed"]', { timeout: 5000 });
  check(scenario, "the gap is closed after the wake", [await count(page, gap), await count(page, '[data-testid="tl-gap"][data-kind="sleeping"][data-open="false"]')], [0, 1]);
  check(scenario, "the closed gap states how long the run slept", ((await page.locator('[data-testid="tl-sleep-label"]').textContent()) ?? "").startsWith("slept 12.02 s"), true);
  check(scenario, "no wake line once the run is awake", await count(page, '[data-testid="wake-line"]'), 0);
  await page.locator('[data-testid="tl-marker"][data-kind="wake"]').click();
  await page.mouse.move(5, 5);
  check(scenario, "detail of the wake marker", (await page.locator('[data-testid="timeline-detail"][data-kind="wake"]').innerText()).includes("woken by"), true);
  check(scenario, "the guide is finished", await page.locator('[data-testid="coach"]').getAttribute("data-finished"), "true");
  await finish();
}

// Threads that continue across a suspension keep their line, and the resume marker names the program source.
async function threadsScenario(browser: Browser) {
  const scenario = "fanout-threads";
  const { page, finish } = await open(browser, scenario, "/?fixture=fanout&speed=instant", "light", { width: 1440, height: 900 });
  // A link is a horizontal line, which has no height, so it counts as attached, not as visible.
  await page.waitForSelector('[data-testid="tl-thread-link"]', { state: "attached" });
  const threads = await page.locator('[data-testid="tl-thread"]').evaluateAll((nodes) =>
    nodes.map((node) => {
      const data = (node as SVGElement).dataset;
      return { thread: Number(data.thread), segment: Number(data.segment), subRow: Number(data.subRow), continued: data.continued === "true", continues: data.continues === "true", y: Math.round(node.querySelector("rect")?.getBoundingClientRect().top ?? -1) };
    }),
  );
  for (const id of [2, 3, 4, 5]) {
    const bars = threads.filter((thread) => thread.thread === id);
    check(scenario, `thread ${id} has a bar in both segments on one line`, [bars.map((bar) => bar.segment), bars[0]?.continues, bars[1]?.continued, bars[0]?.y === bars[1]?.y], [[1, 2], true, true, true]);
  }
  check(scenario, "four thread lines", new Set(threads.filter((thread) => thread.thread <= 5).map((thread) => thread.y)).size, 4);
  const links = await page.locator('[data-testid="tl-thread-link"] line.tl-thread-link').evaluateAll((nodes) => nodes.map((node) => Math.round(node.getBoundingClientRect().top)));
  check(scenario, "each link is on the line of its thread", links.every((y) => threads.some((thread) => Math.abs(thread.y + 3 - y) <= 2)), true);
  await page.locator('[data-testid="tl-marker"][data-kind="resume"]').hover();
  check(scenario, "program source in the resume timings", await page.locator('[data-testid="program-source"]').getAttribute("data-source"), "store");
  check(scenario, "program hash in every run row", await count(page, '[data-testid="row-program"]'), 5);
  // The children fold away under their parent.
  await page.locator('[data-testid="group-toggle"]').click();
  check(scenario, "collapsed group", [await count(page, '[data-testid="run-row"]'), await page.locator('[data-testid="group-toggle"]').getAttribute("aria-expanded")], [1, "false"]);
  await page.locator('[data-testid="group-toggle"]').click();
  check(scenario, "expanded group", await count(page, '[data-testid="run-row"]'), 5);
  await finish();
}

// Children that run at the same time in one lane get rows of their own, and no label box touches another.
async function stackingScenario(browser: Browser) {
  const scenario = "race-stacking";
  const { page, finish } = await open(browser, scenario, "/?fixture=race&speed=instant", "light", { width: 1440, height: 900 });
  await page.waitForSelector('[data-testid="tl-segment"]');
  const boxes = await page.locator('[data-testid="tl-segment"] text.tl-label').evaluateAll((nodes) =>
    nodes.map((node) => {
      const box = node.getBoundingClientRect();
      return { text: node.textContent ?? "", left: box.left, right: box.right, top: box.top, bottom: box.bottom };
    }),
  );
  const overlaps = boxes.flatMap((a, i) => boxes.slice(i + 1).filter((b) => !(a.right <= b.left || b.right <= a.left || a.bottom <= b.top || b.bottom <= a.top)).map((b) => `${a.text} / ${b.text}`));
  check(scenario, "bar labels that overlap", overlaps, []);
  const rows = await page.locator('[data-testid="tl-segment"] rect.tl-bar').evaluateAll((nodes) => nodes.map((node) => `${(node as SVGElement).dataset.site}:${Math.round(node.getBoundingClientRect().top)}`));
  check(scenario, "two children of one lane are on rows of their own", new Set(rows.filter((row) => row.startsWith("cloud:"))).size, 2);

  // The cancelled children: their own style, a connector from the parent, and a marker with a detail panel.
  const hatched = await page.locator('[data-testid="tl-segment"][data-end-kind="cancelled"]').evaluateAll((nodes) => nodes.map((node) => [(node as SVGElement).dataset.run ?? "", node.querySelector('[data-testid="tl-cancelled-hatch"]') !== null] as [string, boolean]));
  check(scenario, "cancelled bars carry the cancelled style", hatched.sort(), [["r-qs7a1r", true], ["r-qu4z9s", true]]);
  check(scenario, "a completed bar does not", await count(page, '[data-testid="tl-segment"][data-end-kind="completed"] [data-testid="tl-cancelled-hatch"]'), 0);
  const stroke = await page.locator('[data-testid="tl-connector"][data-kind="cancel"] path.tl-connector').first().evaluate((node) => [getComputedStyle(node).stroke, getComputedStyle(node).strokeDasharray]);
  const callStroke = await page.locator('[data-testid="tl-connector"][data-kind="call"] path.tl-connector').first().evaluate((node) => [getComputedStyle(node).stroke, getComputedStyle(node).strokeDasharray]);
  check(scenario, "a cancel connector does not look like a call connector", stroke[0] !== callStroke[0] && stroke[1] !== callStroke[1], true);
  await page.locator('[data-testid="tl-marker"][data-kind="cancel"]').first().click();
  await page.mouse.move(5, 5);
  const detail = await page.locator('[data-testid="timeline-detail"][data-kind="cancel"]').innerText();
  check(scenario, "detail of the cancel marker names the call and the child", detail.includes("r-rc3w8n-c2") && detail.includes("cloud2/r-qs7a1r") && detail.includes("race loser"), true);
  await page.locator('[data-testid="tl-connector"][data-kind="cancel"] .tl-connector-hit').first().hover({ force: true });
  check(scenario, "detail of the cancel connector", (await page.locator('[data-testid="timeline-detail"][data-kind="cancel"] h4').innerText()).includes("remote cancel"), true);
  await finish();
}

// The state tree: futures in three states with their values, a cancel token, enums, maps, and nested instances.
async function stateTreeScenario(browser: Browser, colorScheme: ColorScheme) {
  const scenario = `state-tree-${colorScheme}`;
  const { page, finish } = await open(browser, scenario, "/?fixture=settled&speed=instant", colorScheme, { width: 1440, height: 1000 });
  await page.waitForSelector('[data-testid="tl-marker"][data-kind="snapshot"]');
  await page.locator('[data-testid="tl-marker"][data-kind="snapshot"]').click();
  await page.waitForSelector('[data-testid="state-thread"]');
  await page.mouse.move(5, 5);
  check(scenario, "three threads", [await count(page, '[data-testid="state-thread"]'), (await page.locator('[data-testid="state-summary"]').innerText()).includes("2 await, 1 remote_call")], [3, true]);
  const futures = await page.locator('[data-testid="state-thread"]').first().locator('[data-testid="state-future"]').evaluateAll((nodes) => nodes.map((node) => (node as HTMLElement).dataset.future));
  check(scenario, "futures of the main thread, by state", futures, ["resolved", "failed", "pending"]);
  const resolved = page.locator('[data-testid="state-future"][data-future="resolved"]').first();
  check(scenario, "a resolved future shows its value", (await resolved.innerText()).includes("Quote"), true);
  check(scenario, "a failed future shows its error", (await page.locator('[data-testid="state-future"][data-future="failed"]').first().innerText()).includes("QuoteUnavailable"), true);
  // Open the resolved future: a class instance with an enum, a nested class, a map, and an optional field.
  await resolved.click();
  const open1 = page.locator('[data-testid="state-thread"]').first().locator("details details[open]").filter({ has: page.locator('[data-future="resolved"]') }).first();
  await open1.locator("summary", { hasText: "value" }).first().click();
  const tree = await page.locator('[data-testid="state-tree"] .tree').innerText();
  check(scenario, "the value of the future: enum, nested class, map, optional field", ["QuoteRequest {", '"Skyways"', "Money {", "amount", '"insurance": 12', '["flight", "lisbon"]', "note"].every((part) => tree.includes(part)), true);
  // The request inside the quote: an enum, a nested class with an optional field, and a map.
  await open1.locator("summary", { hasText: "request" }).first().click();
  const nested = await page.locator('[data-testid="state-tree"] .tree').innerText();
  check(scenario, "the nested request: enum and nested class", ["QuoteKind.", "Flight", "Traveler {", "loyalty_tier"].every((part) => nested.includes(part)), true);
  // A map lists its entries under quoted keys.
  await page.locator('[data-testid="state-map"]').first().click();
  check(scenario, "map entries under quoted keys", (await page.locator('[data-testid="state-tree"] .map-key').allInnerTexts()).filter((text) => text !== ""), ['"currency"', '"simulate"']);
  await page.locator('[data-testid="state-tree"] .panel-body').evaluate((node) => { node.scrollLeft = 0; });
  check(scenario, "an enum variant is rendered as one", (await count(page, '[data-testid="state-enum"]')) >= 1, true);
  await shot(page, `settled-state-tree-${colorScheme}`);
  await finish();

  const token = `cancel-token-${colorScheme}`;
  const second = await open(browser, token, "/?fixture=deadline&speed=instant", colorScheme, { width: 1440, height: 1000 });
  await second.page.waitForSelector('[data-testid="tl-marker"][data-kind="snapshot"]');
  await second.page.locator('[data-testid="tl-marker"][data-kind="snapshot"]').click();
  await second.page.waitForSelector('[data-testid="state-cancel-token"]');
  await second.page.mouse.move(5, 5);
  check(token, "the cancel token of with_timeout, not cancelled yet", (await second.page.locator('[data-testid="state-cancel-token"]').first().innerText()).includes("armed"), true);
  check(token, "three threads: await, remote_call, sleep", await second.page.locator('[data-testid="state-thread"]').evaluateAll((nodes) => nodes.map((node) => (node as HTMLElement).dataset.parked)), ["await", "remote_call", "sleep"]);
  await shot(second.page, `deadline-state-tree-${colorScheme}`);
  await second.finish();
}

// The scenario gallery: seven cards, the autoplay switch, and the links to the recordings.
async function galleryScenario(browser: Browser, colorScheme: ColorScheme) {
  const scenario = `gallery-${colorScheme}`;
  const { page, finish } = await open(browser, scenario, "/?fixture=central&speed=instant", colorScheme, { width: 1440, height: 900 });
  await page.waitForSelector('[data-testid="tl-segment"]');
  check(scenario, "a fixture that no scenario plays has no guide", await count(page, '[data-testid="coach"]'), 0);
  await page.locator('[data-testid="open-scenarios"]').click();
  await page.waitForSelector('[data-testid="scenario-gallery"]');
  const ids = await page.locator('[data-testid="scenario-card"]').evaluateAll((nodes) => nodes.map((node) => (node as HTMLElement).dataset.scenario));
  check(scenario, "the seven scenarios", ids, ["migrate", "fanout", "race", "deadline", "settled", "recover", "fork"]);
  const cards = await page.locator('[data-testid="scenario-card"]').evaluateAll((nodes) =>
    nodes.map((node) => ({
      title: (node.querySelector("h3")?.textContent ?? "").length > 0,
      summary: (node.querySelector("p")?.textContent ?? "").length > 120,
      starts: /^[a-z_]+\(\{.*\}\) on local$/.test((node.querySelector(".scenario-starts")?.textContent ?? "").trim()),
      steps: node.querySelectorAll(".scenario-steps li").length >= 3,
      picture: node.querySelectorAll("svg.mini-tl rect.tl-bar").length >= 2,
    })),
  );
  check(scenario, "every card has a title, a paragraph, the function and its arguments, steps, and a picture", cards.every((card) => Object.values(card).every(Boolean)), true);
  check(scenario, "a recording cannot start runs", await page.locator('[data-testid="scenario-run"]').evaluateAll((nodes) => nodes.every((node) => (node as HTMLButtonElement).disabled)), true);
  const hrefs = await page.locator('[data-testid="scenario-play"]').evaluateAll((nodes) => nodes.map((node) => node.getAttribute("href")));
  check(scenario, "links to the recordings", hrefs, ["pool", "fanout", "race", "deadline", "settled", "recover", "branch"].map((name) => `?fixture=${name}&speed=2`));
  await page.locator('[data-testid="scenario-gallery"] [data-testid="autoplay"]').click();
  check(scenario, "the autoplay switch", await page.locator('[data-testid="scenario-gallery"] [data-testid="autoplay"]').getAttribute("aria-checked"), "true");
  check(scenario, "autoplay is part of the recording link", await page.locator('[data-testid="scenario-play"]').first().getAttribute("href"), "?fixture=pool&speed=2&autoplay=1");
  check(scenario, "the sheet fits the window", await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth), true);
  await shot(page, `gallery-${colorScheme}`);
  await page.keyboard.press("Escape");
  check(scenario, "Escape closes the sheet", await count(page, '[data-testid="scenario-gallery"]'), 0);
  // The preference survives a reload.
  await page.reload();
  await page.locator('[data-testid="open-scenarios"]').click();
  check(scenario, "autoplay survives a reload", await page.locator('[data-testid="scenario-gallery"] [data-testid="autoplay"]').getAttribute("aria-checked"), "true");
  // A card plays its recording.
  await page.locator('[data-testid="scenario-card"][data-scenario="race"] [data-testid="scenario-play"]').click();
  await page.waitForSelector('[data-testid="coach"][data-scenario="race"]');
  check(scenario, "the recording brings its guide and keeps autoplay", await page.locator('[data-testid="coach"] [data-testid="autoplay"]').getAttribute("aria-checked"), "true");
  await finish();
}

// The guide points at the control of the current step. With autoplay it shows that it presses it.
async function guideScenario(browser: Browser) {
  const scenario = "guide-race";
  const { page, finish } = await open(browser, scenario, "/?fixture=race&speed=1&autoplay=0", "light", { width: 1440, height: 900 });
  const coached = '[data-testid="controls"] button[data-coach]';
  await page.waitForSelector(`${coached}[data-action="pause"]`, { timeout: 5000 });
  check(scenario, "without autoplay the pause button is pointed at", await page.locator(coached).evaluateAll((nodes) => nodes.map((node) => [(node as HTMLElement).dataset.action, (node as HTMLElement).dataset.coach])), [["pause", "hint"]]);
  const hint = await page.locator('[data-testid="coach-hint"]').innerText();
  check(scenario, "the hint asks for the pause and names the sites", hint.includes("Click Pause now: three children are running on cloud and cloud2"), true);
  check(scenario, "the hint is marked optional", hint.toLowerCase().startsWith("optional"), true);
  const ring = await page.locator(coached).evaluate((node) => getComputedStyle(node).borderTopColor);
  const plain = await page.locator('[data-testid="controls"] button[data-action="fork"]').evaluate((node) => getComputedStyle(node).borderTopColor);
  check(scenario, "the highlighted button looks different", ring !== plain, true);
  await shot(page, "race-guide-pause");
  // The recording pauses at 1.3 s and moves the run at 3.4 s. In between the guide points at "Resume on cloud2".
  await page.waitForSelector(`${coached}[data-action="resume_on"][data-target-site="cloud2"]`, { timeout: 5000 });
  check(scenario, "only one control is highlighted at a time", await count(page, coached), 1);
  await page.locator('[data-testid="coach"] [data-testid="autoplay"]').click();
  check(scenario, "with autoplay the highlight announces the press", await page.locator(coached).getAttribute("data-coach"), "press");
  check(scenario, "a recording sends no command", await count(page, '[data-testid="command-error"]'), 0);
  await shot(page, "race-guide-resume-autoplay");
  await page.waitForSelector('[data-testid="coach"][data-finished="true"]', { timeout: 8000 });
  check(scenario, "no control is highlighted at the end", await count(page, coached), 0);
  check(scenario, "every step is done", await page.locator(".coach-steps li").evaluateAll((nodes) => nodes.map((node) => (node as HTMLElement).dataset.state)), ["done", "done", "done", "done", "done"]);
  // The guide can be closed, and the app stays as it is.
  await page.locator('[data-testid="coach"] button', { hasText: "Exit" }).click();
  check(scenario, "Exit closes the guide", await count(page, '[data-testid="coach"]'), 0);
  await finish();

  // The recovery scenario follows a second run: the guide selects it when its step comes.
  const recover = "guide-recover";
  const second = await open(browser, recover, "/?fixture=recover&speed=4&autoplay=1", "light", { width: 1440, height: 900 });
  await second.page.waitForSelector('[data-testid="coach"][data-step="kill-plain"][data-ready="true"]', { timeout: 10000 });
  check(recover, "the guide selected the plain run for its kill", await second.page.locator('[data-testid="run-row"][aria-selected="true"]').getAttribute("data-run"), "r-rp4n7t");
  check(recover, "the kill button is highlighted", await second.page.locator(coached).getAttribute("data-action"), "kill");
  await shot(second.page, "recover-guide-kill-plain");
  await second.page.waitForSelector('[data-testid="coach"][data-finished="true"]', { timeout: 8000 });
  check(recover, "the closing hint states the difference", (await second.page.locator('[data-testid="coach-hint"]').innerText()).includes("The plain run is lost"), true);
  await second.finish();
}

// The live path against the site servers, with the mock or the real worker (LIVE=1).
// It starts runs on the servers behind BASE_URL. The scene is the central one:
// the parent is paused while its remote child runs, the child completes, and
// the parent resumes in a new process.
async function liveScenario(browser: Browser) {
  const scenario = "live";
  const { page, finish } = await open(browser, scenario, "/");
  const controls = '[data-testid="controls"]';
  const waitStatus = (status: string, timeout = 30000) =>
    page.waitForFunction(
      ({ selector, text }) => document.querySelector(`${selector} .target`)?.textContent?.includes(text) ?? false,
      { selector: controls, text: status },
      { timeout },
    );
  const start = async (fn: string): Promise<void> => {
    await page.selectOption('[data-testid="fn-select"]', fn);
    await page.click('[data-testid="start"]');
    await waitStatus("running");
  };
  const commandErrors = () => page.locator('[data-testid="command-error"]').allInnerTexts();

  // The scene that needs `blocked` answers depends on the worker behind the servers.
  const info = (await (await fetch(`${BASE_URL}/local/api/info`)).json()) as { worker_cmd?: string[]; sites?: { name: string }[] };
  const liveSites = (info.sites ?? []).map((site) => site.name);
  check(scenario, "GET /local/api/info names the site registry", liveSites.length >= 2, true);
  for (const site of liveSites) {
    await page.waitForSelector(`[data-testid="conn-${site}"][data-status="open"]`, { timeout: 10000 });
  }
  check(scenario, "one lane per site in registry order", await siteOrder(page).then((order) => [order.logs, order.header]), [liveSites, liveSites]);
  await page.waitForSelector('[data-testid="fn-select"]');
  const mockWorker = (info.worker_cmd ?? []).some((part) => part.includes("mock-worker"));
  console.log(`  live: the site servers use the ${mockWorker ? "mock" : "real"} worker`);
  check(scenario, "function picker lists the demo functions", (await count(page, '[data-testid="fn-select"] option')) >= 4, true);

  await start("durable_plan_trip");
  await page.waitForSelector('[data-testid="tl-connector"][data-kind="call"]', { timeout: 30000 });
  await page.click(`${controls} button[data-action="pause"]`);
  await waitStatus("paused");
  await page.waitForSelector('[data-testid="state-thread"]');
  check(scenario, "paused line highlighted", await count(page, '[data-testid="source-view"] .src-line[data-mark="paused"]'), 1);
  const localNames = await page.locator('[data-testid="state-frame"] .key').allInnerTexts();
  check(scenario, "state tree names the locals city, ideas, and day", ["city", "ideas", "day"].every((name) => localNames.some((text) => text.includes(name))), true);
  check(scenario, "heap table", await count(page, '[data-testid="heap-table"]'), 1);
  if (OUT_DIR) await page.screenshot({ path: join(OUT_DIR, "live-paused.png") });
  check(scenario, "open gap drawn", await count(page, '[data-testid="tl-gap"]'), 1);
  const targets = await page.locator(`${controls} button[data-action="resume_on"]`).evaluateAll((nodes) => nodes.map((node) => (node as HTMLElement).dataset.targetSite ?? ""));
  check(scenario, "one resume button per other site", targets, liveSites.filter((site) => site !== "local"));
  // The child completes while the parent has no process: the return connector is drawn pending.
  await page.waitForSelector('[data-testid="tl-connector"][data-kind="return"]', { timeout: 15000 });

  // A reload in the paused state rebuilds the timeline from GET /api/runs/:id/events.
  await page.reload();
  await page.waitForSelector('[data-testid="tl-connector"][data-kind="return"]');
  const reloaded = await timelineCounts(page);
  check(scenario, "timeline after a reload", { segments: reloaded.segments, gaps: reloaded.gaps, call: reloaded.call, return: reloaded.return, pause_request: reloaded.pause_request }, { segments: 2, gaps: 1, call: 1, return: 1, pause_request: 1 });

  await page.click(`${controls} button[data-action="resume_here"]`);
  await waitStatus("completed");
  const final = await timelineCounts(page);
  check(scenario, "timeline after the resume", { segments: final.segments, gaps: final.gaps, waits: final.waits, call: final.call, return: final.return, resume: final.resume }, { segments: 3, gaps: 1, waits: 1, call: 1, return: 1, resume: 1 });
  check(scenario, "no command error", await commandErrors(), []);
  check(scenario, "both log lanes have lines", [(await count(page, '[data-testid="log-lane-local"] .log-line')) > 0, (await count(page, '[data-testid="log-lane-cloud"] .log-line')) > 0], [true, true]);
  if (OUT_DIR) await page.screenshot({ path: join(OUT_DIR, "live-central.png") });

  // A durable run that loses its process after an automatic snapshot becomes
  // paused. The end of its bar comes from `worker_exit`, so a reload keeps it.
  await start("durable_plan_trip");
  await page.waitForSelector('[data-testid="tl-marker"][data-kind="snapshot"][data-automatic="true"]', { timeout: 20000 });
  await page.click(`${controls} button[data-action="kill"]`);
  await waitStatus("paused");
  const killed = '[data-testid="tl-segment"][data-end-kind="killed"]';
  await page.waitForSelector(killed);
  const killedEnd = await page.locator(killed).getAttribute("data-end");
  await page.reload();
  await page.waitForSelector(killed);
  check(scenario, "the kill time survives a reload", await page.locator(killed).getAttribute("data-end"), killedEnd);
  check(scenario, "automatic snapshots after a reload > 0", (await count(page, '[data-testid="tl-marker"][data-kind="snapshot"][data-automatic="true"]')) > 0, true);

  // Fork the killed run, resume the fork, and cancel it.
  await page.click(`${controls} button[data-action="fork"]`);
  await page.waitForSelector('[data-testid="tl-marker"][data-kind="fork"]');
  check(scenario, "the fork row names its source", await count(page, '[data-testid="run-row"][aria-selected="true"] [data-testid="forked-from"]'), 1);
  check(scenario, "fork connector drawn", await count(page, '[data-testid="tl-connector"][data-kind="fork"]'), 1);
  await page.click(`${controls} button[data-action="resume_here"]`);
  await waitStatus("running");
  await page.click(`${controls} button[data-action="cancel"]`);
  await waitStatus("cancelled");
  await page.waitForSelector('[data-testid="tl-segment"][data-end-kind="cancelled"]');
  check(scenario, "cancelled segment drawn", await count(page, '[data-testid="tl-segment"][data-end-kind="cancelled"]'), 1);
  check(scenario, "log line for the cancel", (await page.locator('[data-testid="log-lane-local"] .log-line', { hasText: "run cancelled" }).count()) >= 1, true);

  // A pause that the worker answers with `blocked`: the status stays pausing and shows the reason.
  if (mockWorker) {
    await page.fill('[data-testid="args"]', '{"city":"Lisbon","mock_blocked":12}');
    await start("durable_plan_trip");
    await page.click(`${controls} button[data-action="pause"]`);
    await page.waitForSelector('[data-testid="blocked-reason"]', { timeout: 10000 });
    check(scenario, "status is pausing while blocked", await page.locator(`${controls} .target .status`).getAttribute("data-status"), "pausing");
    check(scenario, "blocked reason shown", (await page.locator('[data-testid="blocked-reason"]').innerText()).includes("not serializable"), true);
    await waitStatus("paused");
    check(scenario, "blocked reason gone once paused", await count(page, '[data-testid="blocked-reason"]'), 0);
    await page.fill('[data-testid="args"]', '{"city":"Lisbon"}');
  } else {
    // `durable_plan_trip_parallel` holds a pending future and a second thread
    // while it plans. A worker of contract section 9 writes both into the
    // snapshot. A worker that predates it answers the pause request with
    // `blocked` until the run completes.
    await start("durable_plan_trip_parallel");
    await page.waitForSelector('[data-testid="tl-thread"]', { timeout: 20000 });
    await page.waitForSelector('[data-testid="tl-connector"][data-kind="call"]', { timeout: 30000 });
    await page.click(`${controls} button[data-action="pause"]`);
    const answer = await Promise.race([
      page.waitForSelector('[data-testid="blocked-reason"]', { timeout: 15000 }).then(() => "blocked" as const),
      waitStatus("paused", 15000).then(() => "paused" as const),
    ]);
    console.log(`  live: the worker answered the pause of a run with a pending future with \`${answer}\``);
    if (answer === "paused") {
      await page.waitForSelector('[data-testid="state-thread"]');
      check(scenario, "the snapshot holds both threads: the root in its sleep and the spawned one in its remote call",
        (await page.locator('[data-testid="state-thread"]').evaluateAll((nodes) => nodes.map((node) => (node as HTMLElement).dataset.parked ?? ""))).sort(), ["remote_call", "sleep"]);
      check(scenario, "the local weather_future is a pending future in the state tree", (await page.locator('[data-testid="state-future"]').evaluateAll((nodes) => nodes.map((node) => (node as HTMLElement).dataset.future))).includes("pending"), true);
      check(scenario, "no blocked reason and no blocked marker", [await count(page, '[data-testid="blocked-reason"]'), await count(page, '[data-testid="tl-marker"][data-kind="blocked"]')], [0, 0]);
      if (OUT_DIR) await page.screenshot({ path: join(OUT_DIR, "live-parallel-paused.png") });
      await page.click(`${controls} button[data-action="resume_here"]`);
      await waitStatus("completed", 40000);
      const parallel = await timelineCounts(page);
      check(scenario, "the spawned thread continues across the pause on one sub-row, and its remote call returns once",
        { threads: parallel.threads, thread_link: parallel.thread_link, call: parallel.call, return: parallel.return, gaps: parallel.gaps }, { threads: 2, thread_link: 1, call: 1, return: 1, gaps: 1 });
      if (OUT_DIR) await page.screenshot({ path: join(OUT_DIR, "live-parallel-resumed.png") });
    } else {
      check(scenario, "status is pausing while blocked", await page.locator(`${controls} .target .status`).getAttribute("data-status"), "pausing");
      check(scenario, "blocked reason shown", (await page.locator('[data-testid="blocked-reason"]').innerText()).includes("holds a future"), true);
      check(scenario, "blocked marker drawn", (await count(page, '[data-testid="tl-marker"][data-kind="blocked"]')) >= 1, true);
      if (OUT_DIR) await page.screenshot({ path: join(OUT_DIR, "live-blocked.png") });
      await waitStatus("completed");
      check(scenario, "blocked reason gone once the run completed", await count(page, '[data-testid="blocked-reason"]'), 0);
      check(scenario, "spawned thread drawn as a sub-bar", await count(page, '[data-testid="tl-thread"]'), 1);
    }
  }
  check(scenario, "no command error after kill, fork, cancel, and pause", await commandErrors(), []);

  // Killing a run without a snapshot answers 200 with a run record whose
  // `error` field is set. That is a success, not a command error.
  await start("plan_trip");
  await page.click(`${controls} button[data-action="kill"]`);
  await waitStatus("lost");
  check(scenario, "kill of a run without a snapshot shows no command error", await commandErrors(), []);
  check(scenario, "page does not scroll", await page.evaluate(() => document.documentElement.scrollHeight <= window.innerHeight && document.documentElement.scrollWidth <= window.innerWidth), true);
  await finish();
}

// Helpers for the live scenes that move a run between sites (LIVE=1).
function liveDriver(page: Page) {
  const controls = '[data-testid="controls"]';
  const row = (site: string, run: string) => `[data-testid="run-row"][data-site="${site}"][data-run="${run}"]`;
  return {
    controls,
    row,
    waitStatus: (status: string, timeout = 30000) =>
      page.waitForFunction(
        ({ selector, text }) => document.querySelector(`${selector} .target`)?.textContent?.includes(text) ?? false,
        { selector: controls, text: status },
        { timeout },
      ),
    async waitSites(): Promise<string[]> {
      const info = (await (await fetch(`${BASE_URL}/local/api/info`)).json()) as { sites?: { name: string }[] };
      const sites = (info.sites ?? []).map((site) => site.name);
      for (const site of sites) await page.waitForSelector(`[data-testid="conn-${site}"][data-status="open"]`, { timeout: 10000 });
      await page.waitForSelector('[data-testid="fn-select"]');
      return sites;
    },
    /** Starts a run on local and returns its id. The app selects the new run. */
    async start(fn: string): Promise<string> {
      await page.selectOption('[data-testid="fn-select"]', fn);
      await page.click('[data-testid="start"]');
      await this.waitStatus("running");
      return (await page.locator('[data-testid="run-row"][aria-selected="true"]').getAttribute("data-run")) ?? "";
    },
    /** Selects the record of `run` on `site` once it exists. */
    async select(site: string, run: string): Promise<void> {
      await page.waitForSelector(row(site, run), { timeout: 30000 });
      await page.locator(row(site, run)).click();
      await page.waitForSelector(`${row(site, run)}[aria-selected="true"]`);
    },
    /** Waits until the row of `run` on `site` shows `status`. */
    rowStatus: (site: string, run: string, status: string, timeout = 30000) =>
      page.waitForSelector(`${row(site, run)} .status[data-status="${status}"]`, { timeout }),
    resumeOn: (site: string) => page.click(`${controls} button[data-action="resume_on"][data-target-site="${site}"]`),
    resumeTargets: () =>
      page.locator(`${controls} button[data-action="resume_on"]`).evaluateAll((nodes) => nodes.map((node) => (node as HTMLElement).dataset.targetSite ?? "")),
    connector: (kind: string, from: string, to: string, timeout = 30000) =>
      page.waitForSelector(`[data-testid="tl-connector"][data-kind="${kind}"][data-from-site="${from}"][data-to-site="${to}"]`, { timeout, state: "attached" }),
    connectors: async (): Promise<string[]> =>
      (await page.locator('[data-testid="tl-connector"]').evaluateAll((nodes) =>
        nodes.map((node) => {
          const data = (node as SVGElement).dataset;
          return `${data.kind}:${data.fromSite}>${data.toSite}`;
        }),
      )).sort(),
    /** The sites whose timeline lane holds a segment bar, in document order and without repeats. */
    segmentSites: async (): Promise<string[]> =>
      [...new Set(await page.locator('[data-testid="tl-segment"] rect.tl-bar').evaluateAll((nodes) => nodes.map((node) => (node as SVGElement).dataset.site ?? "")))],
    commandErrors: () => page.locator('[data-testid="command-error"]').allInnerTexts(),
  };
}

type LiveRun = {
  id: string; site: string; status: string; segment: number; result: unknown; migrated_to?: string | null;
  parent: { site: string; run: string; call_id: string } | null; position: { function: string; line: number } | null;
  waiting_on: { call_id: string; child_site: string; child_run: string }[];
};
const liveRuns = async (site: string): Promise<LiveRun[]> => (await (await fetch(`${BASE_URL}/${site}/api/runs`)).json()) as LiveRun[];
const liveRun = async (site: string, id: string): Promise<LiveRun> => (await (await fetch(`${BASE_URL}/${site}/api/runs/${id}`)).json()) as LiveRun;
const TRIP_PLAN = { city: "Lisbon", ideas: ["day 1 in Lisbon", "day 2 in Lisbon", "day 3 in Lisbon"], weather: "sunny in Lisbon" };
const sortKeys = (value: unknown): unknown =>
  value !== null && typeof value === "object" && !Array.isArray(value)
    ? Object.fromEntries(Object.entries(value as Record<string, unknown>).sort(([a], [b]) => a.localeCompare(b)).map(([key, item]) => [key, sortKeys(item)]))
    : value;

// The central scene of contract section 8 against the site servers (LIVE=1):
// the run starts on local and is paused in its loop, the user resumes it on
// cloud, the remote pool places `remote_fetch_weather` on cloud2, the result
// returns to cloud, and the run completes there.
async function livePoolScenario(browser: Browser) {
  const scenario = "live-pool";
  const { page, finish } = await open(browser, scenario, "/", "light", { width: 1440, height: 900 });
  const live = liveDriver(page);
  const sites = await live.waitSites();
  check(scenario, "the registry has the three sites", sites, SITES);

  const id = await live.start("durable_plan_trip");
  await page.click(`${live.controls} button[data-action="pause"]`);
  await live.waitStatus("paused");
  const paused = await liveRun("local", id);
  check(scenario, "paused inside the loop, before the remote call", [paused.position?.function, paused.waiting_on.length, await count(page, '[data-testid="tl-connector"][data-kind="call"]')], ["durable_plan_trip", 0, 0]);
  check(scenario, "one resume button per other site", await live.resumeTargets(), ["cloud", "cloud2"]);
  if (OUT_DIR) await page.screenshot({ path: join(OUT_DIR, "live-pool-paused.png") });

  await live.resumeOn("cloud");
  await live.connector("migration", "local", "cloud");
  // The app follows the run to the site that now hosts it, so the controls show the record on cloud.
  await live.rowStatus("local", id, "migrated");
  await page.waitForSelector(`${live.row("cloud", id)}[aria-selected="true"]`, { timeout: 30000 });
  check(scenario, "after Resume on cloud the selection follows the run to cloud", await page.locator('[data-testid="run-row"][aria-selected="true"]').getAttribute("data-site"), "cloud");
  await live.connector("call", "cloud", "cloud2");
  if (OUT_DIR) await page.screenshot({ path: join(OUT_DIR, "live-pool-child-running.png") });
  await live.connector("return", "cloud2", "cloud");
  await live.rowStatus("cloud", id, "completed");
  await live.select("cloud", id);
  await live.waitStatus("completed");

  check(scenario, "connectors between lanes", await live.connectors(), ["call:cloud>cloud2", "migration:local>cloud", "return:cloud2>cloud"]);
  check(scenario, "segment bars on local, cloud, and cloud2", (await live.segmentSites()).sort(), [...SITES].sort());
  const counts = await timelineCounts(page);
  check(scenario, "timeline", { segments: counts.segments, gaps: counts.gaps, waits: counts.waits, migration: counts.migration, call: counts.call, return: counts.return }, { segments: 3, gaps: 1, waits: 1, migration: 1, call: 1, return: 1 });
  const origin = await liveRun("local", id);
  const moved = await liveRun("cloud", id);
  check(scenario, "local keeps a migrated record that names cloud", [origin.status, origin.migrated_to], ["migrated", "cloud"]);
  check(scenario, "the run completed on cloud with the TripPlan", [moved.status, sortKeys(moved.result)], ["completed", sortKeys(TRIP_PLAN)]);
  const children = (await liveRuns("cloud2")).filter((run) => run.parent?.run === id);
  check(scenario, "the remote child ran on cloud2 for the parent on cloud", children.map((run) => [run.status, run.parent?.site, run.result]), [["completed", "cloud", "sunny in Lisbon"]]);
  check(scenario, "no remote child on cloud or local", [(await liveRuns("cloud")).filter((run) => run.parent?.run === id).length, (await liveRuns("local")).filter((run) => run.parent?.run === id).length], [0, 0]);
  check(scenario, "child row in the cloud2 lane of the runs list", await count(page, '[data-testid="run-row"][data-site="cloud2"][data-in-tree="true"]'), 1);
  check(scenario, "the cloud log lane shows the result", await page.locator('[data-testid="log-lane-cloud"] .log-line', { hasText: "completed:" }).filter({ hasText: "sunny in Lisbon" }).count() >= 1, true);
  check(scenario, "every log lane has lines", await Promise.all(SITES.map(async (site) => (await count(page, `[data-testid="log-lane-${site}"] .log-line`)) > 0)), [true, true, true]);
  check(scenario, "no command error", await live.commandErrors(), []);

  // A reload rebuilds the same picture from the three event files.
  await page.reload();
  await live.connector("return", "cloud2", "cloud");
  check(scenario, "connectors after a reload", await live.connectors(), ["call:cloud>cloud2", "migration:local>cloud", "return:cloud2>cloud"]);
  check(scenario, "page does not scroll", await page.evaluate(() => document.documentElement.scrollHeight <= window.innerHeight && document.documentElement.scrollWidth <= window.innerWidth), true);
  await finish();
}

// A migration chain against the site servers (LIVE=1): local -> cloud -> cloud2.
// The run is paused in its loop on local and again on cloud. On cloud2 it
// calls `remote_fetch_weather`, which the pool places on cloud.
async function liveChainScenario(browser: Browser) {
  const scenario = "live-chain";
  const { page, finish } = await open(browser, scenario, "/", "light", { width: 1440, height: 900 });
  const live = liveDriver(page);
  await live.waitSites();

  const id = await live.start("durable_plan_trip");
  await page.click(`${live.controls} button[data-action="pause"]`);
  await live.waitStatus("paused");
  await live.resumeOn("cloud");
  await live.select("cloud", id);
  await live.waitStatus("running");
  await page.click(`${live.controls} button[data-action="pause"]`);
  await live.waitStatus("paused");
  const onCloud = await liveRun("cloud", id);
  check(scenario, "paused on cloud inside the loop, before the remote call", [onCloud.position?.function, onCloud.waiting_on.length, onCloud.segment], ["durable_plan_trip", 0, 2]);
  check(scenario, "a run on cloud can move to local or cloud2", await live.resumeTargets(), ["local", "cloud2"]);
  await live.resumeOn("cloud2");
  await live.connector("migration", "cloud", "cloud2");
  await live.connector("call", "cloud2", "cloud");
  if (OUT_DIR) await page.screenshot({ path: join(OUT_DIR, "live-chain-child-running.png") });
  await live.connector("return", "cloud", "cloud2");
  await live.rowStatus("cloud2", id, "completed");
  await live.select("cloud2", id);
  await live.waitStatus("completed");

  check(scenario, "connectors between lanes", await live.connectors(), ["call:cloud2>cloud", "migration:cloud>cloud2", "migration:local>cloud", "return:cloud>cloud2"]);
  const counts = await timelineCounts(page);
  check(scenario, "timeline", { segments: counts.segments, gaps: counts.gaps, migration: counts.migration, call: counts.call, return: counts.return }, { segments: 4, gaps: 2, migration: 2, call: 1, return: 1 });
  const records = await Promise.all(SITES.map((site) => liveRun(site, id)));
  check(scenario, "records along the chain", records.map((run) => [run.site, run.status, run.migrated_to ?? null]), [["local", "migrated", "cloud"], ["cloud", "migrated", "cloud2"], ["cloud2", "completed", null]]);
  check(scenario, "the run completed on cloud2 in segment 3 with the TripPlan", [records[2]?.segment, sortKeys(records[2]?.result)], [3, sortKeys(TRIP_PLAN)]);
  const children = (await liveRuns("cloud")).filter((run) => run.parent?.run === id);
  check(scenario, "the remote child ran on cloud for the parent on cloud2", children.map((run) => [run.status, run.parent?.site]), [["completed", "cloud2"]]);
  check(scenario, "no command error", await live.commandErrors(), []);
  check(scenario, "page does not scroll", await page.evaluate(() => document.documentElement.scrollHeight <= window.innerHeight && document.documentElement.scrollWidth <= window.innerWidth), true);
  await finish();
}

// A move between lanes that are not adjacent (LIVE=1): the run is paused on
// local and resumed on cloud2. Its remote call runs on cloud, the lane in
// between.
async function liveSkipScenario(browser: Browser) {
  const scenario = "live-skip";
  const { page, finish } = await open(browser, scenario, "/", "dark", { width: 1440, height: 900 });
  const live = liveDriver(page);
  await live.waitSites();
  const id = await live.start("durable_plan_trip");
  await page.click(`${live.controls} button[data-action="pause"]`);
  await live.waitStatus("paused");
  await live.resumeOn("cloud2");
  await live.connector("migration", "local", "cloud2");
  await live.connector("return", "cloud", "cloud2");
  await live.rowStatus("cloud2", id, "completed");
  await live.select("cloud2", id);
  await live.waitStatus("completed");
  check(scenario, "connectors between lanes", await live.connectors(), ["call:cloud2>cloud", "migration:local>cloud2", "return:cloud>cloud2"]);
  const origin = await liveRun("local", id);
  const moved = await liveRun("cloud2", id);
  check(scenario, "local keeps a migrated record that names cloud2", [origin.status, origin.migrated_to], ["migrated", "cloud2"]);
  check(scenario, "the run completed on cloud2 with the TripPlan", [moved.status, sortKeys(moved.result)], ["completed", sortKeys(TRIP_PLAN)]);
  check(scenario, "cloud holds the remote child and no record of the run", [(await liveRuns("cloud")).filter((run) => run.parent?.run === id).length, (await liveRuns("cloud")).filter((run) => run.id === id).length], [1, 0]);
  check(scenario, "no command error", await live.commandErrors(), []);
  await finish();
}

// What each scenario of the gallery must show when autoplay drives it against
// site servers with a worker of contract section 9 (mock or real). The run
// records come from the site servers, and the counts from the drawn timeline.
type GuideOutcome = {
  scenario: string; page: Page; root: string; pressed: string[]; counts: Awaited<ReturnType<typeof timelineCounts>>; connectors: string[];
  suspended: { status: string; threads: string[]; futures: string[]; tokens: number; hint: string } | null;
};
// The children of a run once none of them has a process. A cancel reaches a
// child's site a moment after the parent completed, and later with a slow controller (CHAOS).
const childrenOfRun = async (run: string): Promise<LiveRun[]> => {
  const deadline = Date.now() + 20_000;
  for (;;) {
    const children = (await Promise.all(["local", "cloud", "cloud2"].map(liveRuns))).flat().filter((child) => child.parent?.run === run);
    if (children.every((child) => !["starting", "running", "pausing"].includes(child.status)) || Date.now() > deadline) return children;
    await new Promise((resolve) => setTimeout(resolve, 200));
  }
};
/** True when the site servers delay or reorder their site-to-site requests (CHAOS, contract section 9.3). */
const liveChaos = async (): Promise<boolean> => {
  const info = (await (await fetch(`${BASE_URL}/local/api/info`)).json()) as { chaos?: Record<string, number | boolean> };
  return Object.entries(info.chaos ?? {}).some(([key, value]) => key !== "reorder_hold_ms" && value !== 0 && value !== false);
};
const tally = (list: string[]): Record<string, number> => sortKeys(list.reduce<Record<string, number>>((acc, item) => ({ ...acc, [item]: (acc[item] ?? 0) + 1 }), {})) as Record<string, number>;
type QuoteJson = { vendor: string; request: { kind: string; delay_ms: number }; price: { amount: number; currency: string } };
const PHASE3_GUIDES: Record<string, (outcome: GuideOutcome) => Promise<void>> = {
  async fanout({ scenario, root, pressed, suspended, counts, connectors }) {
    const run = await liveRun("local", root);
    const report = run.result as { quotes: QuoteJson[]; total: { amount: number }; vendors: Record<string, string>; cheapest: QuoteJson };
    check(scenario, "autoplay pressed nothing: the run suspends and wakes by itself", pressed, []);
    check(scenario, "the run completed in a second process with four class-valued quotes in input order", [run.status, run.segment, report.quotes.map((quote) => quote.request.kind), report.total.amount, report.cheapest.vendor, Object.keys(report.vendors).length],
      ["completed", 2, ["Flight", "Hotel", "Car", "Tour"], 1194, "Rodas", 4]);
    const children = await childrenOfRun(root);
    check(scenario, "four children, two on each cloud site, all completed", [tally(children.map((child) => child.site)), tally(children.map((child) => child.status))], [{ cloud: 2, cloud2: 2 }, { completed: 4 }]);
    check(scenario, "while the run slept, the state tree showed its five threads and their futures", [suspended?.status, suspended?.threads.length, (suspended?.futures.length ?? 0) >= 4, suspended?.hint.includes("no parent process exists")], ["sleeping", 5, true, true]);
    check(scenario, "timeline: sleeping gap, wake marker, four calls and returns, four threads that continue across the gap",
      { segments: counts.segments, sleeping: counts.sleeping, wake: counts.wake, call: counts.call, return: counts.return, thread_link: counts.thread_link, pause_request: counts.pause_request },
      { segments: 6, sleeping: 1, wake: 1, call: 4, return: 4, thread_link: 4, pause_request: 0 });
    check(scenario, "connectors", connectors, ["call:local>cloud", "call:local>cloud", "call:local>cloud2", "call:local>cloud2", "return:cloud>local", "return:cloud>local", "return:cloud2>local", "return:cloud2>local"].sort());
  },
  async race({ scenario, page, root, pressed, suspended }) {
    check(scenario, "autoplay pressed Pause and then Resume on cloud2", pressed, ["pause", "resume_on:cloud2"]);
    const run = await liveRun("cloud2", root);
    const quote = run.result as QuoteJson;
    check(scenario, "the race settled on cloud2 with the 2 second vendor", [run.status, run.segment, quote.vendor, quote.request.delay_ms, quote.request.kind], ["completed", 2, "Casa Azul", 2000, "Hotel"]);
    check(scenario, "local keeps a migrated record", (await liveRun("local", root)).status, "migrated");
    const children = await childrenOfRun(root);
    // A cancel is a request. With a slow controller the 6 second loser can finish before the cancel reaches it; its result is discarded.
    const ended = tally(children.map((child) => child.status));
    check(scenario, "one child completed and the two losers were cancelled on their sites", (await liveChaos()) ? [(ended.cancelled ?? 0) >= 1, children.length] : [ended, children.length], (await liveChaos()) ? [true, 3] : [{ cancelled: 2, completed: 1 }, 3]);
    await page.waitForTimeout(500);
    const counts = await timelineCounts(page);
    check(scenario, "the paused race showed five threads and pending futures in the state tree", [suspended?.status, suspended?.threads.length, (suspended?.futures.filter((state) => state === "pending").length ?? 0) >= 3], ["paused", 5, true]);
    check(scenario, "timeline: a migration, two cancel connectors, two cancelled bars", { migration: counts.migration, cancel_connector: counts.cancel_connector, cancelled_bar: (await liveChaos()) ? counts.cancelled_bar >= 1 : counts.cancelled_bar, call: counts.call, return: counts.return },
      { migration: 1, cancel_connector: 2, cancelled_bar: (await liveChaos()) ? true : 2, call: 3, return: 1 });
  },
  async deadline({ scenario, page, root, pressed, suspended }) {
    check(scenario, "autoplay pressed Pause and then Resume here", pressed, ["pause", "resume_here"]);
    const run = await liveRun("local", root);
    check(scenario, "the result is built from the Timeout error", [run.status, run.segment, run.result], ["completed", 2, "no tour quote for Lisbon: operation timed out after 2000ms"]);
    const children = await childrenOfRun(root);
    await page.waitForSelector('[data-testid="tl-segment"][data-end-kind="cancelled"]', { timeout: 10000 }).catch(() => null);
    const counts = await timelineCounts(page);
    check(scenario, "the 6 second child was cancelled on its site", children.map((child) => child.status), ["cancelled"]);
    // The real worker records a thread inside `await` as `runnable`: it executes the await again. The mock writes `await`.
    check(scenario, "the paused run showed the cancel token and its threads (the root in its await, the work thread in its remote call, the deadline thread in its sleep)",
      [suspended?.status, (suspended?.threads ?? []).map((kind) => (kind === "runnable" ? "await" : kind)).sort(), (suspended?.tokens ?? 0) >= 1], ["paused", ["await", "remote_call", "sleep"], true]);
    check(scenario, "timeline: one cancel connector, one cancelled bar, and the two spawned threads continue across the pause",
      { cancel_connector: counts.cancel_connector, cancelled_bar: counts.cancelled_bar, thread_link: counts.thread_link, call: counts.call, return: counts.return },
      { cancel_connector: 1, cancelled_bar: 1, thread_link: 2, call: 1, return: 0 });
  },
  async settled({ scenario, root, pressed, suspended, counts }) {
    check(scenario, "autoplay pressed Pause and then Resume here", pressed, ["pause", "resume_here"]);
    const run = await liveRun("local", root);
    const report = run.result as { succeeded: QuoteJson[]; failed: { kind: string; error: string }[] };
    check(scenario, "two quotes and one typed failure", [run.status, run.segment, report.succeeded.map((quote) => quote.request.kind), report.failed.map((failure) => failure.kind), report.failed[0]?.error.includes("no Car vendor answers in Lisbon")],
      ["completed", 2, ["Flight", "Tour"], ["Car"], true]);
    const children = await childrenOfRun(root);
    check(scenario, "the refusal cancelled nothing", tally(children.map((child) => child.status)), { completed: 2, failed: 1 });
    const states = tally(suspended?.futures ?? []);
    // With a slow controller no vendor has answered at the pause, so every future is still pending.
    check(scenario, "the paused run showed futures in more than one state, one of them pending", [suspended?.status, (states.pending ?? 0) >= 1, Object.keys(states).length >= 2 || (await liveChaos())], ["paused", true, true]);
    check(scenario, "timeline: three calls, three returns, no cancel", { call: counts.call, return: counts.return, cancel_connector: counts.cancel_connector, cancelled_bar: counts.cancelled_bar, thread_link: counts.thread_link >= 1 },
      { call: 3, return: 3, cancel_connector: 0, cancelled_bar: 0, thread_link: true });
  },
  async recover({ scenario, root, pressed }) {
    check(scenario, "autoplay killed the durable run, resumed it, started the plain run, and killed it", pressed, ["kill", "resume_here", "start", "kill"]);
    const run = await liveRun("local", root);
    check(scenario, "the killed durable run recovered from its automatic snapshot and completed", [run.status, run.segment, sortKeys(run.result)], ["completed", 2, sortKeys(TRIP_PLAN)]);
    const plain = (await liveRuns("local")).filter((other) => (other as LiveRun & { function?: string }).function === "plan_trip").at(-1);
    check(scenario, "the plain run is lost", plain?.status, "lost");
  },
  async fork({ scenario, root, pressed, counts }) {
    check(scenario, "autoplay pressed Pause, Fork, and Resume here for both runs", pressed, ["pause", "fork", "resume_here"]);
    const runs = await liveRuns("local");
    const fork = runs.find((other) => (other as LiveRun & { forked_from?: { run: string } | null }).forked_from?.run === root);
    const source = await liveRun("local", root);
    check(scenario, "the source and the fork both completed with the TripPlan", [source.status, fork?.status, sortKeys(source.result), sortKeys(fork?.result)], ["completed", "completed", sortKeys(TRIP_PLAN), sortKeys(TRIP_PLAN)]);
    check(scenario, "timeline: a fork marker and connector, and a remote call for each of the two runs", { fork_marker: counts.fork_marker, fork_connector: counts.fork_connector, call: counts.call, return: counts.return },
      { fork_marker: 1, fork_connector: 1, call: 2, return: 2 });
  },
};

// The scenario guide against the site servers (LIVE=1, LIVE_ONLY=guide): the
// gallery starts the run, and autoplay performs the hinted actions through the
// buttons of the runs panel. `migrate` needs nothing of contract section 9, so
// it runs with any worker. `LIVE_GUIDE=race,fanout` names other scenarios.
async function liveGuideScenario(browser: Browser, id: string) {
  const scenario = `live-guide-${id}`;
  const { page, finish } = await open(browser, scenario, "/?autoplay=1", "light", { width: 1440, height: 900 });
  const live = liveDriver(page);
  await live.waitSites();
  await page.locator('[data-testid="open-scenarios"]').click();
  const card = page.locator(`[data-testid="scenario-card"][data-scenario="${id}"]`);
  check(scenario, "Run live is offered once local is connected", await card.locator('[data-testid="scenario-run"]').isEnabled(), true);
  await card.locator('[data-testid="scenario-run"]').click();
  await page.waitForSelector(`[data-testid="coach"][data-scenario="${id}"]`, { timeout: 15000 });
  check(scenario, "the gallery closes and the guide follows the new run", [await count(page, '[data-testid="scenario-gallery"]'), await count(page, '[data-testid="run-row"][aria-selected="true"]')], [0, 1]);
  const root = (await page.locator('[data-testid="run-row"][aria-selected="true"]').getAttribute("data-run")) ?? "";

  // Autoplay highlights the control of each action step before it presses it.
  const pressed: string[] = [];
  let suspended: { status: string; threads: string[]; futures: string[]; tokens: number; hint: string } | null = null;
  const deadline = Date.now() + 90_000;
  while (Date.now() < deadline) {
    if ((await page.locator('[data-testid="coach"]').getAttribute("data-finished")) === "true") break;
    const highlighted = await page.locator('[data-testid="controls"] button[data-coach="press"], [data-testid="coach-start"][data-coach="press"]').evaluateAll((nodes) =>
      nodes.map((node) => `${(node as HTMLElement).dataset.action ?? "start"}${(node as HTMLElement).dataset.targetSite ? `:${(node as HTMLElement).dataset.targetSite}` : ""}`),
    );
    for (const name of highlighted) if (pressed[pressed.length - 1] !== name) pressed.push(name);
    // The first time the run has no process (paused or sleeping), read the state tree of its snapshot.
    if (suspended === null) {
      const status = await page.locator(`${live.controls} .target .status`).first().getAttribute("data-status").catch(() => null);
      if ((status === "paused" || status === "sleeping") && (await count(page, '[data-testid="state-thread"]')) > 0) {
        suspended = {
          status,
          threads: await page.locator('[data-testid="state-thread"]').evaluateAll((nodes) => nodes.map((node) => (node as HTMLElement).dataset.parked ?? "")),
          futures: await page.locator('[data-testid="state-future"]').evaluateAll((nodes) => nodes.map((node) => (node as HTMLElement).dataset.future ?? "")),
          tokens: await count(page, '[data-testid="state-cancel-token"]'),
          hint: await page.locator('[data-testid="coach-hint"]').innerText(),
        };
        if (OUT_DIR) await page.screenshot({ path: join(OUT_DIR, `${scenario}-suspended.png`) });
      }
    }
    await page.waitForTimeout(120);
  }
  if (OUT_DIR) await page.screenshot({ path: join(OUT_DIR, `${scenario}.png`) });
  check(scenario, "the guide finished", await page.locator('[data-testid="coach"]').getAttribute("data-finished"), "true");
  check(scenario, "the closing hint", (await page.locator('[data-testid="coach-hint"]').innerText()).startsWith("Done."), true);
  check(scenario, "no command error", [await live.commandErrors(), await count(page, '[data-testid="coach-error"]')], [[], 0]);
  if (id === "migrate") {
    check(scenario, "autoplay pressed Pause and then Resume on cloud", pressed, ["pause", "resume_on:cloud"]);
    const moved = await liveRun("cloud", root);
    check(scenario, "the run completed on cloud with the TripPlan", [moved.status, moved.segment, sortKeys(moved.result)], ["completed", 2, sortKeys(TRIP_PLAN)]);
    check(scenario, "local keeps a migrated record", (await liveRun("local", root)).status, "migrated");
    check(scenario, "connectors", await live.connectors(), ["call:cloud>cloud2", "migration:local>cloud", "return:cloud2>cloud"]);
  } else if (PHASE3_GUIDES[id]) {
    await PHASE3_GUIDES[id]({ scenario, page, root, pressed, suspended, counts: await timelineCounts(page), connectors: await live.connectors() });
  } else {
    console.log(`  ${scenario}: autoplay pressed ${JSON.stringify(pressed)}`);
    console.log(`  ${scenario}: steps ${JSON.stringify(await page.locator(".coach-steps li").evaluateAll((nodes) => nodes.map((node) => `${node.textContent?.trim()}=${(node as HTMLElement).dataset.state}`)))}`);
    console.log(`  ${scenario}: timeline ${JSON.stringify(await timelineCounts(page))}`);
  }
  await finish();
}

const browser = await chromium.launch();
try {
  const central = {
    timeline: { segments: 3, gaps: 1, threads: 0, waits: 1, call: 1, return: 1, migration: 0, pause_request: 1, blocked: 0, snapshot: 1, resume: 1, automatic: 0, fork_marker: 0, fork_connector: 0, ...NO_PHASE3 },
    runs: 2,
    enabled: ["fork"],
    ends: ["paused", "completed", "completed"],
  };
  const spawn = {
    timeline: { segments: 3, gaps: 1, threads: 1, waits: 1, call: 1, return: 1, migration: 1, pause_request: 1, blocked: 2, snapshot: 1, resume: 1, automatic: 0, fork_marker: 0, fork_connector: 0, ...NO_PHASE3 },
    runs: 3,
    // The selected record is the origin of the migration. It keeps its snapshot, so it can be forked.
    enabled: ["fork"],
  };
  // A lost process (worker_exit only), three automatic snapshots, a fork, and a cancelled fork.
  const fork = {
    timeline: { segments: 4, gaps: 2, threads: 0, waits: 1, call: 1, return: 1, migration: 0, pause_request: 0, blocked: 0, snapshot: 3, resume: 2, automatic: 3, fork_marker: 1, fork_connector: 1, ...NO_PHASE3, cancelled_bar: 1 },
    runs: 3,
    enabled: ["fork"],
    ends: ["killed", "completed", "cancelled", "completed"],
    forks: 1,
  };
  // The central scene of section 8: paused on local, resumed on cloud, remote child on cloud2.
  const pool = {
    timeline: { segments: 3, gaps: 1, threads: 0, waits: 1, call: 1, return: 1, migration: 1, pause_request: 1, blocked: 0, snapshot: 1, resume: 1, automatic: 0, fork_marker: 0, fork_connector: 0, ...NO_PHASE3 },
    runs: 3,
    enabled: ["fork"],
    ends: ["paused", "completed", "completed"],
    logSites: ["local", "cloud", "cloud2"],
    connectors: ["migration:local>cloud", "call:cloud>cloud2", "return:cloud2>cloud"],
    scenario: "migrate",
  };
  // One run over all three sites and back. The last migration connects lanes that are not adjacent.
  const chain = {
    timeline: { segments: 5, gaps: 3, threads: 0, waits: 2, call: 1, return: 1, migration: 3, pause_request: 3, blocked: 0, snapshot: 3, resume: 3, automatic: 0, fork_marker: 0, fork_connector: 0, ...NO_PHASE3 },
    runs: 4,
    enabled: ["fork"],
    ends: ["paused", "paused", "paused", "completed", "completed"],
    logSites: ["local", "cloud", "cloud2"],
    connectors: ["migration:local>cloud", "migration:cloud>cloud2", "migration:cloud2>local", "call:cloud2>cloud", "return:cloud>local"],
  };
  // Contract section 9.6. Every scenario of the gallery has a recording. A wait
  // in a resumed segment lasts a few milliseconds, and a wait that is narrower
  // than half a pixel is not drawn, so `waits` counts the ones of this viewport.
  const base = { segments: 0, gaps: 1, threads: 0, waits: 0, call: 0, return: 0, migration: 0, pause_request: 1, blocked: 0, snapshot: 1, resume: 1, automatic: 0, fork_marker: 0, fork_connector: 0, ...NO_PHASE3 };
  // Four children on cloud and cloud2, a durable sleep, and four threads that continue across it.
  const fanout = {
    timeline: { ...base, segments: 6, threads: 9, waits: 5, call: 4, return: 4, pause_request: 0, sleeping: 1, wake: 1, thread_link: 4 },
    runs: 5, children: 4, stateThreads: 5, enabled: ["fork"], scenario: "fanout",
    ends: ["paused", "completed", "completed", "completed", "completed", "completed"],
    logSites: ["local", "cloud", "cloud2"],
    connectors: ["call:local>cloud", "call:local>cloud", "call:local>cloud2", "call:local>cloud2", "return:cloud>local", "return:cloud>local", "return:cloud2>local", "return:cloud2>local"],
  };
  // A race that is paused on local and settles on cloud2, where it cancels the two losers.
  const race = {
    timeline: { ...base, segments: 5, threads: 8, waits: 5, call: 3, return: 1, migration: 1, cancel_connector: 2, cancel_marker: 2, cancelled_bar: 2 },
    runs: 5, children: 3, stateThreads: 5, enabled: ["fork"], scenario: "race",
    ends: ["paused", "completed", "completed", "cancelled", "cancelled"],
    logSites: ["local", "cloud", "cloud2"],
    connectors: ["call:local>cloud", "call:local>cloud", "call:local>cloud2", "return:cloud>cloud2", "migration:local>cloud2", "cancel:cloud2>cloud2", "cancel:cloud2>cloud"],
  };
  const deadline = {
    timeline: { ...base, segments: 3, threads: 4, waits: 2, call: 1, thread_link: 2, cancel_connector: 1, cancel_marker: 1, cancelled_bar: 1 },
    runs: 2, children: 1, stateThreads: 3, enabled: ["fork"], scenario: "deadline",
    ends: ["paused", "completed", "cancelled"],
    connectors: ["call:local>cloud", "cancel:local>cloud"],
  };
  const settled = {
    timeline: { ...base, segments: 5, threads: 6, waits: 3, call: 3, return: 3, thread_link: 2 },
    runs: 4, children: 3, stateThreads: 3, enabled: ["fork"], scenario: "settled",
    ends: ["paused", "completed", "completed", "failed", "completed"],
    logSites: ["local", "cloud", "cloud2"],
  };
  // The durable run of the recovery scenario. The plain run is a tree of its own.
  const recover = {
    timeline: { ...base, segments: 3, waits: 1, call: 1, return: 1, pause_request: 0, snapshot: 3, automatic: 3 },
    runs: 3, children: 1, enabled: ["fork"], scenario: "recover",
    ends: ["killed", "completed", "completed"],
  };
  const branch = {
    timeline: { ...base, segments: 5, gaps: 2, waits: 2, call: 2, return: 2, resume: 2, fork_marker: 1, fork_connector: 1 },
    runs: 4, children: 2, forks: 1, enabled: ["fork"], scenario: "fork",
    ends: ["paused", "completed", "completed", "completed", "completed"],
    logSites: ["local", "cloud", "cloud2"],
  };
  // LIVE_ONLY names the live scenes to run and skips the fixture scenes.
  const only = process.env.LIVE === "1" ? (process.env.LIVE_ONLY?.split(",") ?? null) : null;
  // SCENES=phase3 or SCENES=base runs one group of the fixture scenes.
  const groups = process.env.SCENES?.split(",") ?? ["phase3", "base"];
  if (only === null && groups.includes("phase3")) {
    for (const [name, expected] of Object.entries({ fanout, race, deadline, settled, recover, branch })) {
      await fixtureScenario(browser, name, expected, "light");
      await fixtureScenario(browser, name, expected, "dark");
    }
    await sleepingScenario(browser, "light");
    await sleepingScenario(browser, "dark");
    await threadsScenario(browser);
    await stackingScenario(browser);
    await stateTreeScenario(browser, "light");
    await stateTreeScenario(browser, "dark");
    await galleryScenario(browser, "light");
    await galleryScenario(browser, "dark");
    await guideScenario(browser);
  }
  if (only === null && groups.includes("base")) {
    await fixtureScenario(browser, "pool", pool, "light");
    await fixtureScenario(browser, "pool", pool, "dark");
    await fixtureScenario(browser, "chain", chain, "light");
    await fixtureScenario(browser, "chain", chain, "dark");
    await wideScenario(browser, "pool", "light");
    await wideScenario(browser, "chain", "dark");
    await resumeTargetsScenario(browser);
    await layoutPersistenceScenario(browser);
    await fixtureScenario(browser, "central", central, "light");
    await fixtureScenario(browser, "spawn", spawn, "light");
    await fixtureScenario(browser, "central", central, "dark");
    await fixtureScenario(browser, "spawn", spawn, "dark");
    await fixtureScenario(browser, "fork", fork, "light");
    await fixtureScenario(browser, "fork", fork, "dark");
    await forkScenario(browser);
    await blockedScenario(browser);
    await playbackScenario(browser);
    await interactionScenario(browser);
  }
  if (process.env.LIVE === "1") {
    if (only === null || only.includes("pool")) await livePoolScenario(browser);
    if (only === null || only.includes("chain")) await liveChainScenario(browser);
    if (only === null || only.includes("skip")) await liveSkipScenario(browser);
    if (only === null || only.includes("central")) await liveScenario(browser);
    if (only === null || only.includes("guide")) {
      for (const id of (process.env.LIVE_GUIDE ?? "migrate").split(",")) await liveGuideScenario(browser, id);
    }
  }
} finally {
  await browser.close();
}

if (failures.length > 0) {
  console.error(`\n${failures.length} check(s) failed:\n  ${failures.join("\n  ")}`);
  process.exit(1);
}
console.log("\nall headless checks passed");
