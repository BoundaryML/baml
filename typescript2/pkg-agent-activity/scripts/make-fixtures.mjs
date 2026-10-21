#!/usr/bin/env node
// Builds the recorded sessions the Agent Activity UI replays when it runs without
// a `baml agent watch` server (`/agent-activity?mock=<id>`).
//
// Most scenarios replay real history from this repository as if an agent had
// typed it: a commit's hunks become separate edits, new files become writes
// (optionally built up one top-level definition block at a time), and the
// timing follows the burst/think-time rhythm measured from recorded Claude
// Code sessions. One scenario (scenarios/resume-screener.mjs) is scripted to
// cover every event kind. All of them are written through ScenarioBuilder as
// the protocol messages a live server sends; edits are line splices, so a
// file edited many times is stored once.
//
// Usage: node scripts/make-fixtures.mjs   (run from anywhere inside the repo)

import { execFileSync } from 'node:child_process';
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { rng, ScenarioBuilder } from './scenario-builder.mjs';
import { resumeScreener } from './scenarios/resume-screener.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const outDir = resolve(here, '../src/mock/fixtures');
const repo = execFileSync('git', ['rev-parse', '--show-toplevel'], {
  cwd: here,
})
  .toString()
  .trim();

const git = (...args) =>
  execFileSync('git', args, {
    cwd: repo,
    maxBuffer: 256 * 1024 * 1024,
    stdio: ['ignore', 'pipe', 'ignore'],
  }).toString();

// ── Timing model ────────────────────────────────────────────────────────────
// Measured from 25 recent Claude Code sessions (1,283 Edit/Write calls): edits
// arrive in short bursts (55% are a single edit, p90 six) about 5.6s apart,
// separated by a minute or so of reading, building, and testing. Only 3% of
// an agent's active time is spent inside a burst. Values are milliseconds.
const TIMING = {
  // Cumulative probabilities for burst lengths 1, 2, 3, ... (tail spreads to 9).
  burstLengthCdf: [0.45, 0.65, 0.77, 0.85, 0.9, 0.94, 0.97, 0.99, 1],
  commitPause: 12000,
  inBurstMedian: 5600,
  inBurstSpread: 0.6,
  thinkMedian: 58000,
  thinkSpread: 0.8,
};

function burstLength(rand) {
  const r = rand();
  return TIMING.burstLengthCdf.findIndex((p) => r <= p) + 1;
}

function logNormal(rand, median, spread) {
  // Box-Muller; spread is the sigma of the underlying normal.
  const u = Math.max(rand(), 1e-9);
  const v = rand();
  const z = Math.sqrt(-2 * Math.log(u)) * Math.cos(2 * Math.PI * v);
  return median * Math.exp(spread * z);
}

// ── Diff parsing ────────────────────────────────────────────────────────────

/** Hunks of `git diff -U0` between two revisions for one path. */
function hunks(from, to, path) {
  const out = git(
    'diff',
    '-U0',
    '--no-color',
    '--diff-algorithm=histogram',
    from,
    to,
    '--',
    path,
  );
  const result = [];
  let current = null;
  for (const line of out.split('\n')) {
    const header = /^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@/.exec(line);
    if (header) {
      current = {
        added: [],
        newCount: header[4] === undefined ? 1 : Number(header[4]),
        newStart: Number(header[3]),
        oldCount: header[2] === undefined ? 1 : Number(header[2]),
        oldStart: Number(header[1]),
      };
      result.push(current);
      continue;
    }
    if (current && line.startsWith('+') && !line.startsWith('+++')) {
      current.added.push(line.slice(1));
    }
  }
  return result;
}

/**
 * Merge hunks separated by at most `gap` unchanged lines into one edit, the
 * way an agent would rewrite a region in a single Edit call. Unchanged lines
 * between merged hunks are carried in both the removed and inserted text.
 */
