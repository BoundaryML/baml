/// <reference types="vite/client" />

import { createHighlighter, type ThemedToken } from "shiki";
import bamlGrammar from "../baml.tmLanguage.json";

const DEFAULT_THEME = "github-dark";
const THEMES = [
  { id: "github-dark", label: "GitHub Dark", tone: "dark" },
  { id: "github-light", label: "GitHub Light", tone: "light" },
  { id: "dark-plus", label: "Dark+", tone: "dark" },
  { id: "light-plus", label: "Light+", tone: "light" },
  { id: "vitesse-dark", label: "Vitesse Dark", tone: "dark" },
  { id: "vitesse-light", label: "Vitesse Light", tone: "light" },
  { id: "nord", label: "Nord", tone: "dark" },
  { id: "dracula", label: "Dracula", tone: "dark" },
] as const;
type ThemeId = (typeof THEMES)[number]["id"];
const THEME_IDS = new Set<ThemeId>(THEMES.map((theme) => theme.id));

const highlighter = await createHighlighter({
  themes: THEMES.map((theme) => theme.id),
  langs: [bamlGrammar as never],
});

const STATE_KEY = "baml-grammar-preview-state";

const currentView = document.getElementById("currentView")!;
const savedView = document.getElementById("savedView")!;
const currentRuler = document.getElementById("currentRuler")!;
const savedRuler = document.getElementById("savedRuler")!;
const inspect = document.getElementById("inspect")!;
const fixtureList = document.getElementById("fixtureList")!;
const snapshotStatus = document.getElementById("snapshotStatus")!;
const themeSelect = document.getElementById("themeSelect") as HTMLSelectElement;
const acceptSnapshot = document.getElementById(
  "acceptSnapshot",
) as HTMLButtonElement;

type ScopeExplanation = {
  content: string;
  scopes?: { scopeName: string }[];
};
type TokenSpan = {
  line: number;
  start: number;
  end: number;
  scopes: string[];
  color?: string;
};
type SelectedSpan = {
  line: number;
  start: number;
  end: number;
};
type PreviewState = {
  fixture?: string;
  scrollTop?: number;
  scrollLeft?: number;
  selectedSpan?: SelectedSpan | null;
  theme?: string;
};
type FixturePayload = {
  source: string;
  snapshot: string;
};
type FixtureStatus = "loading" | "same" | "different" | "error";
type FixtureSummary = {
  name: string;
  status: FixtureStatus;
  diffCount?: number;
};

let source = "";
let selectedFixture = "";
let currentSnapshot = "";
let savedSnapshot = "";
let fixtures: string[] = [];
let fixtureSummaries = new Map<string, FixtureSummary>();
let fixtureCache = new Map<string, FixturePayload>();
let currentTokens = new Map<number, TokenSpan[]>();
let savedTokens = new Map<number, TokenSpan[]>();
let currentColors = new Map<string, string>();
let diffCount = 0;
let selectedSpan: SelectedSpan | null = null;
let syncingScroll = false;
let restoringState = false;
let currentTheme: ThemeId = DEFAULT_THEME;

const escapeHtml = (s: string) =>
  s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
