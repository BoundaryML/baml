import { createServer } from 'node:http';
import { Cron } from 'croner';
import { renderDashboard } from './app.js';
import { renderSlackChartsPng } from './chart-image.js';
import {
  type DiscordCommunityMetrics,
  loadDiscordCommunityMetrics,
} from './clients/discord.js';
import {
  loadWeeklyLumaAttendance,
  type WeeklyLumaAttendanceMetrics,
} from './clients/luma.js';
import { postToSlack } from './clients/slack.js';
import { database, disconnectDatabase } from './database.js';
import type { ExternalDataSyncRequest } from './external-data.js';
import { createRequestHandler } from './http.js';
import {
  aggregateWeeklyMetric,
  type WeeklyMetricPeriod,
} from './jobs/aggregate-weekly-metric.js';
import { fetchLumaEapData } from './jobs/fetch-luma-eap.js';
import { fetchSheepCouncilZoomData } from './jobs/fetch-sheep-council-zoom.js';
import {
  formatCliFourWeekReport,
  loadFourWeekMetrics,
  recentClosedWeeks,
  type WeeklyCliMetrics,
} from './report.js';
import { currentWeeklySnapshotPeriod } from './snapshot.js';

const port = Number(process.env.PORT ?? '3000');
const botToken = process.env.SLACK_BOUNDARY_BOT_TOKEN ?? '';
const slackChannel = process.env.SLACK_CHANNEL_ID ?? '';
const triggerToken = process.env.SLACK_POST_TRIGGER_TOKEN ?? '';
const posthogHost = process.env.POSTHOG_HOST ?? 'https://us.posthog.com';
const posthogPersonalApiKey =
  process.env.POSTHOG_BOUNDARY_PRODUCT_METRICS_PERSONAL_API_KEY ?? '';
const posthogProjectId =
  process.env.POSTHOG_BOUNDARY_PRODUCT_METRICS_PROJECT_ID ?? '';
const discordInviteCode = process.env.DISCORD_INVITE_CODE ?? 'yzaTpQ3tdT';
const discordGuildName = process.env.DISCORD_GUILD_NAME ?? 'Baml (by Boundary)';
const discordBotToken = process.env.DISCORD_OPSBOT_BOT_TOKEN ?? '';
const discordGuildId = process.env.DISCORD_OPSBOT_GUILD_ID ?? '';
const discordSheepCouncilRoleId =
  process.env.DISCORD_OPSBOT_SHEEP_COUNCIL_ROLE_ID ?? '';
const lumaApiKey = process.env.LUMA_API_KEY ?? '';
const lumaEventName = process.env.LUMA_EVENT_NAME ?? 'BAML Early Access';
const zoomAccountId = process.env.ZOOM_OPSBOT_ACCOUNT_ID ?? '';
const zoomClientId = process.env.ZOOM_OPSBOT_CLIENT_ID ?? '';
const zoomClientSecret = process.env.ZOOM_OPSBOT_CLIENT_SECRET ?? '';
const sheepCouncilZoomMeetingId =
  process.env.ZOOM_SHEEP_COUNCIL_MEETING_ID ?? '89180085282';
const dailyExternalDataSyncCron =
  process.env.DAILY_EXTERNAL_DATA_SYNC_CRON ?? '0 5 * * *';
const dailySnapshotCron = process.env.DAILY_SNAPSHOT_CRON ?? '0 6 * * *';
const weeklyPostCron = process.env.WEEKLY_POST_CRON ?? '0 9 * * 1';
const weeklyPostTimezone =
  process.env.WEEKLY_POST_TIMEZONE ?? 'America/Los_Angeles';

if (!botToken) throw new Error('SLACK_BOUNDARY_BOT_TOKEN is required');
if (!slackChannel) throw new Error('SLACK_CHANNEL_ID is required');
if (!triggerToken) throw new Error('SLACK_POST_TRIGGER_TOKEN is required');
if (!posthogPersonalApiKey) {
  throw new Error(
    'POSTHOG_BOUNDARY_PRODUCT_METRICS_PERSONAL_API_KEY is required',
  );
}
if (!posthogProjectId) {
  throw new Error('POSTHOG_BOUNDARY_PRODUCT_METRICS_PROJECT_ID is required');
}
if (!discordBotToken) throw new Error('DISCORD_OPSBOT_BOT_TOKEN is required');
if (!discordGuildId) throw new Error('DISCORD_OPSBOT_GUILD_ID is required');
if (!discordSheepCouncilRoleId) {
  throw new Error('DISCORD_OPSBOT_SHEEP_COUNCIL_ROLE_ID is required');
}
if (!lumaApiKey) throw new Error('LUMA_API_KEY is required');
if (!zoomAccountId) throw new Error('ZOOM_OPSBOT_ACCOUNT_ID is required');
if (!zoomClientId) throw new Error('ZOOM_OPSBOT_CLIENT_ID is required');
if (!zoomClientSecret) throw new Error('ZOOM_OPSBOT_CLIENT_SECRET is required');

