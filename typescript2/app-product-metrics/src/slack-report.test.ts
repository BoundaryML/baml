import assert from 'node:assert/strict';
import test from 'node:test';
import { dashboardReportText } from './jobs/send-slack-report.js';

test('dashboardReportText links the Slack report to the live dashboard', () => {
  assert.equal(
    dashboardReportText(
      'https://boundary-product-metrics.fly.dev',
      '2026-08-28',
    ),
    '<https://boundary-product-metrics.fly.dev/|Product metrics dashboard> · 2026-08-28',
  );
});