const escapeAttribute = (s: string) => escapeHtml(s).replace(/"/g, "&quot;");

const scopeKey = (scopes: string[]) => scopes.join("\0");

function readState(): PreviewState {
  try {
    return JSON.parse(localStorage.getItem(STATE_KEY) ?? "{}") as PreviewState;
  } catch {
    return {};
  }
}

function writeState(patch: PreviewState) {
  localStorage.setItem(STATE_KEY, JSON.stringify({ ...readState(), ...patch }));
}

function resolveTheme(theme: unknown): ThemeId {
  return typeof theme === "string" &&
    THEME_IDS.has(theme as ThemeId)
    ? (theme as ThemeId)
    : DEFAULT_THEME;
}

function selectedTheme() {
  return THEMES.find((theme) => theme.id === currentTheme) ?? THEMES[0];
}

function setTheme(
  theme: unknown,
  options: { persist?: boolean; render?: boolean } = {},
) {
  const scrollTop = currentView.scrollTop;
  const scrollLeft = currentView.scrollLeft;

  currentTheme = resolveTheme(theme);
  themeSelect.value = currentTheme;
  document.documentElement.dataset.themeTone = selectedTheme().tone;

  if (options.persist !== false) {
    writeState({ theme: currentTheme });
  }

  if (options.render !== false && source) {
    renderCurrentGrammar();
    currentView.scrollTop = scrollTop;
    currentView.scrollLeft = scrollLeft;
    syncScroll(currentView, savedView);
  }
}

function initializeThemeSelector() {
  themeSelect.innerHTML = THEMES.map(
    (theme) =>
      `<option value="${escapeAttribute(theme.id)}">${escapeHtml(
        theme.label,
      )}</option>`,
  ).join("");
  setTheme(readState().theme, { persist: false, render: false });
}

function explanationParts(token: ThemedToken): ScopeExplanation[] {
  return (
    (token.explanation as ScopeExplanation[] | undefined) ?? [
      { content: token.content, scopes: [] },
    ]
  );
}

function formatScopeSnapshot(tokens: ThemedToken[][]) {
  const rows: string[] = [];

  tokens.forEach((line, lineIndex) => {
    let column = 0;

    for (const token of line) {
      for (const part of explanationParts(token)) {
        const start = column;
        const end = start + part.content.length;
        column = end;

        if (/^\s*$/.test(part.content)) {
          continue;
        }

        const range = `${lineIndex + 1}:${start + 1}-${end + 1}`;
        const text = JSON.stringify(part.content).padEnd(18);
        const scopes = (part.scopes ?? []).map((scope) => scope.scopeName);

        rows.push(`${range.padEnd(12)} ${text} ${scopes.join(" ")}`);
      }
    }
  });

  return `${rows.join("\n")}\n`;
}

function parseSnapshot(snapshot: string) {
  const tokens = new Map<number, TokenSpan[]>();
  const row = /^(\d+):(\d+)-(\d+)\s+("(?:\\.|[^"\\])*")\s*(.*)$/;

  for (const line of snapshot.split("\n")) {
    const match = row.exec(line);
    if (!match) {
      continue;
    }

    const lineIndex = Number(match[1]) - 1;
    const token: TokenSpan = {
      line: lineIndex,
      start: Number(match[2]) - 1,
      end: Number(match[3]) - 1,
      scopes: match[5] ? match[5].split(/\s+/) : [],
    };
    tokens.set(lineIndex, [...(tokens.get(lineIndex) ?? []), token]);
  }

  for (const lineTokens of tokens.values()) {
    lineTokens.sort((a, b) => a.start - b.start);
  }

  return tokens;
}

function currentTokensFromShiki(tokens: ThemedToken[][]) {
  const spans = new Map<number, TokenSpan[]>();
  const colors = new Map<string, string>();

  tokens.forEach((line, lineIndex) => {
    let column = 0;

    for (const token of line) {
      for (const part of explanationParts(token)) {
        const start = column;
        const end = start + part.content.length;
        column = end;

        if (/^\s*$/.test(part.content)) {
          continue;
        }

        const scopes = (part.scopes ?? []).map((scope) => scope.scopeName);
        const span: TokenSpan = {
          line: lineIndex,
          start,
          end,
          scopes,
          color: token.color,
        };
        spans.set(lineIndex, [...(spans.get(lineIndex) ?? []), span]);
        if (token.color) {
          colors.set(scopeKey(scopes), token.color);
        }
      }
    }
  });

  return { spans, colors };
}

