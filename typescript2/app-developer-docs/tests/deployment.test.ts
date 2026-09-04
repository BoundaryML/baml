import assert from 'node:assert/strict';
import test from 'node:test';

import { shouldIndexDeployment } from '../lib/deployment.ts';

test('Vercel previews and development deployments are noindex', () => {
  assert.equal(shouldIndexDeployment('preview'), false);
  assert.equal(shouldIndexDeployment('development'), false);
});

test('production and ordinary static builds are indexable', () => {
  assert.equal(shouldIndexDeployment('production'), true);
  assert.equal(shouldIndexDeployment(undefined), true);
});
