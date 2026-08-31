import assert from 'node:assert/strict';
import test from 'node:test';
import { dashboardReportText } from './jobs/send-slack-report.js';

test('dashboardReportText links to the dashboard and maintainer README', () => {
  assert.equal(
    dashboardReportText(
      'https://boundary-product-metrics.fly.dev',
      '2026-08-28',
    ),
    '<https://boundary-product-metrics.fly.dev/|Product metrics dashboard> · <https://github.com/BoundaryML/baml/blob/canary/typescript2/app-product-metrics/README.md|Maintainer README> · 2026-08-28',
  );
});
