import assert from 'node:assert/strict';
import test from 'node:test';
import { dashboardReportText } from './jobs/send-slack-report.js';

test('dashboardReportText links its update guidance to the README', () => {
  assert.equal(
    dashboardReportText(
      'https://boundary-product-metrics.fly.dev',
      '2026-08-28',
    ),
    '<https://boundary-product-metrics.fly.dev/|Product metrics dashboard> · 2026-08-28\n_This report is sent every Monday at 8 AM PT. To update it, see <https://github.com/BoundaryML/baml/blob/canary/typescript2/app-product-metrics/README.md|docs>._',
  );
});
