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
//
// The sitting is asked for by session, so it is the same sitting every run:
// the same questions in the same order, of both kinds, and a failure here is
// a change in the quiz rather than a roll of the dice.

import { spawn } from 'node:child_process';
import { readFile } from 'node:fs/promises';
import { chromium } from 'playwright';

const PORT = 4321;
const SESSION = 7;
const CASES = 8;
const URL = `http://localhost:${PORT}/?session=${SESSION}`;
const WAIT = { timeout: 120000 };

/**
 * Start the dev server. It is handed back as soon as it is spawned, before
 * anything waits on it, because a server that never says it is ready is
 * still running and still has to be stopped.
 */
function startServer() {
  return spawn(
    'pnpm',
    ['exec', 'vite', '--port', String(PORT), '--strictPort'],
    {
      // Its own process group. `vite` spawns esbuild, and signalling only the
      // process we spawned leaves those children alive holding the stdio
      // pipes we opened, which keeps node's event loop alive long after the
      // run is over: CI cancelled a job thirty-two minutes after this script
      // had printed its success line.
      detached: true,
      stdio: ['ignore', 'pipe', 'pipe'],
    },
  );
}

/** Wait until `server` is listening, or fail saying why it is not. */
function listening(server) {
  const said = [];
  server.stderr.on('data', (chunk) => said.push(String(chunk)));
  return new Promise((resolve, reject) => {
    const timer = setTimeout(
      () => reject(new Error(`vite did not start: ${said.join('')}`)),
      60000,
    );
    const fail = (error) => {
      clearTimeout(timer);
      reject(error);
    };
    server.stdout.on('data', (chunk) => {
      if (String(chunk).includes(String(PORT))) {
        clearTimeout(timer);
        resolve();
      }
    });
    // A command that cannot be run at all is reported here, not by `exit`.
    server.on('error', fail);
    server.on('exit', (code) =>
      fail(new Error(`vite exited with ${code}: ${said.join('')}`)),
    );
  });
}

