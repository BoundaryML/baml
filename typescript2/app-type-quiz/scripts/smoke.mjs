// Boot the app in a real browser and sit a short practice sitting.
//
// Everything else in this app is checked by the compiler or by the quiz's own
// BAML suite. What neither can tell us is whether the two WebAssembly modules
// actually load in a page and talk to each other, whether what crosses the
// bridge comes back whole, and whether a sitting survives a reload -- the
// parts most likely to break and the least likely to break loudly.
//
// It drives the dev server rather than a built bundle, because the dev server
// is what a person runs and it is the stricter of the two: only it enforces
// which paths outside the app root may be served, and the bridge's
// WebAssembly is one of them.

import { spawn } from 'node:child_process';
import { readFile } from 'node:fs/promises';
import { chromium } from 'playwright';

const PORT = 4321;
const URL = `http://localhost:${PORT}/`;
const WAIT = { timeout: 120000 };

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

function expect(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

/** The text of the first element matching `selector`, trimmed. */
async function text(page, selector) {
  return ((await page.textContent(selector)) ?? '').trim();
}

/** Wait until the progress line reads exactly `wanted`. */
async function progress(page, wanted) {
  await page.waitForFunction(
    (w) => document.querySelector('.progress')?.textContent?.trim() === w,
    wanted,
    WAIT,
  );
}

/**
 * Check a reveal against itself. The mark and the outcome are worked out
 * separately -- the mark by judging the answer against the case's key, the
 * outcome by asking the compiler -- so their agreement is what catches a
 * boundary that quietly loses the answer, which is how every answer once
 * read wrong. The points must follow the mark under the rule shown.
 */
async function checkReveal(page, said, rule) {
  await page.waitForSelector('.reveal', WAIT);
  const verdict = await text(page, '.reveal p');
  const compiled = verdict.includes('It compiles.');
  expect(
    compiled || verdict.includes('The compiler rejects it.'),
    `the reveal does not say what the compiler did: ${verdict}`,
  );
  const points = await text(page, '.reveal .points');
  if (said === 'unsure') {
    expect(
      verdict.startsWith('You held back.'),
      `held back, but the reveal says: ${verdict}`,
    );
    expect(points === rule.unsure, `held back, but the points read ${points}`);
  } else {
    const marked = verdict.startsWith('Right.');
    expect(
      marked || verdict.startsWith('Wrong.'),
      `the reveal does not mark the answer: ${verdict}`,
    );
    const agreed = (said === 'compiles') === compiled;
    expect(
      marked === agreed,
      `answered "${said}" and the case ${compiled ? 'compiles' : 'does not'}, but the reveal says: ${verdict}`,
    );
    const wanted = marked ? rule.right : rule.wrong;
    expect(
      points === wanted,
      `marked ${verdict} but the points read ${points}`,
    );
  }
  const claims = await page.locator('.claims li').count();
  expect(claims > 0, 'the reveal shows no claims');
  const spans = await page.locator('.claims code').count();
  expect(spans > 0, 'no type in the claims is set as code');
  const literal = await text(page, '.claims');
  expect(!literal.includes('`'), `a backtick reached the page: ${literal}`);
  return { claims, verdict };
}

/** The points rule as shown, so every reveal is checked against it. */
async function ruleShown(page) {
  const line = await text(page, '.rule-line');
  const found = line.match(/^Right (\S+) · wrong (\S+) · not sure (\S+)$/);
  expect(found !== null, `the points rule is not shown whole: ${line}`);
  return { right: found[1], unsure: found[3], wrong: found[2] };
}

async function checkCase(page) {
  await page.waitForSelector('pre.case', WAIT);
  const source = await text(page, 'pre.case');
  expect(
    source.includes('function') ||
      source.includes('class') ||
      source.includes('type '),
    `the case does not look like BAML source: ${source.slice(0, 120)}`,
  );
  expect(
    await page.locator('pre.case .hljs-keyword').first().isVisible(),
    'the case is not highlighted: no token spans',
  );
  const buttons = await page
    .locator('.question .choices button')
    .allTextContents();
  const offered = new Set(buttons.map((b) => b.trim()));
  for (const wanted of ['It compiles', 'It is rejected', 'Not sure']) {
    expect(offered.has(wanted), `no "${wanted}" button among ${buttons}`);
  }
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

  // Three empty slots; start a practice sitting of three in the first.
  await page.waitForSelector('button:has-text("New sitting")', WAIT);
  expect(
    (await page.locator('.slot').count()) === 3,
    'the home does not show three slots',
  );
  await page.locator('button:has-text("New sitting")').first().click();
  await page.click('label:has-text("Practice") input[type=radio]');
  await page.fill('label:has-text("Practice") input.count', '3');
  // The button says "Loading…" until the formatter has instantiated.
  await page.waitForSelector('button:has-text("Start")', WAIT);
  await page.click('button:has-text("Start")');

  await progress(page, 'Case 1 of 3');
  await checkCase(page);
  const rule = await ruleShown(page);
  expect(
    rule.right === '+4' && rule.wrong === '−12' && rule.unsure === '0',
    `the default bar of 75% should read +4 / −12 / 0, not ${JSON.stringify(rule)}`,
  );

  await page.click('button:has-text("It compiles")');
  const first = await checkReveal(page, 'compiles', rule);
  await page.click('button:has-text("Next")');

  await progress(page, 'Case 2 of 3');
  await checkCase(page);
  await page.click('button:has-text("Not sure")');
  await checkReveal(page, 'unsure', rule);
  await page.click('button:has-text("Next")');
  await progress(page, 'Case 3 of 3');

  // Leave mid-sitting and come back: the slot holds two answers and the
  // third case is offered again from where the sitting stood.
  await page.reload({ waitUntil: 'load' });
  await page.waitForSelector('button:has-text("Continue")', WAIT);
  const held = await text(page, '.slot');
  expect(
    held.includes('2 answered'),
    `the slot does not hold the sitting: ${held}`,
  );
  expect(
    held.includes('practice, 3 cases'),
    `the slot does not say what it holds: ${held}`,
  );
  await page.click('button:has-text("Continue")');
  await progress(page, 'Case 3 of 3');
  await checkCase(page);
  await page.click('button:has-text("It is rejected")');
  await checkReveal(page, 'rejected', rule);
  await page.click('button:has-text("Next")');

  // The readout, and the download it comes with.
  await page.waitForSelector('.done', WAIT);
  const readout = await text(page, '.readout');
  expect(
    readout.includes('rules mastered') && readout.includes('bar of 75%'),
    `the readout is missing mastery or calibration: ${readout}`,
  );
  expect(
    readout.includes('3 cases you asked for'),
    `the readout does not say why the sitting ended: ${readout}`,
  );
  const [download] = await Promise.all([
    page.waitForEvent('download', WAIT),
    page.click('button:has-text("Download the sitting")'),
  ]);
  const transcript = JSON.parse(await readFile(await download.path(), 'utf8'));
  expect(
    Array.isArray(transcript.exchanges) && transcript.exchanges.length === 3,
    `the transcript does not hold three exchanges: ${JSON.stringify(transcript).slice(0, 200)}`,
  );
  expect(
    transcript.learner !== null && transcript.learner.knobs.practice === true,
    'the transcript does not carry what the model concluded',
  );
  const verdicts = transcript.exchanges.map((e) => e.answer.verdict);
  expect(
    verdicts[0] !== null && verdicts[1] === null && verdicts[2] !== null,
    `the transcript does not record the abstention: ${JSON.stringify(verdicts)}`,
  );

  // A mastery sitting shows no meter: the case number, and no total.
  await page.click('button:has-text("Back to the slots")');
  await page.waitForSelector('button:has-text("New sitting")', WAIT);
  await page.locator('button:has-text("New sitting")').first().click();
  await page.waitForSelector('button:has-text("Start")', WAIT);
  await page.click('button:has-text("Start")');
  await progress(page, 'Case 1');
  await checkCase(page);
  await page.click('button:has-text("Leave")');
  await page.waitForSelector('button:has-text("New sitting")', WAIT);
  const second = await text(page, '.slot >> nth=1');
  expect(
    second.includes('0 answered') && second.includes('until mastered'),
    `the second slot does not hold the mastery sitting: ${second}`,
  );

  console.log(
    `smoke: a practice sitting of 3 was sat across a reload, "${first.verdict}" with ${first.claims} claims, the download carries the learner, and a mastery sitting shows no total`,
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
