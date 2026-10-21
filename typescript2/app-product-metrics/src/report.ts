import type { DiscordCommunityMetrics } from './clients/discord.js';
import type { WeeklyLumaAttendanceMetrics } from './clients/luma.js';
import { type PostHogConfig, runHogQl } from './clients/posthog.js';

const dayMs = 24 * 60 * 60 * 1000;

export interface WeeklyPeriod {
  end: string;
  previousStart: string;
  start: string;
}

export interface ReleaseMetric {
  release: string;
  users: number;
}

export interface WeeklyCliMetrics {
  distinctUsers: number;
  existingUsers: number;
  invocations: number;
  newUsers: number;
  period: WeeklyPeriod;
  previousUsers: number;
  releases: ReleaseMetric[];
  retentionPercent: number | null;
}

interface Summary {
  distinctUsers: number;
  existingUsers: number;
  invocations: number;
  newUsers: number;
  previousUsers: number;
}

function dateInTimeZone(now: Date, timeZone: string) {
  const parts = new Intl.DateTimeFormat('en-US', {
    day: '2-digit',
    month: '2-digit',
    timeZone,
    weekday: 'short',
    year: 'numeric',
  }).formatToParts(now);
  const get = (type: Intl.DateTimeFormatPartTypes) =>
    parts.find((part) => part.type === type)?.value;
  const year = Number(get('year'));
  const month = Number(get('month'));
  const day = Number(get('day'));
  const weekday = get('weekday');
  if (!year || !month || !day || !weekday) {
    throw new Error('Could not calculate the weekly reporting period');
  }
  return { day, month, weekday, year };
}

function isoDate(timestamp: number): string {
  return new Date(timestamp).toISOString().slice(0, 10);
}

function shiftDate(date: string, days: number): string {
  return isoDate(new Date(`${date}T00:00:00Z`).getTime() + days * dayMs);
}

export function mostRecentClosedWeek(
  now: Date,
  timeZone: string,
): WeeklyPeriod {
  const local = dateInTimeZone(now, timeZone);
  const weekdayIndex = [
    'Sun',
    'Mon',
    'Tue',
    'Wed',
    'Thu',
    'Fri',
    'Sat',
  ].indexOf(local.weekday);
  if (weekdayIndex < 0) throw new Error(`Unknown weekday: ${local.weekday}`);
  const localDate = Date.UTC(local.year, local.month - 1, local.day);
  const daysSinceMonday = (weekdayIndex + 6) % 7;
  const end = localDate - daysSinceMonday * dayMs;
  return {
    end: isoDate(end),
    previousStart: isoDate(end - 14 * dayMs),
    start: isoDate(end - 7 * dayMs),
  };
}

export function recentClosedWeeks(
  now: Date,
  timeZone: string,
  count = 4,
): WeeklyPeriod[] {
  if (!Number.isInteger(count) || count < 1 || count > 12) {
    throw new Error('Week count must be an integer between 1 and 12');
  }
  const latest = mostRecentClosedWeek(now, timeZone);
  return Array.from({ length: count }, (_, index) => {
    const weeksBeforeLatest = count - index - 1;
    const start = shiftDate(latest.start, -7 * weeksBeforeLatest);
    const end = shiftDate(latest.end, -7 * weeksBeforeLatest);
    return { end, previousStart: shiftDate(start, -7), start };
  });
}

function invocationFilter(
  start: string,
  end: string,
  timeZone: string,
): string {
  return `event = 'cli_invocation'
    AND properties.environment = 'production'
    AND properties.robot = 0
    AND timestamp >= toDateTime('${start} 00:00:00', '${timeZone}')
    AND timestamp < toDateTime('${end} 00:00:00', '${timeZone}')`;
}

