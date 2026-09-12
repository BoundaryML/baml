import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

const readConfig = async (app) =>
  JSON.parse(
    await readFile(new URL(`../${app}/vercel.json`, import.meta.url), 'utf8'),
  );

test('BEPs and Prompt Fiddle deploy from Git only on the production branch', async () => {
  for (const app of ['app-beps', 'app-promptfiddle']) {
    const config = await readConfig(app);
    assert.deepEqual(config.git?.deploymentEnabled, {
      '*': false,
      canary: true,
    });
  }
});

test('developer docs filter only skips unaffected preview builds', async () => {
  const config = await readConfig('app-developer-docs');
  assert.match(config.ignoreCommand, /VERCEL_TARGET_ENV/);
  assert.match(config.ignoreCommand, /!= "preview"/);
  assert.match(config.ignoreCommand, /turbo@2\.10\.12 query affected/);
  assert.match(config.ignoreCommand, /--packages app-developer-docs/);
  assert.match(config.ignoreCommand, /--tasks build/);
  assert.match(config.ignoreCommand, /--exit-code/);
});
