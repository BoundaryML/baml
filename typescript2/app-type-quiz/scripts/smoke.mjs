// Boot the app in a real browser and answer one question.
//
// Everything else in this app is checked by the compiler or by the quiz's own
// BAML suite. What neither can tell us is whether the two WebAssembly modules
// actually load in a page and talk to each other, which is the part most
// likely to break and the least likely to break loudly.
//
// It drives the dev server rather than a built bundle, because the dev server
// is what a person runs and it is the stricter of the two: only it enforces
// which paths outside the app root may be served, and the bridge's
// WebAssembly is one of them.

import { spawn } from 'node:child_process';
import { chromium } from 'playwright';

const PORT = 4321;
const URL = `http://localhost:${PORT}/`;

function startServer() {
  const server = spawn(
    'npx',
    ['vite', '--port', String(PORT), '--strictPort'],
    {
      stdio: ['ignore', 'pipe', 'pipe'],
    },
  );
  const said = [];
  server.stderr.on('data', (chunk) => said.push(String(chunk)));
  return new Promise((resolve, reject) => {
    const timer = setTimeout(
      () => reject(new Error(`vite did not start: ${said.join('')}`)),
      60000,
    );
    server.stdout.on('data', (chunk) => {
      if (String(chunk).includes(String(PORT))) {
        clearTimeout(timer);
        resolve(server);
      }
    });
    server.on('exit', (code) =>
      reject(new Error(`vite exited with ${code}: ${said.join('')}`)),
    );
  });
}

const server = await startServer();
const complaints = [];
let browser = null;
let failure = null;
try {
  browser = await chromium.launch();
  const page = await browser.newPage();
  page.on('console', (message) => {
    if (message.type() === 'error') {
      complaints.push(message.text());
    }
  });
  page.on('pageerror', (error) => complaints.push(String(error)));

  await page.goto(URL, { waitUntil: 'load' });

  // The button says "Loading…" until the formatter has instantiated.
  await page.waitForFunction(
    () =>
      document.querySelector('button')?.textContent?.includes('Start') ?? false,
    { timeout: 120000 },
  );
  await page.fill('input[type=number] >> nth=1', '3');
  await page.click('button:has-text("Start")');

  await page.waitForSelector('pre.case', { timeout: 120000 });
  const source = (await page.textContent('pre.case')) ?? '';
  if (
    !source.includes('function') &&
    !source.includes('class') &&
    !source.includes('type ')
  ) {
    throw new Error(
      `the first case does not look like BAML source: ${source.slice(0, 120)}`,
    );
  }
  if (!(await page.locator('pre.case .hljs-keyword').first().isVisible())) {
    throw new Error('the case is not highlighted: no token spans');
  }

  await page.click('button:has-text("It compiles")');
  await page.waitForSelector('.reveal', { timeout: 120000 });
  const claims = await page.locator('.claims li').count();
  if (claims === 0) {
    throw new Error('the reveal shows no claims');
  }
  // Claims name types in backticks. If those are being rendered literally, the
  // reader is doing the typesetting.
  const spans = await page.locator('.claims code').count();
  if (spans === 0) {
    throw new Error('no type in the claims is set as code');
  }
  const literal = (await page.textContent('.claims')) ?? '';
  if (literal.includes('`')) {
    throw new Error(`a backtick reached the page: ${literal.slice(0, 120)}`);
  }
  const verdict = (await page.textContent('.reveal p')) ?? '';
  if (!verdict.startsWith('Right.') && !verdict.startsWith('Wrong.')) {
    throw new Error(`the reveal does not mark the answer: ${verdict}`);
  }
  // The mark and the outcome have to agree. They are worked out separately --
  // the mark by judging the answer against the case's key, the outcome by
  // asking the compiler -- so this is the assertion that catches a boundary
  // that quietly loses the answer, which is how every answer once read wrong.
  const said = 'It compiles';
  const compiled = verdict.includes('It compiles.');
  const marked = verdict.startsWith('Right.');
  if (marked !== compiled) {
    throw new Error(
      `answered "${said}" and the case ${compiled ? 'compiles' : 'does not'}, but the reveal says: ${verdict}`,
    );
  }
  const compiler = await page.locator('.compiler').count();

  console.log(
    `smoke: a case rendered and highlighted, "${verdict.trim()}", ${claims} claims with ${spans} typed spans, ${compiler} compiler panel(s)`,
  );
} catch (error) {
  failure = error;
} finally {
  // The server outlives this process if it is not killed, and the next run
  // would then fail on a port that is already taken rather than on anything
  // to do with the app.
  await browser?.close();
  server.kill();
}

if (complaints.length > 0) {
  console.error('the page logged errors:');
  for (const complaint of complaints) {
    console.error(`  ${complaint}`);
  }
}
if (failure !== null) {
  console.error(`smoke: FAILED — ${failure.message}`);
  process.exit(1);
}
if (complaints.length > 0) {
  process.exit(1);
}