export function summaryQuery(period: WeeklyPeriod, timeZone: string): string {
  return `WITH
  current_users AS (
    SELECT distinct_id, count() AS invocations
    FROM events
    WHERE ${invocationFilter(period.start, period.end, timeZone)}
    GROUP BY distinct_id
  ),
  previous_users AS (
    SELECT DISTINCT distinct_id AS previous_distinct_id
    FROM events
    WHERE ${invocationFilter(period.previousStart, period.start, timeZone)}
  )
SELECT
  coalesce(sum(current_users.invocations), 0) AS invocations,
  count() AS distinct_users,
  countIf(current_users.distinct_id NOT IN (SELECT previous_distinct_id FROM previous_users)) AS new_users,
  (SELECT count() FROM previous_users) AS previous_users
FROM current_users`;
}

export function releasesQuery(period: WeeklyPeriod, timeZone: string): string {
  return `WITH current_users AS (
  SELECT distinct_id, argMax(toString(properties.cli_version), timestamp) AS cli_version
  FROM events
  WHERE ${invocationFilter(period.start, period.end, timeZone)}
  GROUP BY distinct_id
)
SELECT cli_version, count() AS users
FROM current_users
GROUP BY cli_version
ORDER BY users DESC`;
}

function numberAt(row: unknown[], index: number, name: string): number {
  const value = Number(row[index]);
  if (!Number.isFinite(value) || value < 0) {
    throw new Error(`PostHog returned an invalid ${name}`);
  }
  return value;
}

function parseSummary(result: Awaited<ReturnType<typeof runHogQl>>): Summary {
  const expected = [
    'invocations',
    'distinct_users',
    'new_users',
    'previous_users',
  ];
  if (
    result.results.length !== 1 ||
    expected.some((column, index) => result.columns[index] !== column)
  ) {
    throw new Error('PostHog summary query schema changed');
  }
  const row = result.results[0] ?? [];
  const distinctUsers = numberAt(row, 1, 'distinct user count');
  const newUsers = numberAt(row, 2, 'new user count');
  if (newUsers > distinctUsers) {
    throw new Error('PostHog returned more new users than distinct users');
  }
  return {
    distinctUsers,
    existingUsers: distinctUsers - newUsers,
    invocations: numberAt(row, 0, 'invocation count'),
    newUsers,
    previousUsers: numberAt(row, 3, 'previous user count'),
  };
}

function normalizeRelease(version: unknown): string {
  const match = String(version ?? '').match(/^v?(\d+)\.(\d+)/);
  return match ? `${match[1]}.${match[2]}` : 'unknown';
}

function parseReleases(
  result: Awaited<ReturnType<typeof runHogQl>>,
): ReleaseMetric[] {
  if (result.columns[0] !== 'cli_version' || result.columns[1] !== 'users') {
    throw new Error('PostHog release query schema changed');
  }
  const totals = new Map<string, number>();
  for (const row of result.results) {
    const release = normalizeRelease(row[0]);
    totals.set(
      release,
      (totals.get(release) ?? 0) + numberAt(row, 1, 'release user count'),
    );
  }
  return [...totals.entries()]
    .map(([release, users]) => ({ release, users }))
    .sort((a, b) => b.users - a.users || a.release.localeCompare(b.release));
}

function retentionPercent(existingUsers: number, previousUsers: number) {
  return previousUsers === 0 ? null : (existingUsers / previousUsers) * 100;
}

async function loadWeekMetrics(
  config: PostHogConfig,
  period: WeeklyPeriod,
  timeZone: string,
  fetchImpl: typeof fetch,
): Promise<WeeklyCliMetrics> {
  const [summaryResult, releasesResult] = await Promise.all([
    runHogQl(
      config,
      `weekly_cli_summary_${period.start}_${period.end}`,
      summaryQuery(period, timeZone),
      fetchImpl,
    ),
    runHogQl(
      config,
      `weekly_cli_releases_${period.start}_${period.end}`,
      releasesQuery(period, timeZone),
      fetchImpl,
    ),
  ]);
  const summary = parseSummary(summaryResult);
  return {
    ...summary,
    period,
    releases: parseReleases(releasesResult),
    retentionPercent: retentionPercent(
      summary.existingUsers,
      summary.previousUsers,
    ),
  };
}

