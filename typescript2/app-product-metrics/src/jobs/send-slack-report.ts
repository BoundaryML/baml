import { writeFile } from 'node:fs/promises';
import { chromium, type Page } from 'playwright';
import sharp from 'sharp';
import {
  postToSlack,
  resolveSlackChannelId,
  type SlackBlock,
} from '../clients/slack.js';

export interface SlackDashboardReportConfig {
  botToken: string;
  channelName: string;
  dashboardUrl: string;
  screenshotPath?: string;
}

interface EmbeddedDashboardLayout {
  height: number;
  left: number;
  src: string;
  top: number;
  width: number;
}

export interface PostHogCardRenderState {
  hasErrorIndicator: boolean;
  hasLoadingIndicator: boolean;
  height: number;
  opacity: number;
  text: string;
  visibility: string;
  width: number;
}

export interface RetryOptions {
  attempts: number;
  delayMs: number;
  onRetry?: (error: unknown, attempt: number, delayMs: number) => void;
}

const nativePanelTitles = [
  'Total Discord users',
  'Distinct GitHub issue authors',
  'Sheep Council',
  'Early Access Program',
];

const captureAttempts = 3;
const dashboardNavigationTimeoutMs = 60_000;
const dashboardRenderTimeoutMs = 120_000;
const dashboardSettleTimeMs = 20_000;
const dashboardStatePollIntervalMs = 2_000;
const retryDelayMs = 5_000;
const productMetricsReadmeUrl =
  'https://github.com/BoundaryML/baml/blob/canary/typescript2/app-product-metrics/README.md';

const delay = async (delayMs: number): Promise<void> => {
  await new Promise((resolve) => setTimeout(resolve, delayMs));
};

/** Retry a failed capture with exponential backoff, propagating the final failure. */
export async function withRetries<T>(
  operation: (attempt: number) => Promise<T>,
  options: RetryOptions,
): Promise<T> {
  if (!Number.isInteger(options.attempts) || options.attempts < 1) {
    throw new Error('attempts must be a positive integer');
  }
  for (let attempt = 1; attempt <= options.attempts; attempt += 1) {
    try {
      return await operation(attempt);
    } catch (error) {
      if (attempt === options.attempts) throw error;
      const nextDelayMs = options.delayMs * 2 ** (attempt - 1);
      options.onRetry?.(error, attempt, nextDelayMs);
      await delay(nextDelayMs);
    }
  }
  throw new Error('unreachable');
}

/** Reject missing, loading, errored, empty, or hidden cards before starting the settle timer. */
export function postHogCardsAreReady(cards: PostHogCardRenderState[]): boolean {
  return (
    cards.length > 0 &&
    cards.every(
      ({
        hasErrorIndicator,
        hasLoadingIndicator,
        height,
        opacity,
        text,
        visibility,
        width,
      }) =>
        !hasErrorIndicator &&
        !hasLoadingIndicator &&
        height > 0 &&
        opacity > 0 &&
        width > 0 &&
        visibility === 'visible' &&
        text.trim().length > 0,
    )
  );
}

/** Read the visible content and layout used to detect unfinished or changing insight cards. */
async function postHogCardStates(
  page: Page,
): Promise<PostHogCardRenderState[]> {
  return await page
    .locator('[data-attr="insight-card"]')
    .evaluateAll((elements) =>
      elements.map((element) => {
        const bounds = element.getBoundingClientRect();
        const style = getComputedStyle(element);
        return {
          hasErrorIndicator:
            element.getAttribute('data-api-errored') === 'true' ||
            element.querySelector(
              '[data-attr="insight-error-state"], [data-attr="insight-refresh-data-hint"]',
            ) !== null,
          hasLoadingIndicator:
            element.querySelector('[data-attr="loading-bar"]') !== null,
          height: Math.round(bounds.height),
          opacity: Number.parseFloat(style.opacity),
          text: element.textContent ?? '',
          visibility: style.visibility,
          width: Math.round(bounds.width),
        };
      }),
    );
}

/** Require visible, loaded cards whose contents and dimensions remain stable for 20 seconds. */
async function waitForPostHogDashboard(page: Page): Promise<void> {
  const deadline = Date.now() + dashboardRenderTimeoutMs;
  let readySince: number | undefined;
  let stableCardState: string | undefined;
  let lastCards: PostHogCardRenderState[] = [];

  while (Date.now() < deadline) {
    lastCards = await postHogCardStates(page);
    if (postHogCardsAreReady(lastCards)) {
      const cardState = JSON.stringify(lastCards);
      if (stableCardState !== cardState) {
        stableCardState = cardState;
        readySince = Date.now();
      } else if (
        readySince !== undefined &&
        Date.now() - readySince >= dashboardSettleTimeMs
      ) {
        console.log(`PostHog dashboard settled with ${lastCards.length} cards`);
        return;
      }
    } else {
      stableCardState = undefined;
      readySince = undefined;
    }
    await page.waitForTimeout(dashboardStatePollIntervalMs);
  }

  const readyCards = lastCards.filter((card) => postHogCardsAreReady([card]));
  throw new Error(
    `PostHog dashboard did not settle within ${dashboardRenderTimeoutMs / 1_000}s (${readyCards.length}/${lastCards.length} cards ready)`,
  );
}

