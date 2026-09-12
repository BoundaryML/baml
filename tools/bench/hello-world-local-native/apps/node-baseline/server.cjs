const express = require('express');

const port = Number(process.env.PORT || 8501);
const app = express();
app.disable('x-powered-by');
app.disable('etag');
app.get('/', (_req, res) => {
  res.status(200).set({'Content-Type': 'text/plain; charset=utf-8', 'Cache-Control': 'no-store'}).send('hello world');
});
const server = app.listen(port, '127.0.0.1', () => console.log(`listening on ${port}`));
server.keepAliveTimeout = 60000;
server.headersTimeout = 65000;
