import metrics from './runtime_metrics.cjs';
import express from 'express';
import { hello_world_async, heap_stats_async } from './baml_sdk/index.js';
const app = express();
app.disable('x-powered-by');
app.disable('etag');
app.get('/', async (_req, res) => {
  const body = await hello_world_async();
  res.status(200).set({'Content-Type': 'text/plain; charset=utf-8', 'Cache-Control': 'no-store'}).send(body);
});
const server = app.listen(8080, '0.0.0.0', () => console.log('listening on 8080'));
server.keepAliveTimeout = 60000;
server.headersTimeout = 65000;

metrics.startMetricsServer(heap_stats_async);
