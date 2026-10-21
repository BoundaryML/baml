import assert from 'node:assert/strict';
import test from 'node:test';
import { renderDashboard, type WeeklyMetricChartRow } from './app.js';

const metric: WeeklyMetricChartRow = {
  githubIssuesDistinctUserCount: 8,
  lumaEapSignupCount: 21,
  lumaEapZoomAttendanceCount: 13,
  sheepCouncilActiveCount: 17,
  sheepCouncilDiscordUserCount: 71,
  sheepCouncilZoomAttendanceCount: null,
  totalDiscordUserCount: 2695,
  weekStartDate: new Date('2026-08-24T07:00:00.000Z'),
};

test('renders weekly database metrics in the four requested Plotly panels', () => {
  const html = renderDashboard([metric]);
  assert.match(html, /plotly-3\.7\.0\.min\.js/);
  assert.match(html, /barmode: 'group'/);
  assert.match(html, /Aug 24 - Aug 31/);
  assert.doesNotMatch(html, /Week starting/);
  assert.match(html, /body \{\s+background-color: rgb\(243, 244, 240\);\s+\}/);
  assert.match(html, /grid-template-columns: repeat\(2, minmax\(0, 1fr\)\)/);
  assert.match(html, /showlegend: true/);
  assert.match(html, /x: 0\.5,[\s\S]*xanchor: 'center'/);
  for (const id of [
    'discord-users-chart',
    'github-issue-authors-chart',
    'sheep-council-chart',
    'luma-eap-chart',
  ]) {
    assert.match(html, new RegExp(`id="${id}"`));
  }
  for (const color of [
    '#636EFA',
    '#00CC96',
    '#AB63FA',
    '#19D3F3',
    '#B6E880',
    '#EF553B',
    '#FFA15A',
  ]) {
    assert.match(html, new RegExp(color));
  }
  for (const label of [
    'Total Discord users',
    'Distinct GitHub issue authors',
    'Sheep Council',
    'Discord users',
    'Active users',
    'Early Access Program',
    'Luma signups',
    'Zoom attendance',
  ]) {
    assert.match(html, new RegExp(label));
  }
  assert.match(html, /"y":\[null\]/);
  assert.equal((html.match(/<iframe/g) ?? []).length, 2);
  assert.match(html, /<iframe[^>]*title="PLG CLI Metrics"[^>]*height="1450"/);
  assert.match(
    html,
    /<iframe[^>]*title="PLG Website Metrics"[^>]*height="720"/,
  );
  assert.equal(
    (
      html.match(
        /style="box-sizing:border-box;padding:10px;background-color:rgb\(243, 244, 240\)"/g,
      ) ?? []
    ).length,
    2,
  );
});

test('renders only the four most recent weeks in chronological order', () => {
  const rows = [
    '2026-08-24',
    '2026-08-03',
    '2026-08-31',
    '2026-08-10',
    '2026-08-17',
  ].map((date) => ({
    ...metric,
    weekStartDate: new Date(`${date}T07:00:00.000Z`),
  }));
  const html = renderDashboard(rows);
  assert.doesNotMatch(html, /Aug 3 - Aug 10/);
  const labels = [
    'Aug 10 - Aug 17',
    'Aug 17 - Aug 24',
    'Aug 24 - Aug 31',
    'Aug 31 - Sep 7',
  ];
  for (const label of labels) assert.match(html, new RegExp(label));
  assert.ok(
    labels.every(
      (label, index) =>
        index === 0 ||
        html.indexOf(labels[index - 1] ?? '') < html.indexOf(label),
    ),
  );
});