function colorForScopes(scopes: string[]) {
  const leaf = scopes[scopes.length - 1] ?? "";

  if (leaf.startsWith("comment.")) return "#8b949e";
  if (leaf.startsWith("string.")) return "#a5d6ff";
  if (leaf.startsWith("constant.")) return "#79c0ff";
  if (leaf.startsWith("keyword.")) return "#ff7b72";
  if (leaf.startsWith("support.type.")) return "#ffa657";
  if (leaf.startsWith("entity.name.type.")) return "#ffa657";
  if (leaf.startsWith("entity.name.namespace.")) return "#d2a8ff";
  if (leaf.startsWith("entity.name.function.")) return "#d2a8ff";
  if (leaf.startsWith("variable.")) return "#ffa657";

  return "#c9d1d9";
}

function tokenAt(tokens: Map<number, TokenSpan[]>, line: number, column: number) {
  return (
    (tokens.get(line) ?? []).find(
      (token) => column >= token.start && column < token.end,
    ) ?? null
  );
}

function tokenAtSpan(tokens: Map<number, TokenSpan[]>, span: SelectedSpan) {
  return (
    (tokens.get(span.line) ?? []).find(
      (token) => token.start === span.start && token.end === span.end,
    ) ?? null
  );
}

function spanKey(span: Pick<TokenSpan, "line" | "start" | "end">) {
  return `${span.line}:${span.start}:${span.end}`;
}

function tokenMap(tokens: Map<number, TokenSpan[]>) {
  const map = new Map<string, TokenSpan>();

  for (const lineTokens of tokens.values()) {
    for (const token of lineTokens) {
      map.set(spanKey(token), token);
    }
  }

  return map;
}

function diffedSpansFor(
  currentTokens: Map<number, TokenSpan[]>,
  savedTokens: Map<number, TokenSpan[]>,
) {
  const current = tokenMap(currentTokens);
  const saved = tokenMap(savedTokens);
  const spans = new Map<string, "different" | "missing">();

  for (const [key, currentToken] of current) {
    const savedToken = saved.get(key);
    if (!savedToken) {
      spans.set(key, "missing");
      continue;
    }
    if (scopeKey(currentToken.scopes) !== scopeKey(savedToken.scopes)) {
      spans.set(key, "different");
    }
  }

  for (const key of saved.keys()) {
    if (!current.has(key)) {
      spans.set(key, "missing");
    }
  }

  return spans;
}

function diffLines(
  diffs: Map<string, "different" | "missing">,
): Map<number, "different" | "missing"> {
  const lines = new Map<number, "different" | "missing">();

  for (const [key, kind] of diffs) {
    const line = Number(key.split(":", 1)[0]);
    if (!Number.isFinite(line)) {
      continue;
    }

    const existing = lines.get(line);
    lines.set(line, existing === "missing" ? existing : kind);
  }

  return lines;
}

function tokenizeSource(sourceText: string) {
  const { tokens } = highlighter.codeToTokens(sourceText, {
    lang: "baml" as never,
    theme: currentTheme,
    includeExplanation: "scopeName",
  });
  const current = currentTokensFromShiki(tokens);

  return {
    snapshot: formatScopeSnapshot(tokens),
    spans: current.spans,
    colors: current.colors,
  };
}

function setFixtureSummary(summary: FixtureSummary) {
  fixtureSummaries.set(summary.name, summary);
  renderFixtureList();
}

function fixtureBadge(summary: FixtureSummary | undefined) {
  if (!summary || summary.status === "loading") {
    return { className: "", label: "Loading" };
  }
  if (summary.status === "error") {
    return { className: "error", label: "Error" };
  }
  if (summary.status === "same") {
    return { className: "same", label: "Same" };
  }

  return {
    className: "different",
    label: `Diff ${summary.diffCount ?? 0}`,
  };
}

function renderFixtureList() {
  fixtureList.innerHTML = fixtures
    .map((fixture) => {
      const badge = fixtureBadge(fixtureSummaries.get(fixture));
      const selected = fixture === selectedFixture ? " selected" : "";

      return `<button type="button" class="fixture-row${selected}" data-fixture="${escapeAttribute(
        fixture,
      )}" title="${escapeAttribute(fixture)}">
        <span class="fixture-name">${escapeHtml(fixture)}</span>
        <span class="fixture-badge ${badge.className}">${badge.label}</span>
      </button>`;
    })
    .join("");
}

