import sharp from 'sharp';
import { renderSlackChartsSvg } from './chart-svg.js';
import type { WeeklyCliMetrics } from './report.js';

export { renderSlackChartsSvg } from './chart-svg.js';

export async function renderSlackChartsPng(
  weeks: WeeklyCliMetrics[],
): Promise<Buffer> {
  return sharp(Buffer.from(renderSlackChartsSvg(weeks)))
    .png({ compressionLevel: 9 })
    .toBuffer();
}
