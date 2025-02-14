import { b } from "../baml_client"
import { ChartJSNodeCanvas } from 'chartjs-node-canvas';
import { Chart, registerables } from 'chart.js';
import fs from 'fs';

// BAML_LOG=info infisical run --env=test -- npx tsx tests/load.test.ts

// Register Chart.js components
Chart.register(...registerables);



type MemoryEvent = {
  timestamp: number;
  event: string;
}

function createEventLogger(events: MemoryEvent[]) {
  return (event: string) => {
    console.log('logging event', event);
    events.push({
      timestamp: Date.now(),
      event
    });
  };
}

async function measureMemoryUsage<T>(
  operation: (logEvent: (event: string) => void) => Promise<T>,
  options = { sampleInterval: 50 }
): Promise<{
  result: T;
  memoryUsage: {
    peak: { heapUsed: number; rss: number };
    peakDiff: { heapUsed: number; rss: number };
    baseline: { heapUsed: number; rss: number };
    timeline: Array<{ timestamp: number; heapUsed: number; rss: number; heapUsedDiff: number; rssDiff: number }>;
    events: MemoryEvent[];
  };
}> {
  const memoryUsageLog: { timestamp: number; heapUsed: number; rss: number; heapUsedDiff: number; rssDiff: number }[] = [];
  const events: MemoryEvent[] = [];
  
  // Get baseline memory usage
  const baseline = process.memoryUsage();
  let maxHeapUsed = 0;
  let maxRss = 0;
  let maxHeapUsedDiff = 0;
  let maxRssDiff = 0;

  const logEvent = createEventLogger(events);

  const monitor = setInterval(() => {
    const mem = process.memoryUsage();
    const heapUsedDiff = mem.heapUsed - baseline.heapUsed;
    const rssDiff = mem.rss - baseline.rss;
    
    maxHeapUsed = Math.max(maxHeapUsed, mem.heapUsed);
    maxRss = Math.max(maxRss, mem.rss);
    maxHeapUsedDiff = Math.max(maxHeapUsedDiff, heapUsedDiff);
    maxRssDiff = Math.max(maxRssDiff, rssDiff);
    
    memoryUsageLog.push({
      timestamp: Date.now(),
      heapUsed: mem.heapUsed,
      rss: mem.rss,
      heapUsedDiff,
      rssDiff
    });
    console.log(
      'rss (MB)', (mem.rss / 1024 / 1024).toFixed(2),
      'rssDiff (MB)', (rssDiff / 1024 / 1024).toFixed(2),
      'heapUsed (MB)', (mem.heapUsed / 1024 / 1024).toFixed(2),
      'heapUsedDiff (MB)', (heapUsedDiff / 1024 / 1024).toFixed(2),
      'maxHeapUsedDiff (MB)', (maxHeapUsedDiff / 1024 / 1024).toFixed(2)
    );
  }, options.sampleInterval);

  try {
    console.log('starting operation');
    const result = await operation(logEvent);
    return {
      result,
      memoryUsage: {
        peak: {
          heapUsed: maxHeapUsed,
          rss: maxRss
        },
        peakDiff: {
          heapUsed: maxHeapUsedDiff,
          rss: maxRssDiff
        },
        baseline: {
          heapUsed: baseline.heapUsed,
          rss: baseline.rss
        },
        timeline: memoryUsageLog,
        events
      }
    };
  } finally {
    clearInterval(monitor);
  }
}

