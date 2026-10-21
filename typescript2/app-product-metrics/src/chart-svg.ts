import type { ReleaseMetric, WeeklyCliMetrics } from './report.js';

const width = 1440;
const height = 980;
const colors = ['#6d5dfc', '#00a98f', '#f59e0b', '#ef5da8', '#3b82f6'];

function compactDate(date: string): string {
  return new Intl.DateTimeFormat('en-US', {
    day: 'numeric',
    month: 'short',
    timeZone: 'UTC',
  }).format(new Date(`${date}T00:00:00Z`));
}

function previousDate(date: string): string {
  const timestamp = new Date(`${date}T00:00:00Z`).getTime();
  return new Date(timestamp - 24 * 60 * 60 * 1000).toISOString().slice(0, 10);
}

function number(value: number): string {
  return value.toLocaleString('en-US');
}

function axisLabel(value: number): string {
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}m`;
  if (value >= 1_000)
    return `${(value / 1_000).toFixed(value >= 10_000 ? 0 : 1)}k`;
  return Math.round(value).toString();
}

function panel(x: number, y: number, title: string, content: string): string {
  return `<g><rect x="${x}" y="${y}" width="660" height="350" rx="20" fill="#ffffff" stroke="#e3e6ef"/><text class="panel-title" x="${x + 28}" y="${y + 42}">${title}</text>${content}</g>`;
}

function grid(x: number, y: number, max: number): string {
  return [0, 0.5, 1]
    .map((ratio) => {
      const lineY = y + 250 - ratio * 170;
      return `<line class="grid" x1="${x + 70}" y1="${lineY}" x2="${x + 625}" y2="${lineY}"/><text class="axis" x="${x + 60}" y="${lineY + 5}" text-anchor="end">${axisLabel(max * ratio)}</text>`;
    })
    .join('');
}

function bars(
  weeks: WeeklyCliMetrics[],
  x: number,
  y: number,
  value: (week: WeeklyCliMetrics) => number,
  color: string,
): string {
  const values = weeks.map(value);
  const max = Math.max(1, ...values);
  const slot = 135;
  const marks = weeks
    .map((week, index) => {
      const amount = values[index] ?? 0;
      const barHeight = (amount / max) * 170;
      const barX = x + 85 + index * slot;
      const barY = y + 250 - barHeight;
      return `<rect x="${barX}" y="${barY}" width="82" height="${barHeight}" rx="8" fill="${color}"/><text class="value" x="${barX + 41}" y="${Math.max(y + 70, barY - 9)}" text-anchor="middle">${number(amount)}</text><text class="week" x="${barX + 41}" y="${y + 282}" text-anchor="middle">${compactDate(week.period.start)}</text>`;
    })
    .join('');
  return `${grid(x, y, max)}${marks}`;
}

function usersBars(weeks: WeeklyCliMetrics[], x: number, y: number): string {
  const max = Math.max(1, ...weeks.map((week) => week.distinctUsers));
  const slot = 135;
  const marks = weeks
    .map((week, index) => {
      const newHeight = (week.newUsers / max) * 170;
      const existingHeight = (week.existingUsers / max) * 170;
      const barX = x + 85 + index * slot;
      const newY = y + 250 - newHeight;
      const existingY = newY - existingHeight;
      return `<rect x="${barX}" y="${newY}" width="82" height="${newHeight}" rx="8" fill="#6d5dfc"/><rect x="${barX}" y="${existingY}" width="82" height="${existingHeight}" rx="8" fill="#00a98f"/><text class="value" x="${barX + 41}" y="${Math.max(y + 70, existingY - 9)}" text-anchor="middle">${number(week.distinctUsers)}</text><text class="week" x="${barX + 41}" y="${y + 282}" text-anchor="middle">${compactDate(week.period.start)}</text>`;
    })
    .join('');
  const legend = `<rect x="${x + 405}" y="${y + 27}" width="12" height="12" rx="3" fill="#6d5dfc"/><text class="legend" x="${x + 424}" y="${y + 38}">New</text><rect x="${x + 490}" y="${y + 27}" width="12" height="12" rx="3" fill="#00a98f"/><text class="legend" x="${x + 509}" y="${y + 38}">Existing</text>`;
  return `${legend}${grid(x, y, max)}${marks}`;
}

function retentionLine(
  weeks: WeeklyCliMetrics[],
  x: number,
  y: number,
): string {
  const observedMax = Math.max(
    0,
    ...weeks.map((week) => week.retentionPercent ?? 0),
  );
  const axisMax = Math.min(100, Math.max(5, Math.ceil(observedMax / 5) * 5));
  const points = weeks.map((week, index) => {
    const pointX = x + 100 + index * 170;
    const value = week.retentionPercent ?? 0;
    const pointY = y + 250 - (value / axisMax) * 170;
    return { pointX, pointY, value, week };
  });
  const gridLines = [0, 0.25, 0.5, 0.75, 1]
    .map((ratio) => {
      const lineY = y + 250 - ratio * 170;
      const value = axisMax * ratio;
      const label = Number.isInteger(value)
        ? value.toString()
        : value.toFixed(1);
      return `<line class="grid" x1="${x + 70}" y1="${lineY}" x2="${x + 625}" y2="${lineY}"/><text class="axis" x="${x + 60}" y="${lineY + 5}" text-anchor="end">${label}%</text>`;
    })
    .join('');
  const line = points
    .map(({ pointX, pointY }) => `${pointX},${pointY}`)
    .join(' ');
  const marks = points
    .map(
      ({ pointX, pointY, value, week }) =>
        `<circle cx="${pointX}" cy="${pointY}" r="7" fill="#ef5da8"/><text class="value" x="${pointX}" y="${Math.max(y + 70, pointY - 12)}" text-anchor="middle">${value.toFixed(1)}%</text><text class="week" x="${pointX}" y="${y + 282}" text-anchor="middle">${compactDate(week.period.start)}</text>`,
    )
    .join('');
  return `${gridLines}<polyline points="${line}" fill="none" stroke="#ef5da8" stroke-width="5" stroke-linecap="round" stroke-linejoin="round"/>${marks}`;
}

function releaseOrder(weeks: WeeklyCliMetrics[]): string[] {
  const totals = new Map<string, number>();
  for (const week of weeks) {
    for (const release of week.releases) {
      totals.set(
        release.release,
        (totals.get(release.release) ?? 0) + release.users,
      );
    }
  }
  return [...totals.entries()]
    .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
    .map(([release]) => release);
}

function releaseShare(release: ReleaseMetric, week: WeeklyCliMetrics): string {
  if (week.distinctUsers === 0) return 'N/A';
  return `${((release.users / week.distinctUsers) * 100).toFixed(1)}%`;
}

function releaseMix(weeks: WeeklyCliMetrics[], x: number, y: number): string {
  const releases = releaseOrder(weeks);
  const color = new Map(
    releases.map((release, index) => [
      release,
      colors[index % colors.length] ?? '#64748b',
    ]),
  );
  const legend = releases
    .map((release, index) => {
      const legendX = x + 28 + index * 108;
      return `<rect x="${legendX}" y="${y + 60}" width="12" height="12" rx="3" fill="${color.get(release)}"/><text class="legend" x="${legendX + 19}" y="${y + 71}">${release}</text>`;
    })
    .join('');
  const rows = weeks
    .map((week, weekIndex) => {
      let segmentX = x + 105;
      const rowY = y + 105 + weekIndex * 54;
      const segments = releases
        .map((releaseName) => {
          const release = week.releases.find(
            (candidate) => candidate.release === releaseName,
          );
          if (!release || week.distinctUsers === 0) return '';
          const segmentWidth = (release.users / week.distinctUsers) * 520;
          const output = `<rect x="${segmentX}" y="${rowY}" width="${segmentWidth}" height="32" fill="${color.get(releaseName)}"><title>${releaseName}: ${number(release.users)} (${releaseShare(release, week)})</title></rect>`;
          segmentX += segmentWidth;
          return output;
        })
        .join('');
      return `<text class="week" x="${x + 92}" y="${rowY + 21}" text-anchor="end">${compactDate(week.period.start)}</text><clipPath id="release-row-${weekIndex}"><rect x="${x + 105}" y="${rowY}" width="520" height="32" rx="8"/></clipPath><g clip-path="url(#release-row-${weekIndex})">${segments}</g>`;
    })
    .join('');
  return `${legend}${rows}`;
}

export function renderSlackChartsSvg(weeks: WeeklyCliMetrics[]): string {
  if (weeks.length !== 4) {
    throw new Error('The Slack chart requires exactly four weeks of metrics');
  }
  const first = weeks[0];
  const last = weeks.at(-1);
  if (!first || !last) throw new Error('No metrics are available');
  const dateRange = `${compactDate(first.period.start)}–${compactDate(previousDate(last.period.end))}`;
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}"><style>text{font-family:"DejaVu Sans",Arial,sans-serif;fill:#172033}.title{font-size:38px;font-weight:700}.subtitle{font-size:18px;fill:#687086}.panel-title{font-size:23px;font-weight:700}.grid{stroke:#e8eaf1;stroke-width:1}.axis,.week,.legend{font-size:15px;fill:#687086}.value{font-size:15px;font-weight:700}.note{font-size:16px;fill:#713f12}</style><rect width="100%" height="100%" fill="#f4f5fa"/><text class="title" x="50" y="58">PLG CLI metrics · four closed weeks</text><text class="subtitle" x="50" y="92">${dateRange} · production, non-robot CLI activity · America/Los_Angeles</text>${panel(
    50,
    125,
    'CLI invocations',
    bars(weeks, 50, 125, (week) => week.invocations, '#3b82f6'),
  )}${panel(730, 125, 'Distinct users', usersBars(weeks, 730, 125))}${panel(50, 495, 'Retention', retentionLine(weeks, 50, 495))}${panel(730, 495, 'Users by active release', releaseMix(weeks, 730, 495))}<rect x="50" y="870" width="1340" height="68" rx="16" fill="#fffbeb" stroke="#f59e0b"/><text class="note" x="75" y="899"><tspan font-weight="700">Panic and segfault metrics unavailable.</tspan><tspan x="75" dy="24">The CLI emits invocation starts but no completion or crash events.</tspan></text></svg>`;
}