function renderDiffRuler(
  target: HTMLElement,
  diffs: Map<string, "different" | "missing">,
) {
  const lineCount = Math.max(source.split("\n").length, 1);
  const lines = diffLines(diffs);

  target.classList.toggle("has-diffs", lines.size > 0);
  target.innerHTML = [...lines.entries()]
    .sort(([a], [b]) => a - b)
    .map(([line, kind]) => {
      const top = ((line + 0.5) / lineCount) * 100;

      return `<button type="button" class="diff-ruler-marker ${kind}" data-line="${line}" style="top:${top}%" title="Line ${
        line + 1
      }"></button>`;
    })
    .join("");
}

function renderSource(
  target: HTMLElement,
  tokens: Map<number, TokenSpan[]>,
  colors: Map<string, string>,
  side: "current" | "snapshot",
  diffs: Map<string, "different" | "missing">,
) {
  const lines = source.split("\n");

  target.innerHTML =
    `<pre><code>` +
    lines
      .map((line, lineIndex) => {
        let column = 0;
        let html = "";

        for (const token of tokens.get(lineIndex) ?? []) {
          if (token.start > column) {
            html += escapeHtml(line.slice(column, token.start));
          }

          const highlighted =
            selectedSpan &&
            selectedSpan.line === lineIndex &&
            selectedSpan.start === token.start &&
            selectedSpan.end === token.end;
          const diffKind = diffs.get(spanKey(token));
          const classes = [
            highlighted ? "span-mark" : "",
            diffKind === "different" ? "diff-mark" : "",
            diffKind === "missing" ? "missing-mark" : "",
          ]
            .filter(Boolean)
            .join(" ");
          const color =
            side === "current"
              ? (token.color ?? colorForScopes(token.scopes))
              : (colors.get(scopeKey(token.scopes)) ?? colorForScopes(token.scopes));

          html += `<span data-side="${side}" data-line="${lineIndex}" data-start="${token.start}" data-end="${token.end}"${classes ? ` class="${classes}"` : ""} style="color:${color}">${escapeHtml(
            line.slice(token.start, Math.min(token.end, line.length)),
          )}</span>`;
          column = Math.max(column, token.end);
        }

        html += escapeHtml(line.slice(column));
        return html;
      })
      .join("\n") +
    `</code></pre>`;
}

function renderPanes() {
  const diffs = diffedSpansFor(currentTokens, savedTokens);
  diffCount = diffs.size;
  renderSource(currentView, currentTokens, currentColors, "current", diffs);
  renderSource(savedView, savedTokens, currentColors, "snapshot", diffs);
  renderDiffRuler(currentRuler, diffs);
  renderDiffRuler(savedRuler, diffs);
}

function renderScopes(title: string, span: SelectedSpan | null, token: TokenSpan | null) {
  const location = span
    ? `line ${span.line + 1}, cols ${span.start + 1}-${span.end + 1}`
    : "no selected span";
  const scopes =
    token == null || token.scopes.length === 0
      ? `<span class="muted">no token for selected span</span>`
      : token.scopes
          .slice()
          .reverse()
          .map(
            (scope, index) =>
              `<div class="scope${index === 0 ? " leaf" : ""}">${escapeHtml(scope)}</div>`,
          )
          .join("");

  return `<section><div class="loc">${title} ${location}</div>${scopes}</section>`;
}

function updateInspect() {
  const currentToken = selectedSpan
    ? tokenAtSpan(currentTokens, selectedSpan)
    : null;
  const savedToken = selectedSpan ? tokenAtSpan(savedTokens, selectedSpan) : null;

  inspect.innerHTML =
    renderScopes("Current", selectedSpan, currentToken) +
    renderScopes("Snapshot", selectedSpan, savedToken);
}

