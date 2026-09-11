const {startMetricsServer} = require('./runtime_metrics.cjs');
const express = require('express');
const app = express();
app.disable('x-powered-by');
app.disable('etag');
app.get('/', async (_req, res) => {
  const body = 'hello world';
  res.status(200).set({'Content-Type': 'text/plain; charset=utf-8', 'Cache-Control': 'no-store'}).send(body);
});
const server = app.listen(8080, '0.0.0.0', () => console.log('listening on 8080'));
server.keepAliveTimeout = 60000;
server.headersTimeout = 65000;

startMetricsServer();