function dashboardReportHeading(dashboardUrl: string, date: string): string {
  return `<${new URL(dashboardUrl).href}|Product metrics dashboard> · ${date}`;
}

function dashboardReportGuidance(): string {
  return `This report is sent every Friday at 8am PT. To update it, see <${productMetricsReadmeUrl}|docs>.`;
}

export function dashboardReportText(
  dashboardUrl: string,
  date: string,
): string {
  return `${dashboardReportHeading(dashboardUrl, date)}\n${dashboardReportGuidance()}`;
}

export function dashboardReportBlocks(
  dashboardUrl: string,
  date: string,
): SlackBlock[] {
  return [
    {
      text: {
        text: dashboardReportHeading(dashboardUrl, date),
        type: 'mrkdwn',
      },
      type: 'section',
    },
    {
      elements: [{ text: dashboardReportGuidance(), type: 'mrkdwn' }],
      type: 'context',
    },
  ];
}

/** Capture the native charts and fully rendered embeds in a fresh browser. */
async function captureDashboardOnce(dashboardUrl: string): Promise<Buffer> {
  const browser = await chromium.launch({ headless: true });
  try {
    const indexPage = await browser.newPage({
      deviceScaleFactor: 1,
      viewport: { height: 900, width: 1600 },
    });
    const dashboardResponse = await indexPage.goto(dashboardUrl, {
      timeout: dashboardNavigationTimeoutMs,
      waitUntil: 'domcontentloaded',
    });
    if (!dashboardResponse?.ok()) {
      throw new Error(
        `Dashboard returned HTTP ${dashboardResponse?.status() ?? 'unknown'}`,
      );
    }
    const nativeChart = indexPage.locator('#weekly-metrics-charts');
    const hasNativeChart = (await nativeChart.count()) > 0;
    if (hasNativeChart) {
      await nativeChart.locator('.plot-container').first().waitFor({
        state: 'visible',
        timeout: dashboardRenderTimeoutMs,
      });
      await indexPage.waitForFunction(
        (titles) => {
          const chartText =
            document.querySelector('#weekly-metrics-charts')?.textContent ?? '';
          return titles.every((title) => chartText.includes(title));
        },
        nativePanelTitles,
        { timeout: dashboardRenderTimeoutMs },
      );
      await indexPage.waitForTimeout(dashboardSettleTimeMs);
    }
    const layout = await indexPage.locator('iframe').evaluateAll((elements) =>
      elements.map((element) => {
        const frame = element as HTMLIFrameElement;
        const bounds = frame.getBoundingClientRect();
        return {
          height: Math.round(bounds.height),
          left: Math.round(bounds.left),
          src: frame.src,
          top: Math.round(bounds.top),
          width: Math.round(bounds.width),
        } satisfies EmbeddedDashboardLayout;
      }),
    );
    if (layout.length === 0) {
      throw new Error('Dashboard did not contain any embedded dashboards');
    }
    for (const frame of layout) {
      const url = new URL(frame.src);
      if (
        url.protocol !== 'https:' ||
        url.hostname !== 'us.posthog.com' ||
        !url.pathname.startsWith('/embedded/') ||
        frame.width < 1 ||
        frame.height < 1
      ) {
        throw new Error(`Unexpected dashboard iframe: ${frame.src}`);
      }
    }
    const pageDimensions = await indexPage.evaluate(() => ({
      height: document.documentElement.scrollHeight,
      width: document.documentElement.scrollWidth,
    }));
    const renderedNativeChart = [];
    if (hasNativeChart) {
      await indexPage.evaluate(() => document.fonts.ready);
      const bounds = await nativeChart.boundingBox();
      if (!bounds)
        throw new Error('Native weekly metrics chart was not visible');
      const screenshot = Buffer.from(
        await nativeChart.screenshot({ type: 'png' }),
      );
      if ((await sharp(screenshot).stats()).entropy < 0.25) {
        throw new Error('Native weekly metrics chart screenshot was empty');
      }
      renderedNativeChart.push({
        input: screenshot,
        left: Math.round(bounds.x),
        top: Math.round(bounds.y),
      });
    }
    const renderedEmbeds = await Promise.all(
      layout.map(async (frame, index) => {
        const reportPage = await browser.newPage({
          deviceScaleFactor: 1,
          viewport: { height: frame.height, width: frame.width },
        });
        const pageErrors: string[] = [];
        reportPage.on('pageerror', (error) => pageErrors.push(error.message));
        try {
          const response = await reportPage.goto(frame.src, {
            timeout: dashboardNavigationTimeoutMs,
            waitUntil: 'domcontentloaded',
          });
          if (!response?.ok()) {
            throw new Error(
              `Embedded dashboard returned HTTP ${response?.status() ?? 'unknown'}`,
            );
          }
          await waitForPostHogDashboard(reportPage);
          await reportPage.evaluate(() => document.fonts.ready);
          const screenshot = Buffer.from(
            await reportPage.screenshot({ fullPage: true, type: 'png' }),
          );
          if ((await sharp(screenshot).stats()).entropy < 1) {
            throw new Error(
              `PostHog dashboard ${index + 1} screenshot was visually empty`,
            );
          }
          return { ...frame, input: screenshot };
        } catch (error) {
          const details = pageErrors.length
            ? ` Browser errors: ${pageErrors.join(' | ')}`
            : '';
          throw new Error(
            `PostHog dashboard ${index + 1} did not render.${details}`,
            { cause: error },
          );
        } finally {
          await reportPage.close();
        }
      }),
    );
    return await composeDashboardCapture(
      pageDimensions,
      renderedNativeChart,
      renderedEmbeds,
    );
  } finally {
    await browser.close();
  }
}