/** Stop the server and everything it started. */
function stopServer(server) {
  // No pid: it was never spawned, or spawning it failed, so nothing is running.
  if (server?.pid === undefined) {
    return;
  }
  try {
    // The negated pid is the process group: vite, esbuild, and the launcher.
    process.kill(-server.pid, 'SIGTERM');
  } catch {
    // Already gone, which is the same outcome.
  }
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

/** The points rule as shown, so every reveal is checked against it. */
async function ruleShown(page) {
  const line = await text(page, '.rule-line');
  const found = line.match(/^Right (\S+) · wrong (\S+) · not sure (\S+)$/);
  expect(found !== null, `the points rule is not shown whole: ${line}`);
  return { right: found[1], unsure: found[3], wrong: found[2] };
}

/** The question on screen: how many programs, and what may be said about them. */
async function question(page) {
  await page.waitForSelector('pre.case', WAIT);
  const programs = await page.locator('.program').count();
  expect(programs > 0, 'the question shows no program');
  for (const source of await page.locator('pre.case').allTextContents()) {
    expect(
      source.includes('function') ||
        source.includes('class') ||
        source.includes('type ') ||
        source.includes('implement '),
      `a program does not look like BAML source: ${source.slice(0, 120)}`,
    );
  }
  expect(
    await page.locator('pre.case .hljs-keyword').first().isVisible(),
    'the programs are not highlighted: no token spans',
  );
  const buttons = (
    await page.locator('.question .choices button').allTextContents()
  ).map((b) => b.trim());
  const wanted =
    programs > 1
      ? ['The first compiles', 'The second compiles']
      : ['It compiles', 'It is rejected'];
  for (const word of [...wanted, 'Not sure']) {
    expect(buttons.includes(word), `no "${word}" button among ${buttons}`);
  }
  return { buttons: wanted, programs };
}

/**
 * Check a reveal against itself. The mark and the outcome are worked out
 * separately -- the mark by judging the answer against the case's key, the
 * outcome by asking the compiler -- so their agreement is what catches a
 * boundary that quietly loses the answer, which is how every answer once
 * read wrong. The points must follow the mark under the rule shown.
 */
async function reveal(page, asked, said, rule) {
  await page.waitForSelector('.reveal', WAIT);
  const verdict = await text(page, '.mark');
  const points = await text(page, '.mark .points');

  // Which program the compiler accepts, said two ways: the sentence, and the
  // label on each program. For one program the label is not shown, so the
  // sentence stands alone.
  const labels = await page.locator('.program .label').allTextContents();
  let accepted = null;
  if (asked.programs > 1) {
    expect(
      labels.length === asked.programs,
      `${asked.programs} programs but ${labels.length} labelled`,
    );
    const compiling = labels.filter((l) => l.includes('compiles'));
    expect(
      compiling.length === 1,
      `exactly one of two programs should compile, but the labels read ${labels}`,
    );
    accepted = labels.findIndex((l) => l.includes('compiles'));
    const named = accepted === 0 ? 'the first' : 'the second';
    expect(
      verdict.includes(`accepts ${named}`),
      `the labels say ${named} compiles, but the reveal says: ${verdict}`,
    );
    // The compiler's own words appear under the one it rejects.
    const panels = await page.locator('.program .compiler').allTextContents();
    expect(
      panels.length === 2,
      `both programs should carry what the compiler said, got ${panels.length}`,
    );
    expect(
      panels[1 - accepted].includes('E0'),
      `the rejected program carries no diagnostic: ${panels[1 - accepted]}`,
    );
  } else {
    accepted = verdict.includes('It compiles.') ? 0 : -1;
    expect(
      verdict.includes('It compiles.') ||
        verdict.includes('The compiler rejects it.'),
      `the reveal does not say what the compiler did: ${verdict}`,
    );
  }

  if (said === 'Not sure') {
    expect(
      verdict.startsWith('You held back.'),
      `held back, but the reveal says: ${verdict}`,
    );
    expect(points === rule.unsure, `held back, but the points read ${points}`);
    return;
  }
  const marked = verdict.startsWith('Right.');
  expect(
    marked || verdict.startsWith('Wrong.'),
    `the reveal does not mark the answer: ${verdict}`,
  );
  // `accepted` is the index of the program the compiler accepts, or -1 when
  // the one program shown is rejected; the buttons are in the same order.
  const chose = asked.buttons.indexOf(said);
  const agreed =
    asked.programs > 1
      ? chose === accepted
      : (chose === 0) === (accepted === 0);
  expect(
    marked === agreed,
    `answered "${said}" and the compiler accepts ${accepted}, but the reveal says: ${verdict}`,
  );
  const wanted = marked ? rule.right : rule.wrong;
  expect(points === wanted, `marked ${verdict} but the points read ${points}`);

  const claims = await page.locator('.claims li').count();
  expect(claims > 0, 'the reveal shows no claims');
  const spans = await page.locator('.claims code').count();
  expect(spans > 0, 'no type in the claims is set as code');
  const literal = await text(page, '.question');
  expect(!literal.includes('`'), `a backtick reached the page: ${literal}`);
}

const complaints = [];
let browser = null;
let failure = null;
// Held by the guarded region from the moment it is spawned, so every way out
// of the region stops it -- including a server that never says it is ready.
// One left running keeps the port, and the next run fails on it and blames
// the app.
let server = null;
try {
  server = startServer();
  await listening(server);
  browser = await chromium.launch();
  const page = await browser.newPage();
  page.on('console', (message) => {
    if (message.type() === 'error') {
      complaints.push(message.text());
    }
  });
  page.on('pageerror', (error) => complaints.push(String(error)));

  await page.goto(URL, { waitUntil: 'load' });

  // Three empty slots; start a practice sitting in the first, with every
  // case that can be shown beside its opposite shown that way.
  await page.waitForSelector('button:has-text("New sitting")', WAIT);
  expect(
    (await page.locator('.slot').count()) === 3,
    'the home does not show three slots',
  );
  await page.locator('button:has-text("New sitting")').first().click();
  await page.click('label:has-text("Practice") input[type=radio]');
  await page.fill('label:has-text("Practice") input.count', String(CASES));
  await page.click('details summary');
  await page.selectOption(
    'label:has-text("Show two programs") select',
    String(1),
  );
  // The button says "Loading…" until the formatter has instantiated.
  await page.waitForSelector('button:has-text("Start")', WAIT);
  await page.click('button:has-text("Start")');

  const seen = { choices: 0, held: 0, verdicts: 0 };
  let rule = null;
  for (let at = 1; at <= CASES; at++) {
    await progress(page, `Case ${at} of ${CASES}`);
    const asked = await question(page);
    const shown = await ruleShown(page);
    if (rule === null) {
      rule = shown;
      expect(
        rule.right === '+4' && rule.wrong === '−12' && rule.unsure === '0',
        `the default bar of 75% should read +4 / −12 / 0, not ${JSON.stringify(rule)}`,
      );
    }
    if (asked.programs > 1) {
      seen.choices += 1;
    } else {
      seen.verdicts += 1;
    }
    // Hold back on the third, to see an abstention through to the transcript.
    const said = at === 3 ? 'Not sure' : asked.buttons[0];
    if (at === 3) {
      seen.held += 1;
    }
    await page.click(`.question .choices button:text-is("${said}")`);
    await reveal(page, asked, said, shown);

    // Leave mid-sitting and come back once: the slot holds what was
    // answered and the next case is offered again from where it stood.
    if (at === CASES - 1) {
      await page.reload({ waitUntil: 'load' });
      await page.waitForSelector('button:has-text("Continue")', WAIT);
      const held = await text(page, '.slot');
      expect(
        held.includes(`${at} answered`),
        `after ${at} answers the slot says: ${held}`,
      );
      expect(
        held.includes(`practice, ${CASES} cases`),
        `the slot does not say what it holds: ${held}`,
      );
      await page.click('button:has-text("Continue")');
    } else {
      await page.click('button:has-text("Next")');
    }
  }

  expect(
    seen.choices > 0 && seen.verdicts > 0,
    `a sitting of ${CASES} should ask both kinds of question, got ${JSON.stringify(seen)}`,
  );

  // The readout, and the download it comes with.
  await page.waitForSelector('.done', WAIT);
  const readout = await text(page, '.readout');
  expect(
    readout.includes('rules mastered') && readout.includes('the bar asks for'),
    `the readout is missing mastery or calibration: ${readout}`,
  );
  expect(
    readout.includes(`${CASES} cases you asked for`),
    `the readout does not say why the sitting ended: ${readout}`,
  );
  const [download] = await Promise.all([
    page.waitForEvent('download', WAIT),
    page.click('button:has-text("Download the sitting")'),
  ]);
  const transcript = JSON.parse(await readFile(await download.path(), 'utf8'));
  expect(
    Array.isArray(transcript.exchanges) &&
      transcript.exchanges.length === CASES,
    `the transcript does not hold ${CASES} exchanges: ${JSON.stringify(transcript).slice(0, 200)}`,
  );
  expect(
    transcript.learner !== null && transcript.learner.knobs.practice === true,
    'the transcript does not carry what the model concluded',
  );
  // Stamped with the commit the bridge and SDK were built from, so a file
  // that comes back later says which bank made its cases.
  expect(
    typeof transcript.built === 'string' &&
      /^[0-9a-f]{40}$/.test(transcript.built),
    `the transcript is not stamped with the toolchain commit: ${JSON.stringify(transcript.built)}`,
  );
  const held = transcript.exchanges.filter((e) => e.answer.said === null);
  expect(
    held.length === seen.held,
    `${seen.held} answers were held back but the transcript records ${held.length}`,
  );
  const chosen = transcript.exchanges.filter(
    (e) => e.question.posed.compiles !== undefined,
  );
  expect(
    chosen.length === seen.choices,
    `${seen.choices} questions showed two programs but the transcript records ${chosen.length}`,
  );

  // A mastery sitting shows no meter: the case number, and no total.
  await page.click('button:has-text("Back to the slots")');
  await page.waitForSelector('button:has-text("New sitting")', WAIT);
  await page.locator('button:has-text("New sitting")').first().click();
  await page.waitForSelector('button:has-text("Start")', WAIT);
  await page.click('button:has-text("Start")');
  await progress(page, 'Case 1');
  await question(page);
  await page.click('button:has-text("Leave")');
  await page.waitForSelector('button:has-text("New sitting")', WAIT);
  const second = await text(page, '.slot >> nth=1');
  expect(
    second.includes('0 answered') && second.includes('until mastered'),
    `the second slot does not hold the mastery sitting: ${second}`,
  );

  // Who marks the reasoning. A key is kept in session storage, which is the
  // tab, so these seed and clear it there. With no key the learner marks their own, which
  // is the fallback; with one set, the sitting asks for reasons and says the
  // judge will read them. The judgement itself is a call on a real key, so it
  // is not made here: what is checked is that the page asks the right marker,
  // that a key typed in is taken as one, and that a key stored under the
  // older single-key shape still counts.
  await page.locator('button:has-text("New sitting")').first().click();
  await page.waitForSelector('button:has-text("Start")', WAIT);
  const asks = page.locator('label.check:has-text("Ask why")');
  expect(
    (await asks.textContent()).includes('mark your own reasoning'),
    `with no key the learner should mark their own: ${await asks.textContent()}`,
  );
  expect(
    !(await asks.locator('input').isChecked()),
    "with no key, asking why should be the learner's to turn on",
  );

  // Typing a key is enough: the marker changes, and asking why follows it
  // rather than having been settled before the key arrived.
  await page.click('details:has-text("Judge") summary');
  await page.fill('label:has-text("API key") input', 'sk-ant-not-a-key');
  await page.waitForFunction(
    () =>
      document
        .querySelector('label.check')
        ?.textContent?.includes('have the judge mark') === true,
    undefined,
    WAIT,
  );
  expect(
    await asks.locator('input').isChecked(),
    'a key should turn on asking why, since reasons are what it reads',
  );
  // The key is held under the provider that issued it, and the field follows
  // the model: switching to the other provider asks for its own key.
  const anthropic = await page.evaluate(() =>
    JSON.parse(window.sessionStorage.getItem('type-quiz/judge') ?? '{}'),
  );
  expect(
    anthropic.keys?.Anthropic === 'sk-ant-not-a-key',
    `the key is not held under its provider: ${JSON.stringify(anthropic)}`,
  );
  await page.selectOption('label:has-text("Model") select', 'gpt-5');
  expect(
    (await text(page, 'label:has-text("API key")')).includes('OpenAI'),
    'the key field does not follow the model to its provider',
  );
  expect(
    (await page.inputValue('label:has-text("API key") input')) === '',
    "the other provider's key is being shown for OpenAI",
  );

  // Settings written before there was more than one provider held one key,
  // under the model chosen then; they still count.
  await page.evaluate(() =>
    window.sessionStorage.setItem(
      'type-quiz/judge',
      JSON.stringify({ key: 'sk-ant-older-shape', model: 'claude-haiku-4-5' }),
    ),
  );
  await page.reload({ waitUntil: 'load' });
  await page.waitForSelector('button:has-text("New sitting")', WAIT);
  await page.locator('button:has-text("New sitting")').first().click();
  await page.waitForSelector('button:has-text("Start")', WAIT);
  const older = page.locator('label.check:has-text("Ask why")');
  expect(
    (await older.textContent()).includes('have the judge mark'),
    `a key saved in the older shape is not being read: ${await older.textContent()}`,
  );
  expect(
    await older.locator('input').isChecked(),
    'a key saved in the older shape should turn on asking why',
  );

  // Back to no key, and sit two cases the whole way: reasons asked for after
  // the answer and only where there is one to give, skipped on the first and
  // given on the second. A reason that was typed and then skipped is not a
  // reason: nothing is marked, and nothing was sent anywhere.
  await page.evaluate(() =>
    window.sessionStorage.removeItem('type-quiz/judge'),
  );
  await page.reload({ waitUntil: 'load' });
  await page.waitForSelector('button:has-text("New sitting")', WAIT);
  await page.locator('button:has-text("New sitting")').first().click();
  await page.waitForSelector('button:has-text("Start")', WAIT);
  await page.click('label:has-text("Practice") input[type=radio]');
  // A number field hands back whatever was typed. What reaches the engine is
  // a whole number of cases inside the bounds the field advertises.
  await page.fill('label:has-text("Practice") input.count', '1.5');
  expect(
    (await page.inputValue('label:has-text("Practice") input.count')) === '1',
    'a fraction of a question reaches the engine',
  );
  await page.fill('label:has-text("Practice") input.count', '');
  expect(
    (await page.inputValue('label:has-text("Practice") input.count')) === '1',
    'an empty count reaches the engine, which ends the sitting before it starts',
  );
  await page.fill('label:has-text("Practice") input.count', '2');
  await page.locator('label.check:has-text("Ask why") input').check();
  await page.click('button:has-text("Start")');

  for (const at of [1, 2]) {
    const skipping = at === 1;
    await progress(page, `Case ${at} of 2`);
    const one = await question(page);
    // A reason is asked for only where there is one to give: on a rejection,
    // or on either program of a pair. Saying a lone program compiles is asked
    // nothing, so the answer here is the one that leads to the box.
    const explains = one.programs > 1 ? one.buttons[0] : 'It is rejected';
    await page.click(`.question .choices button:text-is("${explains}")`);
    await page.waitForSelector('.why textarea', WAIT);
    const askedWhy = await text(page, 'label.why');
    expect(
      one.programs > 1
        ? askedWhy.includes('fail')
        : askedWhy.includes('reject'),
      `the reason asked for does not name what failed: ${askedWhy}`,
    );
    await page.fill('.why textarea', 'the argument is in a contravariant slot');
    await page.click(
      skipping
        ? 'button:text-is("Skip the reason")'
        : 'button:text-is("That is my reasoning")',
    );
    await page.waitForSelector('.reveal', WAIT);
    const markers = (
      await page.locator('.reveal .choices button').allTextContents()
    ).map((b) => b.trim());
    const marks = ['Sound', 'Partial', 'Wrong'];
    if (skipping) {
      expect(
        !marks.some((word) => markers.includes(word)),
        `a reason typed and then skipped is still being marked: ${markers}`,
      );
      await page.click('.reveal .choices button:text-is("Next")');
    } else {
      for (const word of marks) {
        expect(
          markers.includes(word),
          `with no key the reveal should ask the learner to mark: ${markers}`,
        );
      }
      expect(
        !markers.some((b) => b.includes('judge')),
        `with no key the reveal should not offer a judge: ${markers}`,
      );
      await page.click('.reveal .choices button:text-is("Sound")');
    }
  }
  await page.waitForSelector('.done', WAIT);

  // A browser that cannot start the quiz should say so. Both of these used to
  // render nothing at all: the page hangs off a WebAssembly module fetched
  // behind a top-level await, and it read `localStorage` while loading.
  const blocked = await browser.newPage();
  await blocked.addInitScript(() => {
    Object.defineProperty(window, 'localStorage', {
      configurable: true,
      get() {
        throw new Error('Access is denied for this document.');
      },
    });
  });
  await blocked.goto(URL, { waitUntil: 'load' });
  await blocked.waitForSelector('button:has-text("New sitting")', WAIT);
  // Every slot is empty, though this browser holds three sittings: the page
  // is running on the in-memory store, not on a localStorage that worked
  // after all.
  expect(
    (await blocked.locator('button:has-text("New sitting")').count()) === 3,
    'storage was not actually blocked, so this proves nothing',
  );
  await blocked.close();

  // A browser whose session storage is unavailable keeps a judge's key in
  // memory. It has to be ONE memory: were a fresh store made per call, the
  // key saved from the field would be missing when the panel is next drawn,
  // and a configured judge would silently turn into marking by hand.
  const sessionless = await browser.newPage();
  await sessionless.addInitScript(() => {
    Object.defineProperty(window, 'sessionStorage', {
      configurable: true,
      get() {
        throw new Error('Access is denied for this document.');
      },
    });
  });
  await sessionless.goto(URL, { waitUntil: 'load' });
  await sessionless.waitForSelector('button:has-text("New sitting")', WAIT);
  await sessionless.locator('button:has-text("New sitting")').first().click();
  await sessionless.waitForSelector('button:has-text("Start")', WAIT);
  await sessionless.click('details:has-text("Judge") summary');
  await sessionless.fill('label:has-text("API key") input', 'sk-ant-not-a-key');
  await sessionless.click('button:has-text("Back")');
  await sessionless.waitForSelector('button:has-text("New sitting")', WAIT);
  await sessionless.locator('button:has-text("New sitting")').first().click();
  await sessionless.waitForSelector('button:has-text("Start")', WAIT);
  const kept = sessionless.locator('label.check:has-text("Ask why")');
  expect(
    (await kept.textContent()).includes('have the judge mark'),
    `a key typed without session storage was lost between one drawing of the panel and the next: ${await kept.textContent()}`,
  );
  await sessionless.close();

  // A browser that stops accepting writes part-way through a sitting keeps
  // the sitting in memory. A slot reset after that has to be gone from the
  // browser too: removing frees space, so the browser takes it, and a reset
  // that only reached memory would bring the slot back on the next visit.
  const full = await browser.newPage();
  await full.addInitScript(() => {
    const write = Storage.prototype.setItem;
    let slotWrites = 0;
    Storage.prototype.setItem = function (key, value) {
      if (this === window.localStorage && key.startsWith('type-quiz/slot/')) {
        slotWrites += 1;
        // The first write, as the sitting starts, lands; every later one
        // meets a full quota.
        if (slotWrites > 1) {
          throw new DOMException(
            'The quota has been exceeded.',
            'QuotaExceededError',
          );
        }
      }
      return write.call(this, key, value);
    };
  });
  await full.goto(URL, { waitUntil: 'load' });
  await full.waitForSelector('button:has-text("New sitting")', WAIT);
  await full.locator('button:has-text("New sitting")').first().click();
  await full.click('label:has-text("Practice") input[type=radio]');
  await full.fill('label:has-text("Practice") input.count', '3');
  await full.waitForSelector('button:has-text("Start")', WAIT);
  await full.click('button:has-text("Start")');
  await progress(full, 'Case 1 of 3');
  const first = await question(full);
  await full.click(`.question .choices button:text-is("${first.buttons[0]}")`);
  await full.waitForSelector('.reveal', WAIT);
  expect(
    ((await full.textContent('.notice')) ?? '').includes(
      'not keeping sittings',
    ),
    'a write the browser refused was not reported to the learner',
  );
  // The sitting carries on rather than breaking.
  await full.click('.reveal .choices button:text-is("Next")');
  await progress(full, 'Case 2 of 3');
  await full.click('button:has-text("Leave")');
  await full.waitForSelector('.slot', WAIT);
  await full
    .locator('.slot')
    .first()
    .locator('button:text-is("Reset")')
    .click();
  await full
    .locator('.slot')
    .first()
    .locator('button:text-is("Really reset")')
    .click();
  await full.reload({ waitUntil: 'load' });
  await full.waitForSelector('.slot', WAIT);
  expect(
    ((await full.locator('.slot').first().textContent()) ?? '').includes(
      'Empty',
    ),
    `a slot reset after the quota filled came back on the next visit: ${await full.locator('.slot').first().textContent()}`,
  );
  await full.close();

  const starved = await browser.newPage();
  await starved.route('**/bridge_web_core_bg*.wasm', (route) => route.abort());
  await starved.goto(URL, { waitUntil: 'load' });
  await starved.waitForFunction(
    () =>
      document
        .getElementById('booting')
        ?.textContent?.includes('could not start') === true,
    undefined,
    WAIT,
  );
  const said = (await starved.textContent('#booting')) ?? '';
  expect(
    said.includes('WebAssembly'),
    `the page does not say why it could not start: ${said}`,
  );
  await starved.close();

  console.log(
    `smoke: a practice sitting of ${CASES} was sat across a reload — ${seen.verdicts} of one program and ${seen.choices} of two, ${seen.held} held back — the download carries the learner, a mastery sitting shows no total, a reason is asked only where there is one to give, skipped or marked by the learner when no judge is set, a browser that cannot start says why, and one that cannot keep a key or a sitting carries on without losing either`,
  );
} catch (error) {
  failure = error;
} finally {
  // The browser first, so a page does not log the server going away; the
  // server whatever closing the browser does.
  try {
    await browser?.close();
  } finally {
    stopServer(server);
  }
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
// Explicitly, rather than by running out of work: anything still holding a
// handle — a child that outlived its group, a socket the browser left — would
// otherwise keep this alive until CI's own timeout, and a hang costs the
// whole job's budget where a failure costs a minute.
process.exit(0);
