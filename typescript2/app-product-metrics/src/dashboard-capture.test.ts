import assert from 'node:assert/strict';
import test from 'node:test';
import sharp from 'sharp';
import {
  composeDashboardCapture,
  type PostHogCardRenderState,
  postHogCardsAreReady,
} from './jobs/send-slack-report.js';

test('growing embeds are captured without clipping or overlapping the next dashboard', async () => {
  const image = async (width: number, height: number, background: string) =>
    sharp({ create: { background, channels: 4, height, width } })
      .png()
      .toBuffer();
  const screenshot = await composeDashboardCapture(
    { height: 50, width: 20 },
    [],
    [
      { height: 20, input: await image(24, 40, '#ff0000'), left: 2, top: 5 },
      { height: 10, input: await image(20, 10, '#0000ff'), left: 2, top: 30 },
    ],
  );
  const { data, info } = await sharp(screenshot)
    .raw()
    .toBuffer({ resolveWithObject: true });
  assert.equal(info.width, 26);
  assert.equal(info.height, 70);
  const pixel = (x: number, y: number) => {
    const offset = (y * info.width + x) * info.channels;
    return [...data.subarray(offset, offset + 3)];
  };
  assert.deepEqual(pixel(25, 44), [255, 0, 0]);
  assert.deepEqual(pixel(2, 49), [255, 255, 255]);
  assert.deepEqual(pixel(2, 50), [0, 0, 255]);
  assert.deepEqual(pixel(21, 59), [0, 0, 255]);
});

test('loading, errored, hidden, and empty insight cards cannot pass readiness', () => {
  const ready: PostHogCardRenderState = {
    hasErrorIndicator: false,
    hasLoadingIndicator: false,
    height: 368,
    opacity: 1,
    text: 'CLI invocations 123',
    visibility: 'visible',
    width: 780,
  };
  assert.equal(postHogCardsAreReady([ready]), true);
  assert.equal(postHogCardsAreReady([]), false);
  for (const unfinished of [
    { hasErrorIndicator: true },
    { hasLoadingIndicator: true },
    { height: 0 },
    { opacity: 0 },
    { text: ' ' },
    { visibility: 'hidden' },
    { width: 0 },
  ]) {
    assert.equal(
      postHogCardsAreReady([ready, { ...ready, ...unfinished }]),
      false,
    );
  }
});