interface DashboardCapture {
  input: Buffer;
  left: number;
  top: number;
}

/** Expand the canvas and shift captures below growing embeds while preserving shared rows. */
export async function composeDashboardCapture(
  pageDimensions: { height: number; width: number },
  nativeCharts: DashboardCapture[],
  embeds: (DashboardCapture & { height: number })[],
): Promise<Buffer> {
  const captures: DashboardCapture[] = [];
  let height = pageDimensions.height;
  let width = pageDimensions.width;
  const growth: { bottom: number; offset: number }[] = [];
  // Take the largest displacement above this point: parallel embeds grow their row once.
  const offsetFor = (top: number) =>
    growth.reduce(
      (offset, item) =>
        item.bottom <= top ? Math.max(offset, item.offset) : offset,
      0,
    );
  for (const embed of [...embeds].sort((a, b) => a.top - b.top)) {
    const dimensions = await sharp(embed.input).metadata();
    if (!dimensions.height || !dimensions.width) {
      throw new Error('Embedded dashboard screenshot has no dimensions');
    }
    const top = embed.top + offsetFor(embed.top);
    captures.push({ input: embed.input, left: embed.left, top });
    const bottom = embed.top + embed.height;
    growth.push({ bottom, offset: top + dimensions.height - bottom });
    height = Math.max(height, top + dimensions.height);
    width = Math.max(width, embed.left + dimensions.width);
  }
  for (const chart of nativeCharts) {
    const dimensions = await sharp(chart.input).metadata();
    if (!dimensions.height || !dimensions.width) {
      throw new Error('Native chart screenshot has no dimensions');
    }
    const top = chart.top + offsetFor(chart.top);
    captures.push({ input: chart.input, left: chart.left, top });
    height = Math.max(height, top + dimensions.height);
    width = Math.max(width, chart.left + dimensions.width);
  }
  height = Math.max(
    height,
    pageDimensions.height + offsetFor(pageDimensions.height),
  );
  console.log(`Composing dashboard screenshot: ${width}x${height}`);
  return await sharp({
    create: {
      background: '#ffffff',
      channels: 4,
      height,
      width,
    },
  })
    .composite(captures)
    .png()
    .toBuffer();
}

/** Retry the entire browser capture so failed renders cannot be uploaded to Slack. */
export async function captureDashboard(dashboardUrl: string): Promise<Buffer> {
  return await withRetries(() => captureDashboardOnce(dashboardUrl), {
    attempts: captureAttempts,
    delayMs: retryDelayMs,
    onRetry: (error, attempt, nextDelayMs) => {
      const reason = error instanceof Error ? error.message : String(error);
      console.warn(
        `Dashboard capture attempt ${attempt}/${captureAttempts} failed: ${reason}. Retrying in ${nextDelayMs / 1_000}s.`,
      );
    },
  });
}

/** Capture the live dashboard, save its artifact, and deliver the short report with the PNG. */
export async function sendSlackDashboardReport(
  config: SlackDashboardReportConfig,
  now = new Date(),
): Promise<void> {
  const channelId = await resolveSlackChannelId(
    config.botToken,
    config.channelName,
  );
  const screenshot = await captureDashboard(config.dashboardUrl);
  if (config.screenshotPath) {
    await writeFile(config.screenshotPath, screenshot);
  }
  const date = now.toISOString().slice(0, 10);
  await postToSlack(config.botToken, {
    blocks: dashboardReportBlocks(config.dashboardUrl, date),
    channel: channelId,
    file: {
      altText: 'Screenshot of the live product metrics dashboard',
      bytes: screenshot,
      filename: `product-metrics-dashboard-${date}.png`,
      title: `Product metrics dashboard · ${date}`,
    },
    text: dashboardReportText(config.dashboardUrl, date),
  });
}