function selectSpan(span: SelectedSpan | null) {
  selectedSpan = span;
  writeState({ selectedSpan });
  renderPanes();
  updateInspect();
}

function updateComparison() {
  const same = currentSnapshot === savedSnapshot;
  snapshotStatus.textContent = selectedFixture
    ? same
      ? "Same"
      : `Different (${diffCount})`
    : "No fixture";
  snapshotStatus.className = `status ${same ? "same" : "different"}`;
  acceptSnapshot.disabled = !selectedFixture || same;
}

function renderCurrentGrammar() {
  const current = tokenizeSource(source);
  currentTokens = current.spans;
  currentColors = current.colors;
  currentSnapshot = current.snapshot;
  renderPanes();
  updateComparison();
  updateInspect();
  if (selectedFixture) {
    setFixtureSummary({
      name: selectedFixture,
      status: currentSnapshot === savedSnapshot ? "same" : "different",
      diffCount,
    });
  }
}

async function fetchJson<T>(url: string, init?: RequestInit): Promise<T> {
  const response = await fetch(url, init);
  if (!response.ok) {
    throw new Error(await response.text());
  }
  return response.json() as Promise<T>;
}

async function getFixture(name: string) {
  const cached = fixtureCache.get(name);
  if (cached) {
    return cached;
  }

  const fixture = await fetchJson<FixturePayload>(
    `/__grammar/fixture?name=${encodeURIComponent(name)}`,
  );
  fixtureCache.set(name, fixture);

  return fixture;
}

async function refreshFixtureSummary(name: string) {
  setFixtureSummary({ name, status: "loading" });

  try {
    const fixture = await getFixture(name);
    const current = tokenizeSource(fixture.source);
    const saved = parseSnapshot(fixture.snapshot);
    const diffs = diffedSpansFor(current.spans, saved);

    setFixtureSummary({
      name,
      status: current.snapshot === fixture.snapshot ? "same" : "different",
      diffCount: diffs.size,
    });
  } catch {
    setFixtureSummary({ name, status: "error" });
  }
}

async function refreshFixtureSummaries() {
  await Promise.all(fixtures.map((fixture) => refreshFixtureSummary(fixture)));
}

async function loadFixture(name: string) {
  selectedFixture = name;
  writeState({ fixture: name });
  renderFixtureList();
  if (!restoringState) {
    selectedSpan = null;
    writeState({ selectedSpan: null, scrollTop: 0, scrollLeft: 0 });
  }
  const fixture = await getFixture(name);
  if (selectedFixture !== name) {
    return;
  }

  source = fixture.source;
  savedSnapshot = fixture.snapshot;
  savedTokens = parseSnapshot(savedSnapshot);
  renderCurrentGrammar();

  const state = readState();
  if (restoringState) {
    selectedSpan = state.selectedSpan ?? null;
    renderPanes();
    updateInspect();
    currentView.scrollTop = state.scrollTop ?? 0;
    currentView.scrollLeft = state.scrollLeft ?? 0;
    syncScroll(currentView, savedView);
  }
}

async function loadFixtures() {
  const response = await fetchJson<{ fixtures: string[] }>("/__grammar/fixtures");
  fixtures = response.fixtures;
  fixtureSummaries = new Map(
    fixtures.map((fixture) => [fixture, { name: fixture, status: "loading" }]),
  );
  renderFixtureList();

  if (fixtures.length > 0) {
    const state = readState();
    const fixture = state.fixture && fixtures.includes(state.fixture)
      ? state.fixture
      : fixtures[0];
    restoringState = true;
    await loadFixture(fixture);
    restoringState = false;
    void refreshFixtureSummaries();
  } else {
    source = "";
    renderCurrentGrammar();
  }
}

