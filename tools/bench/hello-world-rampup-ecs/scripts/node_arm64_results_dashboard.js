#!/usr/bin/env node
// Writes a focused CloudWatch dashboard for the retained Node-only ARM64 experiment metrics.
const region = process.env.AWS_REGION || 'us-east-1';
const inVpc = 'hello-vpc-node-binary-01';
const local = 'hello-local-node-10k-01';
const appService = run => `${run}-node-only-arm64`;
const loadService = `${inVpc}-load-node-only-arm64`;
const ecsMetric = (metric, run, service, label, stat = 'Maximum') => ['AWS/ECS', metric, 'ClusterName', run, 'ServiceName', service, { label, stat }];
const customMetric = (metric, label, stat = 'Average', options = {}) => ['BAML/HelloWorldRampup', metric, 'RunName', inVpc, 'Variant', 'node-only', 'Architecture', 'arm64', { label, stat, ...options }];
const metricWidget = (title, x, y, metrics, options = {}) => ({ type: 'metric', x, y, width: 12, height: 7,
  properties: { title, region, view: 'timeSeries', stacked: false, period: 60, metrics,
    yAxis: { left: { min: 0, showUnits: false } }, ...options } });
const widgets = [
  { type: 'text', x: 0, y: 0, width: 24, height: 3, properties: { markdown: '# Node-only ARM64 benchmark results\nIn-VPC binary search: **5,200 active RPS passed; 5,300 failed**. Local/public search: **3,800 passed; 3,900 failed**. Every candidate used six 4s-on/1s-off cycles. Dashed vertical context comes from the offered-rate and completion panels below.' } },
  metricWidget('Application CPU (% of 1-vCPU task allocation)', 0, 3, [
    ecsMetric('CPUUtilization', inVpc, appService(inVpc), 'In-VPC app CPU'),
    ecsMetric('CPUUtilization', local, appService(local), 'Local/public app CPU'),
  ], { annotations: { horizontal: [{ label: '100% task allocation', value: 100, color: '#d62728' }] } }),
  metricWidget('Application memory (% of 1-GiB container limit)', 12, 3, [
    ecsMetric('MemoryUtilization', inVpc, appService(inVpc), 'In-VPC app memory'),
    ecsMetric('MemoryUtilization', local, appService(local), 'Local/public app memory'),
  ]),
  metricWidget('Node process memory (in-VPC)', 0, 10, [
    customMetric('ProcessRssBytes', 'Process RSS', 'Maximum'),
    customMetric('NodeHeapBytes', 'V8 heap used', 'Maximum'),
  ], { yAxis: { left: { min: 0, label: 'Bytes', showUnits: true } } }),
  metricWidget('AWS Vegeta task utilization (4-vCPU / 1-GiB allocation)', 12, 10, [
    ecsMetric('CPUUtilization', inVpc, loadService, 'Load CPU'),
    ecsMetric('MemoryUtilization', inVpc, loadService, 'Load memory'),
  ]),
  metricWidget('Offered active RPS and completed HTTP 200 / wall second', 0, 17, [
    customMetric('TargetRps', 'Offered active RPS', 'Average', { id: 'target', period: 5 }),
    customMetric('Http200', 'HTTP 200 count', 'Sum', { id: 'ok', period: 5, visible: false }),
    [{ expression: 'ok/PERIOD(ok)', label: 'HTTP 200 / wall second', id: 'okrps' }],
  ], { period: 5, annotations: { horizontal: [{ label: '5,200 pass', value: 5200, color: '#2ca02c' }, { label: '5,300 fail', value: 5300, color: '#d62728' }] } }),
  metricWidget('Response latency (in-VPC)', 12, 17, [
    customMetric('LatencyP90Ms', 'p90', 'Average', { period: 5 }),
    customMetric('LatencyP99Ms', 'p99', 'Average', { period: 5 }),
    customMetric('LatencyMaxMs', 'max', 'Maximum', { period: 5 }),
  ], { period: 5, yAxis: { left: { min: 0, label: 'Milliseconds', showUnits: true } } }),
  metricWidget('Request errors and forced cycle stops (in-VPC)', 0, 24, [
    customMetric('TransportErrors', 'Transport errors', 'Sum', { period: 5 }),
    customMetric('HttpErrors', 'HTTP/request errors', 'Sum', { period: 5 }),
    customMetric('BodyMismatches', 'Body mismatches', 'Sum', { period: 5 }),
    customMetric('ForcedStop', 'Forced cycle stop', 'Sum', { period: 5 }),
  ], { period: 5 }),
];
console.log(JSON.stringify({ start: '-PT3H', periodOverride: 'inherit', widgets }, null, 2));