export async function loadFourWeekMetrics(
  config: PostHogConfig,
  now: Date,
  timeZone: string,
  fetchImpl: typeof fetch = fetch,
): Promise<WeeklyCliMetrics[]> {
  return Promise.all(
    recentClosedWeeks(now, timeZone, 4).map((period) =>
      loadWeekMetrics(config, period, timeZone, fetchImpl),
    ),
  );
}

function percentage(value: number | null): string {
  return value === null ? 'N/A' : `${value.toFixed(1)}%`;
}

function share(numerator: number, denominator: number): string {
  return percentage(denominator === 0 ? null : (numerator / denominator) * 100);
}

function displayDate(date: string, includeYear = false): string {
  return new Intl.DateTimeFormat('en-US', {
    day: 'numeric',
    month: 'short',
    timeZone: 'UTC',
    ...(includeYear ? { year: 'numeric' } : {}),
  }).format(new Date(`${date}T00:00:00Z`));
}

export function formatCliFourWeekReport(
  metrics: WeeklyCliMetrics[],
  discord: DiscordCommunityMetrics,
  earlyAccess: WeeklyLumaAttendanceMetrics[],
): string {
  if (metrics.length !== 4 || earlyAccess.length !== 4) {
    throw new Error('The Slack report requires exactly four weeks of metrics');
  }
  const weekLines = metrics.flatMap((week, index) => {
    const attendance = earlyAccess[index];
    if (!attendance || attendance.period.start !== week.period.start) {
      throw new Error('The Slack report metric weeks do not align');
    }
    const releases = week.releases.length
      ? week.releases
          .map(
            ({ release, users }) =>
              `${release}: ${users.toLocaleString('en-US')} (${share(users, week.distinctUsers)})`,
          )
          .join(' · ')
      : 'No users';
    return [
      '',
      `*${displayDate(week.period.start)}–${displayDate(shiftDate(week.period.end, -1), true)}*`,
      `• Invocations: ${week.invocations.toLocaleString('en-US')} · distinct users: ${week.distinctUsers.toLocaleString('en-US')}`,
      `• Existing: ${week.existingUsers.toLocaleString('en-US')} · new: ${week.newUsers.toLocaleString('en-US')} · retention: ${percentage(week.retentionPercent)}`,
      `• Releases: ${releases}`,
      attendance.eventCount === 0
        ? '• BAML Early Access: no sessions'
        : `• BAML Early Access: ${attendance.joinedGuests.toLocaleString('en-US')}/${attendance.registeredGuests.toLocaleString('en-US')} joined online (${percentage(attendance.joinRatePercent)}) · ${attendance.eventCount.toLocaleString('en-US')} ${attendance.eventCount === 1 ? 'session' : 'sessions'}`,
    ];
  });
  return [
    'Product metrics · last 4 closed weeks',
    `Discord community: ${discord.approximateMemberCount.toLocaleString('en-US')} members in ${discord.guildName} (approximate, point-in-time)`,
    `Sheep Council: ${discord.sheepCouncilMemberCount.toLocaleString('en-US')} members (exact role count, point-in-time)`,
    ...weekLines,
    '',
    'Panic/segfault invocations and rate: unavailable for all weeks',
    '_Data quality: the current CLI records invocation starts but does not emit completion or crash events, so panic and segfault counts cannot be measured reliably._',
  ].join('\n');
}

export async function buildCliFourWeekReport(
  config: PostHogConfig,
  now: Date,
  timeZone: string,
  discord: DiscordCommunityMetrics,
  earlyAccess: WeeklyLumaAttendanceMetrics[],
  fetchImpl: typeof fetch = fetch,
): Promise<string> {
  return formatCliFourWeekReport(
    await loadFourWeekMetrics(config, now, timeZone, fetchImpl),
    discord,
    earlyAccess,
  );
}
