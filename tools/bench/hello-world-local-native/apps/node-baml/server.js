import express from 'express';
import { hello_world_async } from './dist/baml_sdk/index.js';

const port = Number(process.env.PORT || 8502);
const app = express();
app.disable('x-powered-by');
app.disable('etag');
app.get('/', async (_req, res) => {
  const body = await hello_world_async();
  res.status(200).set({'Content-Type': 'text/plain; charset=utf-8', 'Cache-Control': 'no-store'}).send(body);
});
const server = app.listen(port, '127.0.0.1', () => console.log(`listening on ${port}`));
server.keepAliveTimeout = 60000;
server.headersTimeout = 65000;
