#!/usr/bin/env node

import { spawn } from 'node:child_process';
import { appendFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';
import { helloWorld } from '../lib/baml-runner/examples.mjs';

const packageRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const require = createRequire(import.meta.url);
const nextRoot = path.dirname(require.resolve('next/package.json'));
const nextBin = path.join(nextRoot, 'dist/bin/next');
const sampleCount = positiveInteger('BAML_RUNNER_BROWSER_SAMPLES', 5);
const warmRunsPerSample = positiveInteger('BAML_RUNNER_BROWSER_WARM_RUNS', 2);
const coldP95BudgetMs = positiveNumber('BAML_RUNNER_BROWSER_COLD_P95_MAX_MS', 10_000);
const warmP95BudgetMs = positiveNumber('BAML_RUNNER_BROWSER_WARM_P95_MAX_MS', 250);

function positiveNumber(name, fallback) {
  const value = Number(process.env[name] ?? fallback);
  if (!Number.isFinite(value) || value <= 0) throw new Error(`${name} must be positive`);
  return value;
}

function positiveInteger(name, fallback) {
  const value = positiveNumber(name, fallback);
  if (!Number.isInteger(value)) throw new Error(`${name} must be an integer`);
  return value;
}

async function availablePort() {
  const server = createServer();
  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  const address = server.address();
  const port = typeof address === 'object' && address ? address.port : null;
  await new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve()));
  if (!port) throw new Error('could not reserve a local port');
  return port;
}

async function waitForServer(url, child) {
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    if (child && child.exitCode !== null) {
      throw new Error(`the docs server exited with code ${child.exitCode}`);
    }
    try {
      const response = await fetch(url, { redirect: 'manual' });
      if (response.status >= 200 && response.status < 500) return;
    } catch {}
    await delay(250);
  }
  throw new Error(`the docs server did not become ready at ${url}`);
}