const posthogConfig = {
  host: posthogHost,
  personalApiKey: posthogPersonalApiKey,
  projectId: posthogProjectId,
};
const discordConfig = {
  botToken: discordBotToken,
  expectedGuildName: discordGuildName,
  guildId: discordGuildId,
  inviteCode: discordInviteCode,
  sheepCouncilRoleId: discordSheepCouncilRoleId,
};
const lumaConfig = { apiKey: lumaApiKey, eventName: lumaEventName };
const zoomConfig = {
  accountId: zoomAccountId,
  clientId: zoomClientId,
  clientSecret: zoomClientSecret,
};
const lumaEapSyncConfig = {
  eventName: lumaEventName,
  lumaApiKey,
  timeZone: weeklyPostTimezone,
  zoom: zoomConfig,
};
const sheepCouncilZoomSyncConfig = {
  meetingId: sheepCouncilZoomMeetingId,
  timeZone: weeklyPostTimezone,
  zoom: zoomConfig,
};
const weeklyMetricAggregationConfig = {
  discord: discordConfig,
  posthog: posthogConfig,
  timeZone: weeklyPostTimezone,
};
interface ProductMetricsSnapshot {
  discord: DiscordCommunityMetrics;
  earlyAccess: WeeklyLumaAttendanceMetrics[];
  weeks: WeeklyCliMetrics[];
}

let metricsCache:
  | { expiresAt: number; metrics: ProductMetricsSnapshot }
  | undefined;
let metricsRequest: Promise<ProductMetricsSnapshot> | undefined;

async function metrics(forceRefresh = false): Promise<ProductMetricsSnapshot> {
  if (!forceRefresh && metricsCache && metricsCache.expiresAt > Date.now()) {
    return metricsCache.metrics;
  }
  if (!forceRefresh && metricsRequest) return metricsRequest;
  const now = new Date();
  const periods = recentClosedWeeks(now, weeklyPostTimezone);
  const request = Promise.all([
    loadFourWeekMetrics(posthogConfig, now, weeklyPostTimezone),
    loadDiscordCommunityMetrics(discordConfig),
    loadWeeklyLumaAttendance(lumaConfig, periods, weeklyPostTimezone),
  ]).then(([weeks, discord, earlyAccess]) => {
    const result = { discord, earlyAccess, weeks };
    metricsCache = { expiresAt: Date.now() + 15 * 60 * 1000, metrics: result };
    return result;
  });
  metricsRequest = request;
  try {
    return await request;
  } finally {
    if (metricsRequest === request) metricsRequest = undefined;
  }
}

async function post(): Promise<void> {
  const latestMetrics = await metrics();
  const text = formatCliFourWeekReport(
    latestMetrics.weeks,
    latestMetrics.discord,
    latestMetrics.earlyAccess,
  );
  const chart = await renderSlackChartsPng(latestMetrics.weeks);
  await postToSlack(botToken, {
    channel: slackChannel,
    file: {
      altText:
        'Four week charts for CLI invocations, distinct users, retention, and users by active BAML release',
      bytes: chart,
      filename: 'plg-cli-metrics-4-weeks.png',
      title: 'PLG CLI metrics · four closed weeks',
    },
    text,
  });
}

async function snapshot() {
  const period = currentWeeklySnapshotPeriod(new Date(), weeklyPostTimezone);
  return aggregateMetric({ end: period.end, start: period.start });
}

async function aggregateMetric(period: WeeklyMetricPeriod) {
  return aggregateWeeklyMetric(
    weeklyMetricAggregationConfig,
    database(),
    period,
  );
}

type ExternalDataSyncResult =
  | Awaited<ReturnType<typeof fetchSheepCouncilZoomData>>
  | Awaited<ReturnType<typeof fetchLumaEapData>>;

