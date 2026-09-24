import { sendSlackDashboardReport } from '../jobs/send-slack-report.js';

function requiredEnvironmentVariable(name: string): string {
  const value = process.env[name]?.trim();
  if (!value) throw new Error(`${name} is required`);
  return value;
}

await sendSlackDashboardReport({
  botToken: requiredEnvironmentVariable('SLACK_BOUNDARY_BOT_TOKEN'),
  channelName: requiredEnvironmentVariable('SLACK_CHANNEL_NAME'),
  dashboardUrl: requiredEnvironmentVariable('PRODUCT_METRICS_DASHBOARD_URL'),
  screenshotPath:
    process.env.PRODUCT_METRICS_SCREENSHOT_PATH?.trim() || undefined,
});