async function startServer() {
  const configured = process.env.BAML_RUNNER_BROWSER_BASE_URL?.replace(/\/$/, '');
  if (configured) {
    await waitForServer(`${configured}/examples/runnable-baml`);
    return { baseUrl: configured };
  }

  const port = await availablePort();
  const baseUrl = `http://127.0.0.1:${port}`;
  const output = [];
  const child = spawn(process.execPath, [nextBin, 'start', '--hostname', '127.0.0.1', '--port', String(port)], {
    cwd: packageRoot,
    env: { ...process.env, NEXT_TELEMETRY_DISABLED: '1' },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  for (const stream of [child.stdout, child.stderr]) {
    stream.setEncoding('utf8');
    stream.on('data', (chunk) => {
      output.push(chunk);
      if (output.length > 40) output.shift();
    });
  }
  try {
    await waitForServer(`${baseUrl}/examples/runnable-baml`, child);
    return { baseUrl, child, output };
  } catch (error) {
    child.kill('SIGTERM');
    throw new Error(`${error.message}\n${output.join('')}`);
  }
}

async function stopServer(server) {
  if (!server.child || server.child.exitCode !== null) return;
  server.child.kill('SIGTERM');
  await Promise.race([
    new Promise((resolve) => server.child.once('exit', resolve)),
    delay(5_000).then(() => server.child.kill('SIGKILL')),
  ]);
}

function percentile(values, percent) {
  const sorted = [...values].sort((left, right) => left - right);
  return sorted[Math.max(0, Math.ceil((percent / 100) * sorted.length) - 1)];
}

async function executeRunner(page) {
  return page.evaluate(() => {
    const root = document.querySelector('.baml-runner');
    const button = root?.querySelector('button');
    if (!(root instanceof HTMLElement) || !(button instanceof HTMLButtonElement)) {
      throw new Error('the runnable BAML control is missing');
    }

    return new Promise((resolve, reject) => {
      let sawRunning = false;
      const startedAt = performance.now();
      const timer = window.setTimeout(() => finish(new Error('the browser runner timed out')), 45_000);
      const observer = new MutationObserver(inspect);

      function finish(error) {
        window.clearTimeout(timer);
        observer.disconnect();
        if (error) reject(error);
      }

      function inspect() {
        const state = root.dataset.state;
        if (state === 'running') sawRunning = true;
        if (!sawRunning || (state !== 'success' && state !== 'error')) return;

        const output = root.querySelector('.baml-runner-result code')?.textContent ?? null;
        const message = root.querySelector('.baml-runner-result span')?.textContent ?? null;
        const timings = Object.fromEntries(
          [...root.querySelectorAll('.baml-runner-timings > div')].map((row) => [
            row.querySelector('dt')?.textContent ?? '',
            row.querySelector('dd')?.textContent ?? '',
          ]),
        );
        window.clearTimeout(timer);
        observer.disconnect();
        resolve({
          durationMs: performance.now() - startedAt,
          message,
          output,
          state,
          timings,
        });
      }

      observer.observe(root, {
        attributes: true,
        attributeFilter: ['data-state'],
        childList: true,
        subtree: true,
      });
      button.click();
      inspect();
    });
  });
}

async function measureContext(browser, baseUrl, index) {
  const context = await browser.newContext();
  await context.addInitScript(() => {
    Object.defineProperty(navigator, 'connection', {
      configurable: true,
      value: { effectiveType: '4g', saveData: true },
    });
  });
  const wasmRequests = [];
  context.on('request', (request) => {
    if (request.url().endsWith('.wasm')) wasmRequests.push(request.url());
  });

  try {
    const page = await context.newPage();
    await page.goto(`${baseUrl}/examples/runnable-baml`, { waitUntil: 'networkidle' });
    await page.locator('.baml-runner button').waitFor({ state: 'visible' });
    await page.waitForTimeout(500);
    if (wasmRequests.length !== 0) {
      throw new Error(`sample ${index}: WASM loaded before the reader selected Run BAML`);
    }

    const cold = await executeRunner(page);
    if (cold.state !== 'success' || cold.output !== helloWorld.expected) {
      throw new Error(`sample ${index}: cold browser run returned ${cold.state}: ${cold.message ?? cold.output}`);
    }
    if (wasmRequests.length !== 1) {
      throw new Error(`sample ${index}: expected one WASM request, received ${wasmRequests.length}`);
    }

    const warm = [];
    for (let run = 0; run < warmRunsPerSample; run += 1) {
      const result = await executeRunner(page);
      if (result.state !== 'success' || result.output !== helloWorld.expected) {
        throw new Error(`sample ${index}: warm browser run returned ${result.state}: ${result.message ?? result.output}`);
      }
      warm.push(result);
    }
    if (wasmRequests.length !== 1) {
      throw new Error(`sample ${index}: warm runs fetched the WASM ${wasmRequests.length} times`);
    }
    return { cold, warm };
  } finally {
    await context.close();
  }
}

function milliseconds(value) {
  return `${value.toFixed(1)} ms`;
}

async function writeSummary(result) {
  if (!process.env.GITHUB_STEP_SUMMARY) return;
  const lines = [
    '## BAML browser runner',
    '',
    '| Measurement | p50 | p95 | CI ceiling |',
    '| --- | ---: | ---: | ---: |',
    `| Cold click to result | ${milliseconds(result.cold.p50Ms)} | ${milliseconds(result.cold.p95Ms)} | ${milliseconds(coldP95BudgetMs)} |`,
    `| Warm click to result | ${milliseconds(result.warm.p50Ms)} | ${milliseconds(result.warm.p95Ms)} | ${milliseconds(warmP95BudgetMs)} |`,
    '',
    `Chromium completed ${sampleCount} isolated cold runs and ${sampleCount * warmRunsPerSample} warm runs. Each cold page made zero WASM requests before the click and exactly one afterward.`,
    '',
  ];
  await appendFile(process.env.GITHUB_STEP_SUMMARY, lines.join('\n'));
}

const server = await startServer();
let browser;
try {
  browser = await chromium.launch({
    headless: true,
    executablePath: process.env.BAML_RUNNER_BROWSER_EXECUTABLE || undefined,
  });
  const samples = [];
  for (let index = 1; index <= sampleCount; index += 1) {
    samples.push(await measureContext(browser, server.baseUrl, index));
  }
  const coldValues = samples.map(({ cold }) => cold.durationMs);
  const warmValues = samples.flatMap(({ warm }) => warm.map(({ durationMs }) => durationMs));
  const result = {
    baseUrl: server.baseUrl,
    samples: { cold: coldValues, warm: warmValues },
    cold: { p50Ms: percentile(coldValues, 50), p95Ms: percentile(coldValues, 95) },
    warm: { p50Ms: percentile(warmValues, 50), p95Ms: percentile(warmValues, 95) },
  };
  console.log(JSON.stringify(result, null, 2));
  await writeSummary(result);
  if (result.cold.p95Ms > coldP95BudgetMs) {
    throw new Error(`cold browser p95 is ${milliseconds(result.cold.p95Ms)}; budget is ${milliseconds(coldP95BudgetMs)}`);
  }
  if (result.warm.p95Ms > warmP95BudgetMs) {
    throw new Error(`warm browser p95 is ${milliseconds(result.warm.p95Ms)}; budget is ${milliseconds(warmP95BudgetMs)}`);
  }
} finally {
  await browser?.close();
  await stopServer(server);
}