function mergeHunks(list, oldLines, gap) {
  const merged = [];
  for (const h of list) {
    const prev = merged.at(-1);
    const hStart = h.oldCount === 0 ? h.oldStart : h.oldStart - 1; // 0-based
    if (prev) {
      const prevEnd = prev.start + prev.remove;
      if (hStart - prevEnd <= gap) {
        const bridge = oldLines.slice(prevEnd, hStart);
        prev.insert.push(...bridge, ...h.added);
        prev.remove = hStart + h.oldCount - prev.start;
        continue;
      }
    }
    merged.push({ insert: [...h.added], remove: h.oldCount, start: hStart });
  }
  return merged;
}

// ── Scenario assembly ───────────────────────────────────────────────────────

const LAYER_RULES = [
  [/baml_std\/.*\.baml$/, 0],
  [/baml\.toml$/, 0],
  [/\/src\/.*\.rs$/, 1],
  [/\/ns_[^/]+\/.*\.baml$|\/tools\/.*\.baml$/, 2],
  [/\.(ts|tsx)$/, 3],
  [/tests?\//, 4],
  [/\.snap$/, 5],
  [/\.(md|mdx)$/, 6],
];

function layerOf(path) {
  for (const [re, layer] of LAYER_RULES) if (re.test(path)) return layer;
  return 3;
}

/** Split new-file content into chunks at top-level definition boundaries. */
function definitionChunks(lines, maxChunks) {
  const boundaries = [0];
  for (let i = 1; i < lines.length; i++) {
    const line = lines[i];
    const prevBlank = lines[i - 1].trim() === '';
    if (
      prevBlank &&
      /^(export |pub |async )?(class|function|fn|struct|enum|interface|type|impl|trait|const|client|test|template_string|mod)\b/.test(
        line,
      )
    ) {
      boundaries.push(i);
    }
  }
  if (boundaries.length <= 1 || maxChunks <= 1) return [lines];
  const step = Math.ceil(boundaries.length / maxChunks);
  const cuts = boundaries.filter((_, i) => i % step === 0);
  const chunks = [];
  for (let i = 0; i < cuts.length; i++) {
    chunks.push(lines.slice(cuts[i], cuts[i + 1] ?? lines.length));
  }
  return chunks;
}

function trimLines(lines, max) {
  if (lines.length <= max) return lines;
  // Cut at the last blank line before the cap so a definition is not split.
  let cut = max;
  for (let i = max; i > max * 0.7; i--) {
    if (lines[i].trim() === '') {
      cut = i;
      break;
    }
  }
  return lines.slice(0, cut);
}

function isBinary(from, to, path) {
  const stat = git('diff', '--numstat', from, to, '--', path).trim();
  return stat.startsWith('-\t-');
}

/**
 * Turn the file-level operations for one commit into agent-shaped steps.
 * Returns ops in the order they should play, before timing is assigned.
 */
function commitOps(from, to, opts, rand, contents) {
  const nameStatus = git('diff', '--name-status', '--no-renames', from, to)
    .trim()
    .split('\n')
    .filter(Boolean)
    .map((l) => l.split('\t'));
  const files = nameStatus
    .map(([status, path]) => ({ path, status }))
    .filter(({ path }) => opts.include(path))
    .filter(({ path }) => !isBinary(from, to, path));

  const perFile = [];
  for (const { status, path } of files) {
    const ops = [];
    if (status === 'D') {
      ops.push({ kind: 'delete', path });
    } else if (status === 'A' || !contents.has(path)) {
      const all = git('show', `${to}:${path}`).split('\n');
      const lines = trimLines(all, opts.maxNewFileLines);
      const chunks = definitionChunks(lines, opts.buildUpNewFiles ? 3 : 1);
      ops.push({ content: chunks[0].join('\n'), kind: 'write', path });
      let at = chunks[0].length;
      for (const chunk of chunks.slice(1)) {
        ops.push({ at, insert: chunk, kind: 'edit', path, remove: 0 });
        at += chunk.length;
      }
    } else {
      const oldLines = git('show', `${from}:${path}`).split('\n');
      const merged = mergeHunks(hunks(from, to, path), oldLines, opts.mergeGap);
      // Apply in a lightly shuffled order: agents mostly move down a file but
      // jump back to fix imports or signatures. Track each hunk's shift.
      const order = merged.map((_, i) => i);
      for (let i = 1; i < order.length; i++) {
        if (rand() < 0.25) [order[i - 1], order[i]] = [order[i], order[i - 1]];
      }
      const applied = [];
      for (const idx of order) {
        const h = merged[idx];
        let shift = 0;
        for (const a of applied)
          if (a.start < h.start) shift += a.insert.length - a.remove;
        ops.push({
          at: h.start + shift,
          insert: h.insert,
          kind: 'edit',
          path,
          remove: h.remove,
        });
        applied.push(h);
      }
    }
    perFile.push({ layer: layerOf(path), ops, path });
  }

  perFile.sort((a, b) => a.layer - b.layer || a.path.localeCompare(b.path));

  // Interleave: take a few ops from a file, sometimes hop to the next file
  // and come back, the way an agent threads a change through its callers.
  const queue = perFile.map((f) => ({ ...f, ops: [...f.ops] }));
  const ordered = [];
  while (queue.length) {
    const file = queue[0];
    const take = Math.max(
      1,
      Math.min(file.ops.length, Math.round(1 + rand() * 3)),
    );
    ordered.push(...file.ops.splice(0, take));
    if (file.ops.length === 0) queue.shift();
    else if (queue.length > 1 && rand() < 0.45) {
      // Hop to the next file and return afterwards.
      const [f] = queue.splice(0, 1);
      queue.splice(1, 0, f);
    }
  }
  return ordered;
}

function applyOp(b, op) {
  if (op.kind === 'delete') b.delete(op.path);
  else if (op.kind === 'write') b.write(op.path, op.content);
  else b.splice(op.path, [{ at: op.at, insert: op.insert, remove: op.remove }]);
}

function buildScenario(spec) {
  const rand = rng(spec.seed);
  const commits = spec.commits;
  const base = git('rev-parse', `${commits[0]}^`).trim();
  const baseSubject = git('show', '-s', '--format=%s', base).trim();
  const b = new ScenarioBuilder({
    branch: spec.branch,
    description: spec.description,
    id: spec.id,
    project: spec.project,
    startHead: { sha: base.slice(0, 9), subject: baseSubject },
    title: spec.title,
  });

  // Baseline: the pre-session content of every file any commit touches.
  const touched = new Set();
  for (const c of commits) {
    for (const p of git('diff', '--name-only', '--no-renames', `${c}^`, c)
      .trim()
      .split('\n')) {
      if (p && spec.include(p) && !isBinary(`${c}^`, c, p)) touched.add(p);
    }
  }
  for (const p of touched) {
    try {
      b.baseline(p, git('show', `${base}:${p}`));
    } catch {
      // Created during the session.
    }
  }

  b.at(1200);
  let edits = 0;
  let burstLeft = 0;
  const notes = [...(spec.notes ?? [])];
  commits.forEach((c, index) => {
    const ops = commitOps(`${c}^`, c, spec, rand, b.contents);
    for (const op of ops) {
      if (burstLeft <= 0) {
        if (edits)
          b.wait(
            logNormal(rand, TIMING.thinkMedian, TIMING.thinkSpread) * spec.pace,
          );
        burstLeft = burstLength(rand);
      } else {
        b.wait(
          logNormal(rand, TIMING.inBurstMedian, TIMING.inBurstSpread) *
            spec.pace,
        );
      }
      // Narrate the first edit to each group of files, a moment before it lands.
      const note = notes.findIndex((n) => n.path.test(op.path));
      if (note >= 0) {
        b.note(notes[note].text);
        notes.splice(note, 1);
        b.wait(1600);
      }
      burstLeft--;
      edits++;
      applyOp(b, op);
    }
    const sha = git('rev-parse', '--short=9', c).trim();
    const subject = git('show', '-s', '--format=%s', c).trim();
    // Checks the agent ran before committing (tests, lints).
    spec.beforeCommit?.(b, { index, sha, subject });
    if (spec.emitCommits) {
      b.wait(TIMING.commitPause * spec.pace);
      b.commit(sha, subject);
      burstLeft = 0;
    }
  });
  return b.build();
}

/** A command run that fails first, then passes after the agent's fix. */
function commandRun(b, { command, touches, status, log, seconds }) {
  b.wait(4000);
  const id = b.start({ command, kind: 'command', touches });
  b.wait(seconds * 1000);
  b.finish(id, { log, status });
}

// ── Scenarios ───────────────────────────────────────────────────────────────

const excludeNoise = (p) =>
  !/(^|\/)(pnpm-lock\.yaml|Cargo\.lock|bun\.lock)$/.test(p) &&
  !p.startsWith('.github/');

const SCENARIOS = [
  {
    beforeCommit: (b) => {
      const tests =
        'baml_language/crates/baml_tests/baml_src/ns_llm_typesafeai_examples/examples.baml';
      b.wait(6000);
      const id = b.start({
        command:
          'baml test --project crates/baml_tests ns_llm_typesafeai_examples',
        kind: 'baml-test',
        target: { name: 'ns_llm_typesafeai_examples', path: tests },
        touches: [
          tests,
          'baml_language/crates/baml_tests/baml_src/ns_llm_typesafeai/typesafeai.baml',
          'baml_language/crates/baml_builtins2/baml_std/typesafeai/client.baml',
          'baml_language/crates/baml_builtins2/baml_std/typesafeai/ns_internal/jev.baml',
        ],
      });
      b.wait(8200);
      b.finish(id, {
        log: [
          'running 24 tests from ns_llm_typesafeai_examples',
          '  ok  1.1 bool request shape (3ms)',
          '  ok  1.2 float request shape (2ms)',
          '  ok  2.1 enum routes to a team (4ms)',
          '  ok  3.4 class with aliases (5ms)',
          '  … 20 more',
          '24 passed, 0 failed in 1.9s',
        ].join('\n'),
        status: 'passed',
      });
    },
    branch: 'sam/typesafe-jev',
    buildUpNewFiles: true,
    commits: ['05a266fc7'],
    description:
      'Standard-library BAML, a reflection change in the VM, new tests, and a blog post across 18 files.',
    emitCommits: true,
    id: 'typesafe-ai',
    // Drop one-line touches to multi-thousand-line files; they add hundreds of
    // kilobytes of baseline for a single highlighted line.
    include: (p) =>
      excludeNoise(p) &&
      !/(sdkgen_go|baml_fmt|baml_compiler2_ast)\/src\/lib\.rs$|lower_cst\.rs$/.test(
        p,
      ),
    maxNewFileLines: 260,
    mergeGap: 4,
    notes: [
      {
        path: /runner\.baml|turn\.baml/,
        text: 'Adding a TypeSafe AI client to the standard library. First the runner and turn types its results flow through.',
      },
      {
        path: /typesafeai\/(client\.baml|baml\.toml)/,
        text: 'Now the client package itself, then the JEV wire format in `ns_internal/jev.baml`.',
      },
      {
        path: /type_kinds\.rs/,
        text: 'The adapter needs reflection to report type kinds, so the VM has to expose them.',
      },
      {
        path: /ns_llm_typesafeai\/typesafeai\.baml/,
        text: 'Writing tests for the adapter internals: wire capture, HTTP errors, malformed bodies.',
      },
      {
        path: /examples\.baml/,
        text: "And tests from the user's side: a declared type in, the exact request out, a typed value back.",
      },
      { path: /\.mdx$/, text: 'Writing the design up as a blog post.' },
    ],
    pace: 1,
    project: 'baml2',
    seed: 7,
    title: 'Add a typesafe AI client',
  },
  {
    // The agent ran the protocol tests before each commit. After the third
    // commit Clippy failed, which the fourth commit ("Satisfy Clippy") fixes.
    beforeCommit: (b, { index, subject }) => {
      const annotations = 'baml_language/crates/baml_ide/src/annotations.rs';
      const protocol = 'baml_language/crates/baml_lsp/tests/protocol.rs';
      const clippy = 'cargo clippy -p baml_ide --all-targets -- -D warnings';
      if (/Clippy/.test(subject)) {
        commandRun(b, {
          command: clippy,
          log: 'Finished `dev` profile [unoptimized + debuginfo] target(s) in 18.84s',
          seconds: 19,
          status: 'passed',
          touches: [annotations],
        });
        return;
      }
      commandRun(b, {
        command: 'cargo nextest run -p baml_lsp -E "test(/inlay/)"',
        log: `     Summary [   2.0${index}6s] ${14 + index} tests run: ${14 + index} passed, 0 skipped`,
        seconds: 34,
        status: 'passed',
        touches: [protocol, annotations],
      });
      if (index === 2) {
        commandRun(b, {
          command: clippy,
          log: [
            'error: this `if` has identical blocks',
            '   --> crates/baml_ide/src/annotations.rs:612:13',
            '    |',
            '612 |             } else {',
            '    |             ^^^^^^',
            '    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#if_same_then_else',
            'error: could not compile `baml_ide` (lib) due to 1 previous error',
          ].join('\n'),
          seconds: 21,
          status: 'failed',
          touches: [annotations],
        });
      }
    },
    branch: 'codex/less-noisy-inlay-hints',
    buildUpNewFiles: false,
    commits: ['fa60295c4', 'a5ca20b0b', 'ef9dcb490', 'd4bb63c79', '027172ee9'],
    description:
      'Five commits refining one Rust module and its protocol tests.',
    emitCommits: true,
    id: 'inlay-hints',
    include: excludeNoise,
    maxNewFileLines: 400,
    mergeGap: 3,
    notes: [
      {
        path: /annotations\.rs/,
        text: 'Inlay hints are noisy on obvious literals. Tightening which hints `annotations.rs` emits, and linking each hint to its declaration.',
      },
      {
        path: /protocol\.rs/,
        text: 'Adding protocol tests that pin which hints disappear.',
      },
      {
        path: /requests\.rs/,
        text: 'Hint labels now carry a location, so the request handler has to pass it through.',
      },
    ],
    pace: 1,
    project: 'baml2',
    seed: 11,
    title: 'Quiet the inlay hints',
  },
  {
    branch: 'type-quiz',
    buildUpNewFiles: true,
    commits: ['da9b950f4'],
    description:
      'A new BAML project and its React front end, written from scratch.',
    emitCommits: false,
    id: 'type-quiz',
    include: (p) =>
      /tools\/type_quiz\/(ns_bank\/(models|names|orders|features|weights|registry)|ns_algebra\/(ty|model|measure)|baml\.toml)/.test(
        p,
      ) || /app-type-quiz\/src\/(quiz|judge|format|main|home)\.tsx?$/.test(p),
    maxNewFileLines: 180,
    mergeGap: 4,
    notes: [
      {
        path: /ns_algebra\//,
        text: 'Starting the type quiz with a small type algebra in BAML: `Ty` and its constructors first.',
      },
      {
        path: /ns_bank\//,
        text: 'Now the question bank: models of how learners get types wrong, and the items that catch each one.',
      },
      {
        path: /app-type-quiz\//,
        text: 'The BAML side can drive a page now. Writing the React front end.',
      },
    ],
    pace: 0.8,
    project: 'baml2',
    seed: 23,
    title: 'Build the type quiz',
  },
];

// ── Workflows ───────────────────────────────────────────────────────────────
// A live server recompiles a BAML project after its `.baml` files change and
// sends the workflows. Recordings get the same messages from the same code:
// after each change to a project's `.baml` files, the project as it stands is
// written to a temporary directory and `baml agent workflows` prints them.

const bamlCli =
  process.env.BAML_CLI ?? resolve(repo, 'baml_language/target/debug/baml-cli');

function withFlows(scenario, projectDir) {
  if (!existsSync(bamlCli)) {
    console.warn(
      `  (no ${bamlCli}; ${scenario.id} is recorded without workflows)`,
    );
    return scenario;
  }
  const prefix = projectDir ? `${projectDir}/` : '';
  const contents = new Map();
  const out = [];
  let pendingAt = null;
  const flush = () => {
    if (pendingAt === null) return;
    const dir = mkdtempSync(join(tmpdir(), 'aa-flows-'));
    for (const [path, text] of contents) {
      if (!path.startsWith(prefix)) continue;
      const target = join(dir, path.slice(prefix.length));
      mkdirSync(dirname(target), { recursive: true });
      writeFileSync(target, text);
    }
    try {
      const json = execFileSync(bamlCli, ['agent', 'workflows', dir], {
        stdio: ['ignore', 'pipe', 'ignore'],
      }).toString();
      const flows = JSON.parse(json);
      const rebase = (p) => (p && !p.startsWith('/') ? `${prefix}${p}` : p);
      for (const w of flows.workflows) {
        w.path = rebase(w.path);
        for (const node of Object.values(w.graph.nodes ?? {})) {
          if (node.sourceSpan)
            node.sourceSpan.filePath = rebase(node.sourceSpan.filePath);
        }
      }
      out.push({
        message: {
          project: projectDir || '.',
          type: 'flows',
          workflows: flows.workflows,
        },
        t: pendingAt + 350,
      });
    } catch {
      // A half-written project that does not compile yet: no workflows at this point.
    }
    rmSync(dir, { force: true, recursive: true });
    pendingAt = null;
  };
  for (const event of scenario.events) {
    // Changes within a second settle into one recompile, as on the server.
    if (pendingAt !== null && event.t - pendingAt > 1000) flush();
    out.push(event);
    const m = event.message;
    if (m.type !== 'change') continue;
    const { path } = m.change;
    if (m.baseline !== undefined && m.baseline !== null && !contents.has(path))
      contents.set(path, m.baseline);
    if (m.change.after === null) contents.delete(path);
    else if (m.change.after !== undefined) contents.set(path, m.change.after);
    else {
      const lines = (contents.get(path) ?? '').split('\n');
      for (const s of m.change.splices)
        lines.splice(s.at, s.remove, ...s.insert);
      contents.set(path, lines.join('\n'));
    }
    if (
      path.startsWith(prefix) &&
      (path.endsWith('.baml') || path.endsWith('baml.toml'))
    )
      pendingAt = event.t;
  }
  flush();
  out.sort((a, b) => a.t - b.t);
  return { ...scenario, events: out };
}

mkdirSync(outDir, { recursive: true });
const byId = Object.fromEntries(SCENARIOS.map((s) => [s.id, s]));
const recorded = [
  withFlows(resumeScreener(), ''),
  buildScenario(byId['typesafe-ai']),
  withFlows(buildScenario(byId['type-quiz']), 'baml_language/tools/type_quiz'),
  buildScenario(byId['inlay-hints']),
];
for (const scenario of recorded) {
  const path = join(outDir, `${scenario.id}.json`);
  const json = JSON.stringify(scenario);
  writeFileSync(path, `${json}\n`);
  const kinds = {};
  for (const { message } of scenario.events) {
    const k = message.type === 'change' ? message.change.kind : message.type;
    kinds[k] = (kinds[k] ?? 0) + 1;
  }
  const minutes = (scenario.events.at(-1).t / 60000).toFixed(1);
  const counts = Object.entries(kinds)
    .map(([k, n]) => `${n} ${k}`)
    .join(', ');
  console.log(
    `${scenario.id}: ${counts} over ${minutes} min, ${(json.length / 1024).toFixed(0)} KB`,
  );
}
