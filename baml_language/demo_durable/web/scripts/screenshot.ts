// Takes one screenshot of the app in headless Chromium.
//
//   node scripts/screenshot.ts "/?fixture=fanout&speed=instant" out.png [--dark] [--size 1440x900] [--wait 500] [--click <selector>]...
//
// BASE_URL or WEB_PORT select the dev server, as in headless-check.ts.

import { chromium } from "playwright-core";

const BASE_URL = process.env.BASE_URL ?? `http://127.0.0.1:${process.env.WEB_PORT ?? 5173}`;
const [path, out, ...flags] = process.argv.slice(2);
if (!path || !out) {
  console.error("usage: node scripts/screenshot.ts <path> <out.png> [--dark] [--size WxH] [--wait ms] [--click selector]");
  process.exit(2);
}
const option = (name: string): string | undefined => {
  const index = flags.indexOf(name);
  return index === -1 ? undefined : flags[index + 1];
};
const [width, height] = (option("--size") ?? "1440x900").split("x").map(Number);
const clicks = flags.flatMap((flag, index) => (flag === "--click" && flags[index + 1] ? [flags[index + 1] as string] : []));

const browser = await chromium.launch();
try {
  const context = await browser.newContext({ viewport: { width: width ?? 1440, height: height ?? 900 }, colorScheme: flags.includes("--dark") ? "dark" : "light" });
  const page = await context.newPage();
  page.on("console", (message) => {
    if (message.type() === "error") console.error(`console.error: ${message.text()}`);
  });
  page.on("pageerror", (error) => console.error(`pageerror: ${error.message}`));
  await page.goto(`${BASE_URL}${path}`);
  await page.waitForSelector('[data-testid="app"]');
  await page.waitForTimeout(Number(option("--wait") ?? 400));
  for (const selector of clicks) {
    await page.locator(selector).first().click();
    await page.waitForTimeout(250);
  }
  await page.mouse.move(4, 4);
  await page.screenshot({ path: out });
  console.log(`wrote ${out}`);
} finally {
  await browser.close();
}
