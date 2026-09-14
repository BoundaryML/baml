import assert from 'node:assert/strict';
import test from 'node:test';

import {
  robotsDisallowEntireSite,
  shouldIndexDeployment,
} from '../lib/deployment.ts';

test('Vercel previews and development deployments are noindex', () => {
  assert.equal(shouldIndexDeployment('preview'), false);
  assert.equal(shouldIndexDeployment('development'), false);
});

test('production and ordinary static builds are indexable', () => {
  assert.equal(shouldIndexDeployment('production'), true);
  assert.equal(shouldIndexDeployment(undefined), true);
});

test('deployment-wide robots exclusions are detected independently of sitemap intent', () => {
  assert.equal(robotsDisallowEntireSite('User-agent: *\nDisallow: /\n'), true);
  assert.equal(
    robotsDisallowEntireSite('User-agent: *\nDisallow: /private\n'),
    false,
  );
});
