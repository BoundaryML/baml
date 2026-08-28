import { writeFile } from 'node:fs/promises';
import { chromium } from 'playwright';
import sharp from 'sharp';
import { postToSlack } from '../clients/slack.js';

export interface SlackDashboardReportConfig {
  botToken: string;
  channel: string;
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

const primaryPanelTitles = [
  'CLI invocations — daily',
  'CLI weekly cohort retention',
  'CLI 7DAU — daily',
  'CLI 30DAU — daily',
  'CLI new users (7d period)',
  'CLI retained users (7d period)',
  'CLI resurrected users (7d period)',
];

export async function captureDashboard(dashboardUrl: string): Promise<Buffer> {
  const browser = await chromium.launch({ headless: true });
  try {
    const indexPage = await browser.newPage({
      deviceScaleFactor: 1,
      viewport: { height: 900, width: 1600 },
    });
    const dashboardResponse = await indexPage.goto(dashboardUrl, {
      timeout: 30_000,
      waitUntil: 'domcontentloaded',
    });
    if (!dashboardResponse?.ok()) {
      throw new Error(
        `Dashboard returned HTTP ${dashboardResponse?.status() ?? 'unknown'}`,
      );
    }
    const nativeChart = indexPage.locator('#weekly-metrics-chart');
    const hasNativeChart = (await nativeChart.count()) > 0;
    if (hasNativeChart) {
      await nativeChart.locator('.plot-container').waitFor({
        state: 'visible',
        timeout: 30_000,
      });
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
            timeout: 30_000,
            waitUntil: 'load',
          });
          if (!response?.ok()) {
            throw new Error(
              `Embedded dashboard returned HTTP ${response?.status() ?? 'unknown'}`,
            );
          }
          if (index === 0) {
            await reportPage.waitForFunction(
              (titles) => {
                const bodyText = document.body.innerText;
                return titles.every((title) => bodyText.includes(title));
              },
              primaryPanelTitles,
              { timeout: 60_000 },
            );
          } else {
            await reportPage
              .getByText('Made with PostHog', { exact: false })
              .waitFor({ state: 'visible', timeout: 60_000 });
          }
          await reportPage.waitForTimeout(1_000);
          const screenshot = Buffer.from(
            await reportPage.screenshot({ type: 'png' }),
          );
          if (index === 0 && (await sharp(screenshot).stats()).entropy < 1) {
            throw new Error('Primary dashboard screenshot was visually empty');
          }
          return { input: screenshot, left: frame.left, top: frame.top };
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
    return await sharp({
      create: {
        background: '#ffffff',
        channels: 4,
        height: pageDimensions.height,
        width: pageDimensions.width,
      },
    })
      .composite([...renderedNativeChart, ...renderedEmbeds])
      .png()
      .toBuffer();
  } finally {
    await browser.close();
  }
}

export async function sendSlackDashboardReport(
  config: SlackDashboardReportConfig,
  now = new Date(),
): Promise<void> {
  const screenshot = await captureDashboard(config.dashboardUrl);
  if (config.screenshotPath) {
    await writeFile(config.screenshotPath, screenshot);
  }
  const date = now.toISOString().slice(0, 10);
  await postToSlack(config.botToken, {
    channel: config.channel,
    file: {
      altText: 'Screenshot of the live product metrics dashboard',
      bytes: screenshot,
      filename: `product-metrics-dashboard-${date}.png`,
      title: `Product metrics dashboard · ${date}`,
    },
    text: `Product metrics dashboard · ${date}`,
  });
}