function handleTokenHover(event: Event) {
  const target = event.target;
  if (!(target instanceof HTMLElement)) {
    return;
  }

  const token = target.closest("[data-line][data-start][data-end]");
  if (!(token instanceof HTMLElement)) {
    return;
  }

  selectSpan({
    line: Number(token.dataset.line),
    start: Number(token.dataset.start),
    end: Number(token.dataset.end),
  });
}

function syncScroll(sourcePane: HTMLElement, targetPane: HTMLElement) {
  if (syncingScroll) {
    return;
  }

  syncingScroll = true;
  targetPane.scrollTop = sourcePane.scrollTop;
  targetPane.scrollLeft = sourcePane.scrollLeft;
  writeState({
    scrollTop: sourcePane.scrollTop,
    scrollLeft: sourcePane.scrollLeft,
  });
  syncingScroll = false;
}

function lineHeightFor(view: HTMLElement) {
  const lineHeight = Number.parseFloat(getComputedStyle(view).lineHeight);
  return Number.isFinite(lineHeight) && lineHeight > 0 ? lineHeight : 21;
}

function scrollToLine(view: HTMLElement, line: number) {
  view.scrollTop = Math.max(
    0,
    line * lineHeightFor(view) - view.clientHeight * 0.35,
  );
  if (view === currentView) {
    syncScroll(currentView, savedView);
  } else {
    syncScroll(savedView, currentView);
  }
}

function handleRulerClick(view: HTMLElement, event: Event) {
  const target = event.target;
  if (!(target instanceof HTMLElement) || !target.dataset.line) {
    return;
  }

  scrollToLine(view, Number(target.dataset.line));
}

currentView.addEventListener("pointerover", handleTokenHover);
savedView.addEventListener("pointerover", handleTokenHover);
currentView.addEventListener("scroll", () => syncScroll(currentView, savedView));
savedView.addEventListener("scroll", () => syncScroll(savedView, currentView));
currentRuler.addEventListener("click", (event) =>
  handleRulerClick(currentView, event),
);
savedRuler.addEventListener("click", (event) =>
  handleRulerClick(savedView, event),
);
fixtureList.addEventListener("click", (event) => {
  const target = event.target;
  if (!(target instanceof HTMLElement)) {
    return;
  }

  const row = target.closest("[data-fixture]");
  if (!(row instanceof HTMLElement) || !row.dataset.fixture) {
    return;
  }

  void loadFixture(row.dataset.fixture);
});
themeSelect.addEventListener("change", () => {
  setTheme(themeSelect.value);
});

async function acceptCurrentSnapshot() {
  if (!selectedFixture || acceptSnapshot.disabled) return;

  acceptSnapshot.disabled = true;
  try {
    await fetchJson<{ ok: true }>("/__grammar/snapshot", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        name: selectedFixture,
        snapshot: currentSnapshot,
      }),
    });

    savedSnapshot = currentSnapshot;
    fixtureCache.set(selectedFixture, { source, snapshot: savedSnapshot });
    savedTokens = parseSnapshot(savedSnapshot);
    renderPanes();
    updateComparison();
    updateInspect();
    setFixtureSummary({ name: selectedFixture, status: "same", diffCount: 0 });
  } catch {
    updateComparison();
  }
}

acceptSnapshot.addEventListener("click", () => {
  void acceptCurrentSnapshot();
});
function isShortcutTarget(target: EventTarget | null) {
  if (!(target instanceof HTMLElement)) {
    return true;
  }

  return !(
    target instanceof HTMLButtonElement ||
    target instanceof HTMLAnchorElement ||
    target instanceof HTMLSelectElement ||
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    target.isContentEditable
  );
}

window.addEventListener("keydown", (event) => {
  const isAcceptKey = event.key === "Enter" || event.key === " ";
  if (isAcceptKey && !event.repeat && isShortcutTarget(event.target)) {
    event.preventDefault();
    void acceptCurrentSnapshot();
  }
});

initializeThemeSelector();
void loadFixtures();