async function generateMemoryPlot(timeline: Array<{ timestamp: number; heapUsed: number; rss: number; heapUsedDiff: number; rssDiff: number }>, events: MemoryEvent[]) {
  const width = 800;
  const height = 600;
  const chartCallback = (ChartJS: any) => {
    ChartJS.defaults.responsive = true;
    ChartJS.defaults.maintainAspectRatio = false;
  };
  
  const chartJSNodeCanvas = new ChartJSNodeCanvas({ width, height, chartCallback });
  
  const startTime = timeline[0].timestamp;
  const timeData = timeline.map(t => (t.timestamp - startTime) / 1000); // Convert to seconds
  const heapData = timeline.map(t => t.heapUsed / 1024 / 1024); // Convert to MB
  const rssData = timeline.map(t => t.rss / 1024 / 1024); // Convert to MB
  const heapDiffData = timeline.map(t => t.heapUsedDiff / 1024 / 1024); // Convert to MB
  
  const eventAnnotations = events.map(event => ({
    type: 'line' as const,
    xMin: (event.timestamp - startTime) / 1000,
    xMax: (event.timestamp - startTime) / 1000,
    borderColor: 'red',
    borderWidth: 2,
    label: {
      content: event.event,
      enabled: true,
      position: 'top'
    }
  }));

  const configuration = {
    type: 'line' as const,
    data: {
      labels: timeData,
      datasets: [
        {
          label: 'Heap Used (MB)',
          data: heapData,
          borderColor: 'rgb(75, 192, 192)',
          tension: 0.1
        },
        {
          label: 'RSS (MB)',
          data: rssData,
          borderColor: 'rgb(255, 99, 132)',
          tension: 0.1
        },
        {
          label: 'Heap Used Diff (MB)',
          data: heapDiffData,
          borderColor: 'rgb(75, 192, 192)',
          tension: 0.1
        }
      ]
    },
    options: {
      plugins: {
        title: {
          display: true,
          text: 'Memory Usage Over Time'
        },
        annotation: {
          annotations: eventAnnotations
        }
      },
      scales: {
        x: {
          title: {
            display: true,
            text: 'Time (seconds)'
          }
        },
        y: {
          title: {
            display: true,
            text: 'Memory (MB)'
          }
        }
      }
    }
  };

  const image = await chartJSNodeCanvas.renderToBuffer(configuration);
  fs.writeFileSync('memory-usage-plot.png', image);
}

async function main() {
  if (process.stdout.isTTY) {
    process.stdout.write = process.stdout.write.bind(process.stdout);
  }

  console.log('Running memory usage test...');
  const { memoryUsage } = await measureMemoryUsage(
    async (logEvent) => {
      {
        logEvent(`Stream last started`);
        const stream = b.stream.TestMemory("poems");
        let chunkCount = 0;
        for await (const chunk of stream) {
          chunkCount++;
        }
        await stream.getFinalResponse();
        logEvent(`Stream last complete`);
      }

      console.log('done streaming');
      console.log('waiting');
    },
    { sampleInterval: 1000 }
  );

  console.log('Memory Usage Summary (MB):');
  console.table({
    'Baseline Heap Used': Math.round(memoryUsage.baseline.heapUsed / 1024 / 1024 * 100) / 100,
    'Baseline RSS': Math.round(memoryUsage.baseline.rss / 1024 / 1024 * 100) / 100,
    'Peak Heap Used': Math.round(memoryUsage.peak.heapUsed / 1024 / 1024 * 100) / 100,
    'Peak RSS': Math.round(memoryUsage.peak.rss / 1024 / 1024 * 100) / 100,
    'Peak Heap Used Diff': Math.round(memoryUsage.peakDiff.heapUsed / 1024 / 1024 * 100) / 100,
    'Peak RSS Diff': Math.round(memoryUsage.peakDiff.rss / 1024 / 1024 * 100) / 100,
  });

  fs.writeFileSync('memory-usage-timeline.json', JSON.stringify(memoryUsage.timeline, null, 2));
  fs.writeFileSync('memory-usage-events.json', JSON.stringify(memoryUsage.events, null, 2));

  return memoryUsage;
}

describe('Memory usage tests', () => {
  test('memory usage stays within limits', async () => {
    const memoryUsage = await main();
    const maxHeapUsedDiffMB = Math.round(memoryUsage.peakDiff.heapUsed / 1024 / 1024 * 100) / 100;
    expect(maxHeapUsedDiffMB).toBeLessThan(50);
  }, 90000);
});

// Modify the main execution block
if (require.main === module) {
  main().catch(console.error);
}