const externalDataSyncRequests = new Map<
  string,
  Promise<ExternalDataSyncResult>
>();

function startExternalDataSync(
  request: ExternalDataSyncRequest,
  recordedAt: Date,
): Promise<ExternalDataSyncResult> {
  const prisma = database();
  switch (request.raw_metric_type) {
    case 'sheep-council-meetings':
      return fetchSheepCouncilZoomData(
        sheepCouncilZoomSyncConfig,
        prisma,
        request.raw_metric_period,
        recordedAt,
      );
    case 'eap-meetings':
      return fetchLumaEapData(
        lumaEapSyncConfig,
        prisma,
        request.raw_metric_period,
        recordedAt,
      );
  }
}

async function syncExternalData(
  syncRequest: ExternalDataSyncRequest,
  recordedAt = new Date(),
): Promise<ExternalDataSyncResult> {
  const key = `${syncRequest.raw_metric_type}:${syncRequest.raw_metric_period.start}:${syncRequest.raw_metric_period.end}`;
  const activeRequest = externalDataSyncRequests.get(key);
  if (activeRequest) return activeRequest;
  const request = startExternalDataSync(syncRequest, recordedAt);
  externalDataSyncRequests.set(key, request);
  try {
    return await request;
  } finally {
    if (externalDataSyncRequests.get(key) === request) {
      externalDataSyncRequests.delete(key);
    }
  }
}

const server = createServer(
  createRequestHandler({
    aggregateWeeklyMetric: aggregateMetric,
    post,
    renderDashboard: async () =>
      renderDashboard(
        await database().plgWeeklyMetric.findMany({
          orderBy: { weekStartDate: 'desc' },
          select: {
            githubIssuesDistinctUserCount: true,
            lumaEapSignupCount: true,
            lumaEapZoomAttendanceCount: true,
            sheepCouncilActiveCount: true,
            sheepCouncilDiscordUserCount: true,
            sheepCouncilZoomAttendanceCount: true,
            totalDiscordUserCount: true,
            weekStartDate: true,
          },
          take: 4,
        }),
      ),
    snapshot,
    syncExternalData,
    triggerToken,
  }),
);

const dailyExternalDataSync = new Cron(
  dailyExternalDataSyncCron,
  { protect: true, timezone: weeklyPostTimezone },
  async () => {
    try {
      const recordedAt = new Date();
      const period = currentWeeklySnapshotPeriod(
        recordedAt,
        weeklyPostTimezone,
      );
      const rawMetricPeriod = { end: period.end, start: period.start };
      await Promise.all([
        syncExternalData(
          {
            raw_metric_period: rawMetricPeriod,
            raw_metric_type: 'sheep-council-meetings',
          },
          recordedAt,
        ),
        syncExternalData(
          {
            raw_metric_period: rawMetricPeriod,
            raw_metric_type: 'eap-meetings',
          },
          recordedAt,
        ),
      ]);
      console.log(
        `Synced external product metrics data at ${recordedAt.toISOString()}`,
      );
    } catch (error) {
      console.error(error);
    }
  },
);

const dailySnapshot = new Cron(
  dailySnapshotCron,
  { protect: true, timezone: weeklyPostTimezone },
  async () => {
    try {
      const result = await snapshot();
      console.log(
        `Recorded daily product metrics snapshot at ${result.recordedAt.toISOString()}`,
      );
    } catch (error) {
      console.error(error);
    }
  },
);

const weeklyPost = new Cron(
  weeklyPostCron,
  { protect: true, timezone: weeklyPostTimezone },
  async () => {
    try {
      await post();
      console.log(`Posted weekly Slack message to ${slackChannel}`);
    } catch (error) {
      console.error(error);
    }
  },
);

await new Promise<void>((resolve) => {
  server.listen(port, '0.0.0.0', resolve);
});

console.log(
  `Product metrics listening on 0.0.0.0:${port}; next external data sync at ${dailyExternalDataSync.nextRun()?.toISOString() ?? 'unknown'}; next daily snapshot at ${dailySnapshot.nextRun()?.toISOString() ?? 'unknown'}; next weekly Slack post at ${weeklyPost.nextRun()?.toISOString() ?? 'unknown'}`,
);

function shutdown(): void {
  dailyExternalDataSync.stop();
  dailySnapshot.stop();
  weeklyPost.stop();
  server.close();
  void disconnectDatabase();
}

process.once('SIGINT', shutdown);
process.once('SIGTERM', shutdown);
