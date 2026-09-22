import { once } from 'node:events';
import { createServer } from 'node:http';
import { resolve } from 'node:path';
import next from 'next';
import { z } from 'zod';

import { validateBuiltStyles } from '../lib/built-styles';

// Book preferences use request-time cookies. Inspect actual production HTML,
// including the stylesheet links Next emits, rather than assuming a static export.
const directory = resolve(import.meta.dirname, '..');
const app = next({ dev: false, dir: directory });
await app.prepare();
const server = createServer(app.getRequestHandler());
try {
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const address = z.object({ port: z.number() }).parse(server.address());
  const response = await fetch(
    `http://127.0.0.1:${address.port}/baml/book/errors`,
    {
      signal: AbortSignal.timeout(30_000),
    },
  );
  if (!response.ok)
    throw new Error(`Built book page returned HTTP ${response.status}`);
  await validateBuiltStyles(resolve(directory, '.next'), await response.text());
  console.log(
    'Validated the book page’s server-rendered HTML and compiled stylesheets.',
  );
} finally {
  server.closeAllConnections();
  if (server.listening)
    await new Promise<void>((done, reject) =>
      server.close((error) => (error ? reject(error) : done())),
    );
  await app.close();
}
